use crate::db::{Download, Episode, Show, Track, TrackSyncMode};
use crate::wfmu;
use crate::AppState;
use rusqlite::params;
use serde::Serialize;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

type CmdResult<T> = Result<T, String>;
type ListenExportRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    i64,
    i64,
    i64,
);
const LIVE_PLAYLIST_REFRESH_SECONDS: i64 = 30;
const CATALOG_CACHE_KEY: &str = "catalog_last_scraped";
const CATALOG_CACHE_MAX_AGE_SECONDS: i64 = 24 * 60 * 60;
const SHOW_CACHE_MAX_AGE_SECONDS: i64 = 6 * 60 * 60;

fn db_err(e: rusqlite::Error) -> String {
    format!("db error: {e}")
}

fn cache_is_stale(last_scraped: Option<i64>, now: i64, max_age_seconds: i64) -> bool {
    match last_scraped {
        Some(timestamp) if timestamp <= now => now.saturating_sub(timestamp) >= max_age_seconds,
        // Missing and future timestamps both need a refresh. Treating a future value as fresh
        // forever would wedge the cache after a clock correction or a damaged setting.
        _ => true,
    }
}

#[derive(Serialize)]
pub struct ShowDetail {
    pub show: Show,
    pub episodes: Vec<Episode>,
}

#[derive(Serialize)]
pub struct AudioSource {
    pub url: String,
    pub local: bool,
    /// Archive pre-roll offset in seconds (0 when unknown / old mp3-era archives).
    pub offset_sec: i64,
}

#[derive(Serialize)]
pub struct TrackHit {
    pub track: Track,
    pub show_id: String,
    pub show_name: String,
    pub air_date: Option<String>,
}

#[derive(Serialize)]
pub struct SearchResults {
    pub shows: Vec<Show>,
    pub tracks: Vec<TrackHit>,
}

#[derive(Serialize, Clone)]
pub struct LiveSong {
    pub artist: Option<String>,
    pub title: Option<String>,
}

#[derive(Serialize)]
pub struct LiveStatus {
    pub episode: Episode,
    pub show_name: String,
    pub current_song: Option<LiveSong>,
    pub tracks: Vec<Track>,
    pub current_track_id: Option<i64>,
    pub playlist_needs_load: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveProgram {
    pub show_id: Option<String>,
    pub name: String,
    pub host: Option<String>,
    pub description: Option<String>,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
}

impl From<wfmu::ParsedLiveProgram> for LiveProgram {
    fn from(program: wfmu::ParsedLiveProgram) -> Self {
        Self {
            show_id: program.show_id,
            name: program.name,
            host: program.host,
            description: program.description,
            starts_at: program.starts_at,
            ends_at: program.ends_at,
        }
    }
}

#[derive(Serialize)]
pub struct LivePage {
    pub tracks: Vec<Track>,
    pub current_track_id: Option<i64>,
    pub current_show: Option<LiveProgram>,
    pub upcoming_shows: Vec<LiveProgram>,
    pub history_source: String,
    pub warning: Option<String>,
    pub updated_at: i64,
}

#[tauri::command]
pub async fn get_catalog(refresh: bool, state: State<'_, AppState>) -> CmdResult<Vec<Show>> {
    let (cached_count, cache_stale) = {
        let db = state.db()?;
        let count = db.show_count().map_err(db_err)?;
        let last_scraped = db
            .get_setting(CATALOG_CACHE_KEY)
            .map_err(db_err)?
            .and_then(|value| value.parse::<i64>().ok());
        (
            count,
            cache_is_stale(
                last_scraped,
                crate::db::Db::now(),
                CATALOG_CACHE_MAX_AGE_SECONDS,
            ),
        )
    };
    let need_scrape = refresh || cached_count == 0 || cache_stale;
    if need_scrape {
        let html = match state.fetcher.get_text(&wfmu::catalog_url()).await {
            Ok(html) => html,
            Err(error) if cached_count > 0 && !refresh => {
                eprintln!("catalog refresh failed; serving cached shows: {error}");
                return state.db()?.list_shows().map_err(db_err);
            }
            Err(error) => return Err(error),
        };
        let shows = wfmu::parse_catalog(&html);
        if shows.is_empty() {
            if cached_count > 0 && !refresh {
                eprintln!("catalog refresh parsed no shows; serving cached catalog");
                return state.db()?.list_shows().map_err(db_err);
            }
            return Err("catalog parse produced no shows (site layout changed?)".into());
        }
        let db = state.db()?;
        for s in &shows {
            db.upsert_show(&s.id, &s.name, s.dj.as_deref(), s.on_air)
                .map_err(db_err)?;
        }
        db.set_setting(CATALOG_CACHE_KEY, &crate::db::Db::now().to_string())
            .map_err(db_err)?;
    }
    state.db()?.list_shows().map_err(db_err)
}

#[tauri::command]
pub async fn get_show(
    show_id: String,
    refresh: bool,
    state: State<'_, AppState>,
) -> CmdResult<ShowDetail> {
    let last_scraped = {
        let db = state.db()?;
        db.show_last_scraped(&show_id).map_err(db_err)?
    };
    let had_cache = last_scraped.is_some();
    let need_scrape = refresh
        || cache_is_stale(
            last_scraped,
            crate::db::Db::now(),
            SHOW_CACHE_MAX_AGE_SECONDS,
        );
    if need_scrape {
        let html = match state.fetcher.get_text(&wfmu::show_url(&show_id)).await {
            Ok(html) if !html.trim().is_empty() => Some(html),
            Ok(_) if had_cache && !refresh => {
                eprintln!("show {show_id} refresh was empty; serving cached episodes");
                None
            }
            Ok(_) => return Err("show page was empty".into()),
            Err(error) if had_cache && !refresh => {
                eprintln!("show {show_id} refresh failed; serving cached episodes: {error}");
                None
            }
            Err(error) => return Err(error),
        };
        if let Some(html) = html {
            let mut episodes = wfmu::parse_show_page(&html);
            let description = wfmu::parse_show_description(&html);

            // Deep archive hydration is needed once (or for an explicit forced refresh). Routine
            // TTL refreshes only fetch the current show page; sync_show_episodes keeps the cached
            // historical tail ordered behind the new rows.
            if !had_cache || refresh {
                let archive_years = wfmu::parse_show_archive_years(&html, &show_id);
                // Year pages link back to the show and to each other, so track what we've
                // already pulled.
                let mut visited: HashSet<&str> = HashSet::new();
                for year_path in &archive_years {
                    if !visited.insert(year_path.as_str()) {
                        continue;
                    }
                    let year_url = format!("{}{}", wfmu::BASE, year_path);
                    // A dead year page must not abort the whole show: bailing here would skip
                    // mark_show_scraped and force a full re-scrape on every later visit.
                    let year_html = match state.fetcher.get_text(&year_url).await {
                        Ok(h) => h,
                        Err(e) => {
                            eprintln!("archive year {year_url} failed: {e}");
                            continue;
                        }
                    };
                    episodes.extend(wfmu::parse_show_page(&year_html));
                }
            }

            let mut db = state.db()?;
            db.sync_show_episodes(&show_id, &episodes).map_err(db_err)?;
            if let Some(desc) = description {
                db.set_show_description(&show_id, &desc).map_err(db_err)?;
            }
            db.mark_show_scraped(&show_id).map_err(db_err)?;
        }
    }
    let db = state.db()?;
    let episodes = db.list_episodes(&show_id).map_err(db_err)?;
    let show = db
        .list_shows()
        .map_err(db_err)?
        .into_iter()
        .find(|s| s.id == show_id)
        .ok_or_else(|| format!("unknown show {show_id}"))?;
    Ok(ShowDetail { show, episodes })
}

#[tauri::command]
pub async fn get_playlist(
    episode_id: i64,
    refresh: bool,
    state: State<'_, AppState>,
) -> CmdResult<Vec<Track>> {
    let need_scrape = {
        let db = state.db()?;
        refresh || !db.episode_tracks_scraped(episode_id).map_err(db_err)?
    };
    if need_scrape {
        let html = state
            .fetcher
            .get_text(&wfmu::playlist_url(episode_id))
            .await?;
        let tracks = wfmu::parse_playlist(&html);
        // The playlist page also carries the pop-up-player link; use it to fill in an
        // archive id for episodes whose show-index block had none, so they become playable.
        let discovered = wfmu::parse_playlist_archive(&html);
        let mut db = state.db()?;
        db.sync_tracks(episode_id, &tracks, TrackSyncMode::Snapshot)
            .map_err(db_err)?;
        if let Some(archive) = discovered {
            if db
                .get_episode(episode_id)
                .map(|e| e.archive_id.is_none())
                .unwrap_or(false)
            {
                db.set_episode_archive(episode_id, archive)
                    .map_err(db_err)?;
            }
        }
    }
    state.db()?.list_tracks(episode_id).map_err(db_err)
}

#[tauri::command]
pub async fn get_live_status(
    stream_id: String,
    status_source: wfmu::LiveStatusSource,
    fallback_name: String,
    state: State<'_, AppState>,
) -> CmdResult<LiveStatus> {
    let body = state.fetcher.get_status_text(&status_source.url()).await?;
    let status = status_source
        .parse(&body)
        .ok_or("the station has no current status yet")?;
    let hosted = status.episode_id.is_some();
    if !hosted && status.artist.is_none() && status.title.is_none() {
        return Err("the station has no current track yet".into());
    }
    let now = crate::db::Db::now();
    let episode_id = status
        .episode_id
        .unwrap_or_else(|| synthetic_live_episode_id(&stream_id, now));
    // A None show_id means the program didn't map to a real archived show, so we mint a
    // synthetic live-* row. Only those get flagged is_live; a real show must stay in the catalog.
    let synthetic_show = status.show_id.is_none();
    let show_id = status
        .show_id
        .clone()
        .unwrap_or_else(|| format!("live-{stream_id}"));
    let show_name = useful_live_show_name(Some(&status)).unwrap_or(fallback_name);
    let air_date = ts_to_iso(now).split(' ').next().map(str::to_string);
    let episode_title = Some(if hosted {
        "Live broadcast"
    } else {
        "Live stream"
    });
    let current_song = if status.artist.is_some() || status.title.is_some() {
        Some(LiveSong {
            artist: status.artist.clone(),
            title: status.title.clone(),
        })
    } else {
        None
    };

    let mut db = state.db()?;
    db.upsert_show(&show_id, &show_name, None, true)
        .map_err(db_err)?;
    if synthetic_show {
        db.set_show_live(&show_id).map_err(db_err)?;
    }
    db.upsert_episode(
        episode_id,
        &show_id,
        air_date.as_deref(),
        episode_title,
        None,
        0,
    )
    .map_err(db_err)?;
    // The main FM homepage exposes the active playlist but no current-song fields.
    // Re-scrape that append-only playlist periodically while listening; otherwise the
    // first snapshot remains cached for the rest of the broadcast. Channel feeds with
    // current-song metadata keep using the lightweight observation path below.
    let playlist_needs_load = hosted
        && if current_song.is_none() {
            db.episode_tracks_stale(episode_id, LIVE_PLAYLIST_REFRESH_SECONDS)
                .map_err(db_err)?
        } else {
            !db.episode_tracks_scraped(episode_id).map_err(db_err)?
        };
    if (!hosted || !playlist_needs_load) && current_song.is_some() {
        let observation = polled_live_track(&status).into_iter().collect::<Vec<_>>();
        db.sync_tracks(episode_id, &observation, TrackSyncMode::AppendObservations)
            .map_err(db_err)?;
    }
    let episode = db.get_episode(episode_id).map_err(db_err)?;
    let tracks = db.list_tracks(episode_id).map_err(db_err)?;
    let current_track_id = current_song
        .as_ref()
        .and_then(|song| {
            tracks
                .iter()
                .rev()
                .find(|track| track.artist == song.artist && track.title == song.title)
        })
        .or_else(|| tracks.last())
        .map(|track| track.id);
    Ok(LiveStatus {
        episode,
        show_name,
        current_song,
        tracks,
        current_track_id,
        playlist_needs_load,
    })
}

#[tauri::command]
pub async fn get_live_page(
    stream_id: String,
    refresh: bool,
    state: State<'_, AppState>,
) -> CmdResult<LivePage> {
    let station = wfmu::live_station(&stream_id)
        .ok_or_else(|| format!("unknown live station {stream_id}"))?;
    let provider_url = wfmu::rethink_playlist_url(station.rethink_code);
    let provider_result = state
        .fetcher
        .get_live_page_text(&provider_url)
        .await
        .and_then(|body| wfmu::parse_rethink_live_page(&body));

    let mut warnings = Vec::new();
    let mut provider = match provider_result {
        Ok(page) => Some(page),
        Err(error) => {
            warnings.push(format!("Recent-song service: {error}"));
            None
        }
    };

    let live_show_id = format!("live-{}", station.id);
    if let Some(page) = provider.as_ref() {
        let grouped = page.tracks.iter().fold(
            BTreeMap::<i64, (String, Vec<wfmu::ParsedRecentTrack>)>::new(),
            |mut grouped, track| {
                grouped
                    .entry(synthetic_live_episode_id(station.id, track.played_at))
                    .or_insert_with(|| (track.air_date.clone(), Vec::new()))
                    .1
                    .push(track.clone());
                grouped
            },
        );
        let mut db = state.db()?;
        db.upsert_show(&live_show_id, station.name, None, true)
            .map_err(db_err)?;
        db.set_show_live(&live_show_id).map_err(db_err)?;
        for (episode_id, (date, tracks)) in grouped {
            db.upsert_episode(
                episode_id,
                &live_show_id,
                Some(&date),
                Some("Live history"),
                None,
                0,
            )
            .map_err(db_err)?;
            db.sync_provider_tracks(episode_id, station.rethink_code, &tracks)
                .map_err(db_err)?;
        }
    }

    let schedule = {
        let cached = state.live_schedule_cache.lock().await;
        cached.get(station.id).and_then(|(fetched, programs)| {
            (!refresh && fetched.elapsed() < Duration::from_secs(300)).then(|| programs.clone())
        })
    };
    let schedule = if let Some(schedule) = schedule {
        schedule
    } else {
        match state.fetcher.get_live_page_text(station.info_url).await {
            Ok(html) => {
                let parsed = wfmu::parse_live_schedule(&html);
                if !parsed.is_empty() {
                    state.live_schedule_cache.lock().await.insert(
                        station.id.to_string(),
                        (std::time::Instant::now(), parsed.clone()),
                    );
                }
                parsed
            }
            Err(error) => {
                warnings.push(format!("Schedule: {error}"));
                Vec::new()
            }
        }
    };

    let current_day = provider
        .as_ref()
        .and_then(|page| page.current_show.as_ref())
        .and_then(|program| program.day.clone())
        .or_else(|| {
            schedule
                .iter()
                .find(|program| program.current)
                .and_then(|program| program.day.clone())
        })
        .or_else(|| schedule.first().and_then(|program| program.day.clone()));
    let today = schedule
        .iter()
        .filter(|program| {
            current_day
                .as_deref()
                .zip(program.day.as_deref())
                .map(|(wanted, actual)| wanted.eq_ignore_ascii_case(actual))
                .unwrap_or(program.current)
        })
        .cloned()
        .collect::<Vec<_>>();

    let schedule_current = today.iter().find(|program| program.current).cloned();
    let mut current = provider
        .as_mut()
        .and_then(|page| page.current_show.take())
        .or(schedule_current.clone());
    if let Some(program) = current.as_mut() {
        enrich_program(program, &today);
    }

    let next = provider.as_mut().and_then(|page| page.next_show.take());
    let start_index = next
        .as_ref()
        .and_then(|next| matching_program_index(&today, next))
        .or_else(|| {
            today
                .iter()
                .position(|program| program.current)
                .map(|index| index + 1)
        })
        .unwrap_or(today.len());
    let mut upcoming = Vec::<wfmu::ParsedLiveProgram>::new();
    if let Some(mut next) = next {
        enrich_program(&mut next, &today);
        upcoming.push(next);
    }
    for program in today.into_iter().skip(start_index) {
        let duplicate = upcoming
            .iter()
            .any(|existing| same_program(existing, &program));
        if !program.current && !duplicate {
            upcoming.push(program);
        }
    }

    let db = state.db()?;
    let tracks = db
        .list_recent_live_tracks(&live_show_id, 20)
        .map_err(db_err)?;
    let current_source = provider
        .as_ref()
        .and_then(|page| page.tracks.last())
        .map(|track| format!("{}:{}", station.rethink_code, track.source_id));
    let current_track_id = current_source
        .as_deref()
        .and_then(|source| {
            tracks
                .iter()
                .find(|track| track.source_id.as_deref() == Some(source))
        })
        .or_else(|| tracks.last())
        .map(|track| track.id);
    let provider_updated = provider.as_ref().and_then(|page| page.updated_at);
    let history_source = if provider
        .as_ref()
        .is_some_and(|page| !page.tracks.is_empty())
    {
        "radio_rethink"
    } else {
        "local_cache"
    };

    Ok(LivePage {
        tracks,
        current_track_id,
        current_show: current.map(Into::into),
        upcoming_shows: upcoming.into_iter().map(Into::into).collect(),
        history_source: history_source.to_string(),
        warning: (!warnings.is_empty()).then(|| warnings.join(" · ")),
        updated_at: provider_updated.unwrap_or_else(crate::db::Db::now),
    })
}

fn same_program(left: &wfmu::ParsedLiveProgram, right: &wfmu::ParsedLiveProgram) -> bool {
    left.show_id
        .as_deref()
        .zip(right.show_id.as_deref())
        .map(|(left, right)| left.eq_ignore_ascii_case(right))
        .unwrap_or_else(|| left.name.eq_ignore_ascii_case(&right.name))
}

fn matching_program_index(
    programs: &[wfmu::ParsedLiveProgram],
    target: &wfmu::ParsedLiveProgram,
) -> Option<usize> {
    programs
        .iter()
        .position(|program| same_program(program, target))
}

fn enrich_program(program: &mut wfmu::ParsedLiveProgram, schedule: &[wfmu::ParsedLiveProgram]) {
    if let Some(match_) = schedule
        .iter()
        .find(|candidate| same_program(candidate, program))
    {
        if program.description.is_none() {
            program.description = match_.description.clone();
        }
        if program.show_id.is_none() {
            program.show_id = match_.show_id.clone();
        }
    }
}

fn useful_live_show_name(status: Option<&wfmu::ParsedLiveStatus>) -> Option<String> {
    status
        .and_then(|s| s.show_name.clone())
        .filter(|name| !name.ends_with(" Radio Stream") && name != "WFMU")
}

fn polled_live_track(status: &wfmu::ParsedLiveStatus) -> Option<wfmu::ParsedTrack> {
    if status.artist.is_none() && status.title.is_none() {
        return None;
    }
    Some(wfmu::ParsedTrack {
        artist: status.artist.clone(),
        title: status.title.clone(),
        album: None,
        label: None,
        comments: None,
        start_sec: None,
    })
}

fn synthetic_live_episode_id(stream_id: &str, timestamp: i64) -> i64 {
    // Negative ids cannot collide with WFMU's real episode ids. Keep one episode per
    // UTC day and channel so stream-only tracks and listening stats stay grouped.
    let channel = stream_id.bytes().fold(0_u64, |hash, byte| {
        hash.wrapping_mul(16777619).wrapping_add(byte as u64)
    }) % 1_000_000;
    -(timestamp.div_euclid(86400) * 1_000_000 + channel as i64 + 1)
}

#[tauri::command]
pub async fn resolve_audio(episode_id: i64, state: State<'_, AppState>) -> CmdResult<AudioSource> {
    let ep = {
        let db = state.db()?;
        db.get_episode(episode_id).map_err(db_err)?
    };
    if ep.downloaded {
        if let Some(path) = ep.download_path.clone() {
            if std::path::Path::new(&path).exists() {
                // Backfill the offset for episodes downloaded before it was tracked.
                let offset_sec = ensure_offset(&state, &ep).await;
                return Ok(AudioSource {
                    url: path,
                    local: true,
                    offset_sec,
                });
            }
        }
    }
    if let Some(url) = ep
        .audio_url
        .as_deref()
        .and_then(|url| wfmu::validate_audio_url(url).ok())
    {
        // Backfill the offset for episodes resolved before it was tracked.
        let offset_sec = ensure_offset(&state, &ep).await;
        return Ok(AudioSource {
            url: url.into(),
            local: false,
            offset_sec,
        });
    }

    // Determine the archive id. If the show-index scrape didn't capture one (common on
    // deep-archive shows that only show a pop-up-player link on the playlist page),
    // fetch the playlist page to discover it.
    let archive_id = match ep.archive_id {
        Some(a) => a,
        None => {
            let html = state
                .fetcher
                .get_text(&wfmu::playlist_url(episode_id))
                .await?;
            let a = wfmu::parse_playlist_archive(&html)
                .ok_or("this episode has no audio archive (playlist only)")?;
            state
                .db
                .lock()
                .unwrap()
                .set_episode_archive(episode_id, a)
                .map_err(db_err)?;
            a
        }
    };

    // Resolve the direct audio URL via the AccuPlayer page (works for both the
    // mp3archives.wfmu.org and s3.amazonaws.com/arch.wfmu.org backends, old and new).
    // The same page carries the archive pre-roll offset, so capture it here too.
    let player_html = state
        .fetcher
        .get_text(&wfmu::archiveplayer_url(episode_id, archive_id))
        .await?;
    let offset_sec = wfmu::parse_archiveplayer_offset(&player_html).unwrap_or(0);
    let url = match wfmu::parse_archiveplayer(&player_html) {
        Some(u) => u,
        None => {
            // Legacy fallback: some episodes still resolve via listen.m3u.
            let body = state
                .fetcher
                .get_text(&wfmu::m3u_url(episode_id, archive_id))
                .await?;
            wfmu::parse_m3u(&body).ok_or("could not resolve an audio URL for this episode")?
        }
    };
    let url = wfmu::validate_audio_url(&url)?.to_string();
    {
        let db = state.db()?;
        db.set_audio_url(episode_id, &url).map_err(db_err)?;
        db.set_episode_offset(episode_id, offset_sec)
            .map_err(db_err)?;
    }
    Ok(AudioSource {
        url,
        local: false,
        offset_sec,
    })
}

/// Return the episode's archive pre-roll offset, fetching the AccuPlayer page to
/// backfill it once for episodes resolved/downloaded before the offset was tracked.
/// Falls back to 0 when it can't be determined (no archive id, or a failed fetch).
async fn ensure_offset(state: &State<'_, AppState>, ep: &Episode) -> i64 {
    if let Some(o) = ep.offset_sec {
        return o;
    }
    let Some(archive_id) = ep.archive_id else {
        return 0;
    };
    match state
        .fetcher
        .get_text(&wfmu::archiveplayer_url(ep.id, archive_id))
        .await
    {
        Ok(html) => {
            let off = wfmu::parse_archiveplayer_offset(&html).unwrap_or(0);
            if let Ok(db) = state.db() {
                let _ = db.set_episode_offset(ep.id, off);
            }
            off
        }
        // Network hiccup: don't persist, so we retry next time.
        Err(_) => 0,
    }
}

#[tauri::command]
pub fn toggle_favourite(
    kind: String,
    ref_id: String,
    state: State<'_, AppState>,
) -> CmdResult<bool> {
    if !matches!(kind.as_str(), "show" | "episode" | "track") {
        return Err(format!("bad favourite kind: {kind}"));
    }
    state.db()?.toggle_favourite(&kind, &ref_id).map_err(db_err)
}

#[derive(Serialize)]
pub struct FavouriteEpisode {
    pub episode: Episode,
    pub show_name: String,
    pub added_at: i64,
}

#[derive(Serialize)]
pub struct FavouriteTrack {
    pub track: Track,
    pub show_id: String,
    pub show_name: String,
    pub air_date: Option<String>,
    pub added_at: i64,
}

#[derive(Serialize)]
pub struct FavouriteShow {
    pub show: Show,
    pub added_at: i64,
}

#[derive(Serialize)]
pub struct Favourites {
    pub shows: Vec<FavouriteShow>,
    pub episodes: Vec<FavouriteEpisode>,
    pub tracks: Vec<FavouriteTrack>,
}

#[tauri::command]
pub fn list_favourites(state: State<'_, AppState>) -> CmdResult<Favourites> {
    let db = state.db()?;
    let all_shows = db.list_shows().map_err(db_err)?;

    let mut shows = Vec::new();
    let mut episodes = Vec::new();
    let mut tracks = Vec::new();

    let mut stmt = db
        .conn
        .prepare("SELECT kind, ref_id, added_at FROM favourites ORDER BY added_at DESC")
        .map_err(db_err)?;
    let favs: Vec<(String, String, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(db_err)?
        .filter_map(|r| r.ok())
        .collect();

    for (kind, ref_id, added_at) in favs {
        match kind.as_str() {
            "show" => {
                if let Some(show) = all_shows.iter().find(|s| s.id == ref_id) {
                    shows.push(FavouriteShow {
                        show: show.clone(),
                        added_at,
                    });
                }
            }
            "episode" => {
                if let Ok(id) = ref_id.parse::<i64>() {
                    if let Ok(episode) = db.get_episode(id) {
                        let show_name = all_shows
                            .iter()
                            .find(|s| s.id == episode.show_id)
                            .map(|s| s.name.clone())
                            .unwrap_or_default();
                        episodes.push(FavouriteEpisode {
                            episode,
                            show_name,
                            added_at,
                        });
                    }
                }
            }
            "track" => {
                if let Ok(id) = ref_id.parse::<i64>() {
                    let row = db.conn.query_row(
                        "SELECT t.id, t.episode_id, t.seq, t.artist, t.title, t.album, t.label,
                                t.comments, t.start_sec, t.source_id, t.played_at, e.show_id, e.air_date
                         FROM tracks t JOIN episodes e ON e.id = t.episode_id WHERE t.id = ?1",
                        [id],
                        |r| {
                            Ok((
                                Track {
                                    id: r.get(0)?,
                                    episode_id: r.get(1)?,
                                    seq: r.get(2)?,
                                    artist: r.get(3)?,
                                    title: r.get(4)?,
                                    album: r.get(5)?,
                                    label: r.get(6)?,
                                    comments: r.get(7)?,
                                    start_sec: r.get(8)?,
                                    source_id: r.get(9)?,
                                    played_at: r.get(10)?,
                                    favourite: true,
                                },
                                r.get::<_, String>(11)?,
                                r.get::<_, Option<String>>(12)?,
                            ))
                        },
                    );
                    if let Ok((track, show_id, air_date)) = row {
                        let show_name = all_shows
                            .iter()
                            .find(|s| s.id == show_id)
                            .map(|s| s.name.clone())
                            .unwrap_or_default();
                        tracks.push(FavouriteTrack {
                            track,
                            show_id,
                            show_name,
                            air_date,
                            added_at,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    Ok(Favourites {
        shows,
        episodes,
        tracks,
    })
}

#[tauri::command]
pub fn search(query: String, state: State<'_, AppState>) -> CmdResult<SearchResults> {
    let q = query.trim().to_string();
    if q.is_empty() {
        return Ok(SearchResults {
            shows: vec![],
            tracks: vec![],
        });
    }
    let db = state.db()?;
    let like = format!("%{}%", q.replace('%', "").replace('_', " "));
    let shows: Vec<Show> = db
        .list_shows()
        .map_err(db_err)?
        .into_iter()
        .filter(|s| {
            s.name.to_lowercase().contains(&q.to_lowercase())
                || s.dj
                    .as_deref()
                    .map(|d| d.to_lowercase().contains(&q.to_lowercase()))
                    .unwrap_or(false)
        })
        .collect();

    let track_sql = if db.fts {
        "SELECT t.id, t.episode_id, t.seq, t.artist, t.title, t.album, t.label, t.comments, t.start_sec,
                t.source_id, t.played_at,
                EXISTS(SELECT 1 FROM favourites f WHERE f.kind='track' AND f.ref_id=CAST(t.id AS TEXT)),
                e.show_id, s.name, e.air_date
         FROM tracks_fts ft
         JOIN tracks t ON t.id = CAST(ft.track_id AS INTEGER)
         JOIN episodes e ON e.id = t.episode_id
         JOIN shows s ON s.id = e.show_id
         WHERE tracks_fts MATCH ?1 AND s.is_live = 0 LIMIT 200"
    } else {
        "SELECT t.id, t.episode_id, t.seq, t.artist, t.title, t.album, t.label, t.comments, t.start_sec,
                t.source_id, t.played_at,
                EXISTS(SELECT 1 FROM favourites f WHERE f.kind='track' AND f.ref_id=CAST(t.id AS TEXT)),
                e.show_id, s.name, e.air_date
         FROM tracks t
         JOIN episodes e ON e.id = t.episode_id
         JOIN shows s ON s.id = e.show_id
         WHERE (t.artist LIKE ?1 OR t.title LIKE ?1 OR t.album LIKE ?1) AND s.is_live = 0 LIMIT 200"
    };
    let param: String = if db.fts {
        // FTS5 query syntax: quote each term to avoid operator injection.
        q.split_whitespace()
            .map(|t| format!("\"{}\"", t.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        like
    };
    let mut stmt = db.conn.prepare(track_sql).map_err(db_err)?;
    let tracks: Vec<TrackHit> = stmt
        .query_map(params![param], |r| {
            Ok(TrackHit {
                track: Track {
                    id: r.get(0)?,
                    episode_id: r.get(1)?,
                    seq: r.get(2)?,
                    artist: r.get(3)?,
                    title: r.get(4)?,
                    album: r.get(5)?,
                    label: r.get(6)?,
                    comments: r.get(7)?,
                    start_sec: r.get(8)?,
                    source_id: r.get(9)?,
                    played_at: r.get(10)?,
                    favourite: r.get::<_, i64>(11)? != 0,
                },
                show_id: r.get(12)?,
                show_name: r.get(13)?,
                air_date: r.get(14)?,
            })
        })
        .map_err(db_err)?
        .filter_map(|r| r.ok())
        .collect();

    Ok(SearchResults { shows, tracks })
}

#[tauri::command]
pub fn record_listen(
    session_id: String,
    episode_id: i64,
    seconds: i64,
    completed: bool,
    position: i64,
    duration: i64,
    state: State<'_, AppState>,
) -> CmdResult<()> {
    state
        .db
        .lock()
        .unwrap()
        .record_listen(
            &session_id,
            episode_id,
            seconds,
            completed,
            position,
            duration,
        )
        .map_err(db_err)
}

/// The folder new downloads are saved to: the user's chosen directory, or the default
/// under the app data dir.
#[tauri::command]
pub fn get_download_dir(app: AppHandle, state: State<'_, AppState>) -> CmdResult<String> {
    if let Some(d) = state.db()?.get_setting("download_dir").map_err(db_err)? {
        if !d.trim().is_empty() {
            return Ok(d);
        }
    }
    let def = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("downloads");
    Ok(def.to_string_lossy().to_string())
}

#[tauri::command]
pub fn set_download_dir(dir: String, app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    // Persist the choice, and grant the asset protocol access to it now. The dialog's
    // temporary scope grant is not persisted across restarts, so downloads under a custom
    // folder would otherwise stop playing after a relaunch (the startup grant in run() covers
    // subsequent launches). Granting the default $APPDATA/downloads dir again is harmless.
    if !dir.trim().is_empty() {
        app.asset_protocol_scope()
            .allow_directory(&dir, true)
            .map_err(|e| format!("could not grant folder access: {e}"))?;
    }
    state
        .db()?
        .set_setting("download_dir", &dir)
        .map_err(db_err)
}

#[derive(Serialize)]
pub struct ShowStat {
    pub show_id: String,
    pub show_name: String,
    pub seconds: i64,
    pub plays: i64,
}

#[derive(Serialize)]
pub struct EpisodeStat {
    pub episode_id: i64,
    pub show_name: String,
    pub air_date: Option<String>,
    pub title: Option<String>,
    pub seconds: i64,
    pub plays: i64,
}

#[derive(Serialize)]
pub struct Stats {
    pub total_seconds: i64,
    pub total_sessions: i64,
    pub shows: Vec<ShowStat>,
    pub episodes: Vec<EpisodeStat>,
}

#[tauri::command]
pub fn get_stats(state: State<'_, AppState>) -> CmdResult<Stats> {
    let db = state.db()?;
    let (total_seconds, total_sessions): (i64, i64) = db
        .conn
        .query_row(
            "SELECT COALESCE(SUM(seconds),0), COUNT(*) FROM listens",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(db_err)?;

    let mut stmt = db
        .conn
        .prepare(
            "SELECT s.id, s.name, SUM(l.seconds), COUNT(l.id)
             FROM listens l
             JOIN episodes e ON e.id = l.episode_id
             JOIN shows s ON s.id = e.show_id
             GROUP BY s.id ORDER BY SUM(l.seconds) DESC LIMIT 100",
        )
        .map_err(db_err)?;
    let shows: Vec<ShowStat> = stmt
        .query_map([], |r| {
            Ok(ShowStat {
                show_id: r.get(0)?,
                show_name: r.get(1)?,
                seconds: r.get(2)?,
                plays: r.get(3)?,
            })
        })
        .map_err(db_err)?
        .filter_map(|r| r.ok())
        .collect();

    let mut stmt = db
        .conn
        .prepare(
            "SELECT e.id, s.name, e.air_date, e.title, SUM(l.seconds), COUNT(l.id)
             FROM listens l
             JOIN episodes e ON e.id = l.episode_id
             JOIN shows s ON s.id = e.show_id
             GROUP BY e.id ORDER BY SUM(l.seconds) DESC LIMIT 100",
        )
        .map_err(db_err)?;
    let episodes: Vec<EpisodeStat> = stmt
        .query_map([], |r| {
            Ok(EpisodeStat {
                episode_id: r.get(0)?,
                show_name: r.get(1)?,
                air_date: r.get(2)?,
                title: r.get(3)?,
                seconds: r.get(4)?,
                plays: r.get(5)?,
            })
        })
        .map_err(db_err)?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Stats {
        total_seconds,
        total_sessions,
        shows,
        episodes,
    })
}

#[derive(Serialize)]
pub struct DownloadRow {
    pub download: Download,
    pub show_id: Option<String>,
    pub show_name: Option<String>,
    pub air_date: Option<String>,
    pub title: Option<String>,
    pub has_audio: bool,
}

#[tauri::command]
pub fn list_downloads(state: State<'_, AppState>) -> CmdResult<Vec<DownloadRow>> {
    let db = state.db()?;
    let mut stmt = db
        .conn
        .prepare(
            "SELECT d.episode_id, d.path, d.bytes, d.total, d.status,
                    e.show_id, s.name, e.air_date, e.title, COALESCE(e.has_audio, 0)
             FROM downloads d
             LEFT JOIN episodes e ON e.id = d.episode_id
             LEFT JOIN shows s ON s.id = e.show_id
             ORDER BY d.episode_id DESC",
        )
        .map_err(db_err)?;
    let rows: Vec<DownloadRow> = stmt
        .query_map([], |r| {
            Ok(DownloadRow {
                download: Download {
                    episode_id: r.get(0)?,
                    path: r.get(1)?,
                    bytes: r.get(2)?,
                    total: r.get(3)?,
                    status: r.get(4)?,
                },
                show_id: r.get(5)?,
                show_name: r.get(6)?,
                air_date: r.get(7)?,
                title: r.get(8)?,
                has_audio: r.get::<_, i64>(9)? != 0,
            })
        })
        .map_err(db_err)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

#[tauri::command]
pub fn delete_download(episode_id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    // Delete the file first; only drop the bookkeeping row once the file is actually gone.
    // Otherwise a locked/inaccessible file becomes an unmanaged orphan.
    let path = state.db()?.download_path(episode_id).map_err(db_err)?;
    if let Some(p) = path {
        match std::fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {} // already gone
            Err(e) => return Err(format!("could not delete file: {e}")),
        }
    }
    state.db()?.remove_download(episode_id).map_err(db_err)?;
    Ok(())
}

fn ts_to_iso(ts: i64) -> String {
    // Days-based civil-from-epoch conversion (UTC), avoids a chrono dependency.
    let days = ts.div_euclid(86400);
    let secs = ts.rem_euclid(86400);
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02}")
}

fn neutralize_csv_cell(value: &str) -> Cow<'_, str> {
    let formula = value
        .as_bytes()
        .first()
        .is_some_and(|first| matches!(*first, b'=' | b'+' | b'-' | b'@' | b'\t' | b'\r' | b'\n'));
    if formula {
        Cow::Owned(format!("'{value}"))
    } else {
        Cow::Borrowed(value)
    }
}

fn write_csv_record<W: Write>(
    writer: &mut csv::Writer<W>,
    record: &[&str],
) -> Result<(), csv::Error> {
    let cells: Vec<_> = record
        .iter()
        .map(|value| neutralize_csv_cell(value))
        .collect();
    writer.write_record(cells.iter().map(|cell| cell.as_bytes()))
}

fn create_csv_temp(dest: &Path) -> CmdResult<(PathBuf, std::fs::File)> {
    let parent = dest
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let filename = dest
        .file_name()
        .ok_or("CSV destination must name a file")?
        .to_string_lossy();
    for attempt in 0..100 {
        let path = parent.join(format!(
            ".{filename}.{}.{}.tmp",
            std::process::id(),
            attempt
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("CSV temp file failed: {error}")),
        }
    }
    Err("could not reserve a CSV temp file".into())
}

#[tauri::command]
pub fn export_csv(kind: String, dest: String, state: State<'_, AppState>) -> CmdResult<String> {
    // Reject an invalid kind before opening anything: the previous implementation truncated
    // the user's chosen destination and only then discovered an unsupported export kind.
    if !matches!(kind.as_str(), "favourites" | "listens" | "stats") {
        return Err(format!("unknown export kind: {kind}"));
    }

    let dest_path = PathBuf::from(&dest);
    let (temp_path, temp_file) = create_csv_temp(&dest_path)?;
    let result = (|| -> CmdResult<usize> {
        let db = state.db()?;
        let mut writer = csv::Writer::from_writer(temp_file);
        let mut rows = 0usize;
        match kind.as_str() {
            "favourites" => {
                write_csv_record(
                    &mut writer,
                    &["kind", "name", "detail", "show", "date_added"],
                )
                .map_err(|e| e.to_string())?;
                let mut stmt = db
                    .conn
                    .prepare(
                        "SELECT f.kind, f.ref_id, f.added_at FROM favourites f ORDER BY f.added_at",
                    )
                    .map_err(db_err)?;
                let favs: Vec<(String, String, i64)> = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                    .map_err(db_err)?
                    .filter_map(|r| r.ok())
                    .collect();
                for (kind, ref_id, added_at) in favs {
                    let (name, detail, show) = match kind.as_str() {
                        "show" => {
                            let r: Result<(String, Option<String>), _> = db.conn.query_row(
                                "SELECT name, dj FROM shows WHERE id=?1",
                                [&ref_id],
                                |r| Ok((r.get(0)?, r.get(1)?)),
                            );
                            match r {
                                Ok((n, dj)) => (n, dj.unwrap_or_default(), String::new()),
                                Err(_) => (ref_id.clone(), String::new(), String::new()),
                            }
                        }
                        "episode" => {
                            let r: Result<(Option<String>, Option<String>, String), _> =
                                db.conn.query_row(
                                    "SELECT e.title, e.air_date, s.name FROM episodes e JOIN shows s ON s.id=e.show_id WHERE e.id=?1",
                                    [&ref_id],
                                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                                );
                            match r {
                                Ok((t, d, s)) => (t.unwrap_or_default(), d.unwrap_or_default(), s),
                                Err(_) => (ref_id.clone(), String::new(), String::new()),
                            }
                        }
                        _ => {
                            let r: Result<(Option<String>, Option<String>, String), _> =
                                db.conn.query_row(
                                    "SELECT t.artist, t.title, s.name FROM tracks t JOIN episodes e ON e.id=t.episode_id JOIN shows s ON s.id=e.show_id WHERE t.id=?1",
                                    [&ref_id],
                                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                                );
                            match r {
                                Ok((a, t, s)) => (t.unwrap_or_default(), a.unwrap_or_default(), s),
                                Err(_) => (ref_id.clone(), String::new(), String::new()),
                            }
                        }
                    };
                    let added_at = ts_to_iso(added_at);
                    write_csv_record(&mut writer, &[&kind, &name, &detail, &show, &added_at])
                        .map_err(|e| e.to_string())?;
                    rows += 1;
                }
            }
            "listens" => {
                write_csv_record(
                    &mut writer,
                    &[
                        "session",
                        "show",
                        "episode_date",
                        "episode_title",
                        "started_at",
                        "seconds",
                        "completed",
                    ],
                )
                .map_err(|e| e.to_string())?;
                let mut stmt = db
                    .conn
                    .prepare(
                        "SELECT l.id, s.name, e.air_date, e.title, l.started_at, l.seconds, l.completed
                         FROM listens l JOIN episodes e ON e.id=l.episode_id JOIN shows s ON s.id=e.show_id
                         ORDER BY l.started_at",
                    )
                    .map_err(db_err)?;
                let items: Vec<ListenExportRow> = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                        ))
                    })
                    .map_err(db_err)?
                    .filter_map(|r| r.ok())
                    .collect();
                for (id, show, date, title, started, secs, done) in items {
                    let date = date.unwrap_or_default();
                    let title = title.unwrap_or_default();
                    let started = ts_to_iso(started);
                    let secs = secs.to_string();
                    let done = (done != 0).to_string();
                    write_csv_record(
                        &mut writer,
                        &[&id, &show, &date, &title, &started, &secs, &done],
                    )
                    .map_err(|e| e.to_string())?;
                    rows += 1;
                }
            }
            "stats" => {
                write_csv_record(
                    &mut writer,
                    &["rank", "show", "seconds_listened", "hours", "plays"],
                )
                .map_err(|e| e.to_string())?;
                let mut stmt = db
                    .conn
                    .prepare(
                        "SELECT s.name, SUM(l.seconds), COUNT(l.id)
                         FROM listens l JOIN episodes e ON e.id=l.episode_id JOIN shows s ON s.id=e.show_id
                         GROUP BY s.id ORDER BY SUM(l.seconds) DESC",
                    )
                    .map_err(db_err)?;
                let items: Vec<(String, i64, i64)> = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                    .map_err(db_err)?
                    .filter_map(|r| r.ok())
                    .collect();
                for (i, (show, secs, plays)) in items.iter().enumerate() {
                    let rank = (i + 1).to_string();
                    let hours = format!("{:.2}", *secs as f64 / 3600.0);
                    let secs = secs.to_string();
                    let plays = plays.to_string();
                    write_csv_record(&mut writer, &[&rank, show, &secs, &hours, &plays])
                        .map_err(|e| e.to_string())?;
                    rows += 1;
                }
            }
            _ => unreachable!("export kind was validated"),
        }
        writer.flush().map_err(|e| e.to_string())?;
        writer
            .get_ref()
            .sync_all()
            .map_err(|e| format!("CSV sync failed: {e}"))?;
        drop(writer);
        drop(db);
        std::fs::rename(&temp_path, &dest_path).map_err(|e| format!("CSV finalize failed: {e}"))?;
        Ok(rows)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    let rows = result?;
    Ok(format!("{rows} rows exported to {dest}"))
}

#[cfg(test)]
mod tests {
    use super::{cache_is_stale, neutralize_csv_cell, write_csv_record};

    #[test]
    fn cache_freshness_handles_boundaries_and_bad_timestamps() {
        let now = 10_000;
        let max_age = 300;
        assert!(cache_is_stale(None, now, max_age));
        assert!(!cache_is_stale(Some(now - max_age + 1), now, max_age));
        assert!(cache_is_stale(Some(now - max_age), now, max_age));
        assert!(cache_is_stale(Some(now + 1), now, max_age));
    }

    #[test]
    fn csv_cells_that_can_start_formulas_are_neutralized() {
        for value in [
            "=1+1",
            "+cmd",
            "-2",
            "@SUM(A1:A2)",
            "\t=1+1",
            "\r=1+1",
            "\n=1+1",
        ] {
            assert_eq!(neutralize_csv_cell(value), format!("'{value}"));
        }
        assert_eq!(neutralize_csv_cell("plain text"), "plain text");

        let mut writer = csv::Writer::from_writer(Vec::new());
        write_csv_record(
            &mut writer,
            &["=1+1", "+cmd", "-2", "@SUM(A1:A2)", "plain text"],
        )
        .expect("write safe row");
        let data = String::from_utf8(writer.into_inner().expect("flush row")).expect("UTF-8 CSV");
        assert_eq!(data, "'=1+1,'+cmd,'-2,'@SUM(A1:A2),plain text\n");
    }
}

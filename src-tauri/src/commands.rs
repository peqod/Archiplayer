use crate::db::{Download, Episode, Show, Track, TrackSyncMode};
use crate::wfmu;
use crate::AppState;
use rusqlite::params;
use serde::Serialize;
use std::collections::BTreeMap;
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

type CmdResult<T> = Result<T, String>;

fn db_err(e: rusqlite::Error) -> String {
    format!("db error: {e}")
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
    let need_scrape = {
        let db = state.db()?;
        refresh || db.show_count().map_err(db_err)? == 0
    };
    if need_scrape {
        let html = state.fetcher.get_text(&wfmu::catalog_url()).await?;
        let shows = wfmu::parse_catalog(&html);
        if shows.is_empty() {
            return Err("catalog parse produced no shows (site layout changed?)".into());
        }
        let db = state.db()?;
        for s in &shows {
            db.upsert_show(&s.id, &s.name, s.dj.as_deref(), s.on_air)
                .map_err(db_err)?;
        }
    }
    state.db()?.list_shows().map_err(db_err)
}

#[tauri::command]
pub async fn get_show(
    show_id: String,
    refresh: bool,
    state: State<'_, AppState>,
) -> CmdResult<ShowDetail> {
    let need_scrape = {
        let db = state.db()?;
        refresh || !db.show_was_scraped(&show_id).map_err(db_err)?
    };
    if need_scrape {
        let html = state.fetcher.get_text(&wfmu::show_url(&show_id)).await?;
        let episodes = wfmu::parse_show_page(&html);
        let description = wfmu::parse_show_description(&html);
        let archive_years = wfmu::parse_show_archive_years(&html, &show_id);

        // Upsert episodes from the main page, then chase each archive year.
        let mut seq: i64 = 0;
        {
            let db = state.db()?;
            for e in &episodes {
                db.upsert_episode(
                    e.id,
                    &show_id,
                    e.air_date.as_deref(),
                    e.title.as_deref(),
                    e.archive_id,
                    seq,
                )
                .map_err(db_err)?;
                seq += 1;
            }
        }
        for year_path in &archive_years {
            let year_url = format!("{}{}", wfmu::BASE, year_path);
            let year_html = state.fetcher.get_text(&year_url).await?;
            let year_eps = wfmu::parse_show_page(&year_html);
            let db = state.db()?;
            for e in &year_eps {
                db.upsert_episode(
                    e.id,
                    &show_id,
                    e.air_date.as_deref(),
                    e.title.as_deref(),
                    e.archive_id,
                    seq,
                )
                .map_err(db_err)?;
                seq += 1;
            }
        }
        let db = state.db()?;
        if let Some(desc) = description {
            db.set_show_description(&show_id, &desc).map_err(db_err)?;
        }
        db.mark_show_scraped(&show_id).map_err(db_err)?;
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
    db.upsert_episode(
        episode_id,
        &show_id,
        air_date.as_deref(),
        episode_title,
        None,
        0,
    )
    .map_err(db_err)?;
    let playlist_needs_load = hosted && !db.episode_tracks_scraped(episode_id).map_err(db_err)?;
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
    if let Some(url) = ep.audio_url.clone() {
        // Backfill the offset for episodes resolved before it was tracked.
        let offset_sec = ensure_offset(&state, &ep).await;
        return Ok(AudioSource {
            url,
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
         WHERE tracks_fts MATCH ?1 LIMIT 200"
    } else {
        "SELECT t.id, t.episode_id, t.seq, t.artist, t.title, t.album, t.label, t.comments, t.start_sec,
                t.source_id, t.played_at,
                EXISTS(SELECT 1 FROM favourites f WHERE f.kind='track' AND f.ref_id=CAST(t.id AS TEXT)),
                e.show_id, s.name, e.air_date
         FROM tracks t
         JOIN episodes e ON e.id = t.episode_id
         JOIN shows s ON s.id = e.show_id
         WHERE t.artist LIKE ?1 OR t.title LIKE ?1 OR t.album LIKE ?1 LIMIT 200"
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
pub fn set_download_dir(dir: String, state: State<'_, AppState>) -> CmdResult<()> {
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
    let path = state.db()?.remove_download(episode_id).map_err(db_err)?;
    if let Some(p) = path {
        let _ = std::fs::remove_file(p);
    }
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

#[tauri::command]
pub fn export_csv(kind: String, dest: String, state: State<'_, AppState>) -> CmdResult<String> {
    let db = state.db()?;
    let mut wtr = csv::Writer::from_path(&dest).map_err(|e| format!("csv open failed: {e}"))?;
    let mut rows = 0usize;
    match kind.as_str() {
        "favourites" => {
            wtr.write_record(["kind", "name", "detail", "show", "date_added"])
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
                        let r: Result<(Option<String>, Option<String>, String), _> = db.conn.query_row(
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
                        let r: Result<(Option<String>, Option<String>, String), _> = db.conn.query_row(
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
                wtr.write_record([&kind, &name, &detail, &show, &ts_to_iso(added_at)])
                    .map_err(|e| e.to_string())?;
                rows += 1;
            }
        }
        "listens" => {
            wtr.write_record([
                "session",
                "show",
                "episode_date",
                "episode_title",
                "started_at",
                "seconds",
                "completed",
            ])
            .map_err(|e| e.to_string())?;
            let mut stmt = db
                .conn
                .prepare(
                    "SELECT l.id, s.name, e.air_date, e.title, l.started_at, l.seconds, l.completed
                 FROM listens l JOIN episodes e ON e.id=l.episode_id JOIN shows s ON s.id=e.show_id
                 ORDER BY l.started_at",
                )
                .map_err(db_err)?;
            let items: Vec<(
                String,
                String,
                Option<String>,
                Option<String>,
                i64,
                i64,
                i64,
            )> = stmt
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
                wtr.write_record([
                    &id,
                    &show,
                    &date.unwrap_or_default(),
                    &title.unwrap_or_default(),
                    &ts_to_iso(started),
                    &secs.to_string(),
                    &(done != 0).to_string(),
                ])
                .map_err(|e| e.to_string())?;
                rows += 1;
            }
        }
        "stats" => {
            wtr.write_record(["rank", "show", "seconds_listened", "hours", "plays"])
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
                wtr.write_record([
                    &(i + 1).to_string(),
                    show,
                    &secs.to_string(),
                    &format!("{:.2}", *secs as f64 / 3600.0),
                    &plays.to_string(),
                ])
                .map_err(|e| e.to_string())?;
                rows += 1;
            }
        }
        other => return Err(format!("unknown export kind: {other}")),
    }
    wtr.flush().map_err(|e| e.to_string())?;
    Ok(format!("{rows} rows exported to {dest}"))
}

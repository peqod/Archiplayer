use crate::db::{Download, Episode, Show, Track};
use crate::wfmu;
use crate::AppState;
use rusqlite::params;
use serde::Serialize;
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

#[tauri::command]
pub async fn get_catalog(refresh: bool, state: State<'_, AppState>) -> CmdResult<Vec<Show>> {
    let need_scrape = {
        let db = state.db.lock().unwrap();
        refresh || db.show_count().map_err(db_err)? == 0
    };
    if need_scrape {
        let html = state.fetcher.get_text(&wfmu::catalog_url()).await?;
        let shows = wfmu::parse_catalog(&html);
        if shows.is_empty() {
            return Err("catalog parse produced no shows (site layout changed?)".into());
        }
        let db = state.db.lock().unwrap();
        for s in &shows {
            db.upsert_show(&s.id, &s.name, s.dj.as_deref(), s.on_air)
                .map_err(db_err)?;
        }
    }
    state.db.lock().unwrap().list_shows().map_err(db_err)
}

#[tauri::command]
pub async fn get_show(
    show_id: String,
    refresh: bool,
    state: State<'_, AppState>,
) -> CmdResult<ShowDetail> {
    let need_scrape = {
        let db = state.db.lock().unwrap();
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
            let db = state.db.lock().unwrap();
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
            let db = state.db.lock().unwrap();
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
        let db = state.db.lock().unwrap();
        if let Some(desc) = description {
            db.set_show_description(&show_id, &desc).map_err(db_err)?;
        }
        db.mark_show_scraped(&show_id).map_err(db_err)?;
    }
    let db = state.db.lock().unwrap();
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
        let db = state.db.lock().unwrap();
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
        let mut db = state.db.lock().unwrap();
        db.replace_tracks(episode_id, &tracks).map_err(db_err)?;
        if let Some(archive) = discovered {
            if db.get_episode(episode_id).map(|e| e.archive_id.is_none()).unwrap_or(false) {
                db.set_episode_archive(episode_id, archive).map_err(db_err)?;
            }
        }
    }
    state.db.lock().unwrap().list_tracks(episode_id).map_err(db_err)
}

#[tauri::command]
pub async fn resolve_audio(episode_id: i64, state: State<'_, AppState>) -> CmdResult<AudioSource> {
    let ep = {
        let db = state.db.lock().unwrap();
        db.get_episode(episode_id).map_err(db_err)?
    };
    if ep.downloaded {
        if let Some(path) = ep.download_path {
            if std::path::Path::new(&path).exists() {
                return Ok(AudioSource { url: path, local: true });
            }
        }
    }
    if let Some(url) = ep.audio_url {
        return Ok(AudioSource { url, local: false });
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
    let player_html = state
        .fetcher
        .get_text(&wfmu::archiveplayer_url(episode_id, archive_id))
        .await?;
    let url = match wfmu::parse_archiveplayer(&player_html) {
        Some(u) => u,
        None => {
            // Legacy fallback: some episodes still resolve via listen.m3u.
            let body = state
                .fetcher
                .get_text(&wfmu::m3u_url(episode_id, archive_id))
                .await?;
            wfmu::parse_m3u(&body)
                .ok_or("could not resolve an audio URL for this episode")?
        }
    };
    state
        .db
        .lock()
        .unwrap()
        .set_audio_url(episode_id, &url)
        .map_err(db_err)?;
    Ok(AudioSource { url, local: false })
}

#[tauri::command]
pub fn toggle_favourite(kind: String, ref_id: String, state: State<'_, AppState>) -> CmdResult<bool> {
    if !matches!(kind.as_str(), "show" | "episode" | "track") {
        return Err(format!("bad favourite kind: {kind}"));
    }
    state
        .db
        .lock()
        .unwrap()
        .toggle_favourite(&kind, &ref_id)
        .map_err(db_err)
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
    let db = state.db.lock().unwrap();
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
                    shows.push(FavouriteShow { show: show.clone(), added_at });
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
                        episodes.push(FavouriteEpisode { episode, show_name, added_at });
                    }
                }
            }
            "track" => {
                if let Ok(id) = ref_id.parse::<i64>() {
                    let row = db.conn.query_row(
                        "SELECT t.id, t.episode_id, t.seq, t.artist, t.title, t.album, t.label,
                                t.comments, t.start_sec, e.show_id, e.air_date
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
                                    favourite: true,
                                },
                                r.get::<_, String>(9)?,
                                r.get::<_, Option<String>>(10)?,
                            ))
                        },
                    );
                    if let Ok((track, show_id, air_date)) = row {
                        let show_name = all_shows
                            .iter()
                            .find(|s| s.id == show_id)
                            .map(|s| s.name.clone())
                            .unwrap_or_default();
                        tracks.push(FavouriteTrack { track, show_id, show_name, air_date, added_at });
                    }
                }
            }
            _ => {}
        }
    }
    Ok(Favourites { shows, episodes, tracks })
}

#[tauri::command]
pub fn search(query: String, state: State<'_, AppState>) -> CmdResult<SearchResults> {
    let q = query.trim().to_string();
    if q.is_empty() {
        return Ok(SearchResults { shows: vec![], tracks: vec![] });
    }
    let db = state.db.lock().unwrap();
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
                EXISTS(SELECT 1 FROM favourites f WHERE f.kind='track' AND f.ref_id=CAST(t.id AS TEXT)),
                e.show_id, s.name, e.air_date
         FROM tracks_fts ft
         JOIN tracks t ON t.id = CAST(ft.track_id AS INTEGER)
         JOIN episodes e ON e.id = t.episode_id
         JOIN shows s ON s.id = e.show_id
         WHERE tracks_fts MATCH ?1 LIMIT 200"
    } else {
        "SELECT t.id, t.episode_id, t.seq, t.artist, t.title, t.album, t.label, t.comments, t.start_sec,
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
                    favourite: r.get::<_, i64>(9)? != 0,
                },
                show_id: r.get(10)?,
                show_name: r.get(11)?,
                air_date: r.get(12)?,
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
        .record_listen(&session_id, episode_id, seconds, completed, position, duration)
        .map_err(db_err)
}

/// The folder new downloads are saved to: the user's chosen directory, or the default
/// under the app data dir.
#[tauri::command]
pub fn get_download_dir(app: AppHandle, state: State<'_, AppState>) -> CmdResult<String> {
    if let Some(d) = state.db.lock().unwrap().get_setting("download_dir").map_err(db_err)? {
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
        .db
        .lock()
        .unwrap()
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
    let db = state.db.lock().unwrap();
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

    Ok(Stats { total_seconds, total_sessions, shows, episodes })
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
    let db = state.db.lock().unwrap();
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
    let path = state
        .db
        .lock()
        .unwrap()
        .remove_download(episode_id)
        .map_err(db_err)?;
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
    let db = state.db.lock().unwrap();
    let mut wtr = csv::Writer::from_path(&dest).map_err(|e| format!("csv open failed: {e}"))?;
    let mut rows = 0usize;
    match kind.as_str() {
        "favourites" => {
            wtr.write_record(["kind", "name", "detail", "show", "date_added"])
                .map_err(|e| e.to_string())?;
            let mut stmt = db.conn.prepare(
                "SELECT f.kind, f.ref_id, f.added_at FROM favourites f ORDER BY f.added_at",
            ).map_err(db_err)?;
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
                            Ok((a, t, s)) => (
                                t.unwrap_or_default(),
                                a.unwrap_or_default(),
                                s,
                            ),
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
            wtr.write_record(["session", "show", "episode_date", "episode_title", "started_at", "seconds", "completed"])
                .map_err(|e| e.to_string())?;
            let mut stmt = db.conn.prepare(
                "SELECT l.id, s.name, e.air_date, e.title, l.started_at, l.seconds, l.completed
                 FROM listens l JOIN episodes e ON e.id=l.episode_id JOIN shows s ON s.id=e.show_id
                 ORDER BY l.started_at",
            ).map_err(db_err)?;
            let items: Vec<(String, String, Option<String>, Option<String>, i64, i64, i64)> = stmt
                .query_map([], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))
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
            let mut stmt = db.conn.prepare(
                "SELECT s.name, SUM(l.seconds), COUNT(l.id)
                 FROM listens l JOIN episodes e ON e.id=l.episode_id JOIN shows s ON s.id=e.show_id
                 GROUP BY s.id ORDER BY SUM(l.seconds) DESC",
            ).map_err(db_err)?;
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

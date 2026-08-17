use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct Show {
    pub id: String,
    pub name: String,
    pub dj: Option<String>,
    pub description: Option<String>,
    pub on_air: bool,
    pub episode_count: i64,
    pub favourite: bool,
    pub last_scraped: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Episode {
    pub id: i64,
    pub show_id: String,
    pub air_date: Option<String>,
    pub title: Option<String>,
    pub archive_id: Option<i64>,
    pub audio_url: Option<String>,
    pub has_audio: bool,
    pub favourite: bool,
    pub downloaded: bool,
    pub download_path: Option<String>,
    pub track_count: i64,
    pub resume_sec: Option<i64>,
    pub duration_sec: Option<i64>,
    pub completed: bool,
    /// Seconds of archive pre-roll (prior-show tail + station IDs + audition jingle)
    /// before the show's playlist timeline. Scraped from the AccuPlayer `data-offset`.
    /// Playlist `start_sec` values are show-relative; audio position = start_sec + offset_sec.
    pub offset_sec: Option<i64>,
    /// Scheduled length of the broadcast, excluding archive pre-roll and post-roll.
    pub broadcast_duration_sec: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Track {
    pub id: i64,
    pub episode_id: i64,
    pub seq: i64,
    pub artist: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub label: Option<String>,
    pub comments: Option<String>,
    pub start_sec: Option<i64>,
    pub source_id: Option<String>,
    pub played_at: Option<i64>,
    pub favourite: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Download {
    pub episode_id: i64,
    pub path: String,
    pub bytes: i64,
    pub total: i64,
    pub status: String,
}

pub struct Db {
    pub conn: Connection,
    pub fts: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackSyncMode {
    Snapshot,
    AppendObservations,
}

fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[..4].iter().all(|c| c.is_ascii_digit())
        && b[4] == b'-'
        && b[5..7].iter().all(|c| c.is_ascii_digit())
        && b[7] == b'-'
        && b[8..].iter().all(|c| c.is_ascii_digit())
}

/// "July 9, 2026" — the canonical scraped form, see `wfmu::mdy_from_dotted` — to
/// "2026-07-09". Already-ISO input passes through, which is what the synthetic live
/// episodes carry. Anything else returns None rather than guessing: a date we cannot read
/// is better left out of the archive timeline than filed under the wrong year.
pub fn air_date_to_iso(raw: &str) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let raw = raw.trim();
    if is_iso_date(raw) {
        return Some(raw.to_string());
    }
    let (month, rest) = raw.split_once(' ')?;
    let (day, year) = rest.split_once(',')?;
    let m = MONTHS
        .iter()
        .position(|name| name.eq_ignore_ascii_case(month))?
        + 1;
    let d: u32 = day.trim().parse().ok()?;
    let y: i32 = year.trim().parse().ok()?;
    // WFMU's archive starts in the 1990s; anything outside this is a parse accident.
    if !(1..=31).contains(&d) || !(1900..=2100).contains(&y) {
        return None;
    }
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS shows (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    dj TEXT,
    description TEXT,
    on_air INTEGER NOT NULL DEFAULT 0,
    last_scraped INTEGER
);
CREATE TABLE IF NOT EXISTS episodes (
    id INTEGER PRIMARY KEY,
    show_id TEXT NOT NULL REFERENCES shows(id),
    air_date TEXT,
    title TEXT,
    archive_id INTEGER,
    audio_url TEXT,
    has_audio INTEGER NOT NULL DEFAULT 0,
    seq INTEGER NOT NULL DEFAULT 0,
    last_scraped INTEGER
);
CREATE INDEX IF NOT EXISTS idx_episodes_show ON episodes(show_id);
CREATE TABLE IF NOT EXISTS tracks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    episode_id INTEGER NOT NULL REFERENCES episodes(id),
    seq INTEGER NOT NULL,
    artist TEXT,
    title TEXT,
    album TEXT,
    label TEXT,
    comments TEXT,
    start_sec INTEGER,
    source_id TEXT,
    played_at INTEGER
);
CREATE INDEX IF NOT EXISTS idx_tracks_episode ON tracks(episode_id);
CREATE TABLE IF NOT EXISTS favourites (
    kind TEXT NOT NULL CHECK (kind IN ('show','episode','track')),
    ref_id TEXT NOT NULL,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (kind, ref_id)
);
CREATE TABLE IF NOT EXISTS listens (
    id TEXT PRIMARY KEY,
    episode_id INTEGER NOT NULL,
    started_at INTEGER NOT NULL,
    seconds INTEGER NOT NULL DEFAULT 0,
    completed INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_listens_episode ON listens(episode_id);
CREATE TABLE IF NOT EXISTS downloads (
    episode_id INTEGER PRIMARY KEY,
    path TEXT NOT NULL,
    bytes INTEGER NOT NULL DEFAULT 0,
    total INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'pending'
);
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

const FTS_SCHEMA: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS tracks_fts USING fts5(
    artist, title, album,
    track_id UNINDEXED, episode_id UNINDEXED
);
"#;

impl Db {
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let mut conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(SCHEMA)?;
        Self::migrate(&mut conn)?;
        // A half-populated index would silently drop results, so a failed backfill falls
        // back to the LIKE search path rather than failing the whole open.
        let fts = conn.execute_batch(FTS_SCHEMA).is_ok() && Self::backfill_fts(&mut conn).is_ok();
        Ok(Db { conn, fts })
    }

    /// Rows only enter `tracks_fts` while syncing a playlist, so a database written before
    /// FTS existed, or one whose index creation failed once and later succeeded, keeps its
    /// tracks permanently unsearchable. Rebuild whenever the index and the table disagree.
    /// The index can also run *ahead* of the table: `migrate` deletes track rows before the
    /// FTS table is even attached, so its leftovers are cleared here.
    fn backfill_fts(conn: &mut Connection) -> Result<(), rusqlite::Error> {
        let indexed: i64 =
            conn.query_row("SELECT COUNT(*) FROM tracks_fts", [], |row| row.get(0))?;
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0))?;
        if indexed == total {
            return Ok(());
        }
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM tracks_fts", [])?;
        // Mirrors the sync inserts: ids are stored as text, artist/title/album keep their NULLs.
        tx.execute(
            "INSERT INTO tracks_fts (artist, title, album, track_id, episode_id)
             SELECT artist, title, album, CAST(id AS TEXT), CAST(episode_id AS TEXT) FROM tracks",
            [],
        )?;
        tx.commit()
    }

    fn migrate(conn: &mut Connection) -> Result<(), rusqlite::Error> {
        let tx = conn.transaction()?;
        let migrations = [
            (
                "shows",
                "description",
                "ALTER TABLE shows ADD COLUMN description TEXT",
            ),
            (
                "shows",
                "is_live",
                "ALTER TABLE shows ADD COLUMN is_live INTEGER NOT NULL DEFAULT 0",
            ),
            (
                "episodes",
                "resume_sec",
                "ALTER TABLE episodes ADD COLUMN resume_sec INTEGER",
            ),
            (
                "episodes",
                "duration_sec",
                "ALTER TABLE episodes ADD COLUMN duration_sec INTEGER",
            ),
            (
                "episodes",
                "completed",
                "ALTER TABLE episodes ADD COLUMN completed INTEGER NOT NULL DEFAULT 0",
            ),
            (
                "episodes",
                "offset_sec",
                "ALTER TABLE episodes ADD COLUMN offset_sec INTEGER",
            ),
            (
                "episodes",
                "broadcast_duration_sec",
                "ALTER TABLE episodes ADD COLUMN broadcast_duration_sec INTEGER",
            ),
            (
                "episodes",
                "playlist_timing_checked",
                "ALTER TABLE episodes ADD COLUMN playlist_timing_checked INTEGER NOT NULL DEFAULT 0",
            ),
            (
                "tracks",
                "source_id",
                "ALTER TABLE tracks ADD COLUMN source_id TEXT",
            ),
            (
                "tracks",
                "played_at",
                "ALTER TABLE tracks ADD COLUMN played_at INTEGER",
            ),
            // Sortable twin of the scraped `air_date` text, so the archive era a listen
            // belongs to can be grouped in SQL instead of parsed month names.
            (
                "episodes",
                "air_date_iso",
                "ALTER TABLE episodes ADD COLUMN air_date_iso TEXT",
            ),
            // Which scraper generation produced the cached rows. Everything already on
            // disk starts at 0, so a build with better parsers re-reads it once. See
            // `commands::SCRAPE_VERSION`.
            (
                "shows",
                "scrape_version",
                "ALTER TABLE shows ADD COLUMN scrape_version INTEGER NOT NULL DEFAULT 0",
            ),
            (
                "episodes",
                "tracks_version",
                "ALTER TABLE episodes ADD COLUMN tracks_version INTEGER NOT NULL DEFAULT 0",
            ),
        ];
        for (table, column, sql) in migrations {
            let exists = {
                let mut stmt = tx.prepare(&format!("PRAGMA table_info({table})"))?;
                let mut columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
                columns.any(|name| name.is_ok_and(|name| name == column))
            };
            if !exists {
                tx.execute(sql, [])?;
            }
        }
        tx.execute_batch(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_tracks_episode_source
             ON tracks(episode_id, source_id) WHERE source_id IS NOT NULL;
             CREATE INDEX IF NOT EXISTS idx_episodes_air_iso ON episodes(air_date_iso);",
        )?;
        // Retroactively flag synthetic live rows created before the is_live column existed,
        // so they stop appearing in the catalog. Real show ids never start with "live-".
        tx.execute_batch("UPDATE shows SET is_live = 1 WHERE id LIKE 'live-%';")?;
        let version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version < 4 {
            Self::purge_implausible_tracks(&tx)?;
        }
        if version < 6 {
            Self::backfill_air_date_iso(&tx)?;
        }
        tx.pragma_update(None, "user_version", 6)?;
        tx.commit()
    }

    /// Fill `air_date_iso` for episodes scraped before the column existed. Runs inside
    /// `migrate`'s transaction, so a failure rolls the whole migration back instead of
    /// leaving half the archive dated.
    fn backfill_air_date_iso(tx: &rusqlite::Transaction) -> Result<(), rusqlite::Error> {
        let rows: Vec<(i64, String)> = {
            let mut stmt = tx.prepare(
                "SELECT id, air_date FROM episodes
                 WHERE air_date IS NOT NULL AND air_date_iso IS NULL",
            )?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?;
            rows
        };
        let mut update = tx.prepare("UPDATE episodes SET air_date_iso=?2 WHERE id=?1")?;
        for (id, raw) in rows {
            if let Some(iso) = air_date_to_iso(&raw) {
                update.execute(params![id, iso])?;
            }
        }
        Ok(())
    }

    /// One-off repair for playlists cached before the parsers learned to reject page chrome.
    /// Those rows hold a slab of the show page — its head scripts, stylesheet and login
    /// widget — in the artist and title columns, and nothing would ever replace them:
    /// `get_playlist` re-scrapes only when `episodes.last_scraped` is NULL. Drop the bad rows
    /// and clear that stamp so the fixed parser gets a turn the next time the episode opens.
    fn purge_implausible_tracks(tx: &rusqlite::Transaction) -> Result<(), rusqlite::Error> {
        let doomed: Vec<(i64, i64)> = {
            let mut stmt =
                tx.prepare("SELECT id, episode_id, artist, title, album, label FROM tracks")?;
            let rows = stmt.query_map([], |row| {
                let track = crate::wfmu::ParsedTrack {
                    artist: row.get(2)?,
                    title: row.get(3)?,
                    album: row.get(4)?,
                    label: row.get(5)?,
                    comments: None,
                    start_sec: None,
                };
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, track))
            })?;
            rows.filter_map(|row| match row {
                Ok((id, episode_id, track)) if !crate::wfmu::plausible_track(&track) => {
                    Some(Ok((id, episode_id)))
                }
                Ok(_) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<_, _>>()?
        };
        if doomed.is_empty() {
            return Ok(());
        }
        for (id, _) in &doomed {
            tx.execute("DELETE FROM tracks WHERE id=?1", params![id])?;
            // The track's favourite would otherwise outlive it as an unreachable row.
            tx.execute(
                "DELETE FROM favourites WHERE kind='track' AND ref_id=?1",
                params![id.to_string()],
            )?;
        }
        // tracks_fts is attached after this runs, so backfill_fts clears its leftovers.
        let mut episodes: Vec<i64> = doomed.into_iter().map(|(_, episode)| episode).collect();
        episodes.sort_unstable();
        episodes.dedup();
        for episode_id in episodes {
            tx.execute(
                "UPDATE episodes SET last_scraped=NULL WHERE id=?1",
                params![episode_id],
            )?;
        }
        Ok(())
    }

    pub fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    pub fn upsert_show(
        &self,
        id: &str,
        name: &str,
        dj: Option<&str>,
        on_air: bool,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO shows (id, name, dj, on_air) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET name=?2, dj=COALESCE(?3, dj), on_air=?4",
            params![id, name, dj, on_air as i64],
        )?;
        Ok(())
    }

    pub fn list_shows(&self) -> Result<Vec<Show>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.name, s.dj, s.description, s.on_air, s.last_scraped,
                    (SELECT COUNT(*) FROM episodes e WHERE e.show_id = s.id),
                    EXISTS(SELECT 1 FROM favourites f WHERE f.kind='show' AND f.ref_id = s.id)
             FROM shows s WHERE s.is_live = 0 ORDER BY s.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Show {
                id: r.get(0)?,
                name: r.get(1)?,
                dj: r.get(2)?,
                description: r.get(3)?,
                on_air: r.get::<_, i64>(4)? != 0,
                last_scraped: r.get(5)?,
                episode_count: r.get(6)?,
                favourite: r.get::<_, i64>(7)? != 0,
            })
        })?;
        rows.collect()
    }

    pub fn set_show_description(&self, id: &str, desc: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE shows SET description=?2 WHERE id=?1",
            params![id, desc],
        )?;
        Ok(())
    }

    pub fn show_count(&self) -> Result<i64, rusqlite::Error> {
        self.conn
            .query_row("SELECT COUNT(*) FROM shows WHERE is_live = 0", [], |r| {
                r.get(0)
            })
    }

    /// Flag a synthetic live-station row so the ordinary catalog and search never surface it.
    pub fn set_show_live(&self, id: &str) -> Result<(), rusqlite::Error> {
        self.conn
            .execute("UPDATE shows SET is_live=1 WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn mark_show_scraped(&self, id: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE shows SET last_scraped=?2 WHERE id=?1",
            params![id, Self::now()],
        )?;
        Ok(())
    }

    pub fn show_last_scraped(&self, id: &str) -> Result<Option<i64>, rusqlite::Error> {
        self.conn.query_row(
            "SELECT last_scraped FROM shows WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
    }

    /// The scraper generation that produced this show's cached episodes, 0 for a show
    /// never stamped and for one this build has not re-read yet. An unknown show reads
    /// as 0 too, so a first visit is treated as stale rather than as up to date.
    pub fn show_scrape_version(&self, id: &str) -> Result<i64, rusqlite::Error> {
        self.conn
            .query_row(
                "SELECT scrape_version FROM shows WHERE id=?1",
                params![id],
                |r| r.get(0),
            )
            .optional()
            .map(|version| version.unwrap_or(0))
    }

    pub fn set_show_scrape_version(&self, id: &str, version: i64) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE shows SET scrape_version=?2 WHERE id=?1",
            params![id, version],
        )?;
        Ok(())
    }

    /// `seq` is `None` for callers with no ordering opinion, such as the live-status path
    /// that meets a hosted episode mid-broadcast. Those keep whatever ordering a show-page
    /// scrape already established instead of resetting it to the top of the list.
    pub fn upsert_episode(
        &self,
        id: i64,
        show_id: &str,
        air_date: Option<&str>,
        title: Option<&str>,
        archive_id: Option<i64>,
        seq: Option<i64>,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO episodes
                 (id, show_id, air_date, title, archive_id, has_audio, seq, air_date_iso)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, COALESCE(?7, 0), ?8)
             ON CONFLICT(id) DO UPDATE SET
               show_id=?2, air_date=COALESCE(?3, air_date), title=COALESCE(?4, title),
               archive_id=COALESCE(?5, archive_id),
               has_audio=CASE WHEN ?5 IS NULL THEN has_audio ELSE ?6 END,
               seq=COALESCE(?7, seq),
               air_date_iso=COALESCE(?8, air_date_iso)",
            params![
                id,
                show_id,
                air_date,
                title,
                archive_id,
                archive_id.is_some() as i64,
                seq,
                air_date.and_then(air_date_to_iso)
            ],
        )?;
        Ok(())
    }

    /// Merge a freshly parsed front page into a show's cached episode history.
    ///
    /// Parsed rows keep their upstream order at the front. Cached rows that are not on the
    /// current page are retained behind them, with unique sequence values, so a lightweight
    /// refresh does not need to re-fetch every archive-year page just to preserve ordering.
    pub fn sync_show_episodes(
        &mut self,
        show_id: &str,
        episodes: &[crate::wfmu::ParsedEpisode],
    ) -> Result<(), rusqlite::Error> {
        let tx = self.conn.transaction()?;
        let existing: Vec<i64> = {
            let mut statement =
                tx.prepare("SELECT id FROM episodes WHERE show_id=?1 ORDER BY seq, id")?;
            let rows = statement
                .query_map([show_id], |row| row.get(0))?
                .collect::<Result<_, _>>()?;
            rows
        };

        let mut order = Vec::with_capacity(episodes.len() + existing.len());
        let mut seen = HashSet::with_capacity(episodes.len() + existing.len());
        for episode in episodes {
            if seen.insert(episode.id) {
                order.push(episode.id);
            }
            tx.execute(
                "INSERT INTO episodes
                     (id, show_id, air_date, title, archive_id, has_audio, seq, air_date_iso)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)
                 ON CONFLICT(id) DO UPDATE SET
                   show_id=?2, air_date=COALESCE(?3, air_date), title=COALESCE(?4, title),
                   archive_id=COALESCE(?5, archive_id),
                   has_audio=CASE WHEN ?5 IS NULL THEN has_audio ELSE ?6 END,
                   air_date_iso=COALESCE(?7, air_date_iso)",
                params![
                    episode.id,
                    show_id,
                    episode.air_date,
                    episode.title,
                    episode.archive_id,
                    episode.archive_id.is_some() as i64,
                    episode.air_date.as_deref().and_then(air_date_to_iso),
                ],
            )?;
        }
        for id in existing {
            if seen.insert(id) {
                order.push(id);
            }
        }
        for (seq, id) in order.into_iter().enumerate() {
            tx.execute(
                "UPDATE episodes SET seq=?2 WHERE id=?1 AND show_id=?3",
                params![id, seq as i64, show_id],
            )?;
        }
        tx.commit()
    }

    /// A show's episodes, newest air date first.
    ///
    /// Order comes from `air_date_iso`, not from `seq`. WFMU's templates are not
    /// consistent: some per-year archive pages list their episodes oldest first, so
    /// scrape order (what `seq` records) runs backwards inside those year blocks and
    /// the UI's reverse toggle cannot straighten it out. Sorting on the parsed date
    /// fixes both directions at once, for already-cached rows as well as fresh ones.
    ///
    /// Episodes whose date could not be parsed keep their scrape order behind the
    /// dated ones — better a stable tail than a guessed date filed under the wrong
    /// year. They therefore lead the list in the oldest-first view.
    pub fn list_episodes(&self, show_id: &str) -> Result<Vec<Episode>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.show_id, e.air_date, e.title, e.archive_id, e.audio_url, e.has_audio,
                    EXISTS(SELECT 1 FROM favourites f WHERE f.kind='episode' AND f.ref_id = CAST(e.id AS TEXT)),
                    d.path, COALESCE(d.status,''),
                    (SELECT COUNT(*) FROM tracks t WHERE t.episode_id = e.id),
                    e.resume_sec, e.duration_sec, e.completed, e.offset_sec, e.broadcast_duration_sec
             FROM episodes e LEFT JOIN downloads d ON d.episode_id = e.id
             WHERE e.show_id = ?1
             ORDER BY (e.air_date_iso IS NULL), e.air_date_iso DESC, e.seq",
        )?;
        let rows = stmt.query_map([show_id], |r| {
            let status: String = r.get(9)?;
            Ok(Episode {
                id: r.get(0)?,
                show_id: r.get(1)?,
                air_date: r.get(2)?,
                title: r.get(3)?,
                archive_id: r.get(4)?,
                audio_url: r.get(5)?,
                has_audio: r.get::<_, i64>(6)? != 0,
                favourite: r.get::<_, i64>(7)? != 0,
                download_path: r.get(8)?,
                downloaded: status == "done",
                track_count: r.get(10)?,
                resume_sec: r.get(11)?,
                duration_sec: r.get(12)?,
                completed: r.get::<_, i64>(13)? != 0,
                offset_sec: r.get(14)?,
                broadcast_duration_sec: r.get(15)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_episode(&self, id: i64) -> Result<Episode, rusqlite::Error> {
        self.conn.query_row(
            "SELECT e.id, e.show_id, e.air_date, e.title, e.archive_id, e.audio_url, e.has_audio,
                    EXISTS(SELECT 1 FROM favourites f WHERE f.kind='episode' AND f.ref_id = CAST(e.id AS TEXT)),
                    d.path, COALESCE(d.status,''),
                    (SELECT COUNT(*) FROM tracks t WHERE t.episode_id = e.id),
                    e.resume_sec, e.duration_sec, e.completed, e.offset_sec, e.broadcast_duration_sec
             FROM episodes e LEFT JOIN downloads d ON d.episode_id = e.id
             WHERE e.id = ?1",
            [id],
            |r| {
                let status: String = r.get(9)?;
                Ok(Episode {
                    id: r.get(0)?,
                    show_id: r.get(1)?,
                    air_date: r.get(2)?,
                    title: r.get(3)?,
                    archive_id: r.get(4)?,
                    audio_url: r.get(5)?,
                    has_audio: r.get::<_, i64>(6)? != 0,
                    favourite: r.get::<_, i64>(7)? != 0,
                    download_path: r.get(8)?,
                    downloaded: status == "done",
                    track_count: r.get(10)?,
                    resume_sec: r.get(11)?,
                    duration_sec: r.get(12)?,
                    completed: r.get::<_, i64>(13)? != 0,
                    offset_sec: r.get(14)?,
                    broadcast_duration_sec: r.get(15)?,
                })
            },
        )
    }

    pub fn set_audio_url(&self, episode_id: i64, url: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE episodes SET audio_url=?2 WHERE id=?1",
            params![episode_id, url],
        )?;
        Ok(())
    }

    /// Forget a cached archive URL that no longer resolves, so the next play re-scrapes it.
    /// The pre-roll offset is left alone: the re-scrape overwrites it from the same page.
    pub fn clear_audio_url(&self, episode_id: i64) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE episodes SET audio_url=NULL WHERE id=?1",
            params![episode_id],
        )?;
        Ok(())
    }

    /// Store the archive pre-roll offset (seconds) scraped from the AccuPlayer page.
    pub fn set_episode_offset(
        &self,
        episode_id: i64,
        offset_sec: i64,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE episodes SET offset_sec=?2 WHERE id=?1",
            params![episode_id, offset_sec],
        )?;
        Ok(())
    }

    /// Record the result of inspecting the playlist schedule. A NULL duration is still
    /// marked checked so unsupported legacy pages do not trigger a request on every play.
    pub fn set_episode_broadcast_duration(
        &self,
        episode_id: i64,
        duration_sec: Option<i64>,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE episodes
             SET broadcast_duration_sec=?2, playlist_timing_checked=1
             WHERE id=?1",
            params![episode_id, duration_sec],
        )?;
        Ok(())
    }

    /// Record an archive id discovered after the initial show-page scrape
    /// (e.g. from the episode's own playlist page), marking the episode playable.
    pub fn set_episode_archive(
        &self,
        episode_id: i64,
        archive_id: i64,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE episodes SET archive_id=?2, has_audio=1 WHERE id=?1",
            params![episode_id, archive_id],
        )?;
        Ok(())
    }

    /// Synchronize playlist rows without deleting existing identities. Snapshots update
    /// rows by sequence and append new rows; observations only append when the last
    /// artist/title changed. Both paths preserve track ids and therefore favourites.
    pub fn sync_tracks(
        &mut self,
        episode_id: i64,
        tracks: &[crate::wfmu::ParsedTrack],
        mode: TrackSyncMode,
    ) -> Result<(), rusqlite::Error> {
        let fts = self.fts;
        let tx = self.conn.transaction()?;
        let mut synced = Vec::new();
        match mode {
            TrackSyncMode::Snapshot => {
                let existing: Vec<(i64, i64)> = {
                    let mut statement =
                        tx.prepare("SELECT seq, id FROM tracks WHERE episode_id=?1 ORDER BY seq")?;
                    let rows = statement
                        .query_map([episode_id], |row| Ok((row.get(0)?, row.get(1)?)))?
                        .collect::<Result<_, _>>()?;
                    rows
                };
                for (index, track) in tracks.iter().enumerate() {
                    let seq = index as i64;
                    let track_id = if let Some((_, id)) = existing.iter().find(|(s, _)| *s == seq) {
                        tx.execute(
                            "UPDATE tracks SET artist=?3, title=?4, album=?5, label=?6, comments=?7, start_sec=?8
                             WHERE episode_id=?1 AND seq=?2",
                            params![episode_id, seq, track.artist, track.title, track.album, track.label, track.comments, track.start_sec],
                        )?;
                        *id
                    } else {
                        tx.execute(
                            "INSERT INTO tracks (episode_id, seq, artist, title, album, label, comments, start_sec)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                            params![episode_id, seq, track.artist, track.title, track.album, track.label, track.comments, track.start_sec],
                        )?;
                        tx.last_insert_rowid()
                    };
                    synced.push((track_id, track));
                }
                // A snapshot is authoritative about its own length, so a playlist that
                // shrank must lose its tail — otherwise rows from an earlier, longer parse
                // survive forever. An *empty* snapshot is not treated as authoritative: a
                // fetch that failed or a template the parsers didn't recognize must not
                // destroy a good cached playlist, so the existing rows stay put.
                if !tracks.is_empty() {
                    let surplus: Vec<i64> = existing
                        .iter()
                        .filter(|(seq, _)| *seq >= tracks.len() as i64)
                        .map(|(_, id)| *id)
                        .collect();
                    for id in surplus {
                        tx.execute("DELETE FROM tracks WHERE id=?1", params![id])?;
                        if fts {
                            tx.execute(
                                "DELETE FROM tracks_fts WHERE track_id=?1",
                                params![id.to_string()],
                            )?;
                        }
                    }
                }
            }
            TrackSyncMode::AppendObservations => {
                let mut previous: Option<(i64, Option<String>, Option<String>)> = tx
                    .query_row(
                        "SELECT seq, artist, title FROM tracks WHERE episode_id=?1 ORDER BY seq DESC LIMIT 1",
                        [episode_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;
                for track in tracks {
                    let duplicate = previous
                        .as_ref()
                        .map(|(_, artist, title)| artist == &track.artist && title == &track.title)
                        .unwrap_or(false);
                    if duplicate {
                        continue;
                    }
                    let seq = previous.as_ref().map(|(seq, _, _)| seq + 1).unwrap_or(0);
                    tx.execute(
                        "INSERT INTO tracks (episode_id, seq, artist, title, album, label, comments, start_sec)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![episode_id, seq, track.artist, track.title, track.album, track.label, track.comments, track.start_sec],
                    )?;
                    synced.push((tx.last_insert_rowid(), track));
                    previous = Some((seq, track.artist.clone(), track.title.clone()));
                }
            }
        }
        if fts {
            for (track_id, track) in synced {
                tx.execute(
                    "DELETE FROM tracks_fts WHERE track_id=?1",
                    params![track_id.to_string()],
                )?;
                tx.execute(
                    "INSERT INTO tracks_fts (artist, title, album, track_id, episode_id)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        track.artist,
                        track.title,
                        track.album,
                        track_id.to_string(),
                        episode_id.to_string()
                    ],
                )?;
            }
        }
        tx.execute(
            "UPDATE episodes SET last_scraped=?2 WHERE id=?1",
            params![episode_id, Self::now()],
        )?;
        tx.commit()
    }

    pub fn list_tracks(&self, episode_id: i64) -> Result<Vec<Track>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.episode_id, t.seq, t.artist, t.title, t.album, t.label, t.comments, t.start_sec,
                    t.source_id, t.played_at,
                    EXISTS(SELECT 1 FROM favourites f WHERE f.kind='track' AND f.ref_id = CAST(t.id AS TEXT))
             FROM tracks t WHERE t.episode_id = ?1 ORDER BY t.seq",
        )?;
        let rows = stmt.query_map([episode_id], |r| {
            Ok(Track {
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
            })
        })?;
        rows.collect()
    }

    /// Merge provider history by its stable identity. A matching local observation
    /// is upgraded in place before inserting, retaining its row id and favourite.
    pub fn sync_provider_tracks(
        &mut self,
        episode_id: i64,
        provider: &str,
        tracks: &[crate::wfmu::ParsedRecentTrack],
    ) -> Result<(), rusqlite::Error> {
        let fts = self.fts;
        let tx = self.conn.transaction()?;
        let mut next_seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq) + 1, 0) FROM tracks WHERE episode_id=?1",
            [episode_id],
            |row| row.get(0),
        )?;
        for track in tracks {
            let source_id = format!("{provider}:{}", track.source_id);
            let existing: Option<i64> = tx
                .query_row(
                    "SELECT id FROM tracks WHERE episode_id=?1 AND source_id=?2",
                    params![episode_id, source_id],
                    |row| row.get(0),
                )
                .optional()?;
            let local_match = if existing.is_none() {
                tx.query_row(
                    "SELECT id FROM tracks
                     WHERE episode_id=?1 AND source_id IS NULL
                       AND artist IS ?2 AND title IS ?3
                     ORDER BY seq DESC LIMIT 1",
                    params![episode_id, track.artist, track.title],
                    |row| row.get(0),
                )
                .optional()?
            } else {
                None
            };
            let track_id = if let Some(id) = existing.or(local_match) {
                tx.execute(
                    "UPDATE tracks SET artist=?2, title=?3, album=?4,
                       source_id=?5, played_at=?6 WHERE id=?1",
                    params![
                        id,
                        track.artist,
                        track.title,
                        track.album,
                        source_id,
                        track.played_at
                    ],
                )?;
                id
            } else {
                tx.execute(
                    "INSERT INTO tracks
                       (episode_id, seq, artist, title, album, source_id, played_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        episode_id,
                        next_seq,
                        track.artist,
                        track.title,
                        track.album,
                        source_id,
                        track.played_at
                    ],
                )?;
                next_seq += 1;
                tx.last_insert_rowid()
            };
            if fts {
                tx.execute(
                    "DELETE FROM tracks_fts WHERE track_id=?1",
                    [track_id.to_string()],
                )?;
                tx.execute(
                    "INSERT INTO tracks_fts (artist, title, album, track_id, episode_id)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        track.artist,
                        track.title,
                        track.album,
                        track_id.to_string(),
                        episode_id.to_string()
                    ],
                )?;
            }
        }
        tx.execute(
            "UPDATE episodes SET last_scraped=?2 WHERE id=?1",
            params![episode_id, Self::now()],
        )?;
        tx.commit()
    }

    pub fn list_recent_live_tracks(
        &self,
        show_id: &str,
        limit: i64,
    ) -> Result<Vec<Track>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.episode_id, t.seq, t.artist, t.title, t.album, t.label,
                    t.comments, t.start_sec, t.source_id, t.played_at,
                    EXISTS(SELECT 1 FROM favourites f WHERE f.kind='track' AND f.ref_id=CAST(t.id AS TEXT))
             FROM tracks t JOIN episodes e ON e.id=t.episode_id
             WHERE e.show_id=?1
             ORDER BY COALESCE(t.played_at, e.last_scraped, 0) DESC, t.seq DESC LIMIT ?2",
        )?;
        let mut tracks = stmt
            .query_map(params![show_id, limit], |r| {
                Ok(Track {
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
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        tracks.reverse();
        Ok(tracks)
    }

    pub fn episode_tracks_scraped(&self, episode_id: i64) -> Result<bool, rusqlite::Error> {
        let scraped: Option<i64> = self.conn.query_row(
            "SELECT last_scraped FROM episodes WHERE id=?1",
            [episode_id],
            |r| r.get(0),
        )?;
        Ok(scraped.is_some())
    }

    /// The scraper generation that produced this episode's cached tracks. Same rule as
    /// `show_scrape_version`: unknown or never stamped reads as 0, i.e. stale.
    pub fn episode_tracks_version(&self, episode_id: i64) -> Result<i64, rusqlite::Error> {
        self.conn
            .query_row(
                "SELECT tracks_version FROM episodes WHERE id=?1",
                [episode_id],
                |r| r.get(0),
            )
            .optional()
            .map(|version| version.unwrap_or(0))
    }

    pub fn set_episode_tracks_version(
        &self,
        episode_id: i64,
        version: i64,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE episodes SET tracks_version=?2 WHERE id=?1",
            params![episode_id, version],
        )?;
        Ok(())
    }

    pub fn episode_playlist_timing_checked(
        &self,
        episode_id: i64,
    ) -> Result<bool, rusqlite::Error> {
        let checked: i64 = self.conn.query_row(
            "SELECT playlist_timing_checked FROM episodes WHERE id=?1",
            [episode_id],
            |r| r.get(0),
        )?;
        Ok(checked != 0)
    }

    pub fn episode_tracks_stale(
        &self,
        episode_id: i64,
        max_age_seconds: i64,
    ) -> Result<bool, rusqlite::Error> {
        let last_scraped: Option<i64> = self.conn.query_row(
            "SELECT last_scraped FROM episodes WHERE id=?1",
            [episode_id],
            |r| r.get(0),
        )?;
        Ok(last_scraped
            .map(|timestamp| Self::now().saturating_sub(timestamp) >= max_age_seconds)
            .unwrap_or(true))
    }

    pub fn toggle_favourite(&self, kind: &str, ref_id: &str) -> Result<bool, rusqlite::Error> {
        let removed = self.conn.execute(
            "DELETE FROM favourites WHERE kind=?1 AND ref_id=?2",
            params![kind, ref_id],
        )?;
        if removed > 0 {
            return Ok(false);
        }
        self.conn.execute(
            "INSERT INTO favourites (kind, ref_id, added_at) VALUES (?1, ?2, ?3)",
            params![kind, ref_id, Self::now()],
        )?;
        Ok(true)
    }

    pub fn record_listen(
        &self,
        session_id: &str,
        episode_id: i64,
        seconds: i64,
        completed: bool,
        position: i64,
        duration: i64,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO listens (id, episode_id, started_at, seconds, completed)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET seconds=MAX(seconds, ?4), completed=MAX(completed, ?5)",
            params![
                session_id,
                episode_id,
                Self::now(),
                seconds,
                completed as i64
            ],
        )?;
        // Remember where the listener left off (for the resume marker and progress bar).
        // Only advance duration when we actually know it; never clear a completed flag.
        self.conn.execute(
            "UPDATE episodes SET
               resume_sec = ?2,
               duration_sec = CASE WHEN ?3 > 0 THEN ?3 ELSE duration_sec END,
               completed = MAX(completed, ?4)
             WHERE id = ?1",
            params![episode_id, position, duration, completed as i64],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, rusqlite::Error> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        )?;
        Ok(())
    }

    /// Show name, air date and title for building a meaningful download filename.
    pub fn episode_filename_parts(
        &self,
        episode_id: i64,
    ) -> Result<(String, Option<String>, Option<String>), rusqlite::Error> {
        self.conn.query_row(
            "SELECT s.name, e.air_date, e.title
             FROM episodes e JOIN shows s ON s.id = e.show_id
             WHERE e.id = ?1",
            [episode_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
    }

    pub fn upsert_download(
        &self,
        episode_id: i64,
        path: &str,
        bytes: i64,
        total: i64,
        status: &str,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO downloads (episode_id, path, bytes, total, status)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(episode_id) DO UPDATE SET path=?2, bytes=?3, total=?4, status=?5",
            params![episode_id, path, bytes, total, status],
        )?;
        Ok(())
    }

    /// The stored destination path of a download, without touching the row.
    pub fn download_path(&self, episode_id: i64) -> Result<Option<String>, rusqlite::Error> {
        self.conn
            .query_row(
                "SELECT path FROM downloads WHERE episode_id=?1",
                [episode_id],
                |r| r.get(0),
            )
            .optional()
    }

    pub fn remove_download(&self, episode_id: i64) -> Result<Option<String>, rusqlite::Error> {
        let path: Option<String> = self
            .conn
            .query_row(
                "SELECT path FROM downloads WHERE episode_id=?1",
                [episode_id],
                |r| r.get(0),
            )
            .ok();
        self.conn.execute(
            "DELETE FROM downloads WHERE episode_id=?1",
            params![episode_id],
        )?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::{Db, TrackSyncMode};
    use crate::wfmu::{ParsedEpisode, ParsedRecentTrack, ParsedTrack};
    use rusqlite::{params, Connection};

    #[test]
    fn legacy_database_is_migrated_without_losing_rows() {
        let path = std::env::temp_dir().join(format!(
            "archiplayer-migration-{}-{}.db",
            std::process::id(),
            Db::now()
        ));
        {
            let legacy = Connection::open(&path).expect("create legacy db");
            legacy
                .execute_batch(
                    "CREATE TABLE shows (id TEXT PRIMARY KEY, name TEXT NOT NULL, dj TEXT, on_air INTEGER NOT NULL DEFAULT 0, last_scraped INTEGER);
                     CREATE TABLE episodes (id INTEGER PRIMARY KEY, show_id TEXT NOT NULL, air_date TEXT, title TEXT, archive_id INTEGER, audio_url TEXT, has_audio INTEGER NOT NULL DEFAULT 0, seq INTEGER NOT NULL DEFAULT 0, last_scraped INTEGER);
                     INSERT INTO shows (id, name) VALUES ('TEST', 'Migration Test');",
                )
                .expect("legacy schema");
        }

        let db = Db::open(&path).expect("migrate legacy db");
        assert_eq!(db.show_count().expect("row survives"), 1);
        let version: i64 = db
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("schema version");
        assert_eq!(version, 6);
        let offset_column: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('episodes') WHERE name='offset_sec'",
                [],
                |row| row.get(0),
            )
            .expect("offset column");
        assert_eq!(offset_column, 1);
        let timing_columns: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('episodes')
                 WHERE name IN ('broadcast_duration_sec', 'playlist_timing_checked')",
                [],
                |row| row.get(0),
            )
            .expect("playlist timing columns");
        assert_eq!(timing_columns, 2);
        let is_live_column: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('shows') WHERE name='is_live'",
                [],
                |row| row.get(0),
            )
            .expect("is_live column");
        assert_eq!(is_live_column, 1);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn playlist_timing_records_success_and_unsupported_pages() {
        let path = std::env::temp_dir().join(format!(
            "archiplayer-playlist-timing-{}-{}.db",
            std::process::id(),
            Db::now()
        ));
        let db = Db::open(&path).expect("open playlist timing test db");
        db.upsert_show("AU", "Anima Mundi", None, true).unwrap();
        db.upsert_episode(166789, "AU", None, None, Some(292106), None)
            .unwrap();
        assert!(!db.episode_playlist_timing_checked(166789).unwrap());

        db.set_episode_broadcast_duration(166789, Some(3 * 60 * 60))
            .unwrap();
        assert!(db.episode_playlist_timing_checked(166789).unwrap());
        assert_eq!(
            db.get_episode(166789).unwrap().broadcast_duration_sec,
            Some(3 * 60 * 60)
        );

        db.set_episode_broadcast_duration(166789, None).unwrap();
        assert!(db.episode_playlist_timing_checked(166789).unwrap());
        assert_eq!(db.get_episode(166789).unwrap().broadcast_duration_sec, None);

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn live_shows_are_hidden_from_catalog() {
        let path = std::env::temp_dir().join(format!(
            "archiplayer-islive-{}-{}.db",
            std::process::id(),
            Db::now()
        ));
        let db = Db::open(&path).expect("open is_live test db");
        db.upsert_show("BK", "Beware of the Blog", None, false)
            .unwrap();
        db.upsert_show("live-drummer", "Drummer (live)", None, true)
            .unwrap();
        db.set_show_live("live-drummer").unwrap();

        // The synthetic live row is excluded from the count and the A–Z listing.
        assert_eq!(db.show_count().unwrap(), 1);
        let shows = db.list_shows().unwrap();
        assert_eq!(shows.len(), 1);
        assert_eq!(shows[0].id, "BK");
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn upsert_without_a_seq_keeps_the_scraped_ordering() {
        let path = std::env::temp_dir().join(format!(
            "archiplayer-live-seq-{}-{}.db",
            std::process::id(),
            Db::now()
        ));
        let db = Db::open(&path).expect("open live seq test db");
        db.upsert_show("WA", "Wake", None, true).unwrap();
        db.upsert_episode(30, "WA", None, Some("Scraped"), Some(30), Some(4))
            .unwrap();

        // The live-status path meets the same hosted episode mid-broadcast.
        db.upsert_episode(30, "WA", None, Some("On air"), None, None)
            .unwrap();

        let (seq, title): (i64, String) = db
            .conn
            .query_row("SELECT seq, title FROM episodes WHERE id=30", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(seq, 4);
        assert_eq!(title, "On air");

        // A fresh episode with no ordering opinion still lands on the schema default.
        db.upsert_episode(31, "WA", None, Some("New"), None, None)
            .unwrap();
        let fresh: i64 = db
            .conn
            .query_row("SELECT seq FROM episodes WHERE id=31", [], |row| row.get(0))
            .unwrap();
        assert_eq!(fresh, 0);

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn shallow_show_refresh_keeps_cached_history_in_stable_order() {
        let path = std::env::temp_dir().join(format!(
            "archiplayer-show-refresh-{}-{}.db",
            std::process::id(),
            Db::now()
        ));
        let mut db = Db::open(&path).expect("open show refresh test db");
        db.upsert_show("WA", "Wake", None, true).unwrap();
        for (seq, id) in [30, 20, 10].into_iter().enumerate() {
            db.upsert_episode(id, "WA", None, None, Some(id), Some(seq as i64))
                .unwrap();
        }

        let current_page = [
            ParsedEpisode {
                id: 40,
                air_date: Some("July 24, 2026".into()),
                title: Some("New".into()),
                archive_id: Some(40),
            },
            ParsedEpisode {
                id: 30,
                air_date: Some("July 17, 2026".into()),
                title: Some("Updated".into()),
                archive_id: Some(30),
            },
        ];
        db.sync_show_episodes("WA", &current_page).unwrap();

        let episodes = db.list_episodes("WA").unwrap();
        assert_eq!(
            episodes
                .iter()
                .map(|episode| episode.id)
                .collect::<Vec<_>>(),
            vec![40, 30, 20, 10]
        );
        assert_eq!(episodes[1].title.as_deref(), Some("Updated"));

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn episodes_are_listed_by_air_date_not_scrape_order() {
        let path = std::env::temp_dir().join(format!(
            "archiplayer-episode-order-{}-{}.db",
            std::process::id(),
            Db::now()
        ));
        let db = Db::open(&path).expect("open episode order test db");
        db.upsert_show("KG", "Kenny", None, false).unwrap();
        // Scrape order as a legacy archive serves it: year blocks newest first, but
        // oldest first inside each block. A plain reversal of this cannot be chronological.
        for (seq, (id, air_date)) in [
            (1, Some("January 8, 2026")),
            (2, Some("December 3, 2026")),
            (3, Some("January 9, 2025")),
            (4, Some("December 4, 2025")),
            (5, None),
        ]
        .into_iter()
        .enumerate()
        {
            db.upsert_episode(id, "KG", air_date, None, Some(id), Some(seq as i64))
                .unwrap();
        }

        let ids: Vec<i64> = db
            .list_episodes("KG")
            .unwrap()
            .iter()
            .map(|episode| episode.id)
            .collect();
        // Newest air date first; the undated episode keeps its scrape place at the tail.
        assert_eq!(ids, vec![2, 1, 4, 3, 5]);

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn scrape_versions_start_stale_and_stamp_once_set() {
        let path = std::env::temp_dir().join(format!(
            "archiplayer-scrape-version-{}-{}.db",
            std::process::id(),
            Db::now()
        ));
        let db = Db::open(&path).expect("open scrape version test db");
        db.upsert_show("KG", "Kenny", None, false).unwrap();
        db.upsert_episode(13141, "KG", None, None, None, Some(0))
            .unwrap();

        // Rows that predate the column, and rows this build has not re-read, both read as 0.
        assert_eq!(db.show_scrape_version("KG").unwrap(), 0);
        assert_eq!(db.episode_tracks_version(13141).unwrap(), 0);
        // A show or episode nobody has cached yet is stale rather than an error.
        assert_eq!(db.show_scrape_version("NOPE").unwrap(), 0);
        assert_eq!(db.episode_tracks_version(-99).unwrap(), 0);

        db.set_show_scrape_version("KG", 3).unwrap();
        db.set_episode_tracks_version(13141, 3).unwrap();
        assert_eq!(db.show_scrape_version("KG").unwrap(), 3);
        assert_eq!(db.episode_tracks_version(13141).unwrap(), 3);

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn live_playlist_merge_preserves_favourite_track_ids() {
        let path = std::env::temp_dir().join(format!(
            "archiplayer-live-{}-{}.db",
            std::process::id(),
            Db::now()
        ));
        let mut db = Db::open(&path).expect("open live test db");
        db.upsert_show("LIVE", "Live", None, true).unwrap();
        db.upsert_episode(-1, "LIVE", None, Some("Live"), None, None)
            .unwrap();
        let first = ParsedTrack {
            artist: Some("Artist one".into()),
            title: Some("Track one".into()),
            album: None,
            label: None,
            comments: None,
            start_sec: Some(0),
        };
        db.sync_tracks(-1, std::slice::from_ref(&first), TrackSyncMode::Snapshot)
            .unwrap();
        let original_id = db.list_tracks(-1).unwrap()[0].id;
        db.toggle_favourite("track", &original_id.to_string())
            .unwrap();

        let second = ParsedTrack {
            artist: Some("Artist two".into()),
            title: Some("Track two".into()),
            start_sec: Some(180),
            ..first.clone()
        };
        db.sync_tracks(-1, &[first.clone(), second], TrackSyncMode::Snapshot)
            .unwrap();
        let merged = db.list_tracks(-1).unwrap();
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].id, original_id);
        assert!(merged[0].favourite);

        let observed_episode = -2;
        db.upsert_episode(observed_episode, "LIVE", None, Some("Observed"), None, None)
            .unwrap();
        db.sync_tracks(
            observed_episode,
            std::slice::from_ref(&first),
            TrackSyncMode::AppendObservations,
        )
        .unwrap();
        db.sync_tracks(
            observed_episode,
            std::slice::from_ref(&first),
            TrackSyncMode::AppendObservations,
        )
        .unwrap();
        assert_eq!(db.list_tracks(observed_episode).unwrap().len(), 1);

        db.record_listen("live-session", observed_episode, 42, false, 0, 0)
            .unwrap();
        let attribution: (i64, i64) = db
            .conn
            .query_row(
                "SELECT episode_id, seconds FROM listens WHERE id='live-session'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(attribution, (observed_episode, 42));

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn snapshot_drops_a_shrunken_tail_but_never_on_an_empty_parse() {
        let path = std::env::temp_dir().join(format!(
            "archiplayer-snapshot-{}-{}.db",
            std::process::id(),
            Db::now()
        ));
        let mut db = Db::open(&path).expect("open snapshot test db");
        db.upsert_show("WA", "Wake", None, false).unwrap();
        db.upsert_episode(500, "WA", None, Some("Episode"), None, None)
            .unwrap();
        let track = |n: i64| ParsedTrack {
            artist: Some(format!("Artist {n}")),
            title: Some(format!("Track {n}")),
            album: None,
            label: None,
            comments: None,
            start_sec: None,
        };

        db.sync_tracks(
            500,
            &[track(1), track(2), track(3)],
            TrackSyncMode::Snapshot,
        )
        .unwrap();
        assert_eq!(db.list_tracks(500).unwrap().len(), 3);

        // A shorter snapshot is authoritative: rows 2 and 3 go.
        db.sync_tracks(500, &[track(1)], TrackSyncMode::Snapshot)
            .unwrap();
        let remaining = db.list_tracks(500).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].title.as_deref(), Some("Track 1"));

        // An empty one is not: a failed fetch must not wipe a good cached playlist.
        db.sync_tracks(500, &[], TrackSyncMode::Snapshot).unwrap();
        assert_eq!(db.list_tracks(500).unwrap().len(), 1);

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn migration_purges_cached_page_chrome_and_forces_a_rescrape() {
        let path = std::env::temp_dir().join(format!(
            "archiplayer-purge-{}-{}.db",
            std::process::id(),
            Db::now()
        ));
        let good_id;
        let bad_id;
        {
            let mut db = Db::open(&path).expect("open purge test db");
            db.upsert_show("TW", "Teenage Wasteland", None, false)
                .unwrap();
            db.upsert_episode(91603, "TW", Some("February 23, 2020"), None, None, None)
                .unwrap();
            // What the old parser stored: the whole page as one track.
            db.sync_tracks(
                91603,
                &[
                    ParsedTrack {
                        artist: Some("document.domain=\"wfmu.org\"; if (top.KDBInPlaylistFrameset) { top.location.replace('x'); }".into()),
                        title: Some("Register | Please enable inline frames! function kdb_login_iframeResize(id){}".into()),
                        album: None,
                        label: None,
                        comments: None,
                        start_sec: None,
                    },
                    ParsedTrack {
                        artist: Some("Mal Thursday & The Cheetahs".into()),
                        title: Some("Torn Up".into()),
                        album: None,
                        label: None,
                        comments: None,
                        start_sec: None,
                    },
                ],
                TrackSyncMode::Snapshot,
            )
            .unwrap();
            let cached = db.list_tracks(91603).unwrap();
            bad_id = cached[0].id;
            good_id = cached[1].id;
            db.toggle_favourite("track", &bad_id.to_string()).unwrap();
            // Pretend the rows predate the repair.
            db.conn
                .pragma_update(None, "user_version", 3)
                .expect("rewind schema version");
        }

        let db = Db::open(&path).expect("reopen and repair");
        let repaired = db.list_tracks(91603).unwrap();
        assert_eq!(repaired.len(), 1);
        assert_eq!(repaired[0].id, good_id);

        // Nothing points at the deleted row any more, in favourites or in the search index.
        let orphan_favourites: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM favourites WHERE kind='track' AND ref_id=?1",
                params![bad_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(orphan_favourites, 0);
        if db.fts {
            let orphan_index: i64 = db
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM tracks_fts WHERE track_id=?1",
                    params![bad_id.to_string()],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(orphan_index, 0);
        }

        // The stamp is cleared, so get_playlist re-scrapes with the fixed parser.
        assert!(!db.episode_tracks_scraped(91603).unwrap());

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn live_playlist_refresh_detects_missing_and_stale_snapshots() {
        let path = std::env::temp_dir().join(format!(
            "archiplayer-live-refresh-{}-{}.db",
            std::process::id(),
            Db::now()
        ));
        let mut db = Db::open(&path).expect("open live refresh test db");
        db.upsert_show("LIVE", "Live", None, true).unwrap();
        db.upsert_episode(-3, "LIVE", None, Some("Live"), None, None)
            .unwrap();
        assert!(db.episode_tracks_stale(-3, 30).unwrap());

        let track = ParsedTrack {
            artist: Some("Artist".into()),
            title: Some("Track".into()),
            album: None,
            label: None,
            comments: None,
            start_sec: Some(0),
        };
        db.sync_tracks(-3, &[track], TrackSyncMode::Snapshot)
            .unwrap();
        assert!(!db.episode_tracks_stale(-3, 30).unwrap());

        db.conn
            .execute(
                "UPDATE episodes SET last_scraped=?2 WHERE id=?1",
                params![-3, Db::now() - 31],
            )
            .unwrap();
        assert!(db.episode_tracks_stale(-3, 30).unwrap());

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn provider_history_upgrades_observations_and_is_idempotent() {
        let path = std::env::temp_dir().join(format!(
            "archiplayer-provider-{}-{}.db",
            std::process::id(),
            Db::now()
        ));
        let mut db = Db::open(&path).unwrap();
        db.upsert_show("live-drummer", "Drummer", None, true)
            .unwrap();
        db.upsert_episode(
            -10,
            "live-drummer",
            Some("2026-07-14"),
            Some("Live"),
            None,
            None,
        )
        .unwrap();
        let observed = ParsedTrack {
            artist: Some("Alice".into()),
            title: Some("Signal".into()),
            album: None,
            label: None,
            comments: None,
            start_sec: None,
        };
        db.sync_tracks(-10, &[observed], TrackSyncMode::AppendObservations)
            .unwrap();
        let id = db.list_tracks(-10).unwrap()[0].id;
        db.toggle_favourite("track", &id.to_string()).unwrap();
        let provider = ParsedRecentTrack {
            source_id: "row-1".into(),
            artist: Some("Alice".into()),
            title: Some("Signal".into()),
            album: Some("Transmission".into()),
            played_at: 1_752_500_000,
            air_date: "2026-07-14".into(),
        };
        db.sync_provider_tracks(-10, "wfmugtd", std::slice::from_ref(&provider))
            .unwrap();
        db.sync_provider_tracks(-10, "wfmugtd", &[provider])
            .unwrap();
        let tracks = db.list_tracks(-10).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, id);
        assert_eq!(tracks[0].source_id.as_deref(), Some("wfmugtd:row-1"));
        assert_eq!(tracks[0].played_at, Some(1_752_500_000));
        assert!(tracks[0].favourite);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn opening_backfills_a_search_index_that_trails_the_tracks_table() {
        let path = std::env::temp_dir().join(format!(
            "archiplayer-fts-backfill-{}-{}.db",
            std::process::id(),
            Db::now()
        ));
        let mut db = Db::open(&path).expect("open fts backfill test db");
        assert!(db.fts);
        db.upsert_show("BK", "Beware of the Blog", None, false)
            .unwrap();
        db.upsert_episode(70, "BK", None, Some("Episode"), None, None)
            .unwrap();
        let track = ParsedTrack {
            artist: Some("Neu".into()),
            title: Some("Hallogallo".into()),
            album: None,
            label: None,
            comments: None,
            start_sec: Some(0),
        };
        db.sync_tracks(70, &[track], TrackSyncMode::Snapshot)
            .unwrap();
        let track_id = db.list_tracks(70).unwrap()[0].id;

        // Stand in for a database whose tracks were written before the index existed.
        db.conn.execute("DELETE FROM tracks_fts", []).unwrap();
        drop(db);

        let db = Db::open(&path).expect("reopen fts backfill test db");
        assert!(db.fts);
        let (indexed, total): (i64, i64) = db
            .conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM tracks_fts), (SELECT COUNT(*) FROM tracks)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(indexed, total);
        assert_eq!(indexed, 1);

        let indexed_track: (i64, i64) = db
            .conn
            .query_row(
                "SELECT CAST(track_id AS INTEGER), CAST(episode_id AS INTEGER)
                 FROM tracks_fts WHERE tracks_fts MATCH 'Hallogallo'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(indexed_track, (track_id, 70));

        drop(db);
        let _ = std::fs::remove_file(path);
    }
}

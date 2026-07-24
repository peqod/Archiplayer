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
        let fts = conn.execute_batch(FTS_SCHEMA).is_ok();
        Ok(Db { conn, fts })
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
                "tracks",
                "source_id",
                "ALTER TABLE tracks ADD COLUMN source_id TEXT",
            ),
            (
                "tracks",
                "played_at",
                "ALTER TABLE tracks ADD COLUMN played_at INTEGER",
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
             ON tracks(episode_id, source_id) WHERE source_id IS NOT NULL;",
        )?;
        // Retroactively flag synthetic live rows created before the is_live column existed,
        // so they stop appearing in the catalog. Real show ids never start with "live-".
        tx.execute_batch("UPDATE shows SET is_live = 1 WHERE id LIKE 'live-%';")?;
        tx.pragma_update(None, "user_version", 3)?;
        tx.commit()
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

    pub fn upsert_episode(
        &self,
        id: i64,
        show_id: &str,
        air_date: Option<&str>,
        title: Option<&str>,
        archive_id: Option<i64>,
        seq: i64,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO episodes (id, show_id, air_date, title, archive_id, has_audio, seq)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
               show_id=?2, air_date=COALESCE(?3, air_date), title=COALESCE(?4, title),
               archive_id=COALESCE(?5, archive_id),
               has_audio=CASE WHEN ?5 IS NULL THEN has_audio ELSE ?6 END,
               seq=?7",
            params![
                id,
                show_id,
                air_date,
                title,
                archive_id,
                archive_id.is_some() as i64,
                seq
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
                "INSERT INTO episodes (id, show_id, air_date, title, archive_id, has_audio, seq)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)
                 ON CONFLICT(id) DO UPDATE SET
                   show_id=?2, air_date=COALESCE(?3, air_date), title=COALESCE(?4, title),
                   archive_id=COALESCE(?5, archive_id),
                   has_audio=CASE WHEN ?5 IS NULL THEN has_audio ELSE ?6 END",
                params![
                    episode.id,
                    show_id,
                    episode.air_date,
                    episode.title,
                    episode.archive_id,
                    episode.archive_id.is_some() as i64,
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

    pub fn list_episodes(&self, show_id: &str) -> Result<Vec<Episode>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.show_id, e.air_date, e.title, e.archive_id, e.audio_url, e.has_audio,
                    EXISTS(SELECT 1 FROM favourites f WHERE f.kind='episode' AND f.ref_id = CAST(e.id AS TEXT)),
                    d.path, COALESCE(d.status,''),
                    (SELECT COUNT(*) FROM tracks t WHERE t.episode_id = e.id),
                    e.resume_sec, e.duration_sec, e.completed, e.offset_sec
             FROM episodes e LEFT JOIN downloads d ON d.episode_id = e.id
             WHERE e.show_id = ?1 ORDER BY e.seq",
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
                    e.resume_sec, e.duration_sec, e.completed, e.offset_sec
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
        assert_eq!(version, 3);
        let offset_column: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('episodes') WHERE name='offset_sec'",
                [],
                |row| row.get(0),
            )
            .expect("offset column");
        assert_eq!(offset_column, 1);
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
    fn shallow_show_refresh_keeps_cached_history_in_stable_order() {
        let path = std::env::temp_dir().join(format!(
            "archiplayer-show-refresh-{}-{}.db",
            std::process::id(),
            Db::now()
        ));
        let mut db = Db::open(&path).expect("open show refresh test db");
        db.upsert_show("WA", "Wake", None, true).unwrap();
        for (seq, id) in [30, 20, 10].into_iter().enumerate() {
            db.upsert_episode(id, "WA", None, None, Some(id), seq as i64)
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
    fn live_playlist_merge_preserves_favourite_track_ids() {
        let path = std::env::temp_dir().join(format!(
            "archiplayer-live-{}-{}.db",
            std::process::id(),
            Db::now()
        ));
        let mut db = Db::open(&path).expect("open live test db");
        db.upsert_show("LIVE", "Live", None, true).unwrap();
        db.upsert_episode(-1, "LIVE", None, Some("Live"), None, 0)
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
        db.upsert_episode(observed_episode, "LIVE", None, Some("Observed"), None, 0)
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
    fn live_playlist_refresh_detects_missing_and_stale_snapshots() {
        let path = std::env::temp_dir().join(format!(
            "archiplayer-live-refresh-{}-{}.db",
            std::process::id(),
            Db::now()
        ));
        let mut db = Db::open(&path).expect("open live refresh test db");
        db.upsert_show("LIVE", "Live", None, true).unwrap();
        db.upsert_episode(-3, "LIVE", None, Some("Live"), None, 0)
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
            0,
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
}

use rusqlite::{params, Connection};
use serde::Serialize;
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
    start_sec INTEGER
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
"#;

const FTS_SCHEMA: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS tracks_fts USING fts5(
    artist, title, album,
    track_id UNINDEXED, episode_id UNINDEXED
);
"#;

impl Db {
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(SCHEMA)?;
        // Migration for DBs created before the description column existed.
        let _ = conn.execute("ALTER TABLE shows ADD COLUMN description TEXT", []);
        let fts = conn.execute_batch(FTS_SCHEMA).is_ok();
        Ok(Db { conn, fts })
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
             FROM shows s ORDER BY s.name COLLATE NOCASE",
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
            .query_row("SELECT COUNT(*) FROM shows", [], |r| r.get(0))
    }

    pub fn mark_show_scraped(&self, id: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE shows SET last_scraped=?2 WHERE id=?1",
            params![id, Self::now()],
        )?;
        Ok(())
    }

    pub fn show_was_scraped(&self, id: &str) -> Result<bool, rusqlite::Error> {
        let scraped: Option<i64> = self
            .conn
            .query_row(
                "SELECT last_scraped FROM shows WHERE id=?1",
                params![id],
                |r| r.get(0),
            )?;
        Ok(scraped.is_some())
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
               air_date=COALESCE(?3, air_date), title=COALESCE(?4, title),
               archive_id=COALESCE(?5, archive_id),
               has_audio=CASE WHEN ?5 IS NULL THEN has_audio ELSE ?6 END,
               seq=?7",
            params![id, show_id, air_date, title, archive_id, archive_id.is_some() as i64, seq],
        )?;
        Ok(())
    }

    pub fn list_episodes(&self, show_id: &str) -> Result<Vec<Episode>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.show_id, e.air_date, e.title, e.archive_id, e.audio_url, e.has_audio,
                    EXISTS(SELECT 1 FROM favourites f WHERE f.kind='episode' AND f.ref_id = CAST(e.id AS TEXT)),
                    d.path, COALESCE(d.status,''),
                    (SELECT COUNT(*) FROM tracks t WHERE t.episode_id = e.id)
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
            })
        })?;
        rows.collect()
    }

    pub fn get_episode(&self, id: i64) -> Result<Episode, rusqlite::Error> {
        self.conn.query_row(
            "SELECT e.id, e.show_id, e.air_date, e.title, e.archive_id, e.audio_url, e.has_audio,
                    EXISTS(SELECT 1 FROM favourites f WHERE f.kind='episode' AND f.ref_id = CAST(e.id AS TEXT)),
                    d.path, COALESCE(d.status,''),
                    (SELECT COUNT(*) FROM tracks t WHERE t.episode_id = e.id)
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

    /// Record an archive id discovered after the initial show-page scrape
    /// (e.g. from the episode's own playlist page), marking the episode playable.
    pub fn set_episode_archive(&self, episode_id: i64, archive_id: i64) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE episodes SET archive_id=?2, has_audio=1 WHERE id=?1",
            params![episode_id, archive_id],
        )?;
        Ok(())
    }

    pub fn replace_tracks(
        &mut self,
        episode_id: i64,
        tracks: &[crate::wfmu::ParsedTrack],
    ) -> Result<(), rusqlite::Error> {
        let fts = self.fts;
        let tx = self.conn.transaction()?;
        if fts {
            tx.execute(
                "DELETE FROM tracks_fts WHERE episode_id = ?1",
                params![episode_id.to_string()],
            )?;
        }
        tx.execute("DELETE FROM tracks WHERE episode_id = ?1", params![episode_id])?;
        for (i, t) in tracks.iter().enumerate() {
            tx.execute(
                "INSERT INTO tracks (episode_id, seq, artist, title, album, label, comments, start_sec)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![episode_id, i as i64, t.artist, t.title, t.album, t.label, t.comments, t.start_sec],
            )?;
            if fts {
                let track_id = tx.last_insert_rowid();
                tx.execute(
                    "INSERT INTO tracks_fts (artist, title, album, track_id, episode_id)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![t.artist, t.title, t.album, track_id.to_string(), episode_id.to_string()],
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
                favourite: r.get::<_, i64>(9)? != 0,
            })
        })?;
        rows.collect()
    }

    pub fn episode_tracks_scraped(&self, episode_id: i64) -> Result<bool, rusqlite::Error> {
        let scraped: Option<i64> = self.conn.query_row(
            "SELECT last_scraped FROM episodes WHERE id=?1",
            [episode_id],
            |r| r.get(0),
        )?;
        Ok(scraped.is_some())
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
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO listens (id, episode_id, started_at, seconds, completed)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET seconds=MAX(seconds, ?4), completed=MAX(completed, ?5)",
            params![session_id, episode_id, Self::now(), seconds, completed as i64],
        )?;
        Ok(())
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

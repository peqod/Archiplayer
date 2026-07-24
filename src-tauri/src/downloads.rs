use crate::AppState;
use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::AsyncWriteExt;

#[derive(Serialize, Clone)]
pub struct DownloadProgress {
    pub episode_id: i64,
    pub bytes: i64,
    pub total: i64,
    pub status: String, // downloading | done | error
    pub error: Option<String>,
}

fn emit(app: &AppHandle, p: &DownloadProgress) {
    let _ = app.emit("download-progress", p);
}

/// Cleans up a half-finished download on any early return: removes the `.part` file and
/// flips the row out of `downloading` to `error`, then emits an error event. `disarm()` is
/// called only after the file is renamed into place and the row is marked `done`, so a
/// successful download skips all of this. Because it runs from `Drop`, every failure path
/// (create, write, flush, rename, and the final DB write) is covered by one mechanism.
struct DownloadGuard<'a> {
    state: &'a AppState,
    app: AppHandle,
    episode_id: i64,
    tmp: std::path::PathBuf,
    dest_str: String,
    total: i64,
    bytes: i64,
    armed: bool,
}

impl DownloadGuard<'_> {
    fn set_bytes(&mut self, bytes: i64) {
        self.bytes = bytes;
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for DownloadGuard<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let _ = std::fs::remove_file(&self.tmp);
        if let Ok(db) = self.state.db() {
            let _ = db.upsert_download(
                self.episode_id,
                &self.dest_str,
                self.bytes,
                self.total,
                "error",
            );
        }
        emit(
            &self.app,
            &DownloadProgress {
                episode_id: self.episode_id,
                bytes: self.bytes,
                total: self.total,
                status: "error".into(),
                error: Some("download did not complete".into()),
            },
        );
    }
}

/// "Show - Air date - Title - Episode id". The id is always present so two episodes with
/// matching metadata cannot collide on one filename.
fn build_name(show: &str, air: Option<&str>, title: Option<&str>, episode_id: i64) -> String {
    let mut s = show.trim().to_string();
    if s.is_empty() {
        s = "Episode".to_string();
    }
    if let Some(a) = air {
        if !a.trim().is_empty() {
            s.push_str(" - ");
            s.push_str(a.trim());
        }
    }
    if let Some(t) = title {
        if !t.trim().is_empty() {
            s.push_str(" - ");
            s.push_str(t.trim());
        }
    }
    s.push_str(&format!(" - {episode_id}"));
    s
}

/// Strip characters that are illegal in Windows filenames and cap the length.
fn sanitize_filename(name: &str) -> String {
    let mapped: String = name
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let collapsed = mapped.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_matches(|c| c == '.' || c == ' ');
    let capped: String = trimmed.chars().take(120).collect();
    let capped = capped.trim_matches(|c| c == '.' || c == ' ').to_string();
    if capped.is_empty() {
        "download".to_string()
    } else {
        capped
    }
}

#[tauri::command]
pub async fn download_episode(
    episode_id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    {
        let mut active = state
            .active_downloads
            .lock()
            .map_err(|_| "download state lock poisoned".to_string())?;
        if !active.insert(episode_id) {
            return Err("this episode is already downloading".into());
        }
    }
    let result = download_episode_inner(episode_id, app, state.clone()).await;
    if let Ok(mut active) = state.active_downloads.lock() {
        active.remove(&episode_id);
    }
    result
}

async fn download_episode_inner(
    episode_id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Resolve source URL (cached or via m3u) and destination path.
    let source = crate::commands::resolve_audio(episode_id, state.clone()).await?;
    if source.local {
        return Ok(source.url); // already downloaded
    }
    // Destination directory: user-configured, or the default under app data.
    let configured = {
        state
            .db()?
            .get_setting("download_dir")
            .map_err(|e| e.to_string())?
    };
    let dir = match configured {
        Some(d) if !d.trim().is_empty() => std::path::PathBuf::from(d),
        _ => app
            .path()
            .app_data_dir()
            .map_err(|e| format!("no app data dir: {e}"))?
            .join("downloads"),
    };
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("mkdir failed: {e}"))?;
    // Meaningful filename: "Show - Air date - Title - Episode id.mp3" (sanitised).
    let filename = {
        let parts = state.db()?.episode_filename_parts(episode_id).ok();
        match parts {
            Some((show, air, title)) => sanitize_filename(&build_name(
                &show,
                air.as_deref(),
                title.as_deref(),
                episode_id,
            )),
            None => episode_id.to_string(),
        }
    };
    // Cached and freshly scraped URLs both pass through the same strict validation immediately
    // before the backend fetch. The audio-only client applies it again to every redirect.
    let audio_url = crate::wfmu::validate_audio_url(&source.url)?;
    let extension = std::path::Path::new(audio_url.path())
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|ext| matches!(ext.as_str(), "mp3" | "mp4" | "m4a" | "aac"))
        .unwrap_or_else(|| "mp3".to_string());
    let dest = dir.join(format!("{filename}.{extension}"));
    let dest_str = dest.to_string_lossy().to_string();
    match tokio::fs::try_exists(&dest).await {
        Ok(true) => return Err(format!("download destination already exists: {dest_str}")),
        Ok(false) => {}
        Err(error) => return Err(format!("could not inspect download destination: {error}")),
    }

    let resp = state
        .fetcher
        .audio_client()
        .get(audio_url)
        .send()
        .await
        .map_err(|e| format!("download request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} downloading audio", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0) as i64;
    let tmp = dir.join(format!("{episode_id}.part"));
    // `create_new` refuses to clobber a stale/concurrent partial file. Construct the guard only
    // after this succeeds so a failed reservation never deletes a file owned by another process.
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .await
        .map_err(|e| format!("reserve partial download failed: {e}"))?;
    // From here on, any early return cleans up the partial via the guard's Drop.
    let mut guard = DownloadGuard {
        state: &state,
        app: app.clone(),
        episode_id,
        tmp: tmp.clone(),
        dest_str: dest_str.clone(),
        total,
        bytes: 0,
        armed: true,
    };
    {
        let db = state.db()?;
        db.upsert_download(episode_id, &dest_str, 0, total, "downloading")
            .map_err(|e| e.to_string())?;
    }
    let mut stream = resp.bytes_stream();
    let mut bytes: i64 = 0;
    let mut last_emit = std::time::Instant::now();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("download interrupted: {e}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("write failed: {e}"))?;
        bytes += chunk.len() as i64;
        guard.set_bytes(bytes);
        if last_emit.elapsed().as_millis() > 300 {
            last_emit = std::time::Instant::now();
            {
                if let Ok(db) = state.db() {
                    let _ = db.upsert_download(episode_id, &dest_str, bytes, total, "downloading");
                }
            }
            emit(
                &app,
                &DownloadProgress {
                    episode_id,
                    bytes,
                    total,
                    status: "downloading".into(),
                    error: None,
                },
            );
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    match tokio::fs::try_exists(&dest).await {
        Ok(true) => {
            return Err(format!(
                "download destination appeared during transfer: {dest_str}"
            ))
        }
        Ok(false) => {}
        Err(error) => return Err(format!("could not inspect download destination: {error}")),
    }
    tokio::fs::rename(&tmp, &dest)
        .await
        .map_err(|e| format!("finalize failed: {e}"))?;

    {
        let db = state.db()?;
        db.upsert_download(episode_id, &dest_str, bytes, bytes.max(total), "done")
            .map_err(|e| e.to_string())?;
    }
    // File is renamed into place and the row is marked done: no cleanup needed.
    guard.disarm();
    emit(
        &app,
        &DownloadProgress {
            episode_id,
            bytes,
            total: bytes.max(total),
            status: "done".into(),
            error: None,
        },
    );
    Ok(dest_str)
}

#[cfg(test)]
mod tests {
    use super::{build_name, sanitize_filename};

    #[test]
    fn download_names_are_portable_and_distinct() {
        assert_eq!(sanitize_filename("Show: A/B?"), "Show- A-B-");
        assert_eq!(
            build_name("Show", Some("July 24, 2026"), Some("Title"), 42),
            "Show - July 24, 2026 - Title - 42"
        );
        assert_eq!(build_name("Show", None, None, 42), "Show - 42");
        assert!(sanitize_filename("...   ").starts_with("download"));
    }
}

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

#[tauri::command]
pub async fn download_episode(
    episode_id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Resolve source URL (cached or via m3u) and destination path.
    let source = crate::commands::resolve_audio(episode_id, state.clone()).await?;
    if source.local {
        return Ok(source.url); // already downloaded
    }
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data dir: {e}"))?
        .join("downloads");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("mkdir failed: {e}"))?;
    let dest = dir.join(format!("{episode_id}.mp3"));
    let dest_str = dest.to_string_lossy().to_string();

    let resp = state
        .fetcher
        .client()
        .get(&source.url)
        .send()
        .await
        .map_err(|e| format!("download request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} downloading audio", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0) as i64;
    {
        let db = state.db.lock().unwrap();
        db.upsert_download(episode_id, &dest_str, 0, total, "downloading")
            .map_err(|e| e.to_string())?;
    }

    let tmp = dir.join(format!("{episode_id}.part"));
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .map_err(|e| format!("create file failed: {e}"))?;
    let mut stream = resp.bytes_stream();
    let mut bytes: i64 = 0;
    let mut last_emit = std::time::Instant::now();

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("download interrupted: {e}");
                {
                    let db = state.db.lock().unwrap();
                    let _ = db.upsert_download(episode_id, &dest_str, bytes, total, "error");
                }
                emit(
                    &app,
                    &DownloadProgress {
                        episode_id,
                        bytes,
                        total,
                        status: "error".into(),
                        error: Some(msg.clone()),
                    },
                );
                let _ = tokio::fs::remove_file(&tmp).await;
                return Err(msg);
            }
        };
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("write failed: {e}"))?;
        bytes += chunk.len() as i64;
        if last_emit.elapsed().as_millis() > 300 {
            last_emit = std::time::Instant::now();
            {
                let db = state.db.lock().unwrap();
                let _ = db.upsert_download(episode_id, &dest_str, bytes, total, "downloading");
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
    tokio::fs::rename(&tmp, &dest)
        .await
        .map_err(|e| format!("finalize failed: {e}"))?;

    {
        let db = state.db.lock().unwrap();
        db.upsert_download(episode_id, &dest_str, bytes, bytes.max(total), "done")
            .map_err(|e| e.to_string())?;
    }
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

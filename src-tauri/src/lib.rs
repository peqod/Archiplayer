pub mod commands;
pub mod db;
pub mod downloads;
pub mod wfmu;

use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, MutexGuard},
    time::Instant,
};
use tauri::Manager;

pub struct AppState {
    pub db: Mutex<db::Db>,
    pub fetcher: wfmu::Fetcher,
    /// Episode ids currently being written. Prevents two UI actions from sharing a .part file.
    pub active_downloads: Mutex<HashSet<i64>>,
    pub live_schedule_cache:
        tokio::sync::Mutex<HashMap<String, (Instant, Vec<wfmu::ParsedLiveProgram>)>>,
}

impl AppState {
    pub fn db(&self) -> Result<MutexGuard<'_, db::Db>, String> {
        self.db
            .lock()
            .map_err(|_| "database lock poisoned".to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let database = db::Db::open(&data_dir.join("library.db"))?;
            // Re-grant asset access to a persisted custom download folder. Tauri does not
            // persist the dialog's runtime scope grant across restarts, so without this,
            // downloads saved outside $APPDATA/downloads stop playing after a relaunch.
            if let Ok(Some(dir)) = database.get_setting("download_dir") {
                if !dir.trim().is_empty() {
                    let _ = app.asset_protocol_scope().allow_directory(&dir, true);
                }
            }
            app.manage(AppState {
                db: Mutex::new(database),
                fetcher: wfmu::Fetcher::new(),
                active_downloads: Mutex::new(HashSet::new()),
                live_schedule_cache: tokio::sync::Mutex::new(HashMap::new()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_catalog,
            commands::get_show,
            commands::get_playlist,
            commands::get_live_status,
            commands::get_live_page,
            commands::resolve_audio,
            commands::toggle_favourite,
            commands::list_favourites,
            commands::search,
            commands::record_listen,
            commands::get_stats,
            commands::list_downloads,
            commands::delete_download,
            commands::export_csv,
            commands::get_download_dir,
            commands::set_download_dir,
            downloads::download_episode,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

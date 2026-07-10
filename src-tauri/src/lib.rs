pub mod commands;
pub mod db;
pub mod downloads;
pub mod wfmu;

use std::sync::Mutex;
use tauri::Manager;

pub struct AppState {
    pub db: Mutex<db::Db>,
    pub fetcher: wfmu::Fetcher,
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
            app.manage(AppState {
                db: Mutex::new(database),
                fetcher: wfmu::Fetcher::new(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_catalog,
            commands::get_show,
            commands::get_playlist,
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

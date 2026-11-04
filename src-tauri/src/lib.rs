mod commands;
mod crypto;
mod db;
mod state;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // %APPDATA%/com.vaulttask.app on Windows,
            // ~/.local/share/com.vaulttask.app on Ubuntu/Fedora.
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let db_path = data_dir.join("vault_task.db");
            app.manage(AppState::new(db_path));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_vault_status,
            commands::setup_vault,
            commands::unlock_vault,
            commands::lock_vault,
            commands::update_security_settings,
            commands::record_activity_heartbeat,
            commands::list_tasks,
            commands::create_task,
            commands::update_task,
            commands::delete_task,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

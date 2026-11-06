pub(crate) mod commands;
mod crypto;
mod db;
mod pkg_update;
mod scheduler;
pub(crate) mod state;
mod tray;
mod watchdog;
// Task 3 ships the pure core only; Tauri glue (later tasks) will call
// these from the crate root. Drop this allow once the glue lands.
#[allow(dead_code)]
mod window_state;

#[cfg(test)]
mod ipc_contract;

use state::AppState;
use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch (Start Menu / App Grid) opens the existing
            // tray window instead of spawning a duplicate process.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            // %APPDATA%/com.vaulttask.app on Windows,
            // ~/.local/share/com.vaulttask.app on Ubuntu/Fedora.
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let db_path = data_dir.join("vault_task.db");
            app.manage(AppState::new(db_path));

            // Persistent tray icon (graceful: Fedora GNOME without an
            // appindicator extension simply runs without a tray host).
            if let Err(e) = tray::create_tray(app.handle()) {
                eprintln!("[vault-task] tray unavailable, continuing without it: {e}");
            }
            // Autostart launches (--minimized) boot straight to the tray.
            if std::env::args().any(|a| a == "--minimized")
                && let Some(window) = app.get_webview_window("main")
            {
                let _ = window.hide();
            }
            // Ctrl+Shift+Space summon (graceful: Wayland may deny it).
            if let Err(e) = tray::register_toggle_shortcut(app.handle()) {
                eprintln!("[vault-task] global shortcut unavailable: {e}");
            }
            // 1s inactivity watchdog + OS session-lock listener.
            watchdog::spawn_watchdog(app.handle());
            watchdog::spawn_session_lock_listener(app.handle());
            // 15s due-reminder notification scheduler.
            scheduler::spawn_scheduler(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Close button hides to tray instead of exiting (OneDrive-style).
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                tray::hide_main_window(window.app_handle());
            }
            // Popover auto-hides on focus loss unless pinned.
            WindowEvent::Focused(false) => {
                let keep_open = window
                    .app_handle()
                    .state::<AppState>()
                    .session
                    .lock()
                    .map(|s| s.is_pinned || s.window_mode != "popover")
                    .unwrap_or(true);
                if !keep_open {
                    tray::hide_main_window(window.app_handle());
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_vault_status,
            commands::setup_vault,
            commands::unlock_vault,
            commands::lock_vault,
            commands::update_security_settings,
            commands::record_activity_heartbeat,
            commands::change_vault_password,
            commands::list_tasks,
            commands::create_task,
            commands::update_task,
            commands::delete_task,
            commands::set_window_mode,
            commands::toggle_maximize_mode,
            commands::hide_to_tray,
            commands::set_pinned,
            commands::export_encrypted_backup,
            commands::import_encrypted_backup,
            commands::export_decrypted_markdown,
            commands::enable_keyring_unlock,
            commands::unlock_with_keyring,
            pkg_update::get_install_info,
            pkg_update::check_system_update_cmd,
            pkg_update::download_system_update_cmd,
            pkg_update::install_system_package_cmd,
            pkg_update::launch_system_installer_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

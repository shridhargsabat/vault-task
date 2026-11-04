//! Automatic inactivity lock engine.
//!
//! - [`spawn_watchdog`]: 1s Tokio loop comparing idle time against
//!   `auto_lock_seconds`. Works on Windows 11, Ubuntu and Fedora, on
//!   Wayland and X11, with no privileges (pure userspace timer).
//! - [`spawn_session_lock_listener`]: locks immediately when the OS
//!   session locks (Linux D-Bus `ScreenSaver.ActiveChanged`; Windows
//!   coverage is deferred — see below).
//! - Tray-hide locking lives in [`crate::tray::hide_main_window`].
//!
//! Every path funnels through [`crate::tray::lock_session_and_emit`]:
//! the `MasterKey` is zeroized + dropped, `vault://locked` is emitted,
//! and the tray reflects the locked state.

use tauri::{AppHandle, Manager, Runtime};

use crate::state::AppState;

/// One watchdog tick: lock the vault if it is unlocked and idle past the
/// configured timeout, and sync the tray tooltip/menu with the lock state
/// (converges within 1s of any lock/unlock without touching IPC commands).
/// Returns true when this tick locked the vault.
/// Sync + runtime-generic so unit tests can drive it on MockRuntime.
pub fn tick<R: Runtime>(app: &AppHandle<R>) -> bool {
    // Always sync the tray first (covers locks initiated via IPC commands,
    // which stay MockRuntime-compatible by avoiding AppHandle params).
    crate::tray::refresh_tray_state(app);
    let idle_past_timeout = {
        let state = app.state::<AppState>();
        let session = match state.session.lock() {
            Ok(s) => s,
            Err(_) => return false,
        };
        if session.master_key.is_none() {
            return false; // already locked
        }
        let timeout = std::time::Duration::from_secs(session.auto_lock_seconds);
        session.last_activity.elapsed() >= timeout
    };
    if idle_past_timeout {
        eprintln!("[vault-task] inactivity timeout reached; locking vault");
        crate::tray::lock_session_and_emit(app);
        true
    } else {
        false
    }
}

/// Spawn the 1s watchdog loop. Fire-and-forget; the loop never exits.
pub fn spawn_watchdog<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            interval.tick().await;
            tick(&app);
        }
    });
}

/// Shared entry for OS session-lock listeners. Idempotent.
pub fn handle_os_lock<R: Runtime>(app: &AppHandle<R>, source: &str) {
    let unlocked = app
        .state::<AppState>()
        .session
        .lock()
        .map(|s| !s.is_locked())
        .unwrap_or(false);
    if unlocked {
        eprintln!("[vault-task] OS session lock ({source}); locking vault");
        crate::tray::lock_session_and_emit(app);
        crate::tray::refresh_tray_state(app);
    }
}

/// Linux: subscribe to `org.freedesktop.ScreenSaver.ActiveChanged` on the
/// session bus (implemented by GNOME Shell, KDE, SwayNC-era compositors).
/// Graceful: no bus / no screensaver service => log + rely on the
/// inactivity watchdog.
#[cfg(target_os = "linux")]
pub fn spawn_session_lock_listener<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = listen_screensaver(&app).await {
            eprintln!("[vault-task] session-lock listener unavailable: {e}");
        }
    });
}

#[cfg(target_os = "linux")]
#[zbus::proxy(
    interface = "org.freedesktop.ScreenSaver",
    default_service = "org.freedesktop.ScreenSaver",
    default_path = "/org/freedesktop/ScreenSaver"
)]
trait ScreenSaver {
    #[zbus(signal)]
    fn active_changed(&self, active: bool) -> zbus::Result<()>;
}

#[cfg(target_os = "linux")]
async fn listen_screensaver<R: Runtime>(app: &AppHandle<R>) -> zbus::Result<()> {
    use futures_util::StreamExt;
    let conn = zbus::Connection::session().await?;
    let proxy = ScreenSaverProxy::new(&conn).await?;
    let mut stream = proxy.receive_active_changed().await?;
    eprintln!("[vault-task] session-lock listener active (org.freedesktop.ScreenSaver)");
    while let Some(signal) = stream.next().await {
        if signal.args()?.active {
            handle_os_lock(app, "ScreenSaver ActiveChanged");
        }
    }
    Ok(())
}

/// Windows session-lock detection (WTSRegisterSessionNotification) needs
/// Win32 message-loop integration that cannot be compile-checked from a
/// Linux host; deferred deliberately. The 1s inactivity watchdog still
/// locks idle vaults on Windows with no privileges.
#[cfg(not(target_os = "linux"))]
pub fn spawn_session_lock_listener<R: Runtime>(_app: &AppHandle<R>) {}

#[cfg(test)]
mod tests {
    use tauri::test::{mock_builder, mock_context, noop_assets};

    use super::*;
    use crate::{crypto::MasterKey, state::AppState};

    fn test_app() -> tauri::App<tauri::test::MockRuntime> {
        let db = std::env::temp_dir().join(format!("vault-wd-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db);
        mock_builder()
            .manage(AppState::new(db))
            .build(mock_context(noop_assets()))
            .unwrap()
    }

    fn unlock_idle(app: &tauri::App<tauri::test::MockRuntime>, idle_secs: u64) {
        let state = app.handle().state::<AppState>();
        let mut s = state.session.lock().unwrap();
        s.unlock(MasterKey([7u8; 32]));
        s.auto_lock_seconds = 60;
        s.last_activity = std::time::Instant::now() - std::time::Duration::from_secs(idle_secs);
    }

    #[test]
    fn tick_locks_idle_vault() {
        let app = test_app();
        unlock_idle(&app, 120);
        assert!(tick(app.handle()));
        let state = app.handle().state::<AppState>();
        let s = state.session.lock().unwrap();
        assert!(s.is_locked());
        assert!(s.master_key.is_none());
    }

    #[test]
    fn tick_ignores_active_vault() {
        let app = test_app();
        unlock_idle(&app, 5);
        assert!(!tick(app.handle()));
        assert!(
            !app.handle()
                .state::<AppState>()
                .session
                .lock()
                .unwrap()
                .is_locked()
        );
    }

    #[test]
    fn tick_ignores_locked_vault() {
        let app = test_app();
        assert!(!tick(app.handle()));
    }

    #[test]
    fn os_lock_handler_locks_unlocked_vault() {
        let app = test_app();
        unlock_idle(&app, 1);
        handle_os_lock(app.handle(), "test");
        assert!(
            app.handle()
                .state::<AppState>()
                .session
                .lock()
                .unwrap()
                .is_locked()
        );
    }
}

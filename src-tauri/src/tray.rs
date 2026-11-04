//! System tray icon + dual-mode window manager.
//!
//! - **Popover mode**: 420x620 frameless flyout anchored near the tray
//!   (OneDrive / Google Drive style), auto-hides on focus loss unless pinned.
//! - **Maximized mode**: full-screen workspace window with taskbar entry.
//!
//! Tray creation and global-shortcut registration are both graceful: on
//! Fedora GNOME without an appindicator extension (or Wayland compositors
//! that deny global shortcuts) the app keeps running and the window can
//! still be summoned from the app grid / dash.

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_positioner::{Position, WindowExt as PositionerExt};

use crate::state::AppState;

/// Compact popover dimensions (OneDrive-style flyout).
pub const POPOVER_WIDTH: u32 = 420;
pub const POPOVER_HEIGHT: u32 = 620;
/// Inset from the work-area corner for the fallback positioner.
pub const POPOVER_MARGIN: i32 = 12;

// ---------------------------------------------------------------------------
// Positioning
// ---------------------------------------------------------------------------

/// Pure anchor math (unit-testable): returns the top-left `(x, y)` for the
/// popover inside a work area at `(work_x, work_y)` of size `work_w x work_h`.
/// `top = true` anchors top-right (GNOME), `false` anchors bottom-right
/// (Windows / KDE).
pub fn anchor_position(
    work_x: i32,
    work_y: i32,
    work_w: i32,
    work_h: i32,
    top: bool,
) -> (i32, i32) {
    let x = work_x + work_w - POPOVER_WIDTH as i32 - POPOVER_MARGIN;
    let y = if top {
        work_y + POPOVER_MARGIN
    } else {
        work_y + work_h - POPOVER_HEIGHT as i32 - POPOVER_MARGIN
    };
    (x, y)
}

fn is_kde() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .map(|s| s.to_lowercase().contains("kde"))
        .unwrap_or(false)
        || std::env::var("KDE_SESSION_VERSION").is_ok()
}

/// Position the popover near the tray: OS-aware positioner first, with a
/// Wayland-safe work-area fallback (Wayland blocks global cursor coords on
/// some compositors, and tray positions are unknown until the first tray
/// event arrives).
pub fn position_popover<R: Runtime>(window: &WebviewWindow<R>) {
    #[cfg(target_os = "windows")]
    let primary = window.move_window_constrained(Position::TrayCenter);
    #[cfg(not(target_os = "windows"))]
    let primary = window.move_window(if is_kde() {
        Position::BottomRight
    } else {
        Position::TopRight
    });

    if primary.is_err() {
        // Fallback: anchor 12px inside the active monitor's work-area corner.
        if let Ok(Some(monitor)) = window.current_monitor() {
            let work = monitor.work_area();
            #[cfg(target_os = "windows")]
            let top = false;
            #[cfg(not(target_os = "windows"))]
            let top = !is_kde();
            let (x, y) = anchor_position(
                work.position.x,
                work.position.y,
                work.size.width as i32,
                work.size.height as i32,
                top,
            );
            let _ = window.set_position(tauri::Position::Physical(PhysicalPosition::new(x, y)));
        }
    }
}

// ---------------------------------------------------------------------------
// Mode switching
// ---------------------------------------------------------------------------

fn record_mode<R: Runtime>(app: &AppHandle<R>, mode: &str) {
    if let Ok(session) = app.state::<AppState>().session.lock() {
        // Poisoned lock: keep the old mode rather than panicking.
        let mut session = session;
        session.window_mode = mode.to_string();
    }
}

/// Enter popover mode: compact, frameless, always-on-top, no taskbar entry.
pub fn show_popover<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) -> Result<(), String> {
    let _ = window.unmaximize();
    window
        .set_size(tauri::Size::Physical(PhysicalSize::new(
            POPOVER_WIDTH,
            POPOVER_HEIGHT,
        )))
        .map_err(|e| format!("set popover size: {e}"))?;
    window
        .set_resizable(false)
        .map_err(|e| format!("set resizable: {e}"))?;
    window
        .set_skip_taskbar(true)
        .map_err(|e| format!("set skip_taskbar: {e}"))?;
    window
        .set_always_on_top(true)
        .map_err(|e| format!("set always_on_top: {e}"))?;
    position_popover(window);
    window.show().map_err(|e| format!("show window: {e}"))?;
    let _ = window.set_focus();
    record_mode(app, "popover");
    Ok(())
}

/// Enter maximized mode: taskbar entry, resizable, OS-maximized workspace.
pub fn show_maximized<R: Runtime>(
    app: &AppHandle<R>,
    window: &WebviewWindow<R>,
) -> Result<(), String> {
    window
        .set_always_on_top(false)
        .map_err(|e| format!("set always_on_top: {e}"))?;
    window
        .set_skip_taskbar(false)
        .map_err(|e| format!("set skip_taskbar: {e}"))?;
    window
        .set_resizable(true)
        .map_err(|e| format!("set resizable: {e}"))?;
    window.show().map_err(|e| format!("show window: {e}"))?;
    window.maximize().map_err(|e| format!("maximize: {e}"))?;
    let _ = window.set_focus();
    record_mode(app, "maximized");
    Ok(())
}

/// Tray-click / hotkey summon: hide when visible, otherwise open the popover.
pub fn toggle_visibility<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    match window.is_visible() {
        Ok(true) => hide_main_window(app),
        _ => {
            let _ = show_popover(app, window);
        }
    }
}

/// Lock the vault and notify the frontend to purge. Funnels every lock
/// path (tray menu, watchdog, OS session lock, Ctrl+L) through one place.
pub fn lock_session_and_emit<R: Runtime>(app: &AppHandle<R>) {
    if let Ok(mut session) = app.state::<AppState>().session.lock() {
        session.lock();
    }
    let _ = app.emit("vault://locked", ());
    refresh_tray_state(app);
}

/// Hide the main window; when `lock_on_tray_hide` is set, lock the vault too.
/// All hide paths (titlebar X, close-to-tray, focus loss, tray toggle) route here.
pub fn hide_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    let should_lock = app
        .state::<AppState>()
        .session
        .lock()
        .map(|s| s.lock_on_tray_hide && !s.is_locked())
        .unwrap_or(false);
    if should_lock {
        eprintln!("[vault-task] window hidden with lock-on-tray-hide; locking vault");
        lock_session_and_emit(app);
    }
}

/// Sync tray tooltip + menu with the current lock state. Best-effort:
/// silently no-ops when no tray host exists (headless tests, bare GNOME).
pub fn refresh_tray_state<R: Runtime>(app: &AppHandle<R>) {
    let locked = app
        .state::<AppState>()
        .session
        .lock()
        .map(|s| s.is_locked())
        .unwrap_or(true);
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some(if locked {
            "VaultTask (Locked)"
        } else {
            "VaultTask"
        }));
        if let Ok(menu) = build_tray_menu(app, locked) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

// ---------------------------------------------------------------------------
// Tray icon
// ---------------------------------------------------------------------------

/// Build the tray context menu for the given lock state.
pub fn build_tray_menu<R: Runtime>(app: &AppHandle<R>, locked: bool) -> tauri::Result<Menu<R>> {
    let open = MenuItem::with_id(app, "tray-open", "Open VaultTask", true, None::<&str>)?;
    let maximize = MenuItem::with_id(
        app,
        "tray-maximize",
        "Maximize Workspace",
        true,
        None::<&str>,
    )?;
    let lock = MenuItem::with_id(
        app,
        "tray-lock",
        if locked { "Vault Locked" } else { "Lock Vault" },
        !locked,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "tray-quit", "Quit", true, None::<&str>)?;
    Menu::with_items(app, &[&open, &maximize, &lock, &quit])
}

/// Build the persistent tray icon with its context menu.
/// Returns Err (logged by the caller) when no tray host exists.
pub fn create_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    // Vaults boot locked (no in-memory key until unlock).
    let menu = build_tray_menu(app, true)?;

    let icon = app
        .default_window_icon()
        .ok_or_else(|| std::io::Error::other("no default window icon for tray"))?
        .clone();

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("VaultTask (Locked)")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            match id {
                "tray-open" => {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = show_popover(app, &w);
                    }
                }
                "tray-maximize" => {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = show_maximized(app, &w);
                    }
                }
                "tray-lock" => lock_session_and_emit(app),
                "tray-quit" => app.exit(0),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            // Feed tray geometry to the positioner (enables TrayCenter).
            tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    toggle_visibility(app, &w);
                }
            }
        })
        .build(app)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Global summon shortcut
// ---------------------------------------------------------------------------

/// Ctrl+Shift+Space summons the popover even without a tray host.
pub fn toggle_shortcut() -> Shortcut {
    Shortcut::new(
        Some(Modifiers::CONTROL | Modifiers::SHIFT),
        Code::Space,
    )
}

pub fn register_toggle_shortcut<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(toggle_shortcut(), |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                if let Some(w) = app.get_webview_window("main") {
                    toggle_visibility(app, &w);
                }
            }
        })
        .map_err(|e| format!("register global shortcut: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_top_right_gnome() {
        // 1920x1080 work area at origin → top-right, 12px inset.
        let (x, y) = anchor_position(0, 0, 1920, 1080, true);
        assert_eq!((x, y), (1920 - 420 - 12, 12));
    }

    #[test]
    fn anchor_bottom_right_windows_kde() {
        let (x, y) = anchor_position(0, 0, 1920, 1080, false);
        assert_eq!((x, y), (1920 - 420 - 12, 1080 - 620 - 12));
    }

    #[test]
    fn anchor_respects_work_area_offset() {
        // Secondary monitor offset + top panel strut.
        let (x, y) = anchor_position(1920, 32, 2560, 1408, true);
        assert_eq!((x, y), (1920 + 2560 - 420 - 12, 32 + 12));
    }

    #[test]
    fn toggle_shortcut_is_ctrl_shift_space() {
        let s = toggle_shortcut();
        assert_eq!(s.key, Code::Space);
        assert!(s.mods.contains(Modifiers::CONTROL));
        assert!(s.mods.contains(Modifiers::SHIFT));
    }
}

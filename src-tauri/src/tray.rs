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

#[cfg(target_os = "linux")]
use tauri::menu::{Menu, MenuItem};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow,
    image::Image as TauriImage,
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
pub fn show_popover<R: Runtime>(
    app: &AppHandle<R>,
    window: &WebviewWindow<R>,
) -> Result<(), String> {
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
    crate::window_state::restore_or_anchor(app, window);
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
/// path (titlebar, watchdog, OS session lock, Ctrl+L) through one place.
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
    let track = app
        .state::<AppState>()
        .session
        .lock()
        .map(|s| crate::window_state::should_track_move(&s.window_mode))
        .unwrap_or(false);
    if track {
        crate::window_state::save_current_position(app);
    }
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

/// Sync the tray tooltip with the current lock state. Best-effort:
/// silently no-ops when no tray host exists (headless tests, bare GNOME).
/// The Linux menu is static (built once, never rebuilt), so only the
/// tooltip is pushed here — still change-gated to avoid redundant IPC.
pub fn refresh_tray_state<R: Runtime>(app: &AppHandle<R>) {
    let locked = app
        .state::<AppState>()
        .session
        .lock()
        .map(|s| s.is_locked())
        .unwrap_or(true);
    if !tray_state_changed(
        &mut last_tray_locked()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
        locked,
    ) {
        return;
    }
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some(if locked {
            "VaultTask (Locked)"
        } else {
            "VaultTask"
        }));
    }
}

/// Last lock state pushed to the tray. Process-wide: there is exactly one
/// `main-tray`, and the single-instance guard means one app per session.
fn last_tray_locked() -> &'static std::sync::Mutex<Option<bool>> {
    static LAST: std::sync::Mutex<Option<bool>> = std::sync::Mutex::new(None);
    &LAST
}

/// Pure change detector (unit-testable): records `locked`, returning true
/// only when it differs from the previously recorded state.
fn tray_state_changed(last: &mut Option<bool>, locked: bool) -> bool {
    if *last == Some(locked) {
        return false;
    }
    *last = Some(locked);
    true
}

// ---------------------------------------------------------------------------
// Tray icon (Linux: minimal static menu; Win/Mac: menu-less click-toggle)
// ---------------------------------------------------------------------------

/// Linux-only tray menu. Two platform facts force this split. First, the
/// GNOME AppIndicator extension only *shows* items whose `Menu` property is
/// a real dbusmenu path (`/NO_DBUSMENU` never reaches its `ready` gate, so
/// a menu-less item is silently invisible). Second, libappindicator never
/// delivers click events to the app (menu activation only), so the flyout
/// toggle must live in the menu.
/// Labels are static on purpose: the menu is built once at startup and never
/// rebuilt, preserving the push-only-on-change guarantee (rebuilding the
/// menu every watchdog tick caused the GNOME menu flash).
#[cfg(target_os = "linux")]
fn build_tray_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let toggle = MenuItem::with_id(
        app,
        "tray-toggle",
        "Show / Hide VaultTask",
        true,
        None::<&str>,
    )?;
    let check_updates = MenuItem::with_id(
        app,
        "tray-check-updates",
        "Check for Updates...",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "tray-quit", "Quit", true, None::<&str>)?;
    Menu::with_items(app, &[&toggle, &check_updates, &quit])
}

/// Dedicated tray asset (full-bleed tile, oversized glyph — the padded
/// app icon reads tiny on dark top bars). Compile-time embedded, so a
/// missing file breaks the build, never the installed app.
const TRAY_ICON_PNG: &[u8] = include_bytes!("../icons/tray-icon.png");

/// Tray icon: embedded asset first, default window icon as fallback.
/// Never a startup failure — worst case is the old (small) icon.
fn tray_icon<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<TauriImage<'_>> {
    if let Ok(img) = image::load_from_memory(TRAY_ICON_PNG) {
        let rgba = img.into_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        return Ok(TauriImage::new_owned(rgba.into_raw(), w, h));
    }
    Ok(app
        .default_window_icon()
        .ok_or_else(|| std::io::Error::other("no tray icon available"))?
        .clone())
}

/// Build the persistent tray icon: full-bleed asset, lock-state tooltip.
/// Linux attaches the minimal static menu (required for the GNOME
/// AppIndicator extension to show the item at all); Windows/macOS stay
/// menu-less so left-click toggles the flyout OneDrive-style.
/// Returns Err (logged by the caller) when no tray host exists.
pub fn create_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let icon = tray_icon(app)?;

    #[cfg(target_os = "linux")]
    let menu = build_tray_menu(app)?;

    let builder = TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("VaultTask (Locked)")
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
        });

    // Linux: menu activation is the only interaction libappindicator
    // delivers (clicks never reach on_tray_icon_event).
    #[cfg(target_os = "linux")]
    let builder = builder
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray-toggle" => {
                if let Some(w) = app.get_webview_window("main") {
                    toggle_visibility(app, &w);
                }
            }
            "tray-check-updates" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = show_popover(app, &w);
                }
                let _ = app.emit("updater://check", ());
            }
            "tray-quit" => app.exit(0),
            _ => {}
        });

    builder.build(app)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Global summon shortcut
// ---------------------------------------------------------------------------

/// Ctrl+Shift+Space summons the popover even without a tray host.
pub fn toggle_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space)
}

pub fn register_toggle_shortcut<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(toggle_shortcut(), |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed
                && let Some(w) = app.get_webview_window("main")
            {
                toggle_visibility(app, &w);
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

    #[test]
    fn tray_sync_pushes_only_on_state_change() {
        // First sync always pushes (cold cache); repeats are skipped so the
        // StatusNotifier menu isn't rebuilt every watchdog tick (GNOME flash).
        let mut last = None;
        assert!(tray_state_changed(&mut last, true));
        assert!(!tray_state_changed(&mut last, true));
        assert!(!tray_state_changed(&mut last, true));
        // Transitions push exactly once, then settle again.
        assert!(tray_state_changed(&mut last, false));
        assert!(!tray_state_changed(&mut last, false));
        assert!(tray_state_changed(&mut last, true));
    }

    #[test]
    fn embedded_tray_icon_decodes_to_expected_size() {
        let img = image::load_from_memory(TRAY_ICON_PNG).expect("tray PNG decodes");
        let rgba = img.into_rgba8();
        assert_eq!((rgba.width(), rgba.height()), (64, 64));
        // Sanity: mostly-opaque tile, not a blank/transparent image.
        let opaque = rgba.pixels().filter(|p| p[3] > 128).count() as f64;
        let total = rgba.pixels().len() as f64;
        assert!(opaque / total > 0.5, "tray icon unexpectedly transparent");
    }
}

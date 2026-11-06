//! Popover window position memory.
//!
//! The frameless popover is draggable (`data-tauri-drag-region` in
//! `CustomTitlebar.tsx`). This module persists its last physical position
//! to `window_state.json` next to the DB (plaintext: position is not
//! secret and must restore before unlock). Pure logic is unit-testable;
//! thin Tauri glue lives below the core, above the test module.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// State filename inside the app data dir.
pub const WINDOW_STATE_FILE: &str = "window_state.json";
/// Minimum ms between drag-move disk writes.
pub const SAVE_THROTTLE_MS: u128 = 500;
/// Sanity bound for stored coordinates (multi-monitor safe, absurd-proof).
const MAX_COORD_ABS: i32 = 20_000;

/// Last known popover top-left, physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedPosition {
    pub x: i32,
    pub y: i32,
}

/// Load the saved position. `None` = missing, corrupt, or absurd file:
/// the caller falls back to the tray anchor. Never errors.
pub fn load_window_position(path: &Path) -> Option<SavedPosition> {
    let bytes = std::fs::read(path).ok()?;
    let pos: SavedPosition = serde_json::from_slice(&bytes).ok()?;
    if pos.x.unsigned_abs() > MAX_COORD_ABS as u32 || pos.y.unsigned_abs() > MAX_COORD_ABS as u32 {
        return None;
    }
    Some(pos)
}

/// Persist the position. Errors are `String` (repo convention); callers
/// log-and-continue since a failed save must never break hiding/moving.
pub fn save_window_position(path: &Path, pos: &SavedPosition) -> Result<(), String> {
    let bytes = serde_json::to_vec(pos).map_err(|e| format!("encode window state: {e}"))?;
    std::fs::write(path, bytes).map_err(|e| format!("write window state: {e}"))
}

/// Throttle predicate (unit-testable): true when `now_ms` is at least
/// `SAVE_THROTTLE_MS` after `last_ms`. Clock skew (now < last) allows.
pub fn should_save_position(now_ms: u128, last_ms: u128) -> bool {
    if now_ms < last_ms {
        return true;
    }
    now_ms - last_ms >= SAVE_THROTTLE_MS
}

/// Only popover drags are remembered: maximized geometry is OS-driven
/// and must never overwrite the saved popover spot.
pub fn should_track_move(window_mode: &str) -> bool {
    window_mode == "popover"
}

/// True when `(x, y)` falls inside any work area. Each monitor is
/// `(work_x, work_y, work_w, work_h)` in physical pixels.
pub fn position_on_any_monitor(x: i32, y: i32, monitors: &[(i32, i32, i32, i32)]) -> bool {
    monitors.iter().any(|&(mx, my, mw, mh)| {
        (mx..mx.saturating_add(mw)).contains(&x) && (my..my.saturating_add(mh)).contains(&y)
    })
}

/// What `setup()` / `show_popover()` should do with the window position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionAction {
    /// Move to the saved spot (valid + on a visible monitor).
    Restore(SavedPosition),
    /// No usable save: use the tray anchor (`position_popover`).
    Anchor,
}

/// Pure restore-or-anchor decision shared by startup restore and summon.
pub fn decide_position_action(
    saved: Option<SavedPosition>,
    monitors: &[(i32, i32, i32, i32)],
) -> PositionAction {
    match saved {
        Some(p) if position_on_any_monitor(p.x, p.y, monitors) => PositionAction::Restore(p),
        _ => PositionAction::Anchor,
    }
}

// ---------------------------------------------------------------------------
// Thin Tauri glue (integration-verified via manual run + E2E; kept
// branch-free so there is nothing to unit-test that the core above
// does not already cover).
// ---------------------------------------------------------------------------

use tauri::{AppHandle, Manager, PhysicalPosition, Runtime, WebviewWindow};

/// `window_state.json` next to the vault DB (same `app_data_dir`).
pub fn state_file_path<R: Runtime>(app: &AppHandle<R>) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join(WINDOW_STATE_FILE)
}

/// Best-effort snapshot of the main window's current position.
/// Never panics, never surfaces errors (a failed save must not break
/// hiding or moving the window).
pub fn save_current_position<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    // INVARIANT: frameless window so inner == outer visually; outer_position()
    // includes compositor shadow extents (measured 74px on GNOME/XWayland)
    // while set_position targets the client surface — do NOT change back to
    // outer_position.
    let Ok(pos) = window.inner_position() else {
        return;
    };
    if pos.x == 0 && pos.y == 0 {
        // Wayland hides position (always (0,0)); on X11 (0,0) sits under
        // the top panel. Either way there is nothing worth persisting.
        return;
    }
    let saved = SavedPosition { x: pos.x, y: pos.y };
    if save_window_position(&state_file_path(app), &saved).is_ok() {
        eprintln!("[vault-task] saved window position {}x{}", saved.x, saved.y);
    }
}

/// Restore the saved spot when valid + on-screen, else tray-anchor.
/// Silent no-op on Wayland (compositor ignores client positioning).
pub fn restore_or_anchor<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let saved = load_window_position(&state_file_path(app));
    let monitors: Vec<(i32, i32, i32, i32)> = window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let p = m.work_area().position;
            let s = m.work_area().size;
            (p.x, p.y, s.width as i32, s.height as i32)
        })
        .collect();
    match decide_position_action(saved, &monitors) {
        PositionAction::Restore(p) => {
            let _ = window.set_position(tauri::Position::Physical(PhysicalPosition::new(p.x, p.y)));
            eprintln!(
                "[vault-task] requested window position restore to {}x{} (compositor may ignore; no-op on Wayland)",
                p.x, p.y
            );
        }
        PositionAction::Anchor => crate::tray::position_popover(window),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_file(tag: &str) -> std::path::PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("vaulttask-wstate-{tag}-{n}.json"))
    }

    #[test]
    fn save_then_load_roundtrip() {
        let p = tmp_file("roundtrip");
        save_window_position(&p, &SavedPosition { x: 1916, y: 12 }).unwrap();
        assert_eq!(
            load_window_position(&p),
            Some(SavedPosition { x: 1916, y: 12 })
        );
        std::fs::remove_file(&p).unwrap();
    }

    #[test]
    fn load_missing_file_returns_none() {
        let p = tmp_file("missing");
        assert_eq!(load_window_position(&p), None);
    }

    #[test]
    fn load_corrupt_file_returns_none() {
        let p = tmp_file("corrupt");
        std::fs::write(&p, b"{\"x\": 12,").unwrap();
        assert_eq!(load_window_position(&p), None);
        std::fs::remove_file(&p).unwrap();
    }

    #[test]
    fn load_absurd_coordinates_returns_none() {
        let p = tmp_file("absurd");
        std::fs::write(&p, b"{\"x\": 99999999, \"y\": 0}").unwrap();
        assert_eq!(load_window_position(&p), None);
        std::fs::remove_file(&p).unwrap();
    }

    #[test]
    fn load_i32_min_returns_none() {
        let p = tmp_file("i32min");
        std::fs::write(&p, b"{\"x\": -2147483648, \"y\": 0}").unwrap();
        assert_eq!(load_window_position(&p), None);
        std::fs::remove_file(&p).unwrap();
    }

    #[test]
    fn throttle_allows_one_save_per_500ms() {
        assert!(should_save_position(1000, 0));
        assert!(!should_save_position(1200, 1000));
        assert!(should_save_position(1500, 1000));
        assert!(should_save_position(1000, 5000)); // clock skew: allow
    }

    #[test]
    fn onscreen_point_detected() {
        let mons = [(0, 0, 1920, 1080)];
        assert!(position_on_any_monitor(100, 100, &mons));
        assert!(!position_on_any_monitor(2500, 100, &mons));
        assert!(!position_on_any_monitor(-50, 100, &mons));
        let dual = [(0, 0, 1920, 1080), (1920, 0, 1920, 1080)];
        assert!(position_on_any_monitor(2000, 500, &dual));
    }

    #[test]
    fn no_saved_position_anchors() {
        assert!(matches!(
            decide_position_action(None, &[(0, 0, 1920, 1080)]),
            PositionAction::Anchor
        ));
    }

    #[test]
    fn onscreen_saved_restores() {
        let action = decide_position_action(
            Some(SavedPosition { x: 100, y: 100 }),
            &[(0, 0, 1920, 1080)],
        );
        assert!(matches!(action, PositionAction::Restore(p) if p.x == 100 && p.y == 100));
    }

    #[test]
    fn offscreen_saved_falls_back_to_anchor() {
        // Unplugged second monitor: stale (2500, 100) must not strand the window.
        let action = decide_position_action(
            Some(SavedPosition { x: 2500, y: 100 }),
            &[(0, 0, 1920, 1080)],
        );
        assert!(matches!(action, PositionAction::Anchor));
    }

    #[test]
    fn no_monitors_anchors() {
        let action = decide_position_action(Some(SavedPosition { x: 100, y: 100 }), &[]);
        assert!(matches!(action, PositionAction::Anchor));
    }

    #[test]
    fn should_track_move_rejects_maximized() {
        assert!(should_track_move("popover"));
        assert!(!should_track_move("maximized"));
        assert!(!should_track_move(""));
    }
}

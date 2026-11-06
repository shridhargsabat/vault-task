//! Popover window position memory.
//!
//! The frameless popover is draggable (`data-tauri-drag-region` in
//! `CustomTitlebar.tsx`). This module persists its last physical position
//! to `window_state.json` next to the DB (plaintext: position is not
//! secret and must restore before unlock). Pure logic is unit-testable;
//! thin Tauri glue lives at the bottom of this file.

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
    if pos.x.abs() > MAX_COORD_ABS || pos.y.abs() > MAX_COORD_ABS {
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

/// True when `(x, y)` falls inside any work area. Each monitor is
/// `(work_x, work_y, work_w, work_h)` in physical pixels.
pub fn position_on_any_monitor(x: i32, y: i32, monitors: &[(i32, i32, i32, i32)]) -> bool {
    monitors
        .iter()
        .any(|&(mx, my, mw, mh)| x >= mx && y >= my && x < mx + mw && y < mh + my)
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
}

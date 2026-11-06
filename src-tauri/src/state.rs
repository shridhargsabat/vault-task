//! Vault session state: unlocked master key, activity timestamp, settings.
//!
//! `Instant` is monotonic and never serialized — the frontend only sees
//! the derived `locked` / `auto_lock_seconds` status.

use std::{path::PathBuf, sync::Mutex, time::Instant};

use crate::crypto::MasterKey;

/// Default auto-lock timeout (5 minutes).
pub const DEFAULT_AUTO_LOCK_SECONDS: u64 = 300;

/// In-memory session. The `MasterKey` is zeroized on lock/drop.
pub struct VaultSession {
    pub master_key: Option<MasterKey>,
    pub last_activity: Instant,
    pub auto_lock_seconds: u64,
    pub lock_on_tray_hide: bool,
    /// Window manager mode: "popover" | "maximized".
    pub window_mode: String,
    /// When true, the popover does not auto-hide on focus loss.
    pub is_pinned: bool,
    /// Last drag-move position save (throttle clock; never serialized).
    pub last_pos_save: Instant,
}

impl Default for VaultSession {
    fn default() -> Self {
        Self {
            master_key: None,
            last_activity: Instant::now(),
            auto_lock_seconds: DEFAULT_AUTO_LOCK_SECONDS,
            lock_on_tray_hide: false,
            window_mode: "popover".to_string(),
            is_pinned: false,
            last_pos_save: Instant::now(),
        }
    }
}

impl VaultSession {
    pub fn is_locked(&self) -> bool {
        self.master_key.is_none()
    }

    /// Zeroize + drop the key and mark locked.
    pub fn lock(&mut self) {
        self.master_key = None; // ZeroizeOnDrop wipes the old key
        self.last_activity = Instant::now();
    }

    pub fn unlock(&mut self, key: MasterKey) {
        self.master_key = Some(key);
        self.last_activity = Instant::now();
    }

    pub fn record_activity(&mut self) {
        self.last_activity = Instant::now();
    }
}

/// Shared Tauri state: session + resolved SQLite path.
pub struct AppState {
    pub session: Mutex<VaultSession>,
    pub db_path: Mutex<PathBuf>,
}

impl AppState {
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            session: Mutex::new(VaultSession::default()),
            db_path: Mutex::new(db_path),
        }
    }
}

//! Tauri IPC commands exposed to the React frontend.
//!
//! All task commands require an unlocked vault; the raw [`MasterKey`]
//! never leaves Rust memory.

use rusqlite::Connection;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::{
    crypto::{MasterKey, VERIFIER_PLAINTEXT, decrypt_bytes, derive_key, encrypt_bytes, generate_salt},
    db::{
        self, CreateTaskInput, Subtask, Task, UpdateTaskPatch, get_vault_meta, init_db,
        insert_vault_meta, update_vault_settings,
    },
    state::{AppState, DEFAULT_AUTO_LOCK_SECONDS},
    tray,
};

#[derive(Debug, Serialize)]
pub struct VaultStatus {
    pub initialized: bool,
    pub locked: bool,
    pub auto_lock_seconds: u64,
    pub lock_on_tray_hide: bool,
}

fn open_db(state: &State<AppState>) -> Result<Connection, String> {
    let path = state
        .db_path
        .lock()
        .map_err(|_| "db path lock poisoned".to_string())?
        .clone();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create data dir: {e}"))?;
    }
    let conn = Connection::open(&path).map_err(|e| format!("open db: {e}"))?;
    init_db(&conn)?;
    Ok(conn)
}

fn require_master(state: &State<AppState>) -> Result<MasterKey, String> {
    let mut session = state
        .session
        .lock()
        .map_err(|_| "session lock poisoned".to_string())?;
    let key = session
        .master_key
        .clone()
        .ok_or_else(|| "vault is locked".to_string())?;
    session.record_activity();
    Ok(key)
}

fn current_status(conn: &Connection, state: &State<AppState>) -> Result<VaultStatus, String> {
    let meta = get_vault_meta(conn)?;
    let session = state
        .session
        .lock()
        .map_err(|_| "session lock poisoned".to_string())?;
    match meta {
        Some(m) => Ok(VaultStatus {
            initialized: true,
            locked: session.is_locked(),
            auto_lock_seconds: m.auto_lock_seconds,
            lock_on_tray_hide: m.lock_on_tray_hide,
        }),
        None => Ok(VaultStatus {
            initialized: false,
            locked: true,
            auto_lock_seconds: session.auto_lock_seconds,
            lock_on_tray_hide: session.lock_on_tray_hide,
        }),
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_vault_status(state: State<AppState>) -> Result<VaultStatus, String> {
    let conn = open_db(&state)?;
    current_status(&conn, &state)
}

#[tauri::command(rename_all = "snake_case")]
pub fn setup_vault(state: State<AppState>, password: String) -> Result<VaultStatus, String> {
    if password.len() < 4 {
        return Err("PIN/passphrase must be at least 4 characters".to_string());
    }
    let conn = open_db(&state)?;
    if get_vault_meta(&conn)?.is_some() {
        return Err("vault is already initialized".to_string());
    }
    let salt = generate_salt();
    let key = derive_key(&password, &salt)?;
    let (verifier_ciphertext, verifier_nonce) = encrypt_bytes(&key, VERIFIER_PLAINTEXT)?;
    insert_vault_meta(
        &conn,
        &salt,
        &verifier_nonce,
        &verifier_ciphertext,
        DEFAULT_AUTO_LOCK_SECONDS,
        false,
    )?;
    {
        let mut session = state
            .session
            .lock()
            .map_err(|_| "session lock poisoned".to_string())?;
        session.auto_lock_seconds = DEFAULT_AUTO_LOCK_SECONDS;
        session.lock_on_tray_hide = false;
        session.unlock(MasterKey(key));
    }
    current_status(&conn, &state)
}

#[tauri::command(rename_all = "snake_case")]
pub fn unlock_vault(state: State<AppState>, password: String) -> Result<VaultStatus, String> {
    let conn = open_db(&state)?;
    let meta = get_vault_meta(&conn)?.ok_or_else(|| "vault is not initialized".to_string())?;
    let key = derive_key(&password, &meta.kdf_salt).map_err(|_| "incorrect PIN/password".to_string())?;
    let mut nonce = [0u8; 12];
    if meta.verifier_nonce.len() != 12 {
        return Err("vault metadata is corrupted".to_string());
    }
    nonce.copy_from_slice(&meta.verifier_nonce);
    let plain = decrypt_bytes(&key, &meta.verifier_ciphertext, &nonce)
        .map_err(|_| "incorrect PIN/password".to_string())?;
    if plain.as_slice() != VERIFIER_PLAINTEXT {
        return Err("incorrect PIN/password".to_string());
    }
    {
        let mut session = state
            .session
            .lock()
            .map_err(|_| "session lock poisoned".to_string())?;
        session.auto_lock_seconds = meta.auto_lock_seconds;
        session.lock_on_tray_hide = meta.lock_on_tray_hide;
        session.unlock(MasterKey(key));
    }
    current_status(&conn, &state)
}

#[tauri::command(rename_all = "snake_case")]
pub fn lock_vault(state: State<AppState>) -> Result<VaultStatus, String> {
    {
        let mut session = state
            .session
            .lock()
            .map_err(|_| "session lock poisoned".to_string())?;
        session.lock();
    }
    let conn = open_db(&state)?;
    current_status(&conn, &state)
}

#[tauri::command(rename_all = "snake_case")]
pub fn update_security_settings(
    state: State<AppState>,
    auto_lock_seconds: u64,
    lock_on_tray_hide: bool,
) -> Result<VaultStatus, String> {
    if !(30..=3600).contains(&auto_lock_seconds) {
        return Err("auto-lock timeout must be between 30s and 3600s".to_string());
    }
    // Changing security posture requires an unlocked vault.
    require_master(&state)?;
    let conn = open_db(&state)?;
    update_vault_settings(&conn, auto_lock_seconds, lock_on_tray_hide)?;
    {
        let mut session = state
            .session
            .lock()
            .map_err(|_| "session lock poisoned".to_string())?;
        session.auto_lock_seconds = auto_lock_seconds;
        session.lock_on_tray_hide = lock_on_tray_hide;
    }
    current_status(&conn, &state)
}

#[tauri::command(rename_all = "snake_case")]
pub fn record_activity_heartbeat(state: State<AppState>) -> Result<(), String> {
    let mut session = state
        .session
        .lock()
        .map_err(|_| "session lock poisoned".to_string())?;
    session.record_activity();
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_tasks(state: State<AppState>) -> Result<Vec<Task>, String> {
    let master = require_master(&state)?;
    let conn = open_db(&state)?;
    db::list_tasks(&conn, &master)
}

#[tauri::command(rename_all = "snake_case")]
#[allow(clippy::too_many_arguments)]
pub fn create_task(
    state: State<AppState>,
    title: String,
    note_markdown: Option<String>,
    status: Option<String>,
    priority: Option<i32>,
    folder_id: Option<String>,
    tags: Option<Vec<String>>,
    due_date: Option<String>,
) -> Result<Task, String> {
    let master = require_master(&state)?;
    let conn = open_db(&state)?;
    db::create_task(
        &conn,
        &master,
        CreateTaskInput {
            title,
            note_markdown,
            status,
            priority,
            folder_id,
            tags,
            due_date,
        },
    )
}

#[tauri::command(rename_all = "snake_case")]
#[allow(clippy::too_many_arguments)]
pub fn update_task(
    state: State<AppState>,
    id: String,
    title: Option<String>,
    note_markdown: Option<String>,
    subtasks: Option<Vec<Subtask>>,
    status: Option<String>,
    priority: Option<i32>,
    is_pinned: Option<bool>,
    folder_id: Option<String>,
    tags: Option<Vec<String>>,
    due_date: Option<String>,
) -> Result<Task, String> {
    let master = require_master(&state)?;
    let conn = open_db(&state)?;
    db::update_task(
        &conn,
        &master,
        &id,
        UpdateTaskPatch {
            title,
            note_markdown,
            subtasks,
            status,
            priority,
            is_pinned,
            folder_id,
            tags,
            due_date,
        },
    )
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_task(state: State<AppState>, id: String) -> Result<bool, String> {
    require_master(&state)?;
    let conn = open_db(&state)?;
    db::delete_task(&conn, &id)
}

// ---------------------------------------------------------------------------
// Window manager commands (Phase 2)
// ---------------------------------------------------------------------------

fn main_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    app.get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())
}

/// Switch to `popover` (420x620 tray flyout) or `maximized` (workspace).
/// Returns the new mode.
#[tauri::command(rename_all = "snake_case")]
pub fn set_window_mode(app: AppHandle, mode: String) -> Result<String, String> {
    let window = main_window(&app)?;
    match mode.as_str() {
        "popover" => tray::show_popover(&app, &window)?,
        "maximized" => tray::show_maximized(&app, &window)?,
        other => return Err(format!("invalid window mode '{other}': expected popover|maximized")),
    }
    Ok(mode)
}

/// Toggle between popover and maximized. Returns the new mode.
#[tauri::command(rename_all = "snake_case")]
pub fn toggle_maximize_mode(app: AppHandle) -> Result<String, String> {
    let current = app
        .state::<AppState>()
        .session
        .lock()
        .map(|s| s.window_mode.clone())
        .unwrap_or_else(|_| "popover".to_string());
    let next = if current == "maximized" { "popover" } else { "maximized" };
    let window = main_window(&app)?;
    if next == "maximized" {
        tray::show_maximized(&app, &window)?;
    } else {
        tray::show_popover(&app, &window)?;
    }
    Ok(next.to_string())
}

/// Hide the window to the tray (close-to-tray button).
#[tauri::command(rename_all = "snake_case")]
pub fn hide_to_tray(app: AppHandle) -> Result<(), String> {
    main_window(&app)?
        .hide()
        .map_err(|e| format!("hide window: {e}"))
}

/// Pin the popover so it does not auto-hide on focus loss.
#[tauri::command(rename_all = "snake_case")]
pub fn set_pinned(app: AppHandle, pinned: bool) -> Result<bool, String> {
    let state = app.state::<AppState>();
    let mut session = state
        .session
        .lock()
        .map_err(|_| "session lock poisoned".to_string())?;
    session.is_pinned = pinned;
    Ok(pinned)
}

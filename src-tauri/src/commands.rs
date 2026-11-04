//! Tauri IPC commands exposed to the React frontend.
//!
//! All task commands require an unlocked vault; the raw [`MasterKey`]
//! never leaves Rust memory.

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use rusqlite::Connection;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::{
    crypto::{
        MasterKey, VERIFIER_PLAINTEXT, decrypt_bytes, derive_key, encrypt_bytes, generate_salt,
        verify_key_bytes,
    },
    db::{
        self, BackupStats, CreateTaskInput, ImportStats, Subtask, Task, UpdateTaskPatch,
        build_backup_payload, get_vault_meta, init_db, insert_vault_meta, parse_backup_bundle,
        render_task_markdown, restore_backup_payload, seal_backup_bundle, serialize_bundle,
        set_keyring_enabled, slugify_filename, update_vault_settings, verify_backup_password,
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
    pub keyring_enabled: bool,
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
            keyring_enabled: m.keyring_enabled,
        }),
        None => Ok(VaultStatus {
            initialized: false,
            locked: true,
            auto_lock_seconds: session.auto_lock_seconds,
            lock_on_tray_hide: session.lock_on_tray_hide,
            keyring_enabled: false,
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
    let key =
        derive_key(&password, &meta.kdf_salt).map_err(|_| "incorrect PIN/password".to_string())?;
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

/// Change the master PIN/passphrase: proves the current one, re-encrypts
/// every task + the verifier under a fresh salt+key (single transaction),
/// then swaps the in-memory session key. Requires an unlocked vault.
#[tauri::command(rename_all = "snake_case")]
pub fn change_vault_password(
    state: State<AppState>,
    old_password: String,
    new_password: String,
) -> Result<VaultStatus, String> {
    if new_password.len() < 4 {
        return Err("new PIN/passphrase must be at least 4 characters".to_string());
    }
    // Requires an unlocked vault (also refreshes activity so a rekey never
    // races the watchdog mid-operation).
    let old_master = require_master(&state)?;
    let mut conn = open_db(&state)?;
    let meta = get_vault_meta(&conn)?.ok_or_else(|| "vault is not initialized".to_string())?;
    // Prove knowledge of the CURRENT password (not just session possession).
    let check = MasterKey(
        derive_key(&old_password, &meta.kdf_salt)
            .map_err(|_| "current PIN/password is incorrect".to_string())?,
    );
    let mut nonce = [0u8; 12];
    if meta.verifier_nonce.len() != 12 {
        return Err("vault metadata is corrupted".to_string());
    }
    nonce.copy_from_slice(&meta.verifier_nonce);
    let plain = decrypt_bytes(check.as_bytes(), &meta.verifier_ciphertext, &nonce)
        .map_err(|_| "current PIN/password is incorrect".to_string())?;
    if plain.as_slice() != VERIFIER_PLAINTEXT {
        return Err("current PIN/password is incorrect".to_string());
    }
    let new_salt = generate_salt();
    let new_key = derive_key(&new_password, &new_salt)?;
    db::rekey_vault(&mut conn, &old_master, &new_key, &new_salt)?;
    {
        let mut session = state
            .session
            .lock()
            .map_err(|_| "session lock poisoned".to_string())?;
        session.unlock(MasterKey(new_key));
    }
    current_status(&conn, &state)
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
        other => {
            return Err(format!(
                "invalid window mode '{other}': expected popover|maximized"
            ));
        }
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
    let next = if current == "maximized" {
        "popover"
    } else {
        "maximized"
    };
    let window = main_window(&app)?;
    if next == "maximized" {
        tray::show_maximized(&app, &window)?;
    } else {
        tray::show_popover(&app, &window)?;
    }
    Ok(next.to_string())
}

/// Hide the window to the tray (close-to-tray button). Locks too when
/// `lock_on_tray_hide` is enabled.
#[tauri::command(rename_all = "snake_case")]
pub fn hide_to_tray(app: AppHandle) -> Result<(), String> {
    tray::hide_main_window(&app);
    Ok(())
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

// ---------------------------------------------------------------------------
// Backup & export commands (Phase 6)
// ---------------------------------------------------------------------------

/// Export the vault as a signed `.vault` bundle (ciphertext only, SHA-256
/// integrity). Requires an unlocked vault.
#[tauri::command(rename_all = "snake_case")]
pub fn export_encrypted_backup(
    state: State<AppState>,
    destination_path: String,
) -> Result<BackupStats, String> {
    require_master(&state)?;
    let conn = open_db(&state)?;
    let payload = build_backup_payload(&conn)?;
    let task_count = payload.tasks.len();
    let bundle = seal_backup_bundle(&payload)?;
    let text = serialize_bundle(&bundle)?;
    let file_bytes = text.len() as u64;
    std::fs::write(&destination_path, text).map_err(|e| format!("write backup: {e}"))?;
    Ok(BackupStats {
        task_count,
        file_bytes,
    })
}

/// Restore a `.vault` bundle: checksum first, then password proof against
/// the backup's own salt+verifier, then an atomic replace. No session
/// needed (fresh-machine restores); the session is locked afterwards
/// because the DB content changed under any old key.
#[tauri::command(rename_all = "snake_case")]
pub fn import_encrypted_backup(
    state: State<AppState>,
    source_path: String,
    master_password: String,
) -> Result<ImportStats, String> {
    let meta = std::fs::metadata(&source_path).map_err(|e| format!("read backup: {e}"))?;
    if meta.len() > db::MAX_BACKUP_BYTES {
        return Err("backup file is too large".to_string());
    }
    let text = std::fs::read_to_string(&source_path).map_err(|e| format!("read backup: {e}"))?;
    let payload = parse_backup_bundle(&text)?;
    verify_backup_password(&payload, &master_password)?;
    let mut conn = open_db(&state)?;
    let task_count = restore_backup_payload(&mut conn, &payload)?;
    // The DB changed under any old key: drop it + mirror the new settings.
    {
        let mut session = state
            .session
            .lock()
            .map_err(|_| "session lock poisoned".to_string())?;
        session.lock();
        session.auto_lock_seconds = payload.meta.auto_lock_seconds;
        session.lock_on_tray_hide = payload.meta.lock_on_tray_hide;
    }
    Ok(ImportStats { task_count })
}

/// Export every task as `<slug>-<id8>.md` with YAML frontmatter.
/// Unlocked session required — this writes DECRYPTED data.
#[tauri::command(rename_all = "snake_case")]
pub fn export_decrypted_markdown(
    state: State<AppState>,
    destination_dir: String,
) -> Result<usize, String> {
    let master = require_master(&state)?;
    let conn = open_db(&state)?;
    let tasks = db::list_tasks(&conn, &master)?;
    let dir = std::path::Path::new(&destination_dir);
    std::fs::create_dir_all(dir).map_err(|e| format!("create export dir: {e}"))?;
    for task in &tasks {
        let name = slugify_filename(&task.title, &task.id);
        let md = render_task_markdown(task);
        std::fs::write(dir.join(name), md).map_err(|e| format!("write markdown: {e}"))?;
    }
    Ok(tasks.len())
}

// ---------------------------------------------------------------------------
// OS keyring quick-unlock (Phase 7)
// ---------------------------------------------------------------------------

/// Service/account under which the derived key lives in the platform store
/// (Windows Credential Manager / Secret Service / KWallet).
pub const KEYRING_SERVICE: &str = "com.vaulttask.desktop";
pub const KEYRING_ACCOUNT: &str = "master_key";

fn keyring_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| format!("OS keyring unavailable: {e}"))
}

fn keyring_store(entry: &keyring::Entry, key: &[u8; 32]) -> Result<(), String> {
    entry
        .set_password(&B64.encode(key))
        .map_err(|e| format!("could not save to OS keyring: {e}"))
}

/// Load the stored key. `Ok(None)` = no credential saved (or daemon has
/// none); hard failures (locked daemon, corruption) are `Err` with a
/// user-facing message. Never panics.
fn keyring_load(entry: &keyring::Entry) -> Result<Option<[u8; 32]>, String> {
    match entry.get_password() {
        Ok(b64) => {
            let raw = B64.decode(b64.trim()).map_err(|_| corrupt_key_msg())?;
            if raw.len() != 32 {
                return Err(corrupt_key_msg());
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&raw);
            Ok(Some(key))
        }
        // Error is #[non_exhaustive]; the `Err(e)` arm below is the wildcard.
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("OS keyring unavailable: {e}")),
    }
}

fn corrupt_key_msg() -> String {
    "stored OS keyring credential is corrupted (disable + re-enable quick-unlock)".to_string()
}

fn keyring_delete(entry: &keyring::Entry) -> Result<(), String> {
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("could not remove OS keyring credential: {e}")),
    }
}

/// Enable/disable quick-unlock. Enabling stores the session key in the OS
/// keyring and records the flag; disabling deletes the credential.
/// Requires an unlocked vault either way.
#[tauri::command(rename_all = "snake_case")]
pub fn enable_keyring_unlock(state: State<AppState>, enable: bool) -> Result<(), String> {
    if enable {
        let master = require_master(&state)?;
        let entry = keyring_entry()?;
        keyring_store(&entry, master.as_bytes())?;
        let conn = open_db(&state)?;
        set_keyring_enabled(&conn, true)?;
    } else {
        require_master(&state)?;
        let entry = keyring_entry()?;
        // Best effort: still clear the flag even if the credential is
        // already gone (delete tolerates NoEntry).
        keyring_delete(&entry)?;
        let conn = open_db(&state)?;
        set_keyring_enabled(&conn, false)?;
    }
    Ok(())
}

/// Unlock from the OS keyring without the passphrase. `Ok(true)` = vault
/// unlocked; `Ok(false)` = no credential stored (fall back to PIN);
/// `Err` = daemon unavailable or stored key invalid.
#[tauri::command(rename_all = "snake_case")]
pub fn unlock_with_keyring(state: State<AppState>) -> Result<bool, String> {
    let conn = open_db(&state)?;
    let meta = get_vault_meta(&conn)?.ok_or_else(|| "vault is not initialized".to_string())?;
    let entry = keyring_entry()?;
    let Some(key) = keyring_load(&entry)? else {
        return Ok(false);
    };
    verify_key_bytes(&key, &meta.verifier_nonce, &meta.verifier_ciphertext)?;
    {
        let mut session = state
            .session
            .lock()
            .map_err(|_| "session lock poisoned".to_string())?;
        session.unlock(MasterKey(key));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_entry() -> keyring::Entry {
        keyring::Entry::new_with_credential(Box::new(keyring::mock::MockCredential::default()))
    }

    #[test]
    fn keyring_store_load_roundtrip() {
        let entry = mock_entry();
        let key = [7u8; 32];
        keyring_store(&entry, &key).unwrap();
        assert_eq!(keyring_load(&entry).unwrap(), Some(key));
    }

    #[test]
    fn keyring_load_missing_returns_none() {
        assert_eq!(keyring_load(&mock_entry()).unwrap(), None);
    }

    #[test]
    fn keyring_load_corrupted_is_err() {
        let entry = mock_entry();
        entry.set_password("!!!not-base64!!!").unwrap();
        let err = keyring_load(&entry).unwrap_err();
        assert!(err.contains("corrupted"), "got: {err}");
        entry.set_password(&B64.encode([1u8; 16])).unwrap();
        let err = keyring_load(&entry).unwrap_err();
        assert!(err.contains("corrupted"), "got: {err}");
    }

    #[test]
    fn keyring_delete_missing_is_ok() {
        keyring_delete(&mock_entry()).unwrap();
        let entry = mock_entry();
        keyring_store(&entry, &[9u8; 32]).unwrap();
        keyring_delete(&entry).unwrap();
        assert_eq!(keyring_load(&entry).unwrap(), None);
    }

    #[test]
    fn keyring_daemon_error_is_friendly() {
        let mock = keyring::mock::MockCredential::default();
        mock.set_error(keyring::Error::PlatformFailure(Box::new(
            std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "d-bus secret service unreachable",
            ),
        )));
        let entry = keyring::Entry::new_with_credential(Box::new(mock));
        let err = keyring_load(&entry).unwrap_err();
        assert!(err.contains("OS keyring unavailable"), "got: {err}");
    }

    #[test]
    fn verify_key_bytes_accepts_right_key_rejects_wrong() {
        let salt = [31u8; 16];
        let key = derive_key("keyring-probe-pw", &salt).unwrap();
        let (ct, nonce) = encrypt_bytes(&key, VERIFIER_PLAINTEXT).unwrap();
        verify_key_bytes(&key, &nonce, &ct).unwrap();
        let mut wrong = key;
        wrong[0] ^= 0xff;
        let err = verify_key_bytes(&wrong, &nonce, &ct).unwrap_err();
        assert!(err.contains("invalid"), "got: {err}");
    }
}

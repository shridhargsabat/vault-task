//! VaultTask encrypted SQLite store.
//!
//! Sensitive columns (`title`, `note_markdown`, `subtasks`) are AES-256-GCM
//! ciphertext blobs; index columns (`status`, `priority`, `due_date`, …)
//! stay plaintext for fast sorting after unlock.

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::crypto::{MasterKey, VERIFIER_PLAINTEXT, decrypt_bytes, derive_key, encrypt_bytes};

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS vault_meta (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    kdf_salt BLOB NOT NULL,
    verifier_nonce BLOB NOT NULL,
    verifier_ciphertext BLOB NOT NULL,
    auto_lock_seconds INTEGER NOT NULL DEFAULT 300,
    lock_on_tray_hide INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    encrypted_title BLOB NOT NULL,
    title_nonce BLOB NOT NULL,
    encrypted_note BLOB NOT NULL,
    note_nonce BLOB NOT NULL,
    encrypted_subtasks BLOB NOT NULL,
    subtasks_nonce BLOB NOT NULL,
    status TEXT NOT NULL DEFAULT 'todo',
    priority INTEGER NOT NULL DEFAULT 2,
    is_pinned INTEGER NOT NULL DEFAULT 0,
    folder_id TEXT DEFAULT 'inbox',
    tags_json TEXT NOT NULL DEFAULT '[]',
    due_date TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tasks_status_updated ON tasks(status, updated_at DESC);
"#;

/// Run migrations / create tables if missing.
pub fn init_db(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(SCHEMA)
        .map_err(|e| format!("db init: {e}"))
}

// ---------------------------------------------------------------------------
// Vault meta
// ---------------------------------------------------------------------------

pub struct VaultMeta {
    pub kdf_salt: Vec<u8>,
    pub verifier_nonce: Vec<u8>,
    pub verifier_ciphertext: Vec<u8>,
    pub auto_lock_seconds: u64,
    pub lock_on_tray_hide: bool,
}

/// Raw `vault_meta` row: (kdf_salt, verifier_nonce, verifier_ciphertext,
/// auto_lock_seconds, lock_on_tray_hide-as-int).
type VaultMetaRow = (Vec<u8>, Vec<u8>, Vec<u8>, i64, i64);
/// Raw full `tasks` row (id, 6 encrypted blobs + nonces, status, priority,
/// is_pinned, folder_id, tags_json, due_date, created_at, updated_at).
type TaskRow = (
    String,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    String,
    i32,
    i64,
    Option<String>,
    String,
    Option<String>,
    String,
    String,
);
/// Raw rekey scan row: (id, 6 encrypted blobs + nonces).
type RekeyRow = (String, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);

pub fn get_vault_meta(conn: &Connection) -> Result<Option<VaultMeta>, String> {
    let row: Option<VaultMetaRow> = conn
        .query_row(
            "SELECT kdf_salt, verifier_nonce, verifier_ciphertext, auto_lock_seconds, lock_on_tray_hide FROM vault_meta WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()
        .map_err(|e| format!("db read vault_meta: {e}"))?;
    Ok(row.map(
        |(kdf_salt, verifier_nonce, verifier_ciphertext, auto_lock_seconds, lock_on_tray_hide)| {
            VaultMeta {
                kdf_salt,
                verifier_nonce,
                verifier_ciphertext,
                auto_lock_seconds: auto_lock_seconds.max(0) as u64,
                lock_on_tray_hide: lock_on_tray_hide != 0,
            }
        },
    ))
}

pub fn insert_vault_meta(
    conn: &Connection,
    kdf_salt: &[u8],
    verifier_nonce: &[u8; 12],
    verifier_ciphertext: &[u8],
    auto_lock_seconds: u64,
    lock_on_tray_hide: bool,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO vault_meta (id, kdf_salt, verifier_nonce, verifier_ciphertext, auto_lock_seconds, lock_on_tray_hide, created_at) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            kdf_salt,
            verifier_nonce.as_slice(),
            verifier_ciphertext,
            auto_lock_seconds as i64,
            i64::from(lock_on_tray_hide),
            now,
        ],
    )
    .map_err(|e| format!("db insert vault_meta: {e}"))?;
    Ok(())
}

pub fn update_vault_settings(
    conn: &Connection,
    auto_lock_seconds: u64,
    lock_on_tray_hide: bool,
) -> Result<(), String> {
    let n = conn
        .execute(
            "UPDATE vault_meta SET auto_lock_seconds = ?1, lock_on_tray_hide = ?2 WHERE id = 1",
            params![auto_lock_seconds as i64, i64::from(lock_on_tray_hide)],
        )
        .map_err(|e| format!("db update settings: {e}"))?;
    if n == 0 {
        return Err("vault is not initialized".to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Subtask {
    pub id: String,
    pub title: String,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub note_markdown: String,
    pub subtasks: Vec<Subtask>,
    pub status: String,
    pub priority: i32,
    pub is_pinned: bool,
    pub folder_id: String,
    pub tags: Vec<String>,
    pub due_date: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskInput {
    pub title: String,
    pub note_markdown: Option<String>,
    pub status: Option<String>,
    pub priority: Option<i32>,
    pub folder_id: Option<String>,
    pub tags: Option<Vec<String>>,
    pub due_date: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateTaskPatch {
    pub title: Option<String>,
    pub note_markdown: Option<String>,
    pub subtasks: Option<Vec<Subtask>>,
    pub status: Option<String>,
    pub priority: Option<i32>,
    pub is_pinned: Option<bool>,
    pub folder_id: Option<String>,
    pub tags: Option<Vec<String>>,
    /// None = leave unchanged; Some("") = clear; Some(date) = set.
    pub due_date: Option<String>,
}

fn validate_status(status: &str) -> Result<(), String> {
    match status {
        "todo" | "in_progress" | "done" => Ok(()),
        other => Err(format!(
            "invalid status '{other}': expected todo|in_progress|done"
        )),
    }
}

fn validate_priority(priority: i32) -> Result<(), String> {
    if (1..=4).contains(&priority) {
        Ok(())
    } else {
        Err(format!("invalid priority {priority}: expected 1..=4"))
    }
}

fn nonce_from_slice(bytes: &[u8]) -> Result<[u8; 12], String> {
    if bytes.len() != 12 {
        return Err("corrupted nonce length".to_string());
    }
    let mut out = [0u8; 12];
    out.copy_from_slice(bytes);
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn decrypt_task_row(
    key: &[u8; 32],
    id: String,
    enc_title: Vec<u8>,
    title_nonce: Vec<u8>,
    enc_note: Vec<u8>,
    note_nonce: Vec<u8>,
    enc_subtasks: Vec<u8>,
    subtasks_nonce: Vec<u8>,
    status: String,
    priority: i32,
    is_pinned: i64,
    folder_id: Option<String>,
    tags_json: String,
    due_date: Option<String>,
    created_at: String,
    updated_at: String,
) -> Result<Task, String> {
    let title_bytes = decrypt_bytes(key, &enc_title, &nonce_from_slice(&title_nonce)?)?;
    let note_bytes = decrypt_bytes(key, &enc_note, &nonce_from_slice(&note_nonce)?)?;
    let subtasks_bytes = decrypt_bytes(key, &enc_subtasks, &nonce_from_slice(&subtasks_nonce)?)?;
    let title = String::from_utf8(title_bytes).map_err(|_| "corrupted title utf-8".to_string())?;
    let note_markdown =
        String::from_utf8(note_bytes).map_err(|_| "corrupted note utf-8".to_string())?;
    let subtasks: Vec<Subtask> = serde_json::from_slice(&subtasks_bytes)
        .map_err(|_| "corrupted subtasks json".to_string())?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
    Ok(Task {
        id,
        title,
        note_markdown,
        subtasks,
        status,
        priority,
        is_pinned: is_pinned != 0,
        folder_id: folder_id.unwrap_or_else(|| "inbox".to_string()),
        tags,
        due_date,
        created_at,
        updated_at,
    })
}

pub fn list_tasks(conn: &Connection, master: &MasterKey) -> Result<Vec<Task>, String> {
    let key = master.as_bytes();
    let mut stmt = conn
        .prepare(
            "SELECT id, encrypted_title, title_nonce, encrypted_note, note_nonce, encrypted_subtasks, subtasks_nonce, status, priority, is_pinned, folder_id, tags_json, due_date, created_at, updated_at FROM tasks ORDER BY is_pinned DESC, updated_at DESC",
        )
        .map_err(|e| format!("db prepare list: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Vec<u8>>(2)?,
                r.get::<_, Vec<u8>>(3)?,
                r.get::<_, Vec<u8>>(4)?,
                r.get::<_, Vec<u8>>(5)?,
                r.get::<_, Vec<u8>>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, i32>(8)?,
                r.get::<_, i64>(9)?,
                r.get::<_, Option<String>>(10)?,
                r.get::<_, String>(11)?,
                r.get::<_, Option<String>>(12)?,
                r.get::<_, String>(13)?,
                r.get::<_, String>(14)?,
            ))
        })
        .map_err(|e| format!("db list tasks: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        let (
            id,
            enc_title,
            title_nonce,
            enc_note,
            note_nonce,
            enc_subtasks,
            subtasks_nonce,
            status,
            priority,
            is_pinned,
            folder_id,
            tags_json,
            due_date,
            created_at,
            updated_at,
        ) = row.map_err(|e| format!("db row: {e}"))?;
        out.push(decrypt_task_row(
            key,
            id,
            enc_title,
            title_nonce,
            enc_note,
            note_nonce,
            enc_subtasks,
            subtasks_nonce,
            status,
            priority,
            is_pinned,
            folder_id,
            tags_json,
            due_date,
            created_at,
            updated_at,
        )?);
    }
    Ok(out)
}

pub fn get_task(conn: &Connection, master: &MasterKey, id: &str) -> Result<Option<Task>, String> {
    let row: Option<TaskRow> = conn
        .query_row(
            "SELECT id, encrypted_title, title_nonce, encrypted_note, note_nonce, encrypted_subtasks, subtasks_nonce, status, priority, is_pinned, folder_id, tags_json, due_date, created_at, updated_at FROM tasks WHERE id = ?1",
            params![id],
            |r| {
                Ok((
                    r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?,
                    r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?,
                    r.get(10)?, r.get(11)?, r.get(12)?, r.get(13)?, r.get(14)?,
                ))
            },
        )
        .optional()
        .map_err(|e| format!("db get task: {e}"))?;
    row.map(
        |(
            id,
            enc_title,
            title_nonce,
            enc_note,
            note_nonce,
            enc_subtasks,
            subtasks_nonce,
            status,
            priority,
            is_pinned,
            folder_id,
            tags_json,
            due_date,
            created_at,
            updated_at,
        )| {
            decrypt_task_row(
                master.as_bytes(),
                id,
                enc_title,
                title_nonce,
                enc_note,
                note_nonce,
                enc_subtasks,
                subtasks_nonce,
                status,
                priority,
                is_pinned,
                folder_id,
                tags_json,
                due_date,
                created_at,
                updated_at,
            )
        },
    )
    .transpose()
}

pub fn create_task(
    conn: &Connection,
    master: &MasterKey,
    input: CreateTaskInput,
) -> Result<Task, String> {
    let title = input.title.trim().to_string();
    if title.is_empty() {
        return Err("title must not be empty".to_string());
    }
    let status = input.status.unwrap_or_else(|| "todo".to_string());
    validate_status(&status)?;
    let priority = input.priority.unwrap_or(2);
    validate_priority(priority)?;
    let note = input.note_markdown.unwrap_or_default();
    let folder_id = input.folder_id.unwrap_or_else(|| "inbox".to_string());
    let tags = input.tags.unwrap_or_default();
    let due_date = input.due_date.filter(|s| !s.is_empty());

    let key = master.as_bytes();
    let (enc_title, title_nonce) = encrypt_bytes(key, title.as_bytes())?;
    let (enc_note, note_nonce) = encrypt_bytes(key, note.as_bytes())?;
    let subtasks_json =
        serde_json::to_vec::<Vec<Subtask>>(&vec![]).map_err(|e| format!("subtasks json: {e}"))?;
    let (enc_subtasks, subtasks_nonce) = encrypt_bytes(key, &subtasks_json)?;
    let tags_json = serde_json::to_string(&tags).map_err(|e| format!("tags json: {e}"))?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO tasks (id, encrypted_title, title_nonce, encrypted_note, note_nonce, encrypted_subtasks, subtasks_nonce, status, priority, is_pinned, folder_id, tags_json, due_date, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10, ?11, ?12, ?13, ?14)",
        params![
            id,
            enc_title,
            title_nonce.as_slice(),
            enc_note,
            note_nonce.as_slice(),
            enc_subtasks,
            subtasks_nonce.as_slice(),
            status,
            priority,
            folder_id,
            tags_json,
            due_date,
            now,
            now,
        ],
    )
    .map_err(|e| format!("db insert task: {e}"))?;

    Ok(Task {
        id,
        title,
        note_markdown: note,
        subtasks: vec![],
        status,
        priority,
        is_pinned: false,
        folder_id,
        tags,
        due_date,
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn update_task(
    conn: &Connection,
    master: &MasterKey,
    id: &str,
    patch: UpdateTaskPatch,
) -> Result<Task, String> {
    let existing = get_task(conn, master, id)?.ok_or_else(|| format!("task '{id}' not found"))?;

    let title = match patch.title {
        Some(t) => {
            let t = t.trim().to_string();
            if t.is_empty() {
                return Err("title must not be empty".to_string());
            }
            t
        }
        None => existing.title.clone(),
    };
    let note_markdown = patch
        .note_markdown
        .unwrap_or(existing.note_markdown.clone());
    let subtasks = patch.subtasks.unwrap_or(existing.subtasks.clone());
    let status = match patch.status {
        Some(s) => {
            validate_status(&s)?;
            s
        }
        None => existing.status.clone(),
    };
    let priority = match patch.priority {
        Some(p) => {
            validate_priority(p)?;
            p
        }
        None => existing.priority,
    };
    let is_pinned = patch.is_pinned.unwrap_or(existing.is_pinned);
    let folder_id = patch.folder_id.unwrap_or(existing.folder_id.clone());
    let tags = patch.tags.unwrap_or(existing.tags.clone());
    let due_date = match patch.due_date {
        None => existing.due_date.clone(),
        Some(s) if s.is_empty() => None,
        Some(s) => Some(s),
    };

    let key = master.as_bytes();
    let (enc_title, title_nonce) = encrypt_bytes(key, title.as_bytes())?;
    let (enc_note, note_nonce) = encrypt_bytes(key, note_markdown.as_bytes())?;
    let subtasks_json = serde_json::to_vec(&subtasks).map_err(|e| format!("subtasks json: {e}"))?;
    let (enc_subtasks, subtasks_nonce) = encrypt_bytes(key, &subtasks_json)?;
    let tags_json = serde_json::to_string(&tags).map_err(|e| format!("tags json: {e}"))?;
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "UPDATE tasks SET encrypted_title = ?1, title_nonce = ?2, encrypted_note = ?3, note_nonce = ?4, encrypted_subtasks = ?5, subtasks_nonce = ?6, status = ?7, priority = ?8, is_pinned = ?9, folder_id = ?10, tags_json = ?11, due_date = ?12, updated_at = ?13 WHERE id = ?14",
        params![
            enc_title,
            title_nonce.as_slice(),
            enc_note,
            note_nonce.as_slice(),
            enc_subtasks,
            subtasks_nonce.as_slice(),
            status,
            priority,
            i64::from(is_pinned),
            folder_id,
            tags_json,
            due_date,
            now,
            id,
        ],
    )
    .map_err(|e| format!("db update task: {e}"))?;

    Ok(Task {
        id: id.to_string(),
        title,
        note_markdown,
        subtasks,
        status,
        priority,
        is_pinned,
        folder_id,
        tags,
        due_date,
        created_at: existing.created_at,
        updated_at: now,
    })
}

pub fn delete_task(conn: &Connection, id: &str) -> Result<bool, String> {
    let n = conn
        .execute("DELETE FROM tasks WHERE id = ?1", params![id])
        .map_err(|e| format!("db delete task: {e}"))?;
    Ok(n > 0)
}

/// Re-encrypt every task + the verifier under a new key (password change).
/// All-or-nothing: all rows are decrypted with the old key BEFORE the write
/// transaction opens, so a corrupt row aborts without touching the DB.
pub fn rekey_vault(
    conn: &mut Connection,
    old_master: &MasterKey,
    new_key: &[u8; 32],
    new_salt: &[u8],
) -> Result<(), String> {
    let old_key = old_master.as_bytes();
    // 1. Load raw rows.
    let rows: Vec<RekeyRow> = {
        let mut stmt = conn
            .prepare("SELECT id, encrypted_title, title_nonce, encrypted_note, note_nonce, encrypted_subtasks, subtasks_nonce FROM tasks")
            .map_err(|e| format!("db prepare rekey: {e}"))?;
        stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Vec<u8>>(2)?,
                r.get::<_, Vec<u8>>(3)?,
                r.get::<_, Vec<u8>>(4)?,
                r.get::<_, Vec<u8>>(5)?,
                r.get::<_, Vec<u8>>(6)?,
            ))
        })
        .map_err(|e| format!("db rekey scan: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("db rekey row: {e}"))?
    };
    // 2. Decrypt with the old key, re-encrypt with the new key (fresh nonces).
    let mut rekeyed = Vec::with_capacity(rows.len());
    for (id, enc_title, title_nonce, enc_note, note_nonce, enc_sub, sub_nonce) in rows {
        let title = decrypt_bytes(old_key, &enc_title, &nonce_from_slice(&title_nonce)?)?;
        let note = decrypt_bytes(old_key, &enc_note, &nonce_from_slice(&note_nonce)?)?;
        let subtasks = decrypt_bytes(old_key, &enc_sub, &nonce_from_slice(&sub_nonce)?)?;
        let (new_enc_title, new_title_nonce) = encrypt_bytes(new_key, &title)?;
        let (new_enc_note, new_note_nonce) = encrypt_bytes(new_key, &note)?;
        let (new_enc_sub, new_sub_nonce) = encrypt_bytes(new_key, &subtasks)?;
        rekeyed.push((
            id,
            new_enc_title,
            new_title_nonce,
            new_enc_note,
            new_note_nonce,
            new_enc_sub,
            new_sub_nonce,
        ));
    }
    let (verifier_ct, verifier_nonce) = encrypt_bytes(new_key, VERIFIER_PLAINTEXT)?;
    // 3. Write everything in one transaction.
    let tx = conn
        .transaction()
        .map_err(|e| format!("db rekey tx: {e}"))?;
    tx.execute(
        "UPDATE vault_meta SET kdf_salt = ?1, verifier_nonce = ?2, verifier_ciphertext = ?3 WHERE id = 1",
        params![new_salt, verifier_nonce.as_slice(), verifier_ct],
    )
    .map_err(|e| format!("db rekey meta: {e}"))?;
    {
        let mut update = tx
            .prepare("UPDATE tasks SET encrypted_title = ?1, title_nonce = ?2, encrypted_note = ?3, note_nonce = ?4, encrypted_subtasks = ?5, subtasks_nonce = ?6 WHERE id = ?7")
            .map_err(|e| format!("db prepare rekey tasks: {e}"))?;
        for (id, enc_title, title_nonce, enc_note, note_nonce, enc_sub, sub_nonce) in &rekeyed {
            update
                .execute(params![
                    enc_title,
                    title_nonce.as_slice(),
                    enc_note,
                    note_nonce.as_slice(),
                    enc_sub,
                    sub_nonce.as_slice(),
                    id
                ])
                .map_err(|e| format!("db rekey task: {e}"))?;
        }
    }
    tx.commit().map_err(|e| format!("db rekey commit: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Encrypted backup (.vault) + Markdown export
// ---------------------------------------------------------------------------
//
// A `.vault` file is a signed JSON bundle: vault_meta (salt, verifier,
// settings) plus every task row's ciphertext + nonces + plaintext metadata.
// Integrity comes from a SHA-256 checksum over the canonical payload JSON.
// Plaintext NEVER touches the backup — restore proves the backup's own
// password against its salt+verifier before the atomic replace.

/// Bump when the on-disk bundle shape changes; restores reject mismatches.
pub const BACKUP_FORMAT_VERSION: u32 = 1;
/// Refuse to even read backup files larger than this (DoS guard).
pub const MAX_BACKUP_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMeta {
    pub kdf_salt_b64: String,
    pub verifier_nonce_b64: String,
    pub verifier_ciphertext_b64: String,
    pub auto_lock_seconds: u64,
    pub lock_on_tray_hide: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupTaskRow {
    pub id: String,
    pub encrypted_title_b64: String,
    pub title_nonce_b64: String,
    pub encrypted_note_b64: String,
    pub note_nonce_b64: String,
    pub encrypted_subtasks_b64: String,
    pub subtasks_nonce_b64: String,
    pub status: String,
    pub priority: i32,
    pub is_pinned: bool,
    pub folder_id: Option<String>,
    pub tags_json: String,
    pub due_date: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupPayload {
    pub format_version: u32,
    pub meta: BackupMeta,
    pub tasks: Vec<BackupTaskRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupBundle {
    pub payload: BackupPayload,
    /// Lowercase hex SHA-256 over the canonical JSON of `payload`.
    pub sha256_hex: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupStats {
    pub task_count: usize,
    pub file_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportStats {
    pub task_count: usize,
}

/// A backup row with its 6 base64 blobs decoded: (row, enc_title,
/// title_nonce, enc_note, note_nonce, enc_subtasks, subtasks_nonce).
type DecodedBackupRow = (
    BackupTaskRow,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
);

fn b64e(bytes: &[u8]) -> String {
    B64.encode(bytes)
}

fn b64d(what: &str, s: &str) -> Result<Vec<u8>, String> {
    B64.decode(s)
        .map_err(|_| format!("backup is corrupted: invalid base64 in {what}"))
}

/// Collect vault_meta + raw encrypted task rows. No decryption happens —
/// the bundle carries ciphertext only.
pub fn build_backup_payload(conn: &Connection) -> Result<BackupPayload, String> {
    let meta = get_vault_meta(conn)?.ok_or_else(|| "vault is not initialized".to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, encrypted_title, title_nonce, encrypted_note, note_nonce, encrypted_subtasks, subtasks_nonce, status, priority, is_pinned, folder_id, tags_json, due_date, created_at, updated_at FROM tasks ORDER BY updated_at",
        )
        .map_err(|e| format!("db prepare backup scan: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            let pinned: i64 = r.get(9)?;
            Ok(BackupTaskRow {
                id: r.get(0)?,
                encrypted_title_b64: b64e(&r.get::<_, Vec<u8>>(1)?),
                title_nonce_b64: b64e(&r.get::<_, Vec<u8>>(2)?),
                encrypted_note_b64: b64e(&r.get::<_, Vec<u8>>(3)?),
                note_nonce_b64: b64e(&r.get::<_, Vec<u8>>(4)?),
                encrypted_subtasks_b64: b64e(&r.get::<_, Vec<u8>>(5)?),
                subtasks_nonce_b64: b64e(&r.get::<_, Vec<u8>>(6)?),
                status: r.get(7)?,
                priority: r.get(8)?,
                is_pinned: pinned != 0,
                folder_id: r.get(10)?,
                tags_json: r.get(11)?,
                due_date: r.get(12)?,
                created_at: r.get(13)?,
                updated_at: r.get(14)?,
            })
        })
        .map_err(|e| format!("db scan backup rows: {e}"))?;
    let mut tasks = Vec::new();
    for row in rows {
        tasks.push(row.map_err(|e| format!("db read backup row: {e}"))?);
    }
    Ok(BackupPayload {
        format_version: BACKUP_FORMAT_VERSION,
        meta: BackupMeta {
            kdf_salt_b64: b64e(&meta.kdf_salt),
            verifier_nonce_b64: b64e(&meta.verifier_nonce),
            verifier_ciphertext_b64: b64e(&meta.verifier_ciphertext),
            auto_lock_seconds: meta.auto_lock_seconds,
            lock_on_tray_hide: meta.lock_on_tray_hide,
        },
        tasks,
    })
}

/// Canonical payload JSON (struct field order is deterministic) + checksum.
pub fn seal_backup_bundle(payload: &BackupPayload) -> Result<BackupBundle, String> {
    let canonical = serde_json::to_string(payload).map_err(|e| format!("backup serialize: {e}"))?;
    let sha256_hex = format!("{:x}", Sha256::digest(canonical.as_bytes()));
    Ok(BackupBundle {
        payload: payload.clone(),
        sha256_hex,
    })
}

pub fn serialize_bundle(bundle: &BackupBundle) -> Result<String, String> {
    serde_json::to_string_pretty(bundle).map_err(|e| format!("backup serialize: {e}"))
}

/// Parse + verify checksum. Any tampering with the payload fails here.
pub fn parse_backup_bundle(text: &str) -> Result<BackupPayload, String> {
    let bundle: BackupBundle =
        serde_json::from_str(text).map_err(|_| "not a valid .vault backup file".to_string())?;
    if bundle.payload.format_version != BACKUP_FORMAT_VERSION {
        return Err(format!(
            "unsupported backup version {} (expected {})",
            bundle.payload.format_version, BACKUP_FORMAT_VERSION
        ));
    }
    let canonical = serde_json::to_string(&bundle.payload)
        .map_err(|_| "backup payload is malformed".to_string())?;
    let recomputed = format!("{:x}", Sha256::digest(canonical.as_bytes()));
    if recomputed != bundle.sha256_hex {
        return Err("backup checksum mismatch: file is corrupted or tampered".to_string());
    }
    Ok(bundle.payload)
}

/// Prove `password` against the backup's own salt+verifier. Returns the
/// derived key (for callers that need it) — wrong passwords fail closed.
pub fn verify_backup_password(payload: &BackupPayload, password: &str) -> Result<[u8; 32], String> {
    let salt = b64d("kdf_salt", &payload.meta.kdf_salt_b64)?;
    let key = derive_key(password, &salt)?;
    let nonce_bytes = b64d("verifier_nonce", &payload.meta.verifier_nonce_b64)?;
    let ct = b64d("verifier_ciphertext", &payload.meta.verifier_ciphertext_b64)?;
    let nonce = nonce_from_slice(&nonce_bytes)?;
    let plain =
        decrypt_bytes(&key, &ct, &nonce).map_err(|_| "incorrect backup password".to_string())?;
    if plain.as_slice() != VERIFIER_PLAINTEXT {
        return Err("incorrect backup password".to_string());
    }
    Ok(key)
}

/// Atomically replace the local vault content with the backup (single txn).
/// Caller must hold no open statement on `conn`.
pub fn restore_backup_payload(
    conn: &mut Connection,
    payload: &BackupPayload,
) -> Result<usize, String> {
    let salt = b64d("kdf_salt", &payload.meta.kdf_salt_b64)?;
    let verifier_nonce = b64d("verifier_nonce", &payload.meta.verifier_nonce_b64)?;
    let verifier_ct = b64d("verifier_ciphertext", &payload.meta.verifier_ciphertext_b64)?;
    // Decode every row BEFORE opening the write txn so a corrupt bundle
    // can never leave a half-replaced vault.
    let mut decoded: Vec<DecodedBackupRow> = Vec::with_capacity(payload.tasks.len());
    for t in &payload.tasks {
        decoded.push((
            t.clone(),
            b64d("encrypted_title", &t.encrypted_title_b64)?,
            b64d("title_nonce", &t.title_nonce_b64)?,
            b64d("encrypted_note", &t.encrypted_note_b64)?,
            b64d("note_nonce", &t.note_nonce_b64)?,
            b64d("encrypted_subtasks", &t.encrypted_subtasks_b64)?,
            b64d("subtasks_nonce", &t.subtasks_nonce_b64)?,
        ));
    }
    let tx = conn
        .transaction()
        .map_err(|e| format!("db restore begin: {e}"))?;
    tx.execute("DELETE FROM tasks", [])
        .map_err(|e| format!("db restore clear tasks: {e}"))?;
    tx.execute("DELETE FROM vault_meta", [])
        .map_err(|e| format!("db restore clear meta: {e}"))?;
    tx.execute(
        "INSERT INTO vault_meta (id, kdf_salt, verifier_nonce, verifier_ciphertext, auto_lock_seconds, lock_on_tray_hide, created_at) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            salt,
            verifier_nonce,
            verifier_ct,
            payload.meta.auto_lock_seconds as i64,
            i64::from(payload.meta.lock_on_tray_hide),
            Utc::now().to_rfc3339(),
        ],
    )
    .map_err(|e| format!("db restore meta: {e}"))?;
    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO tasks (id, encrypted_title, title_nonce, encrypted_note, note_nonce, encrypted_subtasks, subtasks_nonce, status, priority, is_pinned, folder_id, tags_json, due_date, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            )
            .map_err(|e| format!("db restore prepare: {e}"))?;
        for (t, enc_title, title_nonce, enc_note, note_nonce, enc_sub, sub_nonce) in &decoded {
            stmt.execute(params![
                t.id,
                enc_title,
                title_nonce,
                enc_note,
                note_nonce,
                enc_sub,
                sub_nonce,
                t.status,
                t.priority,
                i64::from(t.is_pinned),
                t.folder_id,
                t.tags_json,
                t.due_date,
                t.created_at,
                t.updated_at,
            ])
            .map_err(|e| format!("db restore task: {e}"))?;
        }
    }
    tx.commit().map_err(|e| format!("db restore commit: {e}"))?;
    Ok(decoded.len())
}

/// Render one decrypted task as Markdown with YAML frontmatter.
pub fn render_task_markdown(task: &Task) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("status: {}\n", task.status));
    out.push_str(&format!("priority: {}\n", task.priority));
    match &task.due_date {
        Some(d) => out.push_str(&format!("due_date: {d}\n")),
        None => out.push_str("due_date: null\n"),
    }
    let tags = serde_json::to_string(&task.tags).unwrap_or_else(|_| "[]".to_string());
    out.push_str(&format!("tags: {tags}\n"));
    out.push_str("---\n\n");
    out.push_str(&format!("# {}\n\n", task.title));
    if !task.subtasks.is_empty() {
        for s in &task.subtasks {
            let mark = if s.done { "x" } else { " " };
            out.push_str(&format!("- [{mark}] {}\n", s.title));
        }
        out.push('\n');
    }
    out.push_str(&task.note_markdown);
    if !task.note_markdown.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Filesystem-safe name: slugified title + id fragment (collision-proof).
pub fn slugify_filename(title: &str, id: &str) -> String {
    let mut slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else if c.is_whitespace() || c == '-' || c == '_' {
                '-'
            } else {
                '\0'
            }
        })
        .filter(|&c| c != '\0')
        .collect();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() {
        "task".to_string()
    } else {
        slug.chars()
            .take(60)
            .collect::<String>()
            .trim_matches('-')
            .to_string()
    };
    let slug = if slug.is_empty() {
        "task".to_string()
    } else {
        slug
    };
    let frag: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect();
    format!("{slug}-{frag}.md")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::derive_key;

    fn test_key() -> MasterKey {
        MasterKey(derive_key("test-pin-1234", &[9u8; 16]).unwrap())
    }

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn
    }

    #[test]
    fn crud_roundtrip_encrypted() {
        let conn = mem_db();
        let master = test_key();

        let created = create_task(
            &conn,
            &master,
            CreateTaskInput {
                title: "Write report".to_string(),
                note_markdown: Some("# Notes\n- a".to_string()),
                status: Some("todo".to_string()),
                priority: Some(3),
                folder_id: Some("work".to_string()),
                tags: Some(vec!["urgent".to_string()]),
                due_date: Some("2026-11-10".to_string()),
            },
        )
        .unwrap();
        assert_eq!(created.title, "Write report");
        assert_eq!(created.subtasks.len(), 0);

        // Ciphertext at rest must not contain plaintext.
        let raw_title: Vec<u8> = conn
            .query_row(
                "SELECT encrypted_title FROM tasks WHERE id = ?1",
                params![created.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!raw_title.windows(6).any(|w| w == b"report"));

        let listed = list_tasks(&conn, &master).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].note_markdown, "# Notes\n- a");

        let updated = update_task(
            &conn,
            &master,
            &created.id,
            UpdateTaskPatch {
                status: Some("in_progress".to_string()),
                is_pinned: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(updated.status, "in_progress");
        assert!(updated.is_pinned);

        assert!(delete_task(&conn, &created.id).unwrap());
        assert!(list_tasks(&conn, &master).unwrap().is_empty());
        assert!(!delete_task(&conn, &created.id).unwrap());
    }

    #[test]
    fn wrong_key_cannot_decrypt_tasks() {
        let conn = mem_db();
        let good = test_key();
        let bad = MasterKey(derive_key("other-pin", &[9u8; 16]).unwrap());
        let created = create_task(
            &conn,
            &good,
            CreateTaskInput {
                title: "Secret".to_string(),
                note_markdown: None,
                status: None,
                priority: None,
                folder_id: None,
                tags: None,
                due_date: None,
            },
        )
        .unwrap();
        assert!(get_task(&conn, &bad, &created.id).is_err());
        assert!(list_tasks(&conn, &bad).is_err());
    }

    #[test]
    fn rejects_bad_status_priority_and_empty_title() {
        let conn = mem_db();
        let master = test_key();
        let mk = |title: &str, status: Option<&str>, priority: Option<i32>| CreateTaskInput {
            title: title.to_string(),
            note_markdown: None,
            status: status.map(str::to_string),
            priority,
            folder_id: None,
            tags: None,
            due_date: None,
        };
        assert!(create_task(&conn, &master, mk("  ", None, None)).is_err());
        assert!(create_task(&conn, &master, mk("t", Some("nope"), None)).is_err());
        assert!(create_task(&conn, &master, mk("t", None, Some(9))).is_err());
    }

    #[test]
    fn due_date_and_tags_roundtrip() {
        let conn = mem_db();
        let master = test_key();
        let t = create_task(
            &conn,
            &master,
            CreateTaskInput {
                title: "due probe".to_string(),
                note_markdown: None,
                status: None,
                priority: Some(3),
                folder_id: None,
                tags: Some(vec!["home".to_string()]),
                due_date: Some("2026-11-05".to_string()),
            },
        )
        .unwrap();
        assert_eq!(t.due_date.as_deref(), Some("2026-11-05"));
        assert_eq!(t.tags, vec!["home".to_string()]);
        let listed = list_tasks(&conn, &master).unwrap();
        assert_eq!(listed[0].due_date.as_deref(), Some("2026-11-05"));
        assert_eq!(listed[0].priority, 3);
    }

    fn backup_fixture() -> (Connection, MasterKey) {
        let conn = mem_db();
        let salt = [21u8; 16];
        let key = derive_key("backup-fixture-pw", &salt).unwrap();
        let (ct, nonce) = encrypt_bytes(&key, VERIFIER_PLAINTEXT).unwrap();
        insert_vault_meta(&conn, &salt, &nonce, &ct, 600, true).unwrap();
        (conn, MasterKey(key))
    }

    #[test]
    fn backup_creation_carries_ciphertext_only() {
        let (conn, master) = backup_fixture();
        create_task(
            &conn,
            &master,
            CreateTaskInput {
                title: "Ultrasecret title".to_string(),
                note_markdown: Some("ultrasecret-note-body".to_string()),
                status: None,
                priority: None,
                folder_id: None,
                tags: None,
                due_date: None,
            },
        )
        .unwrap();
        let payload = build_backup_payload(&conn).unwrap();
        assert_eq!(payload.tasks.len(), 1);
        assert_eq!(payload.meta.auto_lock_seconds, 600);
        assert!(payload.meta.lock_on_tray_hide);
        let bundle = seal_backup_bundle(&payload).unwrap();
        let text = serialize_bundle(&bundle).unwrap();
        assert!(text.contains("sha256_hex"));
        assert!(!text.contains("Ultrasecret title"));
        assert!(!text.contains("ultrasecret-note-body"));
        // And it parses back cleanly.
        let parsed = parse_backup_bundle(&text).unwrap();
        assert_eq!(parsed.tasks.len(), 1);
    }

    #[test]
    fn backup_rejects_tampered_payload() {
        let (conn, master) = backup_fixture();
        create_task(
            &conn,
            &master,
            CreateTaskInput {
                title: "tamper probe".to_string(),
                note_markdown: None,
                status: Some("todo".to_string()),
                priority: None,
                folder_id: None,
                tags: None,
                due_date: None,
            },
        )
        .unwrap();
        let payload = build_backup_payload(&conn).unwrap();
        let text = serialize_bundle(&seal_backup_bundle(&payload).unwrap()).unwrap();
        // Flip a status value without touching the checksum.
        let tampered = text.replacen("\"status\": \"todo\"", "\"status\": \"done\"", 1);
        assert_ne!(tampered, text);
        let err = parse_backup_bundle(&tampered).unwrap_err();
        assert!(err.contains("checksum mismatch"), "got: {err}");
    }

    #[test]
    fn backup_rejects_wrong_password() {
        let (conn, master) = backup_fixture();
        create_task(
            &conn,
            &master,
            CreateTaskInput {
                title: "pw probe".to_string(),
                note_markdown: None,
                status: None,
                priority: None,
                folder_id: None,
                tags: None,
                due_date: None,
            },
        )
        .unwrap();
        let payload = build_backup_payload(&conn).unwrap();
        let bundle = seal_backup_bundle(&payload).unwrap();
        let parsed = parse_backup_bundle(&serialize_bundle(&bundle).unwrap()).unwrap();
        let err = verify_backup_password(&parsed, "wrong-password").unwrap_err();
        assert!(err.contains("incorrect backup password"), "got: {err}");
        verify_backup_password(&parsed, "backup-fixture-pw").unwrap();
    }

    #[test]
    fn backup_roundtrip_is_lossless() {
        let (conn_a, master) = backup_fixture();
        for (title, status, priority) in
            [("alpha task", "todo", 1), ("beta task", "in_progress", 4)]
        {
            create_task(
                &conn_a,
                &master,
                CreateTaskInput {
                    title: title.to_string(),
                    note_markdown: Some(format!("# {title}\nbody")),
                    status: Some(status.to_string()),
                    priority: Some(priority),
                    folder_id: Some("work".to_string()),
                    tags: Some(vec!["k".to_string()]),
                    due_date: Some("2026-12-31".to_string()),
                },
            )
            .unwrap();
        }
        let payload = build_backup_payload(&conn_a).unwrap();
        let text = serialize_bundle(&seal_backup_bundle(&payload).unwrap()).unwrap();
        let parsed = parse_backup_bundle(&text).unwrap();
        verify_backup_password(&parsed, "backup-fixture-pw").unwrap();

        let mut conn_b = mem_db();
        let restored = restore_backup_payload(&mut conn_b, &parsed).unwrap();
        assert_eq!(restored, 2);

        let meta_b = get_vault_meta(&conn_b).unwrap().unwrap();
        assert_eq!(meta_b.auto_lock_seconds, 600);
        assert!(meta_b.lock_on_tray_hide);
        let before = serde_json::to_value(list_tasks(&conn_a, &master).unwrap()).unwrap();
        let after = serde_json::to_value(list_tasks(&conn_b, &master).unwrap()).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn markdown_render_and_slug() {
        let task = Task {
            id: "abc-123-def".to_string(),
            title: "Hello, World!".to_string(),
            note_markdown: "line one\nline two".to_string(),
            subtasks: vec![
                Subtask {
                    id: "s1".to_string(),
                    title: "done thing".to_string(),
                    done: true,
                },
                Subtask {
                    id: "s2".to_string(),
                    title: "open thing".to_string(),
                    done: false,
                },
            ],
            status: "in_progress".to_string(),
            priority: 3,
            is_pinned: false,
            folder_id: "inbox".to_string(),
            tags: vec!["a".to_string(), "b".to_string()],
            due_date: Some("2026-12-01".to_string()),
            created_at: "2026-11-04T00:00:00Z".to_string(),
            updated_at: "2026-11-04T00:00:00Z".to_string(),
        };
        let md = render_task_markdown(&task);
        assert!(md.contains("status: in_progress"));
        assert!(md.contains("priority: 3"));
        assert!(md.contains("due_date: 2026-12-01"));
        assert!(md.contains("tags: [\"a\",\"b\"]"));
        assert!(md.contains("# Hello, World!"));
        assert!(md.contains("- [x] done thing"));
        assert!(md.contains("- [ ] open thing"));
        assert!(md.contains("line one\nline two\n"));
        assert_eq!(
            slugify_filename(&task.title, &task.id),
            "hello-world-abc123de.md"
        );
        assert_eq!(slugify_filename("!!!", "id"), "task-id.md");
    }
}

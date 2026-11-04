//! VaultTask encrypted SQLite store.
//!
//! Sensitive columns (`title`, `note_markdown`, `subtasks`) are AES-256-GCM
//! ciphertext blobs; index columns (`status`, `priority`, `due_date`, …)
//! stay plaintext for fast sorting after unlock.

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::crypto::{MasterKey, decrypt_bytes, encrypt_bytes};

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

pub fn get_vault_meta(conn: &Connection) -> Result<Option<VaultMeta>, String> {
    let row: Option<(Vec<u8>, Vec<u8>, Vec<u8>, i64, i64)> = conn
        .query_row(
            "SELECT kdf_salt, verifier_nonce, verifier_ciphertext, auto_lock_seconds, lock_on_tray_hide FROM vault_meta WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()
        .map_err(|e| format!("db read vault_meta: {e}"))?;
    Ok(row.map(|(kdf_salt, verifier_nonce, verifier_ciphertext, auto_lock_seconds, lock_on_tray_hide)| {
        VaultMeta {
            kdf_salt,
            verifier_nonce,
            verifier_ciphertext,
            auto_lock_seconds: auto_lock_seconds.max(0) as u64,
            lock_on_tray_hide: lock_on_tray_hide != 0,
        }
    }))
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
        other => Err(format!("invalid status '{other}': expected todo|in_progress|done")),
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
    let subtasks: Vec<Subtask> =
        serde_json::from_slice(&subtasks_bytes).map_err(|_| "corrupted subtasks json".to_string())?;
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
    let row: Option<(
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
    )> = conn
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
    let subtasks_json = serde_json::to_vec::<Vec<Subtask>>(&vec![])
        .map_err(|e| format!("subtasks json: {e}"))?;
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
    let existing = get_task(conn, master, id)?
        .ok_or_else(|| format!("task '{id}' not found"))?;

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
    let note_markdown = patch.note_markdown.unwrap_or(existing.note_markdown.clone());
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
    let subtasks_json =
        serde_json::to_vec(&subtasks).map_err(|e| format!("subtasks json: {e}"))?;
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
}

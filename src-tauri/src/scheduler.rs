//! Scheduled reminder notifications (15s poll).
//!
//! Privacy model: the scheduler reads due rows straight from SQLite. When
//! the vault is locked — or the user hides previews — the OS notification
//! carries a generic body with NO decrypted content. Titles are decrypted
//! in memory only for unlocked vaults with previews allowed.

use chrono::{Local, SecondsFormat, Utc};
use rusqlite::Connection;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_notification::Notification;

use crate::{db, db::DueReminder, state::AppState};

/// Poll cadence for due reminders.
pub const SCHEDULER_INTERVAL_SECS: u64 = 15;
/// Generic locked-state notification (no decrypted content).
pub const GENERIC_TITLE: &str = "VaultTask Reminder";
pub const GENERIC_BODY: &str = "You have a scheduled task due now. Click to unlock and view.";

/// Build the (title, body) for one due reminder. `title` is the decrypted
/// task title (`Some` only when unlocked); `previews_allowed` is
/// `unlocked && !hide_notification_preview`.
pub fn build_notification(
    title: Option<&str>,
    reminder_at: &str,
    previews_allowed: bool,
) -> (String, String) {
    match (previews_allowed, title) {
        (true, Some(t)) => (
            format!("Task Due: {t}"),
            format!("Due at {}", format_due_time(reminder_at)),
        ),
        _ => (GENERIC_TITLE.to_string(), GENERIC_BODY.to_string()),
    }
}

/// Canonical `…Z` timestamp -> local `HH:MM` (today) or `Mon D, HH:MM`.
/// Falls back to the raw string when unparseable.
fn format_due_time(reminder_at: &str) -> String {
    let parsed =
        chrono::DateTime::parse_from_rfc3339(reminder_at).map(|dt| dt.with_timezone(&Local));
    match parsed {
        Ok(local) => {
            if local.date_naive() == Local::now().date_naive() {
                local.format("%H:%M").to_string()
            } else {
                local.format("%b %-d, %H:%M").to_string()
            }
        }
        Err(_) => reminder_at.to_string(),
    }
}

/// Canonical UTC `…Z` "now" for the due comparison.
fn now_canonical() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// One scheduler tick: dispatch newly-due reminders, mark fired. Returns
/// the number dispatched. Never panics, and never records activity (a tick
/// must not keep an idle vault unlocked).
pub fn tick<R: Runtime>(app: &AppHandle<R>) -> usize {
    let state = app.state::<AppState>();
    let db_path = match state.db_path.lock() {
        Ok(p) => p.clone(),
        Err(_) => return 0,
    };
    let conn = match Connection::open(&db_path) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    if db::init_db(&conn).is_err() {
        return 0;
    }
    let meta = match db::get_vault_meta(&conn) {
        Ok(Some(m)) => m,
        _ => return 0, // fresh vault: nothing scheduled
    };
    // Clone the key WITHOUT recording activity (unlike require_master).
    let master = match state.session.lock() {
        Ok(s) => s.master_key.clone(),
        Err(_) => return 0,
    };
    let previews_allowed = master.is_some() && !meta.hide_notification_preview;
    let due: Vec<DueReminder> =
        match db::collect_due_reminders(&conn, &now_canonical(), master.as_ref()) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[vault-task] reminder scan failed: {e}");
                return 0;
            }
        };
    if due.is_empty() {
        return 0;
    }
    let notifier = match app.try_state::<Notification<R>>() {
        Some(n) => n,
        None => {
            eprintln!("[vault-task] notification plugin unavailable; retrying later");
            return 0;
        }
    };
    let mut fired = Vec::new();
    for reminder in &due {
        let (title, body) = build_notification(
            reminder.title.as_deref(),
            &reminder.reminder_at,
            previews_allowed,
        );
        match notifier.builder().title(title).body(body).show() {
            Ok(()) => fired.push(reminder.id.clone()),
            Err(e) => eprintln!("[vault-task] notification dispatch failed: {e}"),
        }
    }
    // Mark AFTER dispatch: failures stay unfired and retry next tick.
    if let Err(e) = db::mark_reminders_fired(&conn, &fired) {
        eprintln!("[vault-task] mark reminders fired failed: {e}");
    }
    fired.len()
}

/// Spawn the 15s scheduler loop. Fire-and-forget; the loop never exits.
pub fn spawn_scheduler<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(SCHEDULER_INTERVAL_SECS));
        loop {
            interval.tick().await;
            tick(&app);
        }
    });
}

#[cfg(test)]
mod tests {
    use tauri::test::{mock_builder, mock_context, noop_assets};

    use super::*;
    use crate::crypto::{MasterKey, VERIFIER_PLAINTEXT, derive_key, encrypt_bytes, generate_salt};

    fn test_app_with_db(db_path: std::path::PathBuf) -> tauri::App<tauri::test::MockRuntime> {
        let _ = std::fs::remove_file(&db_path);
        mock_builder()
            .manage(AppState::new(db_path))
            .build(mock_context(noop_assets()))
            .unwrap()
    }

    #[test]
    fn payload_matrix_redacts_correctly() {
        // Unlocked + previews: full title.
        let (t, b) = build_notification(Some("Buy milk"), "2026-11-05T09:00:00Z", true);
        assert_eq!(t, "Task Due: Buy milk");
        assert!(b.starts_with("Due at "), "got: {b}");
        // Locked (title unavailable): generic, no leak.
        let (t, b) = build_notification(None, "2026-11-05T09:00:00Z", false);
        assert_eq!(t, GENERIC_TITLE);
        assert_eq!(b, GENERIC_BODY);
        // Unlocked but previews hidden: generic despite title present.
        let (t, b) = build_notification(Some("Buy milk"), "2026-11-05T09:00:00Z", false);
        assert_eq!(t, GENERIC_TITLE);
        assert_eq!(b, GENERIC_BODY);
        assert!(!t.contains("milk") && !b.contains("milk"));
        // Previews allowed but title missing (shouldn't happen): generic.
        let (t, _) = build_notification(None, "2026-11-05T09:00:00Z", true);
        assert_eq!(t, GENERIC_TITLE);
    }

    #[test]
    fn tick_without_plugin_marks_nothing() {
        let db = std::env::temp_dir().join(format!("vault-sch-{}.db", std::process::id()));
        let app = test_app_with_db(db.clone());
        let salt = generate_salt();
        let key = derive_key("scheduler-probe", &salt).unwrap();
        let (ct, nonce) = encrypt_bytes(&key, VERIFIER_PLAINTEXT).unwrap();
        let conn = Connection::open(&db).unwrap();
        db::init_db(&conn).unwrap();
        db::insert_vault_meta(&conn, &salt, &nonce, &ct, 300, false, false, true).unwrap();
        let master = MasterKey(key);
        let past =
            (Utc::now() - chrono::Duration::hours(1)).to_rfc3339_opts(SecondsFormat::Secs, true);
        db::create_task(
            &conn,
            &master,
            db::CreateTaskInput {
                title: "probe".to_string(),
                note_markdown: None,
                status: None,
                priority: None,
                folder_id: None,
                tags: None,
                due_date: None,
                reminder_at: Some(past),
            },
        )
        .unwrap();
        drop(conn);
        // No notification plugin in the mock app: graceful no-op.
        assert_eq!(tick(app.handle()), 0);
        let conn = Connection::open(&db).unwrap();
        let still_due =
            db::collect_due_reminders(&conn, "2999-01-01T00:00:00Z", Some(&master)).unwrap();
        assert_eq!(still_due.len(), 1);
        drop(conn);
        let _ = std::fs::remove_file(&db);
    }
}

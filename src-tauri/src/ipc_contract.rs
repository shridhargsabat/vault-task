//! IPC contract tests: prove the frontend's snake_case JSON arg shapes
//! deserialize correctly through the real `#[command]` wrappers.
//!
//! Regression test for the Tauri v2 camelCase-default pitfall: command
//! params map to camelCase IPC keys unless annotated with
//! `rename_all = "snake_case"`. Missing `Option` args fail SILENTLY
//! (None, no error), so without the attribute every multi-word arg
//! (`due_date`, `note_markdown`, `is_pinned`, `folder_id`,
//! `auto_lock_seconds`, `lock_on_tray_hide`) was quietly dropped.

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tauri::{
        test::{
            INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
        },
        webview::InvokeRequest,
    };

    use crate::state::AppState;

    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn temp_db() -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("vault-ipc-{}-{}.db", std::process::id(), n));
        let _ = std::fs::remove_file(&p);
        p
    }

    fn request(cmd: &str, body: serde_json::Value) -> InvokeRequest {
        // Mirror tauri::protocol::origin: WebView2 (Windows) serves the app
        // over http://<scheme>.localhost instead of a custom protocol.
        // A hardcoded tauri://localhost is treated as REMOTE there, which
        // trips the ACL gate ("not allowed. Plugin not found") in mock tests.
        let url = if cfg!(any(windows, target_os = "android")) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        };
        InvokeRequest {
            cmd: cmd.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: url.parse().unwrap(),
            body: tauri::ipc::InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        }
    }

    fn call(
        webview: &tauri::WebviewWindow<MockRuntime>,
        cmd: &str,
        body: serde_json::Value,
    ) -> serde_json::Value {
        get_ipc_response(webview, request(cmd, body))
            .unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
            .deserialize::<serde_json::Value>()
            .unwrap()
    }

    #[test]
    fn snake_case_args_survive_ipc() {
        let db_path = temp_db();
        let app = mock_builder()
            .manage(AppState::new(db_path.clone()))
            .invoke_handler(tauri::generate_handler![
                crate::commands::get_vault_status,
                crate::commands::setup_vault,
                crate::commands::unlock_vault,
                crate::commands::lock_vault,
                crate::commands::list_tasks,
                crate::commands::create_task,
                crate::commands::update_task,
                crate::commands::update_security_settings,
                crate::commands::change_vault_password,
            ])
            .build(mock_context(noop_assets()))
            .unwrap();
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();

        // 1. Setup unlocks the vault.
        let status = call(
            &webview,
            "setup_vault",
            json!({"password": "test-ipc-1234"}),
        );
        assert_eq!(status["initialized"], true);
        assert_eq!(status["locked"], false);

        // 2. create_task with every multi-word snake_case arg.
        let created = call(
            &webview,
            "create_task",
            json!({
                "title": "IPC probe",
                "note_markdown": "# hello",
                "status": "in_progress",
                "priority": 3,
                "folder_id": "work",
                "tags": ["t"],
                "due_date": "2026-11-05",
            }),
        );
        assert_eq!(created["title"], "IPC probe");
        assert_eq!(created["note_markdown"], "# hello");
        assert_eq!(created["status"], "in_progress");
        assert_eq!(created["priority"], 3);
        assert_eq!(created["folder_id"], "work");
        assert_eq!(created["tags"], json!(["t"]));
        assert_eq!(created["due_date"], "2026-11-05");
        let id = created["id"].as_str().unwrap().to_string();

        // 3. update_task partial snake_case patch.
        let updated = call(
            &webview,
            "update_task",
            json!({"id": id, "due_date": "2026-11-06", "is_pinned": true, "note_markdown": "edited", "reminder_at": "2026-11-06T09:00:00Z"}),
        );
        assert_eq!(updated["due_date"], "2026-11-06");
        assert_eq!(updated["is_pinned"], true);
        assert_eq!(updated["note_markdown"], "edited");
        assert_eq!(updated["title"], "IPC probe"); // untouched
        assert_eq!(updated["reminder_at"], "2026-11-06T09:00:00Z");
        assert_eq!(updated["reminder_fired"], false);

        // 4. Security settings (non-optional => hard failure pre-fix).
        let settings = call(
            &webview,
            "update_security_settings",
            json!({"auto_lock_seconds": 600, "lock_on_tray_hide": true, "hide_notification_preview": false}),
        );
        assert_eq!(settings["auto_lock_seconds"], 600);
        assert_eq!(settings["lock_on_tray_hide"], true);
        assert_eq!(settings["hide_notification_preview"], false);

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn change_password_rekeys_and_swaps_session_key() {
        let db_path = temp_db();
        let app = mock_builder()
            .manage(AppState::new(db_path.clone()))
            .invoke_handler(tauri::generate_handler![
                crate::commands::setup_vault,
                crate::commands::unlock_vault,
                crate::commands::lock_vault,
                crate::commands::list_tasks,
                crate::commands::create_task,
                crate::commands::change_vault_password,
            ])
            .build(mock_context(noop_assets()))
            .unwrap();
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();

        call(&webview, "setup_vault", json!({"password": "old-pass-123"}));
        let created = call(
            &webview,
            "create_task",
            json!({"title": "Keep me", "note_markdown": "secret note"}),
        );
        assert_eq!(created["title"], "Keep me");

        // Wrong current password is rejected; nothing is rekeyed.
        let rejected = get_ipc_response(
            &webview,
            request(
                "change_vault_password",
                json!({"old_password": "nope", "new_password": "new-pass-123"}),
            ),
        );
        assert!(rejected.is_err());

        // Rekey with the correct current password.
        let status = call(
            &webview,
            "change_vault_password",
            json!({"old_password": "old-pass-123", "new_password": "new-pass-123"}),
        );
        assert_eq!(status["locked"], false);

        // Data intact under the new key.
        let tasks = call(&webview, "list_tasks", json!({}));
        assert_eq!(tasks[0]["title"], "Keep me");
        assert_eq!(tasks[0]["note_markdown"], "secret note");

        // Old password is dead, new password works.
        call(&webview, "lock_vault", json!({}));
        let bad_login = get_ipc_response(
            &webview,
            request("unlock_vault", json!({"password": "old-pass-123"})),
        );
        assert!(bad_login.is_err());
        let ok_login = call(
            &webview,
            "unlock_vault",
            json!({"password": "new-pass-123"}),
        );
        assert_eq!(ok_login["locked"], false);

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn backup_export_import_roundtrip_via_ipc() {
        let db_path = temp_db();
        let app = mock_builder()
            .manage(AppState::new(db_path.clone()))
            .invoke_handler(tauri::generate_handler![
                crate::commands::setup_vault,
                crate::commands::unlock_vault,
                crate::commands::create_task,
                crate::commands::list_tasks,
                crate::commands::export_encrypted_backup,
                crate::commands::import_encrypted_backup,
                crate::commands::export_decrypted_markdown,
            ])
            .build(mock_context(noop_assets()))
            .unwrap();
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();

        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let bak_path =
            std::env::temp_dir().join(format!("vault-test-{}-{}.vault", std::process::id(), n));
        let md_dir =
            std::env::temp_dir().join(format!("vault-test-md-{}-{}", std::process::id(), n));

        call(
            &webview,
            "setup_vault",
            json!({"password": "backup-pass-1"}),
        );
        call(
            &webview,
            "create_task",
            json!({"title": "Pack me", "note_markdown": "s3cret-note", "tags": ["t"], "due_date": "2026-12-01"}),
        );

        // Export carries ciphertext only — the plaintext note is absent.
        let stats = call(
            &webview,
            "export_encrypted_backup",
            json!({"destination_path": bak_path.to_string_lossy()}),
        );
        assert_eq!(stats["task_count"], 1);
        assert!(stats["file_bytes"].as_u64().unwrap() > 0);
        let raw = std::fs::read_to_string(&bak_path).unwrap();
        assert!(raw.contains("sha256_hex"));
        assert!(!raw.contains("s3cret-note"));

        // Wrong password: rejected, local vault untouched (still unlocked).
        let bad = get_ipc_response(
            &webview,
            request(
                "import_encrypted_backup",
                json!({"source_path": bak_path.to_string_lossy(), "master_password": "wrong"}),
            ),
        );
        assert!(bad.is_err());
        let still_there = call(&webview, "list_tasks", json!({}));
        assert_eq!(still_there[0]["title"], "Pack me");

        // Markdown export decrypts with the session key.
        let count = call(
            &webview,
            "export_decrypted_markdown",
            json!({"destination_dir": md_dir.to_string_lossy()}),
        );
        assert_eq!(count, 1);
        let md_file = std::fs::read_dir(&md_dir).unwrap().next().unwrap().unwrap();
        assert!(md_file.file_name().to_string_lossy().ends_with(".md"));
        let md = std::fs::read_to_string(md_file.path()).unwrap();
        assert!(md.contains("status: todo"));
        assert!(md.contains("due_date: 2026-12-01"));
        assert!(md.contains("# Pack me"));
        assert!(md.contains("s3cret-note"));

        // Correct password: atomic restore, session locked, data lossless.
        let imported = call(
            &webview,
            "import_encrypted_backup",
            json!({"source_path": bak_path.to_string_lossy(), "master_password": "backup-pass-1"}),
        );
        assert_eq!(imported["task_count"], 1);
        let locked_out = get_ipc_response(&webview, request("list_tasks", json!({})));
        assert!(locked_out.is_err());
        let ok_login = call(
            &webview,
            "unlock_vault",
            json!({"password": "backup-pass-1"}),
        );
        assert_eq!(ok_login["locked"], false);
        let tasks = call(&webview, "list_tasks", json!({}));
        assert_eq!(tasks[0]["title"], "Pack me");
        assert_eq!(tasks[0]["note_markdown"], "s3cret-note");

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&bak_path);
        let _ = std::fs::remove_dir_all(&md_dir);
    }
}

# VaultTask — Encrypted Tray Task Manager + Markdown Notes

Tauri v2 (Rust + React 18 + TypeScript + Tailwind + Zustand) desktop app for
Windows 11, Ubuntu 24.04+, and Fedora 41+.

## Phase 1 — Encrypted SQLite backend + auth state machine ✅

- `src-tauri/src/crypto.rs` — Argon2id KDF + AES-256-GCM, zeroized `MasterKey`
- `src-tauri/src/db.rs` — `vault_task.db` in OS app-data dir, `vault_meta` + `tasks` tables
- `src-tauri/src/state.rs` — `AppState(Mutex<VaultSession>)`
- `src-tauri/src/commands.rs` — IPC: `get_vault_status`, `setup_vault`,
  `unlock_vault`, `lock_vault`, `update_security_settings`, `list_tasks`,
  `create_task`, `update_task`, `delete_task` (+ `record_activity_heartbeat`)
- `cargo test` — 8 tests green (KDF, roundtrips, wrong-PIN rejection, encrypted CRUD)

## Prerequisites

**Ubuntu 24.04+:**

```bash
sudo apt install -y libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev build-essential curl
```

**Fedora 41+:**

```bash
sudo dnf install -y webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel openssl-devel
```

**Windows 11:** WebView2 evergreen runtime (pre-installed) + Rust + Node 20+.

## Develop

```bash
npm install
npm run tauri dev
```

## Verify

```bash
npm run typecheck   # tsc --noEmit
npm run build       # tsc + vite build
cd src-tauri && cargo test
```

## Project layout

```text
vault-task/
├── src-tauri/src/
│   ├── crypto.rs    # Argon2id + AES-256-GCM + MasterKey
│   ├── db.rs        # SQLite migrations + encrypted CRUD
│   ├── state.rs     # VaultSession + AppState
│   ├── commands.rs  # Tauri IPC commands
│   └── lib.rs       # App builder + state wiring
└── src/
    ├── types/models.ts
    ├── store/useVaultStore.ts
    └── App.tsx      # Phase 1 minimal vault UI (full UI lands in Phase 3/4)
```

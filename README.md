# VaultTask — Encrypted Tray Task Manager + Markdown Notes

Tauri v2 (Rust + React 18 + TypeScript + Tailwind + Zustand) desktop app for
Windows 11, Ubuntu 24.04+, and Fedora 41+.

## Phase 3 — List + side pane UI ✅

- `FilterRail` (pills / 240px sidebar), `QuickAddBar` (`!high` `@today`
  shortcuts with live hints), `TaskListView` (search, `#tag`, badges,
  subtask progress, note/pin indicators)
- `TaskDetailPane` (320px drawer / docked): status, `PrioritySelect`,
  `DueDatePicker` (custom calendar popover), tags, `SubtaskList`,
  `NoteEditor` (toolbar, write/preview, 300ms debounced autosave
  flushed on task switch)
- **Fix:** all commands now `rename_all = "snake_case"` — Tauri v2
  defaults IPC arg keys to camelCase and silently drops mismatched
  `Option` args (due dates/notes never arrived). Pinned by
  `src-tauri/src/ipc_contract.rs` mock-runtime IPC test (fails pre-fix).
- `cargo test` 14 green · `typecheck` 0 · `vite build` 0
- Headless E2E: setup/unlock, 5 tasks via quick-add, drawer open/close,
  note autosave persisted (36B ciphertext), maximized 3-pane + docked
  detail, restart persistence

## Phase 2 — Tray + dual-mode window manager ✅

- `src-tauri/src/tray.rs` — `TrayIconBuilder` (Open / Maximize / Lock / Quit),
  OS-aware popover positioner (`TrayCenter` Windows, `TopRight` GNOME,
  `BottomRight` KDE) + 12px work-area fallback, Ctrl+Shift+Space summon
- Close-to-tray + focus-loss auto-hide (unless pinned) via `on_window_event`
- IPC: `set_window_mode`, `toggle_maximize_mode`, `hide_to_tray`, `set_pinned`
- `src/components/Titlebar/CustomTitlebar.tsx` + `src/hooks/useWindowMode.ts`
- `cargo test` — 12 green (incl. anchor-math + shortcut-combo tests)
- Headless E2E (Xvfb + openbox + xdotool): popover 420x620 renders,
  maximize → 1280x800, restore → 420x620, hide-to-tray → process persists

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

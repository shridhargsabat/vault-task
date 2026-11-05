# VaultTask — Encrypted Tray Task Manager + Markdown Notes

Tauri v2 (Rust + React 18 + TypeScript + Tailwind + Zustand) desktop app for
Windows 11, Ubuntu 24.04+, and Fedora 41+.

## Release

CI (`ci.yml`) runs on every push to `main` and every PR targeting `main`:
Rust fmt + clippy + tests, typecheck + Vite build, a Windows test job,
and `cargo deny`.

To cut a release, bump the version in `src-tauri/Cargo.toml`,
`src-tauri/tauri.conf.json`, and `package.json` (all three must match),
then tag and push:

```bash
git tag v0.2.0 && git push origin v0.2.0
```

Pushing a `v*` tag runs `release.yml`: it verifies the tag matches all
three package versions, builds `.msi` + `-setup.exe` on Windows,
`.deb` + `.AppImage` on Ubuntu 24.04, and `.rpm` for Fedora (with an
`rpm -qpR` dependency check), then publishes all five installers plus
`SHA256SUMS.txt` to a GitHub Release.

## Phase 10 — In-app updater (tauri-plugin-updater + process) ✅

- Signed self-updates from `releases/latest/download/latest.json`
  (minisign keypair; private key in the `TAURI_SIGNING_PRIVATE_KEY`
  repo secret, pubkey in `tauri.conf.json`); Windows NSIS +
  Linux AppImage artifacts, per-platform manifests merged at publish
- Updates & Version card + tray "Check for Updates..." entry;
  `cargo test` 38 green · clippy 0 · typecheck 0 · deny clean
- Headless E2E: v0.3.0 badge renders, missing-manifest check fails
  gracefully with a retryable message (full install flow unlocks once
  v0.3.0 publishes `latest.json`)
- E2E note: the app needs a session bus to boot headless
  (`dbus-daemon --session --fork --print-address`, exported as
  `DBUS_SESSION_BUS_ADDRESS`) — without one it idles pre-setup

## Phase 9 — Scheduled reminders + privacy-safe notifications ✅

- `tauri-plugin-notification` + 15s `scheduler.rs` tick: fires native
  OS toasts for due reminders, marks `reminder_fired` only after
  successful dispatch (failures retry; no duplicates, no activity
  recorded so ticks never delay auto-lock)
- Locked vaults (or hidden previews) get a generic "VaultTask
  Reminder" — titles decrypt in memory only when unlocked with
  previews allowed; `cargo test` 38 green · clippy 0 · typecheck 0
- Reminder picker (presets + calendar + HH:MM), `@remind(15m|1h|…)`
  quick-add, Settings privacy toggle; schema migrated + backup-carried
- Headless E2E with a D-Bus notification catcher: verified generic /
  titled / locked payload bytes, fired flags, picker, and toggle live

## Phase 7 — OS keyring quick-unlock ✅

- `keyring` crate (Windows Credential Manager / Secret Service via
  D-Bus, pure-Rust bus crypto, vendored libdbus — no new system deps);
  derived key stored base64 under `com.vaulttask.desktop` / `master_key`
- `enable_keyring_unlock` + `unlock_with_keyring` IPC; `keyring_enabled`
  flag in `vault_meta` with ALTER TABLE migration for v0.1 databases
  (proven on a real upgraded DB) and carried through `.vault` backups
- Settings toggle + Quick Unlock button (Ctrl+U) on the lock screen;
  `cargo test` 32 green · clippy 0 · typecheck 0 · deny clean
- Headless E2E: enable without a daemon fails gracefully (clear banner,
  flag stays 0, no panic); flag-on shows the button; Ctrl+U with no
  daemon falls back to PIN with a clear message; PIN unlock unaffected

## Phase 6 — Encrypted backup + Markdown export ✅

- Signed `.vault` bundles (salt, verifier, all encrypted rows, SHA-256
  checksum — never any plaintext); restore verifies checksum + backup
  password, then atomically replaces the local vault and locks
- `export_decrypted_markdown` (unlocked only): one `.md` per task with
  YAML frontmatter + subtask checkboxes + note body
- Backup & Data section in Security settings (native file dialogs);
  `cargo test` 25 green · typecheck 0 · `vite build` 0 · deny clean
- Headless E2E: modal renders all three actions; export/import/markdown
  proven through the real IPC command layer with real files

## Phase 5 — Packaging + autostart + single-instance ✅

- `tauri-plugin-autostart` (Windows `HKCU\...\Run` + Linux
  `~/.config/autostart/VaultTask.desktop` with `--minimized` so login boots
  straight to the tray) with a Startup toggle in settings;
  `tauri-plugin-single-instance` (second launch shows + focuses the
  existing window, then exits)
- Bundle targets: `msi`, `nsis`, `deb`, `rpm`, `appimage`; deb depends on
  `libwebkit2gtk-4.1-0` + `libayatana-appindicator3-1`; rpm depends on
  `webkit2gtk4.1` + `libappindicator-gtk3` and recommends
  `gnome-shell-extension-appindicator`
- `cargo test` 19 green · typecheck 0 · `vite build` 0 · release `.deb`
  built clean
- Headless E2E: 2nd instance exits (D-Bus name owned), `--minimized`
  boots unmapped, autostart toggle creates/removes the `.desktop` entry

### Build & dependencies per OS

Prereqs everywhere: Rust stable via rustup + Node 20+.

**Windows 11 / 10** (PowerShell):
- `winget install Rustlang.Rustup OpenJS.NodeJS`
- Visual Studio Build Tools with the C++ workload (rustup prompts if missing)
- WebView2: preinstalled on Win11 / Win10 1803+
- MSI: WiX Toolset 3.11+ (wixtoolset.org) · NSIS `.exe`:
  `winget install NSIS.NSIS`
- `npm run tauri build -- --bundles msi nsis`

**Ubuntu 24.04+**:
- `sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev build-essential curl wget file`
- `npm run tauri build -- --bundles deb appimage`
- Running the AppImage needs FUSE at runtime (`sudo apt install libfuse2`)

**Fedora 41+**:
- `sudo dnf install webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel openssl-devel @development-tools`
- `npm run tauri build -- --bundles rpm appimage`
- GNOME tray icons need the AppIndicator shell extension (the `.rpm`
  recommends `gnome-shell-extension-appindicator`)

## Phase 4 — Auto-lock engine + lock screens ✅

- `watchdog.rs`: 1s Tokio loop (idle > `auto_lock_seconds` → zeroize +
  `vault://locked` + tray sync); Linux D-Bus `ScreenSaver.ActiveChanged`
  listener (graceful); all hide paths route via `hide_main_window`
  (honors `lock_on_tray_hide`); tray tooltip/menu reflect lock state
- `change_vault_password` IPC + transactional `rekey_vault` (decrypt-all
  before write; fresh salt/nonces; session key swap)
- `useActivityTracker` (2s-throttled heartbeats, Ctrl/Cmd+L),
  `SetupVaultView` (confirm field), `UnlockVaultView` (shake + banner),
  `SecuritySettingsModal` (30s–1h presets, hide-toggle, rekey)
- `cargo test` 19 green (watchdog ticks, rekey IPC flow) · typecheck 0
- Headless E2E: Ctrl+L, wrong-PIN banner, 30s watchdog lock, fake
  ScreenSaver-service D-Bus lock, passphrase change + old-dead/new-works,
  lock-on-tray-hide (X button → unmapped + session lock)

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

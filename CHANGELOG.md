# Changelog

All notable changes to VaultTask are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Fixed

- Tray icon too small on dark top bars: dedicated full-bleed tray asset
  (`icons/tray-icon.png`, oversized glyph) embedded in the binary instead
  of the padded app icon, with fallback to the default window icon

## [0.4.0] - 2026-11-05

### Added

- Unique VaultTask app logo (emerald vault dial + check on dark tile,
  `assets/logo.svg` source): full icon set regenerated via `tauri icon`
  (png/ico/icns + store/mobile variants) replacing the stock Tauri icon
- In-app updates for rpm/deb installs: the Updates card detects
  system-package installs, downloads the distro-native asset from
  `linux-packages.json`, verifies its minisign signature, and installs
  it via PackageKit (`pkcon install-local`, native password prompt),
  then restarts — no terminal needed; falls back to manual
  instructions when PackageKit is missing

### Fixed

- Tray menu no longer flashes on Fedora GNOME: `refresh_tray_state`
  now pushes tooltip/menu updates only when the lock state changed
  instead of rebuilding the StatusNotifier menu every watchdog tick

## [0.3.0] - 2026-11-05

### Added — Phase 10: In-app updater (tauri-plugin-updater + process)

- Self-update from GitHub Releases on Windows (NSIS) and Linux
  (AppImage): signed `latest.json` manifest, minisign verification,
  `TAURI_SIGNING_PRIVATE_KEY` in GitHub Secrets
- Updates & Version settings card (live version badge, Check for
  Updates, progress bar, release-notes preview, Restart to Apply)
  plus a "Check for Updates..." tray menu item; offline/missing
  manifest fails gracefully with a retryable message
- `release.yml` signs bundles, merges per-platform manifests into one
  `latest.json`, and injects CHANGELOG notes before publishing

### Added — Phase 9: Scheduled reminders + privacy-safe notifications

- `tauri-plugin-notification` (Windows toast / freedesktop D-Bus) with
  a 15s Rust scheduler: due = `status != 'done'` + unfired +
  `reminder_at <= now`, marked fired only after successful dispatch
- Privacy-safe payloads: locked vaults (or hidden previews) get a
  generic "VaultTask Reminder" with zero decrypted content; unlocked
  vaults with previews on get "Task Due: \<title\>"
- `reminder_at` (canonical UTC ISO-8601) + `reminder_fired` on tasks,
  `hide_notification_preview` (default ON) in `vault_meta` — all via
  safe ALTER TABLE migration, all carried through `.vault` backups
  (pre-reminder bundles restore with safe defaults)
- Reminder picker in the task pane (presets + calendar + HH:MM),
  `@remind(15m|1h|tomorrow|17:30)` quick-add syntax, and a "Show task
  titles in notifications when unlocked" Settings toggle

## [0.2.0] - 2026-11-05

### Added — Phase 6: Encrypted backup + Markdown export

- Export/import of signed `.vault` backup bundles (SHA-256 integrity,
  password-verified atomic restore, ciphertext only — safe to move
  across Windows / Ubuntu / Fedora machines)
- Unlocked-session Markdown export: one `.md` per task with YAML
  frontmatter (status, priority, due date, tags) plus subtasks and notes
- Backup & Data section in Security settings with native file dialogs

### Added — Phase 7: OS keyring quick-unlock

- Optional quick-unlock via Windows Credential Manager / Linux Secret
  Service (GNOME Keyring, KWallet): derived key stored in the OS
  keyring, verified against the vault verifier, no passphrase typing
- `keyring_enabled` vault flag with automatic migration for existing
  databases; flag carried through encrypted backups
- Settings toggle + Quick Unlock button (Ctrl+U) on the lock screen;
  graceful fallback to PIN with a clear message when the keyring
  daemon is locked or unavailable

### Changed — Phase 8: CI/CD to spec

- CI on pinned `ubuntu-24.04` plus a new `windows-latest` test job
  (`cargo test`, typecheck, Vite build); PRs targeting `main` only
- Release pipeline split into `build-windows` (msi,nsis),
  `build-ubuntu` (deb,appimage), and `build-fedora` (rpm with an
  `rpm -qpR` dependency check); publish attaches all five installers
  plus `SHA256SUMS.txt` with unmatched-files failure enabled

### Fixed — Phase 8: Windows test-harness comctl32 crash

- The dialog plugin (rfd) imports `TaskDialogIndirect` (comctl32
  v6-only). App binaries bind v6 via tauri-build's SxS manifest, but
  the manifest-less unit-test harness bound v5 and died at Windows
  startup with `STATUS_ENTRYPOINT_NOT_FOUND`. `build.rs` now
  delay-loads `comctl32.dll` (stable plain link-args), deferring
  import resolution to first call

## [0.1.0] - 2026-11-04

First release: encrypted tray task manager with Markdown notes for
Windows 11, Ubuntu 24.04+, and Fedora 41+.

### Added — Phase 1: Encrypted backend

- Argon2id (`m=65536,t=3,p=4`) key derivation + AES-256-GCM field
  encryption over a local SQLite store (`vault_meta` + `tasks`)
- `MasterKey` with `ZeroizeOnDrop`; verifier-gated setup/unlock/lock IPC

### Added — Phase 2: Tray + dual-mode window manager

- Persistent tray icon (Open/Maximize/Lock/Quit), OS-aware popover
  positioning, Ctrl+Shift+Space summon, close-to-tray + focus-loss hide
- 420x620 frameless popover ↔ maximized workspace toggle

### Added — Phase 3: List + side pane UI

- Filter rail, quick-add with `!priority` / `@date` shortcuts, task rows
  with status/priority/due/subtask/note indicators
- App-native priority select, calendar date picker, subtasks, Markdown
  note editor with 300ms autosave (sanitized preview)
- `snake_case` IPC contract with mock-runtime tests pinning the
  frontend's exact JSON shapes

### Added — Phase 4: Auto-lock engine + lock screens

- 1s Rust watchdog (inactivity timeout → zeroize + `vault://locked` +
  tray sync), Linux D-Bus session-lock listener, lock-on-tray-hide
- DOM activity tracker (2s-throttled heartbeats), Ctrl/Cmd+L instant lock
- Setup/unlock views (wrong-PIN shake), security settings modal
  (30s–1h presets, hide-toggle, transactional passphrase rekey)

### Added — Phase 5: Packaging + autostart + single-instance

- Launch-on-startup toggle (Windows Run key / Linux autostart entry,
  boots `--minimized` to tray); second launch focuses the existing window
- Installers: `.msi` + NSIS `.exe`, `.deb`, `.rpm`, `.AppImage`

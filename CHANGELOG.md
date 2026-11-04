# Changelog

All notable changes to VaultTask are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added — Phase 6: Encrypted backup + Markdown export

- Export/import of signed `.vault` backup bundles (SHA-256 integrity,
  password-verified atomic restore, ciphertext only — safe to move
  across Windows / Ubuntu / Fedora machines)
- Unlocked-session Markdown export: one `.md` per task with YAML
  frontmatter (status, priority, due date, tags) plus subtasks and notes
- Backup & Data section in Security settings with native file dialogs

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

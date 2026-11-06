# Changelog

All notable changes to VaultTask are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Changed

- README download matrix lists all five installers with direct
  download links; license badge matches the dual MIT/Apache-2.0 texts

### Fixed

- v0.4.3 release backfilled with the nsis/deb/appimage bundles that
  the msi+rpm test track omitted (v0.4.4+ ships all five natively)
- Popover window now reopens at its last dragged position instead of
  the screen center / tray corner (position persists across
  hide, summon, and restart; stale off-screen saves fall back to the
  tray anchor). Note: native Wayland compositors ignore client
  window positioning, so restore is fully effective on Windows, X11,
  and XWayland (`GDK_BACKEND=x11`), and a graceful no-op on native
  Wayland

## [0.4.4] - 2026-11-05

### Changed

- Release pipeline builds the full installer matrix again (msi + nsis
  + deb + rpm + appimage); the msi+rpm test track is over

### Fixed

- In-app PackageKit/MSI update failed at Download with
  `download_system_update_cmd missing required key file_name`: the
  frontend sent camelCase `fileName` (matching the struct's serialized
  shape) but Tauri matches command args exactly (`file_name`). All
  other updater invokes audited — no further mismatches. Dev binaries
  classify as Manual, so E2E never drove this path; two test-only env
  hatches (`VAULT_TASK_INSTALL_KIND`,
  `VAULT_TASK_PKG_MANIFEST_URL` + same-origin allowlist) now let
  headless E2E run check → download → signature-verify against a
  local fixture (proven live: fixture v9.9.9,
  "Download complete — signature verified").

## [0.4.3] - 2026-11-05

### Fixed

- Tray icon missing on GNOME (v0.4.1+): the menu-less StatusNotifier
  item never reaches the AppIndicator extension's `ready` gate, so no
  icon appears. Linux now ships a minimal static tray menu (Show/Hide,
  Check for Updates, Quit); Windows/macOS keep the menu-less
  click-toggle. Menu labels are static (built once, never rebuilt), so
  the push-only-on-change flash fix still holds.

## [0.4.2] - 2026-11-05

### Fixed

- In-app package downloads timed out on slow links: the 20s HTTP
  budget covered the whole body read, killing any 6MB+ fetch under
  ~300KB/s. Package downloads now get 600s; manifests keep 20s.
  Download failures also surface the real backend reason (timeout vs
  signature vs network) instead of one generic message

### Added

- MSI installs self-update in-app (assisted): the updater detects MSI
  via the uninstall registry, verifies the new package against
  `windows-packages.json`, then opens the MSI wizard and exits so no
  files are locked (the Tauri plugin is NSIS-only on Windows)
- 15 package-updater tests (registry classify, MSI manifest select,
  wizard gating); Windows side proven via GNU cross-compile +
  Windows CI, on-device wizard run still pending

### Changed

- Releases ship msi + rpm installers only (testing focus); manifests
  tolerate any bundle subset so other formats can return later

## [0.4.1] - 2026-11-05

### Changed

- OneDrive-style tray: the tray item carries no menu, so left-click
  always toggles the flyout (GNOME shells show an item's menu on
  left-click whenever one exists). Quit moved in-app — titlebar ⋯ menu
  and a Settings row (immediate exit, vault stays encrypted at rest);
  Check for Updates lives in the ⋯ menu too

### Fixed

- Tray icon too small on dark top bars: dedicated full-bleed tray asset
  (`icons/tray-icon.png`, oversized glyph) embedded in the binary instead
  of the padded app icon, with fallback to the default window icon
- OneDrive flyout deletes the tray menu (net −12 lines): left-click
  always toggles, Quit + Check for Updates move in-app (titlebar ⋯
  menu, Settings row); before/after icon sheets + maximize/detach
  verified headless

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
- 11 package-updater unit tests (mock `pkcon`, signed-vector verify);
  headless E2E: Updates card branches screenshot-verified (real
  `pkcon` + polkit prompt needs on-device proving)

### Fixed

- Tray menu no longer flashes on Fedora GNOME: `refresh_tray_state`
  now pushes tooltip/menu updates only when the lock state changed
  instead of rebuilding the StatusNotifier menu every watchdog tick
  (root-caused via 40-frame pixel-diff of a screen recording: the open
  popup re-rendered on every ~1s tick)

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
- 38 tests green; headless E2E: version badge renders, missing-manifest
  check fails gracefully with a retryable message (booting headless
  needs a session bus via `dbus-daemon --session`); the v2 bundler
  emits only `.sig` sidecars, so `latest.json` is generated in the
  publish job and verified live (URLs 200, minisign envelopes valid)

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
- 38 tests green; headless E2E with a D-Bus notification catcher:
  generic/titled/locked payload bytes, fired flags, picker, and
  toggle all verified live

## [0.2.0] - 2026-11-05

### Added — Phase 6: Encrypted backup + Markdown export

- Export/import of signed `.vault` backup bundles (SHA-256 integrity,
  password-verified atomic restore, ciphertext only — safe to move
  across Windows / Ubuntu / Fedora machines)
- Unlocked-session Markdown export: one `.md` per task with YAML
  frontmatter (status, priority, due date, tags) plus subtasks and notes
- Backup & Data section in Security settings with native file dialogs
- 25 tests green; headless E2E: modal renders all three actions,
  export/import/markdown proven through the real IPC command layer
  with real files

### Added — Phase 7: OS keyring quick-unlock

- Optional quick-unlock via Windows Credential Manager / Linux Secret
  Service (GNOME Keyring, KWallet): derived key stored in the OS
  keyring, verified against the vault verifier, no passphrase typing
- `keyring_enabled` vault flag with automatic migration for existing
  databases; flag carried through encrypted backups
- Settings toggle + Quick Unlock button (Ctrl+U) on the lock screen;
  graceful fallback to PIN with a clear message when the keyring
  daemon is locked or unavailable
- 32 tests green (mock-credential roundtrips, pre-keyring migration);
  headless E2E: enable without a daemon fails gracefully (flag stays
  0, no panic), flag-on shows the button, Ctrl+U falls back to PIN

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
- Mock IPC harness uses the platform-correct origin URL
  (`http://tauri.localhost` on Windows, `tauri://localhost` elsewhere);
  32/32 tests green on Windows CI

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
- 12 unit tests (anchor math, shortcut combo); headless E2E
  (Xvfb + openbox + xdotool): popover renders, maximize → 1280x800,
  restore → 420x620, hide-to-tray keeps the process alive

### Added — Phase 3: List + side pane UI

- Filter rail, quick-add with `!priority` / `@date` shortcuts, task rows
  with status/priority/due/subtask/note indicators
- App-native priority select, calendar date picker, subtasks, Markdown
  note editor with 300ms autosave (sanitized preview)
- `snake_case` IPC contract with mock-runtime tests pinning the
  frontend's exact JSON shapes
- 14 tests green; headless E2E: 5 tasks via quick-add, drawer
  open/close, note autosave persisted (36B ciphertext), maximized
  3-pane + docked detail, restart persistence

### Added — Phase 4: Auto-lock engine + lock screens

- 1s Rust watchdog (inactivity timeout → zeroize + `vault://locked` +
  tray sync), Linux D-Bus session-lock listener, lock-on-tray-hide
- DOM activity tracker (2s-throttled heartbeats), Ctrl/Cmd+L instant lock
- Setup/unlock views (wrong-PIN shake), security settings modal
  (30s–1h presets, hide-toggle, transactional passphrase rekey)
- 19 tests green (watchdog ticks, rekey IPC flow); headless E2E: Ctrl+L,
  wrong-PIN banner, 30s watchdog lock, fake ScreenSaver-service D-Bus
  lock, passphrase change (old dead / new works), lock-on-tray-hide

### Added — Phase 5: Packaging + autostart + single-instance

- Launch-on-startup toggle (Windows Run key / Linux autostart entry,
  boots `--minimized` to tray); second launch focuses the existing window
- Installers: `.msi` + NSIS `.exe`, `.deb`, `.rpm`, `.AppImage`
  (deb: `libwebkit2gtk-4.1-0` + `libayatana-appindicator3-1`; rpm:
  `webkit2gtk4.1` + `libappindicator-gtk3`, recommends the GNOME
  AppIndicator shell extension)
- 19 tests green; headless E2E: 2nd instance exits after D-Bus
  handoff, `--minimized` boots unmapped to tray, Startup toggle
  creates/removes the autostart entry

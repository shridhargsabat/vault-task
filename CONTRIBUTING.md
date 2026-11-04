# Contributing to VaultTask

> This project follows a [Code of Conduct](CODE_OF_CONDUCT.md). By
> participating you agree to uphold it. Found a security issue? Please report
> it privately per [SECURITY.md](SECURITY.md), not as a public issue.

Bug reports, OS-compatibility fixes, and pull requests are welcome.

## Ground rules

- **The MasterKey never leaves Rust.** Decrypted bytes cross the IPC
  boundary to the UI only as needed, and the lock path must purge them
  from the store. Changes touching `crypto.rs`, `state.rs`, or the
  watchdog's lock funnel need extra review care.
- **IPC is snake_case with contract tests.** Tauri v2 defaults command
  args to camelCase and silently drops mismatched `Option` args — every
  command keeps `rename_all = "snake_case"`, and new commands need a case
  in `src-tauri/src/ipc_contract.rs` proving the frontend's exact JSON
  shape arrives intact.
- **Prove it with gates.** Every PR must show `cargo test`, `cargo clippy
  --all-targets -- -D warnings`, `cargo fmt --all -- --check` (in
  `src-tauri/`), plus `npm run typecheck` and `npm run build`. UI changes
  should include before/after screenshots on at least one platform.
- **No native dialogs or selects.** The app is frameless and app-native;
  use the in-app components (calendar, selects, modals), never
  `window.confirm` or OS popups.

## Setup

```bash
git clone https://github.com/shridhargsabat/vault-task.git && cd vault-task

# Linux system deps (Tauri WebKitGTK + tray)
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  librsvg2-dev libssl-dev   # Ubuntu; see README for Fedora/Windows

npm ci
npm run typecheck && npm run build

cd src-tauri
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

Run the desktop app in dev mode with `npm run tauri dev` from the repo
root (needs a display; headless CI uses Xvfb — see the E2E notes in
`README.md`).

## Good first issues

Look for the [`good first issue`](https://github.com/shridhargsabat/vault-task/labels/good%20first%20issue)
label — small, well-scoped starting points (UI polish, new task filters,
packaging). Comment on the issue before starting work so efforts don't
overlap.

## Release process (maintainers)

1. Bump `version` in `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`,
   and `package.json` (all three must match; the release preflight
   enforces it), and add a `CHANGELOG.md` entry.
2. Tag `vX.Y.Z` and push the tag — `release.yml` builds the `.msi`/`.exe`
   (Windows) and `.deb`/`.rpm`/`.AppImage` (Linux) bundles and publishes
   the GitHub Release.
3. Verify a clean install on at least Windows 11 + Ubuntu (vault setup,
   unlock, auto-lock), then update the README install notes.

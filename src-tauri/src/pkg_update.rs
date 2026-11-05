//! Native Linux package updates (rpm/deb) via PackageKit.
//!
//! `tauri-plugin-updater` is AppImage-only on Linux, and system packages
//! installed to `/usr` need root to replace — so for rpm/deb installs this
//! module downloads the distro-native asset and installs it through
//! `pkcon install-local` (PackageKit), which raises the session's native
//! polkit password prompt. Same mechanism GNOME Software uses: check,
//! download, password prompt, install, restart.
//!
//! Security model (mirrors the plugin): HTTPS fetch + MANDATORY minisign
//! signature verification with the embedded release pubkey BEFORE the
//! payload ever reaches the installer. URL allowlisting is defense in
//! depth; the signature is the real gate — a tampered/redirected payload
//! fails verification and is never installed.
//!
//! The manifest (`linux-packages.json`, published beside `latest.json`)
//! is deliberately a separate file: the plugin's `latest.json` schema has
//! fixed platform keys, and unknown keys could break its deserialization.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};

/// Published by `release.yml` beside `latest.json` on every tagged release.
pub const PACKAGES_MANIFEST_URL: &str =
    "https://github.com/shridhargsabat/vault-task/releases/latest/download/linux-packages.json";

/// Release minisign pubkey — the SAME base64 blob as tauri.conf.json
/// `plugins.updater.pubkey` (public material, shipped in every bundle).
const UPDATE_PUBKEY_B64: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDdEMjAxNEUxMzZFNDA5MUYKUldRZkNlUTI0UlFnZlo0UUtxWUdtWHVBWEpQS1lzRjBUTkJjQjhBak93dnl1cFpjUEdSVU5qR2UK";

const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_PACKAGE_BYTES: u64 = 128 * 1024 * 1024;
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);
const PKCON_TIMEOUT: Duration = Duration::from_secs(600);
const STAGED_PREFIX: &str = "vaulttask-update-";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Which update flow the frontend should use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallKind {
    /// NSIS/AppImage/macOS — the tauri updater plugin handles everything.
    PluginManaged,
    /// rpm/deb in /usr — download + PackageKit install + relaunch.
    SystemPackage,
    /// Dev builds, portables, unknown distros — manual download link.
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DistroFamily {
    Rpm,
    Deb,
}

impl DistroFamily {
    fn asset_key(self) -> &'static str {
        match self {
            DistroFamily::Rpm => "rpm",
            DistroFamily::Deb => "deb",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            DistroFamily::Rpm => "rpm",
            DistroFamily::Deb => "deb",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallInfo {
    pub kind: InstallKind,
    pub family: Option<DistroFamily>,
    pub packagekit: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PackageManifest {
    pub version: String,
    #[serde(default)]
    pub notes: String,
    #[allow(dead_code)] // Manifest schema field; read by future UI, kept for format fidelity.
    pub pub_date: String,
    pub packages: HashMap<String, PackageAsset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PackageAsset {
    pub url: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemUpdate {
    pub version: String,
    pub notes: String,
    pub url: String,
    pub file_name: String,
    /// Minisign signature blob (public, published in the manifest).
    /// Returned so download can verify; a mismatched url fails verification.
    pub signature: String,
}

// ---------------------------------------------------------------------------
// Detection (pure core + thin env wrapper, so unit tests pin the logic)
// ---------------------------------------------------------------------------

/// Pure install-kind classifier over injected environment facts.
pub fn classify_install(
    appimage_env: bool,
    exe_path: &str,
    os_release: Option<&str>,
    pkcon_present: bool,
) -> InstallInfo {
    if appimage_env {
        return InstallInfo {
            kind: InstallKind::PluginManaged,
            family: None,
            packagekit: pkcon_present,
        };
    }
    #[cfg(target_os = "windows")]
    {
        let _ = (exe_path, os_release);
        return InstallInfo {
            kind: InstallKind::PluginManaged,
            family: None,
            packagekit: false,
        };
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (exe_path, os_release);
        return InstallInfo {
            kind: InstallKind::PluginManaged,
            family: None,
            packagekit: false,
        };
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let under_usr = exe_path == "/usr" || exe_path.starts_with("/usr/");
        if !under_usr {
            return InstallInfo {
                kind: InstallKind::Manual,
                family: None,
                packagekit: pkcon_present,
            };
        }
        match os_release.and_then(distro_family_from_os_release) {
            Some(family) => InstallInfo {
                kind: InstallKind::SystemPackage,
                family: Some(family),
                packagekit: pkcon_present,
            },
            None => InstallInfo {
                kind: InstallKind::Manual,
                family: None,
                packagekit: pkcon_present,
            },
        }
    }
}

/// Parse `/etc/os-release` content into a package family.
/// Checks `ID` first, then `ID_LIKE` tokens (e.g. Pop!_OS → ubuntu → deb,
/// Nobara → fedora → rpm). Unknown distros (Arch, NixOS, …) yield None.
pub fn distro_family_from_os_release(content: &str) -> Option<DistroFamily> {
    const RPM_IDS: &[&str] = &[
        "fedora",
        "rhel",
        "centos",
        "rocky",
        "alma",
        "almalinux",
        "ol",
        "opensuse",
        "suse",
        "sles",
        "mageia",
        "nobara",
    ];
    const DEB_IDS: &[&str] = &[
        "debian",
        "ubuntu",
        "linuxmint",
        "mint",
        "pop",
        "popos",
        "zorin",
        "kali",
        "raspbian",
        "elementary",
    ];

    let mut id: Option<&str> = None;
    let mut id_like: Option<&str> = None;
    for line in content.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("ID=") {
            id = Some(v.trim_matches('"').trim());
        } else if let Some(v) = line.strip_prefix("ID_LIKE=") {
            id_like = Some(v.trim_matches('"').trim());
        }
    }
    let classify = |token: &str| {
        let t = token.to_lowercase();
        if RPM_IDS.contains(&t.as_str()) {
            Some(DistroFamily::Rpm)
        } else if DEB_IDS.contains(&t.as_str()) {
            Some(DistroFamily::Deb)
        } else {
            None
        }
    };
    if let Some(hit) = id.and_then(classify) {
        return Some(hit);
    }
    id_like
        .unwrap_or("")
        .split_whitespace()
        .flat_map(|t| t.split(','))
        .find_map(classify)
}

/// `true` when `candidate` is strictly newer than `current` (semver).
/// Unparseable versions never trigger an update.
pub fn is_newer(current: &str, candidate: &str) -> bool {
    match (
        semver::Version::parse(current),
        semver::Version::parse(candidate),
    ) {
        (Ok(cur), Ok(cand)) => cand > cur,
        _ => false,
    }
}

/// Locate `pkcon` via PATH scan (no process spawn — cheap enough to call
/// on every settings open).
pub fn resolve_pkcon() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    #[cfg(windows)]
    let exe = "pkcon.exe";
    #[cfg(not(windows))]
    let exe = "pkcon";
    std::env::split_paths(&path)
        .map(|dir| dir.join(exe))
        .find(|p| p.is_file())
}

/// Thin env wrapper over [`classify_install`] — the only untested seam.
pub fn detect_install() -> InstallInfo {
    let appimage = std::env::var_os("APPIMAGE").is_some();
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let os_release = std::fs::read_to_string("/etc/os-release").ok();
    classify_install(
        appimage,
        &exe,
        os_release.as_deref(),
        resolve_pkcon().is_some(),
    )
}

// ---------------------------------------------------------------------------
// Fetch + verify
// ---------------------------------------------------------------------------

fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .into()
}

/// Fetch a URL to memory with a hard byte cap (defense against hostile
/// length lies — the reader is truncated, never unbounded).
fn fetch_capped(url: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
    if !url.starts_with("https://github.com/") {
        return Err("refusing non-GitHub update URL".to_string());
    }
    let res = http_agent()
        .get(url)
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    if let Some(len) = res.body().content_length()
        && len > max_bytes
    {
        return Err(format!("payload too large ({len} bytes)"));
    }
    let mut reader = res.into_body().into_reader().take(max_bytes + 1);
    let mut buf = Vec::new();
    reader
        .read_to_end(&mut buf)
        .map_err(|e| format!("download failed: {e}"))?;
    if buf.len() as u64 > max_bytes {
        return Err("payload exceeded size cap".to_string());
    }
    Ok(buf)
}

/// Verify a payload against a manifest signature blob — byte-for-byte the
/// plugin's chain: base64-decode both sides, minisign-verify, allow legacy.
pub fn verify_signature(payload: &[u8], signature_b64: &str) -> Result<(), String> {
    let key_text = B64
        .decode(UPDATE_PUBKEY_B64.trim())
        .map_err(|_| "embedded pubkey corrupt".to_string())?;
    let key_text =
        String::from_utf8(key_text).map_err(|_| "embedded pubkey corrupt".to_string())?;
    let public_key =
        PublicKey::decode(&key_text).map_err(|e| format!("embedded pubkey invalid: {e:?}"))?;

    let sig_text = B64
        .decode(signature_b64.trim())
        .map_err(|_| "signature is not valid base64".to_string())?;
    let sig_text =
        String::from_utf8(sig_text).map_err(|_| "signature is not valid text".to_string())?;
    let signature =
        Signature::decode(&sig_text).map_err(|e| format!("signature malformed: {e:?}"))?;

    public_key
        .verify(payload, &signature, true)
        .map_err(|e| format!("signature verification failed: {e:?}"))
}

/// Pure check step: parse + select asset + compare versions.
pub fn check_against(
    manifest_json: &str,
    family: DistroFamily,
    current_version: &str,
) -> Result<Option<SystemUpdate>, String> {
    let manifest: PackageManifest =
        serde_json::from_str(manifest_json).map_err(|e| format!("update manifest invalid: {e}"))?;
    if !is_newer(current_version, &manifest.version) {
        return Ok(None);
    }
    let asset = manifest.packages.get(family.asset_key()).ok_or_else(|| {
        format!(
            "release {} has no {} package",
            manifest.version,
            family.asset_key()
        )
    })?;
    let file_name = asset
        .url
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .ok_or_else(|| "package URL malformed".to_string())?
        .to_string();
    if !file_name.ends_with(&format!(".{}", family.extension())) {
        return Err("package URL has wrong extension".to_string());
    }
    Ok(Some(SystemUpdate {
        version: manifest.version,
        notes: manifest.notes,
        url: asset.url.clone(),
        file_name,
        signature: asset.signature.clone(),
    }))
}

/// Fetch the manifest and check for an update (thin wrapper).
pub fn check_system_update() -> Result<Option<SystemUpdate>, String> {
    let info = detect_install();
    if info.kind != InstallKind::SystemPackage {
        return Err("not a system-package install".to_string());
    }
    let family = info
        .family
        .ok_or_else(|| "unsupported distro".to_string())?;
    let bytes = fetch_capped(PACKAGES_MANIFEST_URL, MAX_MANIFEST_BYTES)?;
    let text = String::from_utf8(bytes).map_err(|_| "update manifest invalid".to_string())?;
    check_against(&text, family, env!("CARGO_PKG_VERSION"))
}

/// Download + verify + stage a package into the temp dir. Returns the
/// staged path. The signature is ALWAYS verified before the path is
/// returned — callers can never install an unverified file.
pub fn download_system_update(
    url: &str,
    file_name: &str,
    signature_b64: &str,
) -> Result<PathBuf, String> {
    if file_name.contains('/') || file_name.contains("..") {
        return Err("package file name invalid".to_string());
    }
    let info = detect_install();
    if info.kind != InstallKind::SystemPackage {
        return Err("not a system-package install".to_string());
    }
    let family = info
        .family
        .ok_or_else(|| "unsupported distro".to_string())?;
    if !file_name.ends_with(&format!(".{}", family.extension())) {
        return Err("package extension does not match distro".to_string());
    }
    let bytes = fetch_capped(url, MAX_PACKAGE_BYTES)?;
    verify_signature(&bytes, signature_b64)?;
    let staged = std::env::temp_dir().join(format!("{STAGED_PREFIX}{file_name}"));
    std::fs::write(&staged, &bytes).map_err(|e| format!("could not stage package: {e}"))?;
    Ok(staged)
}

// ---------------------------------------------------------------------------
// Install via PackageKit
// ---------------------------------------------------------------------------

/// Validate a staged path before it ever reaches pkcon: must be the exact
/// temp-dir file we staged (prefix + rpm/deb extension, no traversal).
pub fn validate_install_path(path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);
    let file_name = p
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "install path invalid".to_string())?;
    if !file_name.starts_with(STAGED_PREFIX) {
        return Err("not a staged update package".to_string());
    }
    if !(file_name.ends_with(".rpm") || file_name.ends_with(".deb")) {
        return Err("not a staged update package".to_string());
    }
    if p.parent() != Some(std::env::temp_dir().as_path()) {
        return Err("install path outside temp dir".to_string());
    }
    if !p.is_file() {
        return Err("staged package missing — re-download the update".to_string());
    }
    Ok(p.to_path_buf())
}

fn drain_to_string<R: Read>(mut r: R) -> String {
    let mut s = String::new();
    let _ = r.read_to_string(&mut s);
    s
}

/// Run a command with piped output and a kill-on-timeout watchdog.
/// std-only: drain threads prevent pipe deadlock, polling `try_wait`
/// keeps the `Child` handle available for `kill`.
fn run_with_timeout(
    mut cmd: std::process::Command,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    use std::process::Stdio;
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not start installer: {e}"))?;
    let stdout_handle = std::thread::spawn({
        let out = child.stdout.take();
        move || out.map(drain_to_string).unwrap_or_default()
    });
    let stderr_handle = std::thread::spawn({
        let err = child.stderr.take();
        move || err.map(drain_to_string).unwrap_or_default()
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = stdout_handle.join().unwrap_or_default();
                let stderr = stderr_handle.join().unwrap_or_default();
                return Ok(std::process::Output {
                    status,
                    stdout: stdout.into_bytes(),
                    stderr: stderr.into_bytes(),
                });
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout_handle.join();
                    let _ = stderr_handle.join();
                    return Err("installer timed out".to_string());
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(e) => return Err(format!("installer failed: {e}")),
        }
    }
}

fn pkcon_cancelled(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    [
        "cancelled",
        "canceled",
        "dismissed",
        "not authorized",
        "authorization failed",
    ]
    .iter()
    .any(|m| lower.contains(m))
}

/// Install a staged package through PackageKit (thin wrapper).
/// The polkit password prompt appears as a native OS dialog owned by the
/// session agent — the app just waits for pkcon to exit.
pub fn install_package_file(staged: &Path, pkcon: &Path) -> Result<(), String> {
    let mut cmd = std::process::Command::new(pkcon);
    cmd.arg("install-local").arg("--noninteractive").arg(staged);
    let out = run_with_timeout(cmd, PKCON_TIMEOUT)?;
    if out.status.success() {
        let _ = std::fs::remove_file(staged);
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    if pkcon_cancelled(&stderr) {
        return Err("cancelled — no changes were made".to_string());
    }
    let tail = stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("unknown error")
        .trim();
    let short: String = tail.chars().take(300).collect();
    Err(format!("install failed: {short}"))
}

/// Validate + resolve + install (thin wrapper for the IPC command).
pub fn install_system_package(path: &str) -> Result<(), String> {
    let staged = validate_install_path(path)?;
    let pkcon =
        resolve_pkcon().ok_or_else(|| "PackageKit (pkcon) not found on this system".to_string())?;
    install_package_file(&staged, &pkcon)
}

// ---------------------------------------------------------------------------
// IPC commands
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
pub fn get_install_info() -> InstallInfo {
    detect_install()
}

#[tauri::command(rename_all = "snake_case")]
pub fn check_system_update_cmd() -> Result<Option<SystemUpdate>, String> {
    check_system_update()
}

#[tauri::command(rename_all = "snake_case")]
pub fn download_system_update_cmd(
    url: String,
    file_name: String,
    signature: String,
) -> Result<String, String> {
    download_system_update(&url, &file_name, &signature).map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command(rename_all = "snake_case")]
pub fn install_system_package_cmd(path: String) -> Result<(), String> {
    install_system_package(&path)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_appimage_uses_plugin_flow() {
        let info = classify_install(true, "/tmp/.mount_Foo/usr/bin/vault-task", None, false);
        assert_eq!(info.kind, InstallKind::PluginManaged);
        assert_eq!(info.family, None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn classify_usr_install_with_distro_is_system_package() {
        let os = "NAME=\"Fedora Linux\"\nID=fedora\nID_LIKE=\"rhel centos fedora\"\n";
        let info = classify_install(false, "/usr/bin/vault-task", Some(os), true);
        assert_eq!(info.kind, InstallKind::SystemPackage);
        assert_eq!(info.family, Some(DistroFamily::Rpm));
        assert!(info.packagekit);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn classify_dev_paths_are_manual() {
        for exe in [
            "/root/vault-task/src-tauri/target/debug/vault-task",
            "/home/u/.local/bin/vault-task",
            "/opt/vault-task/vault-task",
        ] {
            let info = classify_install(false, exe, None, true);
            assert_eq!(info.kind, InstallKind::Manual, "for {exe}");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn classify_unknown_distro_is_manual() {
        let os = "NAME=\"Arch Linux\"\nID=arch\n";
        let info = classify_install(false, "/usr/bin/vault-task", Some(os), false);
        assert_eq!(info.kind, InstallKind::Manual);
        assert_eq!(info.family, None);
    }

    #[test]
    fn os_release_id_wins_over_id_like() {
        // Nobara: ID=nobara (rpm family) despite rhel-like ID_LIKE.
        let os = "ID=nobara\nID_LIKE=\"rhel centos fedora\"\n";
        assert_eq!(distro_family_from_os_release(os), Some(DistroFamily::Rpm));
        // Pop!_OS: ID=pop (unknown) falls back to ID_LIKE=ubuntu (deb).
        let os = "ID=pop\nID_LIKE=\"ubuntu debian\"\n";
        assert_eq!(distro_family_from_os_release(os), Some(DistroFamily::Deb));
        // Quoted values + unknown distro.
        assert_eq!(
            distro_family_from_os_release("ID=\"ubuntu\"\n"),
            Some(DistroFamily::Deb)
        );
        assert_eq!(distro_family_from_os_release("ID=arch\n"), None);
        assert_eq!(distro_family_from_os_release("garbage\n"), None);
    }

    #[test]
    fn version_gate_is_strict_semver() {
        assert!(is_newer("0.3.0", "0.4.0"));
        assert!(is_newer("0.3.0", "0.3.1"));
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(!is_newer("0.4.0", "0.3.9"));
        assert!(!is_newer("0.3.0", "not-a-version"));
        assert!(!is_newer("bogus", "0.4.0"));
    }

    const MANIFEST_FIXTURE: &str = r#"{
        "version": "0.4.0",
        "notes": "Package updater!",
        "pub_date": "2026-11-05T00:00:00Z",
        "packages": {
            "rpm": {"url": "https://github.com/shridhargsabat/vault-task/releases/download/v0.4.0/VaultTask-0.4.0-1.x86_64.rpm", "signature": "SIG"},
            "deb": {"url": "https://github.com/shridhargsabat/vault-task/releases/download/v0.4.0/VaultTask_0.4.0_amd64.deb", "signature": "SIG"}
        }
    }"#;

    #[test]
    fn check_selects_distro_asset_and_gates_version() {
        let hit = check_against(MANIFEST_FIXTURE, DistroFamily::Rpm, "0.3.0")
            .unwrap()
            .unwrap();
        assert_eq!(hit.version, "0.4.0");
        assert!(hit.file_name.ends_with(".rpm"));

        let hit = check_against(MANIFEST_FIXTURE, DistroFamily::Deb, "0.3.0")
            .unwrap()
            .unwrap();
        assert!(hit.file_name.ends_with(".deb"));

        // Same version → None, no asset needed.
        assert!(
            check_against(MANIFEST_FIXTURE, DistroFamily::Rpm, "0.4.0")
                .unwrap()
                .is_none()
        );

        // Missing asset for the family → hard error, not a wrong file.
        let bare = r#"{"version":"0.4.0","notes":"","pub_date":"","packages":{}}"#;
        assert!(check_against(bare, DistroFamily::Rpm, "0.3.0").is_err());
        assert!(serde_json::from_str::<PackageManifest>("nope").is_err());
    }

    /// Signed with the REAL release key (`tauri signer sign` over the exact
    /// bytes below) — proves the verify chain accepts production signatures.
    const FIXTURE_DATA: &[u8] = b"vault-task pkg-update signature fixture v1";
    const FIXTURE_SIG_B64: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVRZkNlUTI0UlFnZmJVdzk0OEJic1NkaUhjZDdHUmFDUk5zWHhSOGdWMmxnZmJmVWdMM2phcFE1bjBUeXdmTTVFcHkzR055TWpPbjhxZ0hOUUdwUFFzV3JwSnkrYkhnK3dvPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkzODcwNDAyCWZpbGU6dnQtcGtnLWZpeHR1cmUuYmluCjJ3aVIxYUgwNmR6VWswMWQzMGxGaXRMOVhzN2JaTUNrRDk1cjZFS2hGNmpzWGtWZnhLd0NONkhtS3NmTEZHZUdadEwyOW80QW1SdTlMYThTcExkd0JBPT0K";

    #[test]
    fn real_release_signature_verifies_and_tampering_fails() {
        assert!(verify_signature(FIXTURE_DATA, FIXTURE_SIG_B64).is_ok());
        // One flipped byte → reject.
        let mut tampered = FIXTURE_DATA.to_vec();
        tampered[0] ^= 1;
        assert!(verify_signature(&tampered, FIXTURE_SIG_B64).is_err());
        // Garbage signature → reject, no panic.
        assert!(verify_signature(FIXTURE_DATA, "aGVsbG8=").is_err());
        assert!(verify_signature(FIXTURE_DATA, "!!!not-base64!!!").is_err());
    }

    #[test]
    fn install_path_validation_rejects_escapes() {
        // Wrong prefix / extension / location all rejected before pkcon.
        assert!(validate_install_path("/tmp/evil.rpm").is_err());
        assert!(validate_install_path("/tmp/vaulttask-update-x.AppImage").is_err());
        assert!(validate_install_path("/etc/vaulttask-update-x.rpm").is_err());
        assert!(validate_install_path("/tmp/sub/vaulttask-update-x.rpm").is_err());
        // Correct shape but missing file → re-download hint.
        let err = validate_install_path(
            &std::env::temp_dir()
                .join("vaulttask-update-missing-1.rpm")
                .to_string_lossy(),
        )
        .unwrap_err();
        assert!(err.contains("re-download"), "got: {err}");
    }

    /// Mock-pkcon runner test: PATH is untouched (the mock path is passed
    /// directly), so parallel tests cannot interfere.
    #[cfg(unix)]
    fn write_mock_pkcon(dir: &Path, tag: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        // Unique per test + case: parallel tests share the process id.
        let p = dir.join(format!("pkcon-mock-{}-{}", std::process::id(), tag));
        std::fs::write(&p, body).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    #[cfg(unix)]
    #[test]
    fn install_runner_maps_success_cancel_and_failure() {
        // Stage a fake package the validator accepts.
        let staged = std::env::temp_dir().join(format!(
            "vaulttask-update-runner-test-{}.rpm",
            std::process::id()
        ));
        std::fs::write(&staged, b"fake").unwrap();

        let dir = std::env::temp_dir();

        // Success: exit 0 → Ok + staged file cleaned up.
        let ok = write_mock_pkcon(&dir, "ok", "#!/bin/sh\nexit 0\n");
        assert!(install_package_file(&staged, &ok).is_ok());
        assert!(!staged.exists(), "staged file cleaned after success");
        std::fs::write(&staged, b"fake").unwrap();

        // Failure: exit 1 with stderr → surfaced tail.
        let fail = write_mock_pkcon(&dir, "fail", "#!/bin/sh\necho boom >&2\nexit 1\n");
        let err = install_package_file(&staged, &fail).unwrap_err();
        assert!(err.contains("boom"), "got: {err}");

        // Cancel: polkit dismissal phrasing → friendly message.
        let cancel = write_mock_pkcon(
            &dir,
            "cancel",
            "#!/bin/sh\necho 'Authorization failed: cancelled' >&2\nexit 1\n",
        );
        let err = install_package_file(&staged, &cancel).unwrap_err();
        assert!(err.contains("cancelled"), "got: {err}");

        std::fs::remove_file(&staged).ok();
        std::fs::remove_file(&ok).ok();
        std::fs::remove_file(&fail).ok();
        std::fs::remove_file(&cancel).ok();
    }

    #[cfg(unix)]
    #[test]
    fn install_runner_kills_on_timeout() {
        let staged = std::env::temp_dir().join(format!(
            "vaulttask-update-timeout-test-{}.rpm",
            std::process::id()
        ));
        std::fs::write(&staged, b"fake").unwrap();
        let dir = std::env::temp_dir();
        // `exec` replaces the shell so kill() reaps the sleeper directly —
        // otherwise the orphaned grandchild holds the pipes past the timeout.
        let slow = write_mock_pkcon(&dir, "slow", "#!/bin/sh\nexec sleep 300\n");
        let mut cmd = std::process::Command::new(&slow);
        cmd.arg("install-local").arg(&staged);
        let err = run_with_timeout(cmd, Duration::from_millis(600)).unwrap_err();
        assert!(err.contains("timed out"), "got: {err}");
        std::fs::remove_file(&staged).ok();
        std::fs::remove_file(&slow).ok();
    }
}

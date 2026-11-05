//! Native package updates: rpm/deb via PackageKit, MSI via assisted wizard.
//!
//! `tauri-plugin-updater` is AppImage-only on Linux and NSIS-only on
//! Windows. System installs need their own flows, sharing one security
//! model: HTTPS fetch + MANDATORY minisign verification with the embedded
//! release pubkey BEFORE the payload ever reaches the installer.
//!
//! - rpm/deb in `/usr`: download the distro-native asset, install through
//!   `pkcon install-local` (PackageKit raises the native polkit prompt).
//! - MSI in Program Files: download + verify, then spawn `msiexec /i`
//!   (full wizard UI) and exit the app so no files are locked.
//!
//! The manifests (`linux-packages.json` / `windows-packages.json`,
//! published beside `latest.json`) are deliberately separate files: the
//! plugin's `latest.json` schema has fixed platform keys, and unknown
//! keys could break its deserialization.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};

/// Published by `release.yml` beside `latest.json` on every tagged release.
/// Per-OS file (same schema): the pkg flow only ever reads its own.
#[cfg(target_os = "windows")]
pub const PACKAGES_MANIFEST_URL: &str =
    "https://github.com/shridhargsabat/vault-task/releases/latest/download/windows-packages.json";
#[cfg(not(target_os = "windows"))]
pub const PACKAGES_MANIFEST_URL: &str =
    "https://github.com/shridhargsabat/vault-task/releases/latest/download/linux-packages.json";

/// Release minisign pubkey — the SAME base64 blob as tauri.conf.json
/// `plugins.updater.pubkey` (public material, shipped in every bundle).
const UPDATE_PUBKEY_B64: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDdEMjAxNEUxMzZFNDA5MUYKUldRZkNlUTI0UlFnZlo0UUtxWUdtWHVBWEpQS1lzRjBUTkJjQjhBak93dnl1cFpjUEdSVU5qR2UK";

const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_PACKAGE_BYTES: u64 = 128 * 1024 * 1024;
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);
/// Package downloads are megabytes, not kilobytes: the global budget
/// covers the whole body read, so a slow link needs headroom (6MB at
/// 65KB/s is ~100s; the 85MB AppImage needs more).
const PACKAGE_HTTP_TIMEOUT: Duration = Duration::from_secs(600);
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
    /// MSI in Program Files — verified download, then the MSI wizard.
    /// (Constructed only on Windows; the fan-out lives in cfg'd code.)
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    MsiAssisted,
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

/// Which Windows installer owns this copy, read from the uninstall
/// registry. MSI sets `WindowsInstaller=1` on its key; NSIS doesn't.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowsInstallFacts {
    pub msi: bool,
    pub nsis: bool,
}

/// Pure Windows classifier: MSI → assisted flow, NSIS → plugin flow,
/// neither (portable/dev) or both (ambiguous) → manual link.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn classify_windows_install(facts: &WindowsInstallFacts) -> InstallKind {
    match (facts.msi, facts.nsis) {
        (true, false) => InstallKind::MsiAssisted,
        (false, true) => InstallKind::PluginManaged,
        _ => InstallKind::Manual,
    }
}

#[cfg(target_os = "windows")]
/// Scan the uninstall registry for our product. Best-effort: unreadable
/// hives/keys are skipped, never fatal.
pub fn read_windows_install_facts() -> WindowsInstallFacts {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    let mut facts = WindowsInstallFacts::default();
    let hives = [
        (
            HKEY_LOCAL_MACHINE,
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
        (
            HKEY_LOCAL_MACHINE,
            "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
        (
            HKEY_CURRENT_USER,
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ),
    ];
    for (hive, path) in hives {
        let Ok(key) = RegKey::predef(hive).open_subkey(path) else {
            continue;
        };
        for name in key.enum_keys().flatten() {
            let Ok(sub) = key.open_subkey(&name) else {
                continue;
            };
            let display: String = sub.get_value("DisplayName").unwrap_or_default();
            if display.eq_ignore_ascii_case("VaultTask") {
                let msi_flag: u32 = sub.get_value("WindowsInstaller").unwrap_or(0);
                if msi_flag == 1 {
                    facts.msi = true;
                } else {
                    facts.nsis = true;
                }
            }
        }
    }
    facts
}

/// Pure install-kind classifier over injected environment facts.
pub fn classify_install(
    appimage_env: bool,
    exe_path: &str,
    os_release: Option<&str>,
    pkcon_present: bool,
    windows: Option<&WindowsInstallFacts>,
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
        let _ = (exe_path, os_release, pkcon_present);
        let kind = windows
            .map(classify_windows_install)
            .unwrap_or(InstallKind::Manual);
        return InstallInfo {
            kind,
            family: None,
            packagekit: false,
        };
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (exe_path, os_release, windows);
        return InstallInfo {
            kind: InstallKind::PluginManaged,
            family: None,
            packagekit: false,
        };
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = windows;
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
/// TEST HATCH: `VAULT_TASK_INSTALL_KIND` (system-package | msi-assisted |
/// plugin-managed | manual) forces the reported kind so headless E2E can
/// drive the native updater flows (dev binaries classify as Manual).
pub fn detect_install() -> InstallInfo {
    if let Ok(forced) = std::env::var("VAULT_TASK_INSTALL_KIND") {
        let kind = match forced.as_str() {
            "system-package" => InstallKind::SystemPackage,
            "msi-assisted" => InstallKind::MsiAssisted,
            "plugin-managed" => InstallKind::PluginManaged,
            "manual" => InstallKind::Manual,
            _ => return classify_fallback(),
        };
        return InstallInfo {
            kind,
            family: if kind == InstallKind::SystemPackage {
                std::fs::read_to_string("/etc/os-release")
                    .ok()
                    .as_deref()
                    .and_then(distro_family_from_os_release)
            } else {
                None
            },
            packagekit: resolve_pkcon().is_some(),
        };
    }
    classify_fallback()
}

/// Real environment classification (no overrides).
fn classify_fallback() -> InstallInfo {
    let appimage = std::env::var_os("APPIMAGE").is_some();
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let os_release = std::fs::read_to_string("/etc/os-release").ok();
    #[cfg(target_os = "windows")]
    let windows = Some(read_windows_install_facts());
    #[cfg(not(target_os = "windows"))]
    let windows: Option<WindowsInstallFacts> = None;
    classify_install(
        appimage,
        &exe,
        os_release.as_deref(),
        resolve_pkcon().is_some(),
        windows.as_ref(),
    )
}

// ---------------------------------------------------------------------------
// Fetch + verify
// ---------------------------------------------------------------------------

fn http_agent_with_timeout(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into()
}

/// Fetch a URL to memory with a hard byte cap (defense against hostile
/// length lies — the reader is truncated, never unbounded).
fn fetch_capped(url: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
    fetch_capped_with_timeout(url, max_bytes, HTTP_TIMEOUT)
}

/// Update fetches only ever touch GitHub release artifacts. TEST HATCH:
/// when `VAULT_TASK_PKG_MANIFEST_URL` overrides the manifest, the
/// fixture's own origin is additionally allowed (the override already
/// fully controls the manifest, so same-origin packages add no trust).
fn update_url_allowed(url: &str) -> bool {
    if url.starts_with("https://github.com/") {
        return true;
    }
    if let Ok(manifest) = std::env::var("VAULT_TASK_PKG_MANIFEST_URL")
        && !manifest.is_empty()
        && let Some(origin) = url_origin(&manifest)
    {
        return url.starts_with(&origin);
    }
    false
}

/// Scheme + authority prefix of a URL (e.g. `http://127.0.0.1:8899`).
fn url_origin(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split('/').next()?;
    if scheme.is_empty() || authority.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{authority}"))
}

fn fetch_capped_with_timeout(
    url: &str,
    max_bytes: u64,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    if !update_url_allowed(url) {
        return Err("refusing non-GitHub update URL".to_string());
    }
    let res = http_agent_with_timeout(timeout)
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

/// Resolve the manifest asset key + file extension for an install.
/// SystemPackage reads the distro family; MsiAssisted is always `msi`.
fn update_asset_spec(info: &InstallInfo) -> Result<(&'static str, &'static str), String> {
    match info.kind {
        InstallKind::SystemPackage => info
            .family
            .map(|f| (f.asset_key(), f.extension()))
            .ok_or_else(|| "unsupported distro".to_string()),
        InstallKind::MsiAssisted => Ok(("msi", "msi")),
        _ => Err("not an updatable install".to_string()),
    }
}

/// Pure check step: parse + select asset + compare versions.
pub fn check_against(
    manifest_json: &str,
    asset_key: &str,
    extension: &str,
    current_version: &str,
) -> Result<Option<SystemUpdate>, String> {
    let manifest: PackageManifest =
        serde_json::from_str(manifest_json).map_err(|e| format!("update manifest invalid: {e}"))?;
    if !is_newer(current_version, &manifest.version) {
        return Ok(None);
    }
    let asset = manifest
        .packages
        .get(asset_key)
        .ok_or_else(|| format!("release {} has no {asset_key} package", manifest.version))?;
    let file_name = asset
        .url
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .ok_or_else(|| "package URL malformed".to_string())?
        .to_string();
    if !file_name.ends_with(&format!(".{extension}")) {
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

/// Manifest URL with a test hatch: `VAULT_TASK_PKG_MANIFEST_URL` (when
/// set and non-empty) overrides the release manifest so headless E2E can
/// serve a fixture locally. Production builds never set it.
fn packages_manifest_url() -> String {
    std::env::var("VAULT_TASK_PKG_MANIFEST_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| PACKAGES_MANIFEST_URL.to_string())
}

/// Fetch the manifest and check for an update (thin wrapper).
pub fn check_system_update() -> Result<Option<SystemUpdate>, String> {
    let info = detect_install();
    let (asset_key, extension) = update_asset_spec(&info)?;
    let bytes = fetch_capped(&packages_manifest_url(), MAX_MANIFEST_BYTES)?;
    let text = String::from_utf8(bytes).map_err(|_| "update manifest invalid".to_string())?;
    check_against(&text, asset_key, extension, env!("CARGO_PKG_VERSION"))
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
    let (_, extension) = update_asset_spec(&info)?;
    if !file_name.ends_with(&format!(".{extension}")) {
        return Err("package extension does not match install".to_string());
    }
    let bytes = fetch_capped_with_timeout(url, MAX_PACKAGE_BYTES, PACKAGE_HTTP_TIMEOUT)?;
    verify_signature(&bytes, signature_b64)?;
    let staged = std::env::temp_dir().join(format!("{STAGED_PREFIX}{file_name}"));
    std::fs::write(&staged, &bytes).map_err(|e| format!("could not stage package: {e}"))?;
    Ok(staged)
}

// ---------------------------------------------------------------------------
// Install via PackageKit
// ---------------------------------------------------------------------------

/// Validate a staged path before it ever reaches an installer: must be
/// the exact temp-dir file we staged (prefix + rpm/deb/msi extension,
/// no traversal).
pub fn validate_install_path(path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);
    let file_name = p
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "install path invalid".to_string())?;
    if !file_name.starts_with(STAGED_PREFIX) {
        return Err("not a staged update package".to_string());
    }
    if !(file_name.ends_with(".rpm") || file_name.ends_with(".deb") || file_name.ends_with(".msi"))
    {
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
// MSI assisted install (Windows)
// ---------------------------------------------------------------------------

/// Spawn the MSI wizard for a staged package. Detached: returns once the
/// installer process exists — the frontend exits the app next so no
/// files are locked when the user clicks through (UAC prompt included).
#[cfg(target_os = "windows")]
pub fn launch_msi_installer(staged: &Path) -> Result<(), String> {
    std::process::Command::new("msiexec")
        .arg("/i")
        .arg(staged)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not launch installer: {e}"))
}

#[cfg(not(target_os = "windows"))]
pub fn launch_msi_installer(_staged: &Path) -> Result<(), String> {
    Err("MSI install is Windows-only".to_string())
}

/// Validate + launch (thin wrapper for the IPC command). Only `.msi`
/// files may ride the wizard path — rpm/deb go through PackageKit.
/// The staged file is deliberately NOT deleted: msiexec reads it after
/// we exit, and the temp dir is OS-cleaned.
pub fn launch_system_installer(path: &str) -> Result<(), String> {
    let staged = validate_install_path(path)?;
    let is_msi = staged
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("msi"));
    if !is_msi {
        return Err("only MSI packages use the installer wizard".to_string());
    }
    launch_msi_installer(&staged)
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

#[tauri::command(rename_all = "snake_case")]
pub fn launch_system_installer_cmd(path: String) -> Result<(), String> {
    launch_system_installer(&path)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_allowlist_github_plus_fixture_origin() {
        assert_eq!(
            url_origin("http://127.0.0.1:8899/linux-packages.json"),
            Some("http://127.0.0.1:8899".to_string())
        );
        assert_eq!(url_origin("not a url"), None);
        // SAFETY: no other test reads or writes this var.
        unsafe {
            std::env::remove_var("VAULT_TASK_PKG_MANIFEST_URL");
        }
        assert!(update_url_allowed(
            "https://github.com/shridhargsabat/vault-task/releases/download/v9/f.rpm"
        ));
        assert!(!update_url_allowed("http://127.0.0.1:8899/fixture.rpm"));
        unsafe {
            std::env::set_var(
                "VAULT_TASK_PKG_MANIFEST_URL",
                "http://127.0.0.1:8899/linux-packages.json",
            );
        }
        assert!(update_url_allowed("http://127.0.0.1:8899/fixture.rpm"));
        assert!(!update_url_allowed("http://127.0.0.1:9999/other.rpm"));
        unsafe {
            std::env::remove_var("VAULT_TASK_PKG_MANIFEST_URL");
        }
    }

    #[test]
    fn detect_install_hatch_forces_kind() {
        // SAFETY: no other test reads or writes this var.
        unsafe {
            std::env::set_var("VAULT_TASK_INSTALL_KIND", "system-package");
        }
        let info = detect_install();
        unsafe {
            std::env::remove_var("VAULT_TASK_INSTALL_KIND");
        }
        assert_eq!(info.kind, InstallKind::SystemPackage);
        // Family follows the real machine (this lab is Ubuntu → Deb).
        #[cfg(target_os = "linux")]
        assert_eq!(info.family, Some(DistroFamily::Deb));
    }

    #[test]
    fn manifest_url_hatch_defaults_and_overrides() {
        // SAFETY: no other test reads or writes this var, so no racing
        // access is possible even with the threaded test runner.
        unsafe {
            std::env::remove_var("VAULT_TASK_PKG_MANIFEST_URL");
        }
        assert_eq!(packages_manifest_url(), PACKAGES_MANIFEST_URL);
        unsafe {
            std::env::set_var(
                "VAULT_TASK_PKG_MANIFEST_URL",
                "http://127.0.0.1:9/fixture.json",
            );
        }
        assert_eq!(packages_manifest_url(), "http://127.0.0.1:9/fixture.json");
        unsafe {
            std::env::remove_var("VAULT_TASK_PKG_MANIFEST_URL");
        }
    }

    #[test]
    fn classify_appimage_uses_plugin_flow() {
        let info = classify_install(
            true,
            "/tmp/.mount_Foo/usr/bin/vault-task",
            None,
            false,
            None,
        );
        assert_eq!(info.kind, InstallKind::PluginManaged);
        assert_eq!(info.family, None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn classify_usr_install_with_distro_is_system_package() {
        let os = "NAME=\"Fedora Linux\"\nID=fedora\nID_LIKE=\"rhel centos fedora\"\n";
        let info = classify_install(false, "/usr/bin/vault-task", Some(os), true, None);
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
            let info = classify_install(false, exe, None, true, None);
            assert_eq!(info.kind, InstallKind::Manual, "for {exe}");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn classify_unknown_distro_is_manual() {
        let os = "NAME=\"Arch Linux\"\nID=arch\n";
        let info = classify_install(false, "/usr/bin/vault-task", Some(os), false, None);
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
        let hit = check_against(MANIFEST_FIXTURE, "rpm", "rpm", "0.3.0")
            .unwrap()
            .unwrap();
        assert_eq!(hit.version, "0.4.0");
        assert!(hit.file_name.ends_with(".rpm"));

        let hit = check_against(MANIFEST_FIXTURE, "deb", "deb", "0.3.0")
            .unwrap()
            .unwrap();
        assert!(hit.file_name.ends_with(".deb"));

        // Same version → None, no asset needed.
        assert!(
            check_against(MANIFEST_FIXTURE, "rpm", "rpm", "0.4.0")
                .unwrap()
                .is_none()
        );

        // Missing asset for the family → hard error, not a wrong file.
        let bare = r#"{"version":"0.4.0","notes":"","pub_date":"","packages":{}}"#;
        assert!(check_against(bare, "rpm", "rpm", "0.3.0").is_err());
        assert!(serde_json::from_str::<PackageManifest>("nope").is_err());
    }

    const MSI_MANIFEST_FIXTURE: &str = r#"{
        "version": "0.5.0",
        "notes": "MSI updater!",
        "pub_date": "2026-11-05T00:00:00Z",
        "packages": {
            "msi": {"url": "https://github.com/shridhargsabat/vault-task/releases/download/v0.5.0/VaultTask_0.5.0_x64_en-US.msi", "signature": "SIG"}
        }
    }"#;

    #[test]
    fn windows_classifier_routes_msi_nsis_and_ambiguous() {
        let msi = WindowsInstallFacts {
            msi: true,
            nsis: false,
        };
        assert_eq!(classify_windows_install(&msi), InstallKind::MsiAssisted);
        let nsis = WindowsInstallFacts {
            msi: false,
            nsis: true,
        };
        assert_eq!(classify_windows_install(&nsis), InstallKind::PluginManaged);
        // Portable/dev: neither flag → manual link, never a wrong flow.
        assert_eq!(
            classify_windows_install(&WindowsInstallFacts::default()),
            InstallKind::Manual
        );
        // Both installed: ambiguous → manual link (safe over sorry).
        let both = WindowsInstallFacts {
            msi: true,
            nsis: true,
        };
        assert_eq!(classify_windows_install(&both), InstallKind::Manual);
    }

    #[test]
    fn check_selects_msi_asset_and_rejects_wrong_extension() {
        let hit = check_against(MSI_MANIFEST_FIXTURE, "msi", "msi", "0.4.0")
            .unwrap()
            .unwrap();
        assert_eq!(hit.version, "0.5.0");
        assert!(hit.file_name.ends_with(".msi"));

        // Same version → None.
        assert!(
            check_against(MSI_MANIFEST_FIXTURE, "msi", "msi", "0.5.0")
                .unwrap()
                .is_none()
        );

        // URL/extension mismatch → hard error.
        assert!(check_against(MSI_MANIFEST_FIXTURE, "msi", "exe", "0.4.0").is_err());
        assert!(check_against(MANIFEST_FIXTURE, "msi", "msi", "0.3.0").is_err());
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
        // Wrong prefix / extension / location all rejected before any installer.
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

    #[test]
    fn msi_staged_path_validates_and_non_msi_rejected_from_wizard() {
        // Correctly staged .msi passes validation…
        let staged = std::env::temp_dir().join(format!(
            "vaulttask-update-msi-test-{}.msi",
            std::process::id()
        ));
        std::fs::write(&staged, b"fake").unwrap();
        assert!(validate_install_path(&staged.to_string_lossy()).is_ok());
        // …but a staged .rpm may never ride the wizard path.
        let rpm = std::env::temp_dir().join(format!(
            "vaulttask-update-msi-test-{}.rpm",
            std::process::id()
        ));
        std::fs::write(&rpm, b"fake").unwrap();
        let err = launch_system_installer(&rpm.to_string_lossy()).unwrap_err();
        assert!(err.contains("only MSI"), "got: {err}");
        std::fs::remove_file(&staged).ok();
        std::fs::remove_file(&rpm).ok();
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn msi_launch_is_windows_only_off_windows() {
        let err = launch_msi_installer(Path::new("/tmp/x.msi")).unwrap_err();
        assert!(err.contains("Windows-only"), "got: {err}");
    }
}

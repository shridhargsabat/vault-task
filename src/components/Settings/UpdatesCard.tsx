import { useEffect, useRef, useState } from "react";
import { Download, ExternalLink, RefreshCw, RotateCcw } from "lucide-react";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { check } from "@tauri-apps/plugin-updater";
import type { Update } from "@tauri-apps/plugin-updater";
import { exit, relaunch } from "@tauri-apps/plugin-process";
import { useVaultStore } from "../../store/useVaultStore";

type Phase =
  | "idle"
  | "checking"
  | "uptodate"
  | "available"
  | "downloading"
  | "downloaded"
  | "installing"
  | "error";

type InstallKind = "plugin-managed" | "system-package" | "msi-assisted" | "manual";

interface InstallInfo {
  kind: InstallKind;
  family: "rpm" | "deb" | null;
  packagekit: boolean;
}

interface SystemUpdate {
  version: string;
  notes: string;
  url: string;
  fileName: string;
  signature: string;
}

interface PkgUpdateState extends SystemUpdate {
  stagedPath: string | null;
}

const RELEASES_URL = "https://github.com/shridhargsabat/vault-task/releases";

/** Updates & Version card: plugin (NSIS/AppImage), PackageKit (rpm/deb), or MSI assist. */
export function UpdatesCard() {
  const nonce = useVaultStore((s) => s.updaterCheckNonce);
  const [version, setVersion] = useState("…");
  const [install, setInstall] = useState<InstallInfo | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");
  const [message, setMessage] = useState<string | null>(null);
  const [notes, setNotes] = useState<string | null>(null);
  const [newVersion, setNewVersion] = useState<string | null>(null);
  const [stagedName, setStagedName] = useState<string | null>(null);
  // null = indeterminate (server gave no content length).
  const [progress, setProgress] = useState<number | null>(0);
  const updateRef = useRef<Update | null>(null);
  const pkgRef = useRef<PkgUpdateState | null>(null);
  const seenNonce = useRef(nonce);

  useEffect(() => {
    getVersion()
      .then((v) => setVersion(v))
      .catch(() => setVersion("dev"));
    invoke<InstallInfo>("get_install_info")
      .then(setInstall)
      .catch(() => setInstall({ kind: "plugin-managed", family: null, packagekit: false }));
  }, []);

  const isPkgFlow = install?.kind === "system-package";
  const isMsiFlow = install?.kind === "msi-assisted";
  // Both native flows share check/download; only the apply step differs.
  const isNativeFlow = isPkgFlow || isMsiFlow;

  const doCheck = async () => {
    setPhase("checking");
    setMessage(null);
    setNotes(null);
    setNewVersion(null);
    setStagedName(null);
    setProgress(0);
    updateRef.current = null;
    pkgRef.current = null;
    try {
      if (isNativeFlow) {
        const found = await invoke<SystemUpdate | null>("check_system_update_cmd");
        if (!found) {
          setPhase("uptodate");
          return;
        }
        pkgRef.current = { ...found, stagedPath: null };
        setNewVersion(found.version);
        setNotes(found.notes || null);
        setPhase("available");
        return;
      }
      const update = await check();
      if (!update) {
        setPhase("uptodate");
        return;
      }
      updateRef.current = update;
      setNewVersion(update.version);
      setNotes(update.body ?? null);
      setPhase("available");
    } catch {
      // Offline, DNS failure, 404 manifest, or unsigned dev build —
      // all surface here; never a crash, always a retryable message.
      setPhase("error");
      setMessage(
        "Couldn't reach the update server. Check your connection and try again.",
      );
    }
  };

  // Titlebar "Check for Updates" opens Settings and bumps the nonce.
  useEffect(() => {
    if (nonce === seenNonce.current) return;
    seenNonce.current = nonce;
    void doCheck();
    // doCheck is stable enough; re-running only on nonce change is intended.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nonce]);

  const doDownload = async () => {
    if (isNativeFlow) {
      const pkg = pkgRef.current;
      if (!pkg) return;
      setPhase("downloading");
      setProgress(null);
      try {
        const stagedPath = await invoke<string>("download_system_update_cmd", {
          url: pkg.url,
          fileName: pkg.fileName,
          signature: pkg.signature,
        });
        pkgRef.current = { ...pkg, stagedPath };
        setStagedName(`vaulttask-update-${pkg.fileName}`);
        setProgress(1);
        setPhase("downloaded");
      } catch {
        setPhase("error");
        setMessage("Download or signature check failed. Please try again.");
      }
      return;
    }
    const update = updateRef.current;
    if (!update) return;
    setPhase("downloading");
    setProgress(0);
    let received = 0;
    let total: number | undefined;
    try {
      await update.download((ev) => {
        if (ev.event === "Started") {
          total = ev.data.contentLength;
        } else if (ev.event === "Progress") {
          received += ev.data.chunkLength;
          setProgress(total ? Math.min(1, received / total) : null);
        }
      });
      setProgress(1);
      setPhase("downloaded");
    } catch {
      setPhase("error");
      setMessage("Download failed. Please try again.");
    }
  };

  const doInstall = async () => {
    if (isMsiFlow) {
      const pkg = pkgRef.current;
      if (!pkg?.stagedPath) return;
      setPhase("installing");
      try {
        // Wizard is detached; exit so no files are locked when the user
        // clicks through (UAC prompt included).
        await invoke("launch_system_installer_cmd", { path: pkg.stagedPath });
        await exit(0);
      } catch {
        setPhase("error");
        setMessage("Couldn't launch the installer. Please try again.");
      }
      return;
    }
    if (isPkgFlow) {
      const pkg = pkgRef.current;
      if (!pkg?.stagedPath) return;
      setPhase("installing");
      try {
        await invoke("install_system_package_cmd", { path: pkg.stagedPath });
        await relaunch();
      } catch (e) {
        setPhase("error");
        // Backend distinguishes user-cancelled polkit from real failures.
        setMessage(
          typeof e === "string" && e.includes("cancelled")
            ? "Update cancelled — no changes were made."
            : "Install failed. Please try again.",
        );
      }
      return;
    }
    const update = updateRef.current;
    if (!update) return;
    setPhase("installing");
    try {
      await update.install();
      await relaunch();
    } catch {
      setPhase("error");
      setMessage("Install failed. Please try again.");
    }
  };

  const busy = phase === "checking" || phase === "downloading" || phase === "installing";
  const manualInstallCmd =
    install?.family === "deb"
      ? "sudo apt install ./<file>.deb"
      : "sudo dnf install ./<file>.rpm";

  return (
    <div className="rounded-lg bg-neutral-800 px-3 py-2.5">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium text-neutral-200">
          Updates &amp; Version
        </span>
        <span className="rounded bg-neutral-700 px-1.5 py-0.5 font-mono text-[11px] text-neutral-300">
          v{version}
        </span>
      </div>

      {install?.kind === "manual" ? (
        <div className="mt-2">
          <p className="text-[11px] leading-snug text-neutral-400">
            Portable install — new releases don&apos;t apply in-app. Check the
            releases page for updates.
          </p>
          <button
            onClick={() => void openUrl(RELEASES_URL)}
            className="mt-2 flex w-full items-center justify-center gap-1.5 rounded-md bg-neutral-700 py-1.5 text-xs font-medium text-neutral-100 hover:bg-neutral-600"
          >
            <ExternalLink className="h-3.5 w-3.5" /> Open Releases Page
          </button>
        </div>
      ) : (
        <>
          {(phase === "idle" ||
            phase === "checking" ||
            phase === "uptodate" ||
            phase === "error") && (
            <button
              onClick={() => void doCheck()}
              disabled={busy || install === null}
              className="mt-2 flex w-full items-center justify-center gap-1.5 rounded-md bg-neutral-700 py-1.5 text-xs font-medium text-neutral-100 hover:bg-neutral-600 disabled:opacity-50"
            >
              <RefreshCw className={`h-3.5 w-3.5 ${phase === "checking" ? "animate-spin" : ""}`} />
              {phase === "checking" ? "Checking…" : "Check for Updates"}
            </button>
          )}
          {phase === "uptodate" && (
            <p className="mt-1.5 text-center text-[11px] text-emerald-400">
              You&apos;re on the latest version.
            </p>
          )}
          {phase === "error" && message && (
            <p className="mt-1.5 text-center text-[11px] text-red-400">{message}</p>
          )}

          {phase === "available" && (
            <div className="mt-2">
              <p className="text-xs text-neutral-200">
                Update available:{" "}
                <span className="font-mono font-medium text-emerald-400">
                  v{newVersion}
                </span>
              </p>
              {notes && (
                <pre className="mt-1.5 max-h-24 overflow-y-auto whitespace-pre-wrap rounded-md bg-neutral-900 p-2 text-[11px] leading-snug text-neutral-400">
                  {notes.slice(0, 2000)}
                </pre>
              )}
              <button
                onClick={() => void doDownload()}
                className="mt-2 flex w-full items-center justify-center gap-1.5 rounded-md bg-emerald-600 py-1.5 text-xs font-medium text-white hover:bg-emerald-500"
              >
                <Download className="h-3.5 w-3.5" /> Download Update
              </button>
            </div>
          )}

          {(phase === "downloading" || phase === "downloaded") && (
            <div className="mt-2">
              <div className="h-2 overflow-hidden rounded-full bg-neutral-700">
                {progress === null ? (
                  <div className="h-full w-full animate-pulse rounded-full bg-emerald-600" />
                ) : (
                  <div
                    className="h-full rounded-full bg-emerald-600 transition-all"
                    style={{ width: `${Math.round(progress * 100)}%` }}
                  />
                )}
              </div>
              <p className="mt-1 text-center text-[11px] text-neutral-400">
                {phase === "downloaded"
                  ? "Download complete — signature verified."
                  : progress === null
                    ? "Downloading…"
                    : `Downloading… ${Math.round(progress * 100)}%`}
              </p>
            </div>
          )}
          {phase === "downloaded" &&
            (isPkgFlow && install?.packagekit === false ? (
              <div className="mt-2 rounded-md bg-neutral-900 p-2">
                <p className="text-[11px] leading-snug text-neutral-400">
                  PackageKit isn&apos;t installed, so the update can&apos;t
                  apply in-app. Install the verified download manually:
                </p>
                <p className="mt-1.5 select-all rounded bg-neutral-800 p-1.5 font-mono text-[11px] text-neutral-200">
                  cd /tmp && {manualInstallCmd.replace("<file>", stagedName ?? "package")}
                </p>
              </div>
            ) : (
              <button
                onClick={() => void doInstall()}
                className="mt-2 flex w-full items-center justify-center gap-1.5 rounded-md bg-emerald-600 py-1.5 text-xs font-medium text-white hover:bg-emerald-500"
              >
                <RotateCcw className="h-3.5 w-3.5" />
                {isMsiFlow
                  ? "Install Update & Exit"
                  : isPkgFlow
                    ? "Install Update & Restart"
                    : "Restart to Apply Update"}
              </button>
            ))}
          {phase === "installing" && (
            <p className="mt-2 text-center text-[11px] text-neutral-400">
              {isMsiFlow
                ? "Opening the installer… VaultTask exits so files aren't locked."
                : isPkgFlow
                  ? "Installing… a system password prompt may appear."
                  : "Installing… the app will restart."}
            </p>
          )}
        </>
      )}
    </div>
  );
}

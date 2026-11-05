import { useEffect, useRef, useState } from "react";
import { Download, RefreshCw, RotateCcw } from "lucide-react";
import { getVersion } from "@tauri-apps/api/app";
import { check } from "@tauri-apps/plugin-updater";
import type { Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
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

/** Updates & Version card: check, download with progress, notes, restart-to-apply. */
export function UpdatesCard() {
  const nonce = useVaultStore((s) => s.updaterCheckNonce);
  const [version, setVersion] = useState("…");
  const [phase, setPhase] = useState<Phase>("idle");
  const [message, setMessage] = useState<string | null>(null);
  const [notes, setNotes] = useState<string | null>(null);
  const [newVersion, setNewVersion] = useState<string | null>(null);
  // null = indeterminate (server gave no content length).
  const [progress, setProgress] = useState<number | null>(0);
  const updateRef = useRef<Update | null>(null);
  const seenNonce = useRef(nonce);

  useEffect(() => {
    getVersion()
      .then((v) => setVersion(v))
      .catch(() => setVersion("dev"));
  }, []);

  const doCheck = async () => {
    setPhase("checking");
    setMessage(null);
    setNotes(null);
    setNewVersion(null);
    setProgress(0);
    updateRef.current = null;
    try {
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
      // Offline, DNS failure, 404 latest.json, or unsigned dev build —
      // all surface here; never a crash, always a retryable message.
      setPhase("error");
      setMessage(
        "Couldn't reach the update server. Check your connection and try again.",
      );
    }
  };

  // Tray "Check for Updates..." opens Settings and bumps the nonce.
  useEffect(() => {
    if (nonce === seenNonce.current) return;
    seenNonce.current = nonce;
    void doCheck();
    // doCheck is stable enough; re-running only on nonce change is intended.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nonce]);

  const doDownload = async () => {
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

      {(phase === "idle" ||
        phase === "checking" ||
        phase === "uptodate" ||
        phase === "error") && (
        <button
          onClick={() => void doCheck()}
          disabled={busy}
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
              ? "Download complete."
              : progress === null
                ? "Downloading…"
                : `Downloading… ${Math.round(progress * 100)}%`}
          </p>
        </div>
      )}
      {phase === "downloaded" && (
        <button
          onClick={() => void doInstall()}
          className="mt-2 flex w-full items-center justify-center gap-1.5 rounded-md bg-emerald-600 py-1.5 text-xs font-medium text-white hover:bg-emerald-500"
        >
          <RotateCcw className="h-3.5 w-3.5" /> Restart to Apply Update
        </button>
      )}
      {phase === "installing" && (
        <p className="mt-2 text-center text-[11px] text-neutral-400">
          Installing… the app will restart.
        </p>
      )}
    </div>
  );
}

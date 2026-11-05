import { useEffect, useState } from "react";
import { Check, Settings2, X } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { useVaultStore } from "../../store/useVaultStore";
import type { BackupResult } from "../../types/models";

const TIMEOUT_PRESETS: { seconds: number; label: string }[] = [
  { seconds: 30, label: "30s" },
  { seconds: 60, label: "1m" },
  { seconds: 300, label: "5m" },
  { seconds: 900, label: "15m" },
  { seconds: 1800, label: "30m" },
  { seconds: 3600, label: "1h" },
];

export function SecuritySettingsModal() {
  const isOpen = useVaultStore((s) => s.isSettingsOpen);
  const setOpen = useVaultStore((s) => s.setSettingsOpen);
  const status = useVaultStore((s) => s.status);
  const updateSecuritySettings = useVaultStore((s) => s.updateSecuritySettings);
  const changePassword = useVaultStore((s) => s.changePassword);
  const autostartEnabled = useVaultStore((s) => s.autostartEnabled);
  const refreshAutostart = useVaultStore((s) => s.refreshAutostart);
  const setAutostart = useVaultStore((s) => s.setAutostart);
  const exportBackup = useVaultStore((s) => s.exportBackup);
  const restoreBackup = useVaultStore((s) => s.restoreBackup);
  const exportMarkdown = useVaultStore((s) => s.exportMarkdown);
  const enableKeyringUnlock = useVaultStore((s) => s.enableKeyringUnlock);
  const [saving, setSaving] = useState(false);
  const [savedTick, setSavedTick] = useState(0);
  const [currentPw, setCurrentPw] = useState("");
  const [newPw, setNewPw] = useState("");
  const [confirmPw, setConfirmPw] = useState("");
  const [pwBusy, setPwBusy] = useState(false);
  const [autostartBusy, setAutostartBusy] = useState(false);
  const [keyringBusy, setKeyringBusy] = useState(false);
  const [pwMessage, setPwMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const [backupBusy, setBackupBusy] = useState(false);
  const [backupMessage, setBackupMessage] = useState<BackupResult | null>(null);
  const [restorePath, setRestorePath] = useState<string | null>(null);
  const [restorePw, setRestorePw] = useState("");

  useEffect(() => {
    if (!isOpen) return;
    setPwMessage(null);
    setBackupMessage(null);
    setRestorePath(null);
    setRestorePw("");
    void refreshAutostart();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [isOpen, setOpen, refreshAutostart]);

  if (!isOpen || !status) return null;

  const saveSettings = async (seconds: number, hide: boolean, hidePreview: boolean) => {
    setSaving(true);
    try {
      const ok = await updateSecuritySettings(seconds, hide, hidePreview);
      if (ok) setSavedTick((t) => t + 1);
    } finally {
      setSaving(false);
    }
  };

  const savePassword = async () => {
    if (newPw.length < 4) {
      setPwMessage({ ok: false, text: "New passphrase needs 4+ characters." });
      return;
    }
    if (newPw !== confirmPw) {
      setPwMessage({ ok: false, text: "New passphrases do not match." });
      return;
    }
    setPwBusy(true);
    try {
      const ok = await changePassword(currentPw, newPw);
      if (ok) {
        setPwMessage({ ok: true, text: "Passphrase changed. Vault re-encrypted." });
        setCurrentPw("");
        setNewPw("");
        setConfirmPw("");
      } else {
        setPwMessage({ ok: false, text: "Current passphrase is incorrect." });
      }
    } finally {
      setPwBusy(false);
    }
  };

  const toggleAutostart = async () => {
    if (autostartEnabled === null) return;
    setAutostartBusy(true);
    try {
      await setAutostart(!autostartEnabled);
    } finally {
      setAutostartBusy(false);
    }
  };

  const toggleKeyring = async () => {
    setKeyringBusy(true);
    try {
      const ok = await enableKeyringUnlock(!status.keyring_enabled);
      if (ok) setSavedTick((t) => t + 1);
    } finally {
      setKeyringBusy(false);
    }
  };

  const doExportBackup = async () => {
    setBackupBusy(true);
    try {
      const res = await exportBackup();
      if (res) setBackupMessage(res);
    } finally {
      setBackupBusy(false);
    }
  };

  const doExportMarkdown = async () => {
    setBackupBusy(true);
    try {
      const res = await exportMarkdown();
      if (res) setBackupMessage(res);
    } finally {
      setBackupBusy(false);
    }
  };

  const pickRestoreFile = async () => {
    try {
      const picked = await open({
        title: "Restore from backup",
        filters: [{ name: "VaultTask backup", extensions: ["vault"] }],
      });
      if (typeof picked === "string") {
        setRestorePath(picked);
        setBackupMessage(null);
      }
    } catch (e) {
      setBackupMessage({ ok: false, text: String(e) });
    }
  };

  const doRestore = async () => {
    if (!restorePath || restorePw.length === 0) {
      setBackupMessage({ ok: false, text: "Enter the backup's master PIN / passphrase." });
      return;
    }
    setBackupBusy(true);
    try {
      const res = await restoreBackup(restorePath, restorePw);
      setBackupMessage(res);
      if (res.ok) {
        setRestorePath(null);
        setRestorePw("");
      }
    } finally {
      setBackupBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div
        className="absolute inset-0 bg-black/60"
        onClick={() => setOpen(false)}
      />
      <div
        role="dialog"
        aria-label="Security settings"
        className="vt-modal-pop relative flex max-h-full w-full max-w-sm flex-col gap-4 overflow-y-auto rounded-xl bg-neutral-900 p-4"
      >
        <header className="flex items-center gap-2">
          <Settings2 className="h-4 w-4 text-neutral-400" />
          <h2 className="font-medium">Security</h2>
          {savedTick > 0 && (
            <span
              key={savedTick}
              className="vt-fade-up ml-1 flex items-center gap-1 text-[11px] text-emerald-400"
            >
              <Check className="h-3 w-3" /> Saved
            </span>
          )}
          <button
            title="Close settings"
            onClick={() => setOpen(false)}
            className="ml-auto grid h-7 w-7 place-items-center rounded-md text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100"
          >
            <X className="h-4 w-4" />
          </button>
        </header>

        <section>
          <p className="mb-1.5 text-xs font-medium text-neutral-300">
            Lock after inactivity
          </p>
          <div className="grid grid-cols-6 gap-1">
            {TIMEOUT_PRESETS.map((p) => {
              const active = status.auto_lock_seconds === p.seconds;
              return (
                <button
                  key={p.seconds}
                  disabled={saving}
                  onClick={() => void saveSettings(p.seconds, status.lock_on_tray_hide, status.hide_notification_preview)}
                  className={`rounded-md py-1.5 text-xs font-medium disabled:opacity-50 ${
                    active
                      ? "bg-emerald-600 text-white"
                      : "bg-neutral-800 text-neutral-300 hover:bg-neutral-700"
                  }`}
                >
                  {p.label}
                </button>
              );
            })}
          </div>
        </section>

        <section>
          <button
            disabled={saving}
            onClick={() =>
              void saveSettings(status.auto_lock_seconds, !status.lock_on_tray_hide, status.hide_notification_preview)
            }
            className="flex w-full items-center gap-2.5 rounded-lg bg-neutral-800 px-3 py-2 text-left text-xs disabled:opacity-50"
          >
            <span
              className={`relative h-5 w-9 shrink-0 rounded-full transition-colors ${
                status.lock_on_tray_hide ? "bg-emerald-600" : "bg-neutral-700"
              }`}
            >
              <span
                className={`absolute top-0.5 h-4 w-4 rounded-full bg-white transition-all ${
                  status.lock_on_tray_hide ? "left-[18px]" : "left-0.5"
                }`}
              />
            </span>
            <span className="text-neutral-200">
              Lock immediately when hidden to tray
            </span>
          </button>
        </section>

        {autostartEnabled !== null && (
          <section className="border-t border-neutral-800 pt-3">
            <button
              disabled={autostartBusy}
              onClick={() => void toggleAutostart()}
              className="flex w-full items-center gap-2.5 rounded-lg bg-neutral-800 px-3 py-2 text-left text-xs disabled:opacity-50"
            >
              <span
                className={`relative h-5 w-9 shrink-0 rounded-full transition-colors ${
                  autostartEnabled ? "bg-emerald-600" : "bg-neutral-700"
                }`}
              >
                <span
                  className={`absolute top-0.5 h-4 w-4 rounded-full bg-white transition-all ${
                    autostartEnabled ? "left-[18px]" : "left-0.5"
                  }`}
                />
              </span>
              <span className="text-neutral-200">
                Launch in system tray on OS startup
              </span>
            </button>
          </section>
        )}

        <section className="border-t border-neutral-800 pt-3">
          <button
            disabled={keyringBusy}
            onClick={() => void toggleKeyring()}
            className="flex w-full items-center gap-2.5 rounded-lg bg-neutral-800 px-3 py-2 text-left text-xs disabled:opacity-50"
          >
            <span
              className={`relative h-5 w-9 shrink-0 rounded-full transition-colors ${
                status.keyring_enabled ? "bg-emerald-600" : "bg-neutral-700"
              }`}
            >
              <span
                className={`absolute top-0.5 h-4 w-4 rounded-full bg-white transition-all ${
                  status.keyring_enabled ? "left-[18px]" : "left-0.5"
                }`}
              />
            </span>
            <span className="leading-snug text-neutral-200">
              Enable OS Keyring Quick-Unlock (Windows Credential Manager /
              Linux Secret Service)
            </span>
          </button>
        </section>

        <section className="border-t border-neutral-800 pt-3">
          <button
            disabled={saving}
            onClick={() =>
              void saveSettings(status.auto_lock_seconds, status.lock_on_tray_hide, !status.hide_notification_preview)
            }
            className="flex w-full items-center gap-2.5 rounded-lg bg-neutral-800 px-3 py-2 text-left text-xs disabled:opacity-50"
          >
            <span
              className={`relative h-5 w-9 shrink-0 rounded-full transition-colors ${
                !status.hide_notification_preview ? "bg-emerald-600" : "bg-neutral-700"
              }`}
            >
              <span
                className={`absolute top-0.5 h-4 w-4 rounded-full bg-white transition-all ${
                  !status.hide_notification_preview ? "left-[18px]" : "left-0.5"
                }`}
              />
            </span>
            <span className="leading-snug text-neutral-200">
              Show task titles in notifications when unlocked
              <span className="block text-[11px] text-neutral-500">
                Locked notifications never show content.
              </span>
            </span>
          </button>
        </section>

        <section className="flex flex-col gap-1.5 border-t border-neutral-800 pt-3">
          <p className="text-xs font-medium text-neutral-300">
            Change master PIN / passphrase
          </p>
          <input
            type="password"
            value={currentPw}
            onChange={(e) => setCurrentPw(e.currentTarget.value)}
            placeholder="Current PIN / passphrase"
            className="rounded-md bg-neutral-800 px-3 py-2 text-xs outline-none ring-emerald-500 focus:ring-2"
          />
          <input
            type="password"
            value={newPw}
            onChange={(e) => setNewPw(e.currentTarget.value)}
            placeholder="New PIN / passphrase"
            className="rounded-md bg-neutral-800 px-3 py-2 text-xs outline-none ring-emerald-500 focus:ring-2"
          />
          <input
            type="password"
            value={confirmPw}
            onChange={(e) => setConfirmPw(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void savePassword();
            }}
            placeholder="Confirm new PIN / passphrase"
            className="rounded-md bg-neutral-800 px-3 py-2 text-xs outline-none ring-emerald-500 focus:ring-2"
          />
          {pwMessage && (
            <p
              className={`text-xs ${
                pwMessage.ok ? "text-emerald-400" : "text-red-300"
              }`}
            >
              {pwMessage.text}
            </p>
          )}
          <button
            onClick={() => void savePassword()}
            disabled={pwBusy}
            className="rounded-md bg-neutral-700 px-3 py-2 text-xs font-medium hover:bg-neutral-600 disabled:opacity-50"
          >
            {pwBusy ? "Re-encrypting vault…" : "Change passphrase"}
          </button>
        </section>

        <section className="flex flex-col gap-1.5 border-t border-neutral-800 pt-3">
          <p className="text-xs font-medium text-neutral-300">Backup &amp; Data</p>
          <div className="grid grid-cols-2 gap-1.5">
            <button
              onClick={() => void doExportBackup()}
              disabled={backupBusy}
              className="rounded-md bg-neutral-700 px-3 py-2 text-xs font-medium hover:bg-neutral-600 disabled:opacity-50"
            >
              Export Encrypted Backup (.vault)
            </button>
            <button
              onClick={() => void doExportMarkdown()}
              disabled={backupBusy}
              className="rounded-md bg-neutral-700 px-3 py-2 text-xs font-medium hover:bg-neutral-600 disabled:opacity-50"
            >
              Export All as Markdown
            </button>
          </div>
          <button
            onClick={() => void pickRestoreFile()}
            disabled={backupBusy}
            className="rounded-md bg-neutral-700 px-3 py-2 text-xs font-medium hover:bg-neutral-600 disabled:opacity-50"
          >
            Restore from Backup…
          </button>
          {restorePath && (
            <>
              <p className="truncate text-[11px] text-neutral-400" title={restorePath}>
                {restorePath}
              </p>
              <input
                type="password"
                value={restorePw}
                onChange={(e) => setRestorePw(e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void doRestore();
                }}
                placeholder="Backup's master PIN / passphrase"
                className="rounded-md bg-neutral-800 px-3 py-2 text-xs outline-none ring-emerald-500 focus:ring-2"
              />
              <button
                onClick={() => void doRestore()}
                disabled={backupBusy}
                className="rounded-md bg-red-900/80 px-3 py-2 text-xs font-medium text-red-100 hover:bg-red-800 disabled:opacity-50"
              >
                {backupBusy ? "Restoring…" : "Restore now (replaces current vault)"}
              </button>
            </>
          )}
          {backupMessage && (
            <p
              className={`text-xs ${
                backupMessage.ok ? "text-emerald-400" : "text-red-300"
              }`}
            >
              {backupMessage.text}
            </p>
          )}
        </section>
      </div>
    </div>
  );
}

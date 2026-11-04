import { useEffect, useState } from "react";
import { Check, Settings2, X } from "lucide-react";
import { useVaultStore } from "../../store/useVaultStore";

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
  const [saving, setSaving] = useState(false);
  const [savedTick, setSavedTick] = useState(0);
  const [currentPw, setCurrentPw] = useState("");
  const [newPw, setNewPw] = useState("");
  const [confirmPw, setConfirmPw] = useState("");
  const [pwBusy, setPwBusy] = useState(false);
  const [pwMessage, setPwMessage] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    setPwMessage(null);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [isOpen, setOpen]);

  if (!isOpen || !status) return null;

  const saveSettings = async (seconds: number, hide: boolean) => {
    setSaving(true);
    try {
      const ok = await updateSecuritySettings(seconds, hide);
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
                  onClick={() => void saveSettings(p.seconds, status.lock_on_tray_hide)}
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
              void saveSettings(status.auto_lock_seconds, !status.lock_on_tray_hide)
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
      </div>
    </div>
  );
}

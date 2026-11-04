import { useEffect, useRef, useState } from "react";
import { Lock, Unlock } from "lucide-react";
import { useVaultStore } from "../../store/useVaultStore";

/**
 * Lock screen for both popover and maximized modes. Shakes on wrong
 * PIN/password; the global error banner carries the message.
 */
export function UnlockVaultView() {
  const unlockVault = useVaultStore((s) => s.unlockVault);
  const loading = useVaultStore((s) => s.loading);
  const error = useVaultStore((s) => s.error);
  const [password, setPassword] = useState("");
  const [shakeKey, setShakeKey] = useState(0);
  const prevError = useRef<string | null>(null);

  useEffect(() => {
    if (error && error !== prevError.current) {
      setShakeKey((k) => k + 1);
      setPassword("");
    }
    prevError.current = error;
  }, [error]);

  const submit = () => {
    if (!password || loading) return;
    void unlockVault(password);
  };

  return (
    <main className="mx-auto flex w-full max-w-xl flex-1 flex-col justify-center gap-4 p-6">
      <section
        key={shakeKey}
        className={`flex flex-col gap-2 rounded-lg bg-neutral-900 p-4 ${
          shakeKey > 0 ? "vt-shake" : "vt-fade-up"
        }`}
      >
        <h2 className="flex items-center gap-2 font-medium">
          <Lock className="h-4 w-4 text-amber-400" /> Vault locked
        </h2>
        <p className="text-xs text-neutral-500">
          Locked due to inactivity, tray hide, or system lock. Your key was
          wiped from memory.
        </p>
        <input
          autoFocus
          type="password"
          value={password}
          onChange={(e) => setPassword(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") submit();
          }}
          placeholder="Enter PIN / passphrase"
          className="rounded-md bg-neutral-800 px-3 py-2 text-sm outline-none ring-emerald-500 focus:ring-2"
        />
        <button
          onClick={submit}
          disabled={loading || !password}
          className="flex items-center justify-center gap-2 rounded-md bg-emerald-600 px-3 py-2 text-sm font-medium hover:bg-emerald-500 disabled:opacity-60"
        >
          <Unlock className="h-4 w-4" />
          {loading ? "Unlocking…" : "Unlock"}
        </button>
        <p className="text-[11px] text-neutral-600">
          Tip: Ctrl+L locks instantly from anywhere in the app.
        </p>
      </section>
    </main>
  );
}

import { useState } from "react";
import { ShieldCheck } from "lucide-react";
import { useVaultStore } from "../../store/useVaultStore";

export function SetupVaultView() {
  const setupVault = useVaultStore((s) => s.setupVault);
  const loading = useVaultStore((s) => s.loading);
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [localError, setLocalError] = useState<string | null>(null);

  const submit = () => {
    if (password.length < 4) {
      setLocalError("Use at least 4 characters.");
      return;
    }
    if (password !== confirm) {
      setLocalError("Passphrases do not match.");
      return;
    }
    setLocalError(null);
    void setupVault(password);
  };

  return (
    <main className="vt-fade-up mx-auto flex w-full max-w-xl flex-1 flex-col justify-center gap-4 p-6">
      <header className="flex items-center gap-2">
        <ShieldCheck className="h-6 w-6 text-emerald-400" />
        <h1 className="text-xl font-semibold">VaultTask</h1>
      </header>
      <section className="flex flex-col gap-2 rounded-lg bg-neutral-900 p-4">
        <h2 className="font-medium">Set up your vault</h2>
        <p className="text-xs text-neutral-500">
          A master PIN or passphrase encrypts everything (AES-256-GCM). It
          never leaves this device.
        </p>
        <input
          autoFocus
          type="password"
          value={password}
          onChange={(e) => setPassword(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") submit();
          }}
          placeholder="Master PIN / passphrase"
          className="rounded-md bg-neutral-800 px-3 py-2 text-sm outline-none ring-emerald-500 focus:ring-2"
        />
        <input
          type="password"
          value={confirm}
          onChange={(e) => setConfirm(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") submit();
          }}
          placeholder="Confirm PIN / passphrase"
          className="rounded-md bg-neutral-800 px-3 py-2 text-sm outline-none ring-emerald-500 focus:ring-2"
        />
        {localError && (
          <p className="text-xs text-red-300">{localError}</p>
        )}
        <button
          onClick={submit}
          disabled={loading}
          className="rounded-md bg-emerald-600 px-3 py-2 text-sm font-medium hover:bg-emerald-500 disabled:opacity-60"
        >
          {loading ? "Creating vault…" : "Create vault"}
        </button>
      </section>
    </main>
  );
}

import { useEffect, useState } from "react";
import { Lock, Plus, ShieldCheck, Unlock } from "lucide-react";
import { useVaultStore } from "./store/useVaultStore";

function App() {
  const {
    status,
    tasks,
    loading,
    error,
    refreshStatus,
    setupVault,
    unlockVault,
    lockVault,
    createTask,
  } = useVaultStore();
  const [password, setPassword] = useState("");
  const [newTitle, setNewTitle] = useState("");

  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus]);

  return (
    <main className="vt-transition mx-auto flex min-h-full w-full max-w-xl flex-col gap-4 p-6">
      <header className="flex items-center gap-2">
        <ShieldCheck className="h-6 w-6 text-emerald-400" />
        <h1 className="text-xl font-semibold">VaultTask — Phase 1</h1>
        {status && (
          <span className="ml-auto rounded-full bg-neutral-800 px-3 py-1 text-xs text-neutral-300">
            {status.initialized
              ? status.locked
                ? "Locked"
                : "Unlocked"
              : "Not initialized"}
          </span>
        )}
      </header>

      {error && (
        <p className="rounded-md bg-red-950/60 px-3 py-2 text-sm text-red-300">
          {error}
        </p>
      )}

      {!status || loading ? (
        <p className="text-sm text-neutral-400">Loading vault status…</p>
      ) : !status.initialized ? (
        <section className="flex flex-col gap-2 rounded-lg bg-neutral-900 p-4">
          <h2 className="font-medium">Set up your vault</h2>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.currentTarget.value)}
            placeholder="Master PIN / passphrase"
            className="rounded-md bg-neutral-800 px-3 py-2 text-sm outline-none ring-emerald-500 focus:ring-2"
          />
          <button
            onClick={() => void setupVault(password)}
            className="rounded-md bg-emerald-600 px-3 py-2 text-sm font-medium hover:bg-emerald-500"
          >
            Create vault
          </button>
        </section>
      ) : status.locked ? (
        <section className="flex flex-col gap-2 rounded-lg bg-neutral-900 p-4">
          <h2 className="flex items-center gap-2 font-medium">
            <Lock className="h-4 w-4" /> Vault locked
          </h2>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.currentTarget.value)}
            placeholder="Enter PIN / passphrase"
            className="rounded-md bg-neutral-800 px-3 py-2 text-sm outline-none ring-emerald-500 focus:ring-2"
          />
          <button
            onClick={() => void unlockVault(password)}
            className="flex items-center justify-center gap-2 rounded-md bg-emerald-600 px-3 py-2 text-sm font-medium hover:bg-emerald-500"
          >
            <Unlock className="h-4 w-4" /> Unlock
          </button>
        </section>
      ) : (
        <section className="flex flex-col gap-3">
          <div className="flex gap-2">
            <input
              value={newTitle}
              onChange={(e) => setNewTitle(e.currentTarget.value)}
              placeholder="Add a task…"
              className="flex-1 rounded-md bg-neutral-800 px-3 py-2 text-sm outline-none ring-emerald-500 focus:ring-2"
            />
            <button
              onClick={() => {
                if (newTitle.trim()) {
                  void createTask(newTitle.trim());
                  setNewTitle("");
                }
              }}
              className="flex items-center gap-1 rounded-md bg-emerald-600 px-3 py-2 text-sm font-medium hover:bg-emerald-500"
            >
              <Plus className="h-4 w-4" /> Add
            </button>
            <button
              onClick={() => void lockVault()}
              className="rounded-md bg-neutral-800 px-3 py-2 text-sm hover:bg-neutral-700"
            >
              Lock
            </button>
          </div>
          <p className="text-xs text-neutral-400">
            {tasks.length} task(s) · encrypted SQLite backend (Phase 1)
          </p>
          <ul className="flex flex-col gap-2">
            {tasks.map((t) => (
              <li
                key={t.id}
                className="rounded-md bg-neutral-900 px-3 py-2 text-sm"
              >
                {t.title}
              </li>
            ))}
          </ul>
        </section>
      )}
    </main>
  );
}

export default App;

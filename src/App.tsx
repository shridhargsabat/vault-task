import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Lock, Plus, ShieldCheck, Unlock } from "lucide-react";
import { useVaultStore } from "./store/useVaultStore";
import { CustomTitlebar } from "./components/Titlebar/CustomTitlebar";

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
  const windowMode = useVaultStore((s) => s.windowMode);
  const [password, setPassword] = useState("");
  const [newTitle, setNewTitle] = useState("");

  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus]);

  // Tray-menu / hotkey locks (backend) must purge the frontend too.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen("vault://locked", () => {
      const { clearSensitiveData, refreshStatus: refresh } =
        useVaultStore.getState();
      clearSensitiveData();
      setPassword("");
      void refresh();
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {
        /* not running under Tauri (browser preview) */
      });
    return () => unlisten?.();
  }, []);

  return (
    <div className="flex h-full w-full flex-col overflow-hidden">
      <CustomTitlebar />
      <main
        className={`vt-transition mx-auto flex min-h-0 w-full flex-1 flex-col gap-4 overflow-y-auto p-6 ${
          windowMode === "maximized" ? "max-w-3xl" : "max-w-xl"
        }`}
      >
        <header className="flex items-center gap-2">
          <ShieldCheck className="h-6 w-6 text-emerald-400" />
          <h1 className="text-xl font-semibold">VaultTask — Phase 2</h1>
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
              {tasks.length} task(s) · {windowMode} mode · tray + hotkey ready
              (Phase 2)
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
    </div>
  );
}

export default App;

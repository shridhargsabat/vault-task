import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Lock, MousePointerClick, ShieldCheck, Unlock } from "lucide-react";
import { useVaultStore } from "./store/useVaultStore";
import { CustomTitlebar } from "./components/Titlebar/CustomTitlebar";
import { FilterRail } from "./components/TaskList/FilterRail";
import { QuickAddBar } from "./components/TaskList/QuickAddBar";
import { TaskListView } from "./components/TaskList/TaskListView";
import { TaskDetailPane } from "./components/SidePane/TaskDetailPane";

function App() {
  const { status, loading, error, refreshStatus, setupVault, unlockVault } =
    useVaultStore();
  const windowMode = useVaultStore((s) => s.windowMode);
  const tasks = useVaultStore((s) => s.tasks);
  const selectedTaskId = useVaultStore((s) => s.selectedTaskId);
  const setSelectedTaskId = useVaultStore((s) => s.setSelectedTaskId);
  const [password, setPassword] = useState("");

  const selectedTask =
    tasks.find((t) => t.id === selectedTaskId) ?? null;

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

      {error && (
        <p className="shrink-0 bg-red-950/60 px-3 py-1.5 text-xs text-red-300">
          {error}
        </p>
      )}

      {!status || loading ? (
        <p className="p-6 text-sm text-neutral-400">Loading vault status…</p>
      ) : !status.initialized ? (
        <main className="mx-auto flex w-full max-w-xl flex-1 flex-col justify-center gap-4 p-6">
          <header className="flex items-center gap-2">
            <ShieldCheck className="h-6 w-6 text-emerald-400" />
            <h1 className="text-xl font-semibold">VaultTask</h1>
          </header>
          <section className="flex flex-col gap-2 rounded-lg bg-neutral-900 p-4">
            <h2 className="font-medium">Set up your vault</h2>
            <input
              autoFocus
              type="password"
              value={password}
              onChange={(e) => setPassword(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && password) void setupVault(password);
              }}
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
        </main>
      ) : status.locked ? (
        <main className="mx-auto flex w-full max-w-xl flex-1 flex-col justify-center gap-4 p-6">
          <section className="flex flex-col gap-2 rounded-lg bg-neutral-900 p-4">
            <h2 className="flex items-center gap-2 font-medium">
              <Lock className="h-4 w-4" /> Vault locked
            </h2>
            <input
              autoFocus
              type="password"
              value={password}
              onChange={(e) => setPassword(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && password) void unlockVault(password);
              }}
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
        </main>
      ) : windowMode === "popover" ? (
        <main className="relative flex min-h-0 flex-1 flex-col">
          <FilterRail orientation="pills" />
          <QuickAddBar />
          <TaskListView />
          {selectedTask && (
            <>
              {/* Dim the exposed list strip; click closes the drawer. */}
              <div
                className="absolute bottom-0 left-0 top-0 z-10 w-[calc(100%-320px)] bg-black/40"
                onClick={() => setSelectedTaskId(null)}
              />
              <TaskDetailPane
                task={selectedTask}
                mode="drawer"
                onClose={() => setSelectedTaskId(null)}
              />
            </>
          )}
        </main>
      ) : (
        <main className="flex min-h-0 flex-1 flex-row">
          <aside className="w-60 shrink-0 border-r border-neutral-800/70">
            <FilterRail orientation="sidebar" />
          </aside>
          <section className="flex w-[400px] shrink-0 flex-col border-r border-neutral-800/70 pt-2">
            <QuickAddBar />
            <TaskListView />
          </section>
          <section className="min-w-0 flex-1">
            {selectedTask ? (
              <TaskDetailPane task={selectedTask} mode="docked" />
            ) : (
              <div className="flex h-full flex-col items-center justify-center gap-2 text-neutral-600">
                <MousePointerClick className="h-8 w-8" />
                <p className="text-sm">Select a task to see details</p>
              </div>
            )}
          </section>
        </main>
      )}
    </div>
  );
}

export default App;

import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { MousePointerClick } from "lucide-react";
import { useVaultStore } from "./store/useVaultStore";
import { useActivityTracker } from "./hooks/useActivityTracker";
import { CustomTitlebar } from "./components/Titlebar/CustomTitlebar";
import { FilterRail } from "./components/TaskList/FilterRail";
import { QuickAddBar } from "./components/TaskList/QuickAddBar";
import { TaskListView } from "./components/TaskList/TaskListView";
import { TaskDetailPane } from "./components/SidePane/TaskDetailPane";
import { SetupVaultView } from "./components/Auth/SetupVaultView";
import { UnlockVaultView } from "./components/Auth/UnlockVaultView";
import { SecuritySettingsModal } from "./components/Settings/SecuritySettingsModal";

function App() {
  const { status, loading, error, refreshStatus } = useVaultStore();
  const windowMode = useVaultStore((s) => s.windowMode);
  const tasks = useVaultStore((s) => s.tasks);
  const selectedTaskId = useVaultStore((s) => s.selectedTaskId);
  const setSelectedTaskId = useVaultStore((s) => s.setSelectedTaskId);

  const unlocked = !!status && status.initialized && !status.locked;
  useActivityTracker(unlocked);

  const selectedTask = tasks.find((t) => t.id === selectedTaskId) ?? null;

  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus]);

  // Backend-initiated locks (watchdog, tray menu, OS session lock) must
  // purge the frontend: zero plaintext in tasks, selection, search, modal.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen("vault://locked", () => {
      const {
        clearSensitiveData,
        setSettingsOpen,
        refreshStatus: refresh,
      } = useVaultStore.getState();
      clearSensitiveData();
      setSettingsOpen(false);
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
        <SetupVaultView />
      ) : status.locked ? (
        <UnlockVaultView />
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

      <SecuritySettingsModal />
    </div>
  );
}

export default App;

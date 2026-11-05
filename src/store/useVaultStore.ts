import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { open, save } from "@tauri-apps/plugin-dialog";
import type {
  ActiveFilter,
  BackupResult,
  BackupStats,
  CreateTaskInput,
  ImportStats,
  Task,
  UpdateTaskInput,
  VaultStatus,
  WindowMode,
} from "../types/models";

interface VaultState {
  status: VaultStatus | null;
  tasks: Task[];
  selectedTaskId: string | null;
  activeFilter: ActiveFilter;
  searchQuery: string;
  windowMode: WindowMode;
  isPinned: boolean;
  loading: boolean;
  error: string | null;
  refreshStatus: () => Promise<void>;
  setupVault: (password: string) => Promise<void>;
  unlockVault: (password: string) => Promise<void>;
  lockVault: () => Promise<void>;
  refreshTasks: () => Promise<void>;
  createTask: (input: CreateTaskInput) => Promise<Task | null>;
  updateTask: (id: string, patch: UpdateTaskInput) => Promise<void>;
  deleteTask: (id: string) => Promise<void>;
  toggleTaskDone: (id: string) => Promise<void>;
  clearSensitiveData: () => void;
  setSelectedTaskId: (id: string | null) => void;
  setActiveFilter: (f: ActiveFilter) => void;
  setSearchQuery: (q: string) => void;
  setWindowMode: (m: WindowMode) => void;
  setPinnedState: (p: boolean) => void;
  isSettingsOpen: boolean;
  setSettingsOpen: (open: boolean) => void;
  /** Bumped by the tray "Check for Updates..." event; the Updates card checks on change. */
  updaterCheckNonce: number;
  requestUpdateCheck: () => void;
  updateSecuritySettings: (
    autoLockSeconds: number,
    lockOnTrayHide: boolean,
    hideNotificationPreview: boolean,
  ) => Promise<boolean>;
  changePassword: (oldPassword: string, newPassword: string) => Promise<boolean>;
  autostartEnabled: boolean | null;
  refreshAutostart: () => Promise<void>;
  setAutostart: (enabled: boolean) => Promise<void>;
  exportBackup: () => Promise<BackupResult | null>;
  restoreBackup: (sourcePath: string, password: string) => Promise<BackupResult>;
  exportMarkdown: () => Promise<BackupResult | null>;
  enableKeyringUnlock: (enable: boolean) => Promise<boolean>;
  unlockWithKeyring: () => Promise<void>;
}

/** Optimistic local merge of an update patch (mirrors backend semantics). */
function applyPatch(t: Task, patch: UpdateTaskInput): Task {
  return {
    ...t,
    ...(patch.title !== undefined ? { title: patch.title } : {}),
    ...(patch.note_markdown !== undefined
      ? { note_markdown: patch.note_markdown }
      : {}),
    ...(patch.subtasks !== undefined ? { subtasks: patch.subtasks } : {}),
    ...(patch.status !== undefined ? { status: patch.status } : {}),
    ...(patch.priority !== undefined ? { priority: patch.priority } : {}),
    ...(patch.is_pinned !== undefined ? { is_pinned: patch.is_pinned } : {}),
    ...(patch.folder_id !== undefined ? { folder_id: patch.folder_id } : {}),
    ...(patch.tags !== undefined ? { tags: patch.tags } : {}),
    ...(patch.due_date !== undefined ? { due_date: patch.due_date } : {}),
    // Re-arming a reminder clears the fired flag (mirrors backend).
    ...(patch.reminder_at !== undefined
      ? { reminder_at: patch.reminder_at, reminder_fired: false }
      : {}),
  };
}

export const useVaultStore = create<VaultState>((set, get) => ({
  status: null,
  tasks: [],
  selectedTaskId: null,
  activeFilter: "all",
  searchQuery: "",
  windowMode: "popover",
  isPinned: false,
  isSettingsOpen: false,
  updaterCheckNonce: 0,
  autostartEnabled: null,
  loading: false,
  error: null,

  refreshStatus: async () => {
    set({ loading: true, error: null });
    try {
      const status = await invoke<VaultStatus>("get_vault_status");
      set({ status, loading: false });
      if (!status.locked && status.initialized) {
        try {
          const tasks = await invoke<Task[]>("list_tasks");
          set({ tasks });
        } catch (e) {
          set({ error: String(e) });
        }
      }
    } catch (e) {
      set({ loading: false, error: String(e) });
    }
  },

  setupVault: async (password: string) => {
    set({ loading: true, error: null });
    try {
      const status = await invoke<VaultStatus>("setup_vault", { password });
      set({ status, loading: false, tasks: [] });
    } catch (e) {
      set({ loading: false, error: String(e) });
    }
  },

  unlockVault: async (password: string) => {
    set({ loading: true, error: null });
    try {
      const status = await invoke<VaultStatus>("unlock_vault", { password });
      set({ status, loading: false });
      const tasks = await invoke<Task[]>("list_tasks");
      set({ tasks });
    } catch (e) {
      set({ loading: false, error: String(e) });
    }
  },

  lockVault: async () => {
    try {
      const status = await invoke<VaultStatus>("lock_vault");
      set({
        status,
        tasks: [],
        selectedTaskId: null,
        searchQuery: "",
        isSettingsOpen: false,
        error: null,
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  refreshTasks: async () => {
    try {
      const tasks = await invoke<Task[]>("list_tasks");
      set({ tasks, error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  createTask: async (input: CreateTaskInput) => {
    try {
      const task = await invoke<Task>("create_task", {
        title: input.title,
        note_markdown: input.note_markdown ?? "",
        status: input.status,
        priority: input.priority ?? 2,
        folder_id: input.folder_id,
        tags: input.tags,
        due_date: input.due_date ?? null,
        reminder_at: input.reminder_at ?? null,
      });
      set((s) => ({ tasks: [task, ...s.tasks], error: null }));
      return task;
    } catch (e) {
      set({ error: String(e) });
      return null;
    }
  },

  updateTask: async (id: string, patch: UpdateTaskInput) => {
    set((s) => ({
      tasks: s.tasks.map((t) => (t.id === id ? applyPatch(t, patch) : t)),
    }));
    try {
      const { due_date, reminder_at, ...rest } = patch;
      const updated = await invoke<Task>("update_task", {
        id,
        ...rest,
        // Backend clears the date on empty string; null would mean "unchanged".
        ...(due_date !== undefined ? { due_date: due_date ?? "" } : {}),
        ...(reminder_at !== undefined ? { reminder_at: reminder_at ?? "" } : {}),
      });
      set((s) => ({
        tasks: s.tasks.map((t) => (t.id === id ? updated : t)),
        error: null,
      }));
    } catch (e) {
      // Saves racing a lock are expected (autosave flush on unmount):
      // stay silent instead of flashing an error + failed resync.
      if (!String(e).includes("vault is locked")) {
        set({ error: String(e) });
        // Resync: drop the optimistic patch on failure.
        await get().refreshTasks();
      }
    }
  },

  deleteTask: async (id: string) => {
    const prev = get().tasks;
    set((s) => ({
      tasks: s.tasks.filter((t) => t.id !== id),
      selectedTaskId: s.selectedTaskId === id ? null : s.selectedTaskId,
    }));
    try {
      await invoke("delete_task", { id });
    } catch (e) {
      set({ tasks: prev, error: String(e) });
    }
  },

  toggleTaskDone: async (id: string) => {
    const t = get().tasks.find((x) => x.id === id);
    if (!t) return;
    await get().updateTask(id, {
      status: t.status === "done" ? "todo" : "done",
    });
  },

  updateSecuritySettings: async (autoLockSeconds, lockOnTrayHide, hideNotificationPreview) => {
    try {
      const status = await invoke<VaultStatus>("update_security_settings", {
        auto_lock_seconds: autoLockSeconds,
        lock_on_tray_hide: lockOnTrayHide,
        hide_notification_preview: hideNotificationPreview,
      });
      set({ status, error: null });
      return true;
    } catch (e) {
      set({ error: String(e) });
      return false;
    }
  },

  changePassword: async (oldPassword, newPassword) => {
    try {
      const status = await invoke<VaultStatus>("change_vault_password", {
        old_password: oldPassword,
        new_password: newPassword,
      });
      set({ status, error: null });
      return true;
    } catch {
      // Modal shows its own inline message; keep the global banner quiet.
      return false;
    }
  },

  refreshAutostart: async () => {
    try {
      set({ autostartEnabled: await isEnabled() });
    } catch {
      // Browser preview or unsupported platform: hide the toggle.
      set({ autostartEnabled: null });
    }
  },

  setAutostart: async (enabled: boolean) => {
    try {
      if (enabled) {
        await enable();
      } else {
        await disable();
      }
      set({ autostartEnabled: enabled, error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  exportBackup: async () => {
    try {
      const dest = await save({
        defaultPath: "vaulttask-backup.vault",
        filters: [{ name: "VaultTask backup", extensions: ["vault"] }],
      });
      if (!dest) return null;
      const stats = await invoke<BackupStats>("export_encrypted_backup", {
        destination_path: dest,
      });
      return { ok: true, text: `Backup saved (${stats.task_count} tasks).` };
    } catch (e) {
      return { ok: false, text: String(e) };
    }
  },

  restoreBackup: async (sourcePath: string, password: string) => {
    try {
      const stats = await invoke<ImportStats>("import_encrypted_backup", {
        source_path: sourcePath,
        master_password: password,
      });
      // Backend locked the session (DB replaced) — purge + refresh status.
      const status = await invoke<VaultStatus>("get_vault_status");
      set({
        status,
        tasks: [],
        selectedTaskId: null,
        searchQuery: "",
        isSettingsOpen: false,
        error: null,
      });
      return {
        ok: true,
        text: `Restored ${stats.task_count} tasks. Vault is locked — unlock to continue.`,
      };
    } catch (e) {
      return { ok: false, text: String(e) };
    }
  },

  exportMarkdown: async () => {
    try {
      const dir = await open({ directory: true, title: "Export Markdown to folder" });
      if (typeof dir !== "string") return null;
      const count = await invoke<number>("export_decrypted_markdown", {
        destination_dir: dir,
      });
      return { ok: true, text: `Exported ${count} tasks as Markdown.` };
    } catch (e) {
      return { ok: false, text: String(e) };
    }
  },

  enableKeyringUnlock: async (enable: boolean) => {
    try {
      await invoke("enable_keyring_unlock", { enable });
      const status = await invoke<VaultStatus>("get_vault_status");
      set({ status, error: null });
      return true;
    } catch (e) {
      set({ error: String(e) });
      return false;
    }
  },

  unlockWithKeyring: async () => {
    set({ loading: true, error: null });
    try {
      const ok = await invoke<boolean>("unlock_with_keyring");
      if (!ok) {
        set({
          loading: false,
          error: "No OS keyring credential saved — enter your PIN, or enable quick-unlock in Settings.",
        });
        return;
      }
      const status = await invoke<VaultStatus>("get_vault_status");
      set({ status, loading: false });
      try {
        const tasks = await invoke<Task[]>("list_tasks");
        set({ tasks });
      } catch (e) {
        set({ error: String(e) });
      }
    } catch (e) {
      set({ loading: false, error: String(e) });
    }
  },

  clearSensitiveData: () =>
    set({ tasks: [], selectedTaskId: null, searchQuery: "" }),

  setSelectedTaskId: (id) => set({ selectedTaskId: id }),
  setActiveFilter: (f) => set({ activeFilter: f }),
  setSearchQuery: (q) => set({ searchQuery: q }),
  setWindowMode: (m) => set({ windowMode: m }),
  setPinnedState: (p) => set({ isPinned: p }),
  setSettingsOpen: (open) => set({ isSettingsOpen: open }),
  requestUpdateCheck: () =>
    set((s) => ({ isSettingsOpen: true, updaterCheckNonce: s.updaterCheckNonce + 1 })),
}));

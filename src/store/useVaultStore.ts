import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type {
  ActiveFilter,
  Task,
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
  createTask: (title: string) => Promise<void>;
  clearSensitiveData: () => void;
  setSelectedTaskId: (id: string | null) => void;
  setActiveFilter: (f: ActiveFilter) => void;
  setSearchQuery: (q: string) => void;
}

export const useVaultStore = create<VaultState>((set) => ({
  status: null,
  tasks: [],
  selectedTaskId: null,
  activeFilter: "all",
  searchQuery: "",
  windowMode: "popover",
  isPinned: false,
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

  createTask: async (title: string) => {
    try {
      const task = await invoke<Task>("create_task", {
        title,
        note_markdown: "",
        priority: 2,
      });
      set((s) => ({ tasks: [task, ...s.tasks], error: null }));
    } catch (e) {
      set({ error: String(e) });
    }
  },

  clearSensitiveData: () =>
    set({ tasks: [], selectedTaskId: null, searchQuery: "" }),

  setSelectedTaskId: (id) => set({ selectedTaskId: id }),
  setActiveFilter: (f) => set({ activeFilter: f }),
  setSearchQuery: (q) => set({ searchQuery: q }),
}));

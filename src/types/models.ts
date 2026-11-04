export type TaskStatus = "todo" | "in_progress" | "done";

export type TaskPriority = 1 | 2 | 3 | 4; // 1 Low, 2 Medium, 3 High, 4 Urgent

export type ActiveFilter = "all" | "today" | "important" | "completed";

export type WindowMode = "popover" | "maximized";

export interface Subtask {
  id: string;
  title: string;
  done: boolean;
}

export interface Task {
  id: string;
  title: string;
  note_markdown: string;
  subtasks: Subtask[];
  status: TaskStatus;
  priority: TaskPriority;
  is_pinned: boolean;
  folder_id: string;
  tags: string[];
  due_date: string | null;
  created_at: string;
  updated_at: string;
}

export interface VaultStatus {
  initialized: boolean;
  locked: boolean;
  auto_lock_seconds: number;
  lock_on_tray_hide: boolean;
}

export interface VaultConfig {
  auto_lock_seconds: number;
  lock_on_tray_hide: boolean;
}

export interface CreateTaskInput {
  title: string;
  note_markdown?: string;
  status?: TaskStatus;
  priority?: TaskPriority;
  folder_id?: string;
  tags?: string[];
  due_date?: string | null;
}

export interface UpdateTaskInput {
  title?: string;
  note_markdown?: string;
  subtasks?: Subtask[];
  status?: TaskStatus;
  priority?: TaskPriority;
  is_pinned?: boolean;
  folder_id?: string;
  tags?: string[];
  due_date?: string | null;
}

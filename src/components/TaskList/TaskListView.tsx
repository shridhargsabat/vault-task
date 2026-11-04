import { useEffect, useMemo, useRef } from "react";
import {
  Check,
  FileText,
  ListChecks,
  Minus,
  Pin,
  Search,
  X,
} from "lucide-react";
import { useVaultStore } from "../../store/useVaultStore";
import { formatDue, isOverdue, isToday } from "../../utils/dates";
import { priorityMeta } from "../../utils/priorities";
import type { Task } from "../../types/models";

function matchesQuery(t: Task, q: string): boolean {
  const query = q.trim().toLowerCase();
  if (!query) return true;
  if (query.startsWith("#")) {
    const tag = query.slice(1);
    return t.tags.some((x) => x.toLowerCase() === tag);
  }
  return (
    t.title.toLowerCase().includes(query) ||
    t.note_markdown.toLowerCase().includes(query) ||
    t.tags.some((x) => x.toLowerCase().includes(query))
  );
}

function StatusBox({ task }: { task: Task }) {
  const toggleTaskDone = useVaultStore((s) => s.toggleTaskDone);
  if (task.status === "done") {
    return (
      <button
        title="Mark as todo"
        onClick={(e) => {
          e.stopPropagation();
          void toggleTaskDone(task.id);
        }}
        className="grid h-5 w-5 shrink-0 place-items-center rounded-md bg-emerald-600 text-white"
      >
        <Check className="h-3.5 w-3.5" />
      </button>
    );
  }
  if (task.status === "in_progress") {
    return (
      <button
        title="Mark as done"
        onClick={(e) => {
          e.stopPropagation();
          void toggleTaskDone(task.id);
        }}
        className="grid h-5 w-5 shrink-0 place-items-center rounded-md border border-amber-500/60 bg-amber-500/15 text-amber-300"
      >
        <Minus className="h-3.5 w-3.5" />
      </button>
    );
  }
  return (
    <button
      title="Mark as done"
      onClick={(e) => {
        e.stopPropagation();
        void toggleTaskDone(task.id);
      }}
      className="h-5 w-5 shrink-0 rounded-md border border-neutral-600 hover:border-emerald-500"
    />
  );
}

function TaskRow({ task }: { task: Task }) {
  const selectedTaskId = useVaultStore((s) => s.selectedTaskId);
  const setSelectedTaskId = useVaultStore((s) => s.setSelectedTaskId);
  const selected = selectedTaskId === task.id;
  const doneCount = task.subtasks.filter((s) => s.done).length;
  const pm = priorityMeta(task.priority);
  const overdue = isOverdue(task.due_date) && task.status !== "done";

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={() => setSelectedTaskId(task.id)}
      onKeyDown={(e) => {
        if (e.key === "Enter") setSelectedTaskId(task.id);
      }}
      className={`flex w-full cursor-pointer items-start gap-2.5 rounded-lg px-3 py-2.5 text-left transition-colors outline-none focus-visible:ring-1 focus-visible:ring-emerald-600/60 ${
        selected ? "bg-emerald-600/15 ring-1 ring-emerald-600/40" : "hover:bg-neutral-800/70"
      }`}
    >
      <span className="mt-0.5">
        <StatusBox task={task} />
      </span>
      <span className="min-w-0 flex-1">
        <span
          className={`block truncate text-sm font-medium ${
            task.status === "done" ? "text-neutral-500 line-through" : "text-neutral-100"
          }`}
        >
          {task.title}
        </span>
        <span className="mt-1 flex flex-wrap items-center gap-1.5">
          <span className={`rounded-full px-1.5 py-0.5 text-[10px] font-medium ${pm.badge}`}>
            {pm.label}
          </span>
          {task.due_date && (
            <span
              className={`rounded-full px-1.5 py-0.5 text-[10px] ${
                overdue
                  ? "bg-red-500/15 text-red-300"
                  : isToday(task.due_date)
                    ? "bg-amber-500/15 text-amber-300"
                    : "bg-neutral-800 text-neutral-400"
              }`}
            >
              {formatDue(task.due_date)}
            </span>
          )}
          {task.subtasks.length > 0 && (
            <span className="flex items-center gap-1 text-[10px] text-neutral-400">
              <ListChecks className="h-3 w-3" />
              {doneCount}/{task.subtasks.length}
            </span>
          )}
          {task.note_markdown.trim().length > 0 && (
            <FileText className="h-3 w-3 text-neutral-500" />
          )}
          {task.is_pinned && <Pin className="h-3 w-3 text-emerald-400" />}
        </span>
        {task.tags.length > 0 && (
          <span className="mt-1 flex flex-wrap gap-1">
            {task.tags.slice(0, 3).map((tag) => (
              <span key={tag} className="text-[10px] text-neutral-500">
                #{tag}
              </span>
            ))}
            {task.tags.length > 3 && (
              <span className="text-[10px] text-neutral-600">
                +{task.tags.length - 3}
              </span>
            )}
          </span>
        )}
      </span>
    </div>
  );
}

export function TaskListView() {
  const tasks = useVaultStore((s) => s.tasks);
  const activeFilter = useVaultStore((s) => s.activeFilter);
  const searchQuery = useVaultStore((s) => s.searchQuery);
  const setSearchQuery = useVaultStore((s) => s.setSearchQuery);
  const searchRef = useRef<HTMLInputElement>(null);

  // Titlebar search button focuses this input (both modes).
  useEffect(() => {
    const onFocus = () => searchRef.current?.focus();
    window.addEventListener("vaulttask:focus-search", onFocus);
    return () => window.removeEventListener("vaulttask:focus-search", onFocus);
  }, []);

  const visible = useMemo(() => {
    return tasks.filter((t) => {
      if (activeFilter === "today" && !isToday(t.due_date)) return false;
      if (activeFilter === "important" && t.priority < 3) return false;
      if (activeFilter === "completed" && t.status !== "done") return false;
      return matchesQuery(t, searchQuery);
    });
  }, [tasks, activeFilter, searchQuery]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="shrink-0 px-3 pb-2">
        <div className="flex items-center gap-2 rounded-lg bg-neutral-900 px-3 py-1.5">
          <Search className="h-3.5 w-3.5 shrink-0 text-neutral-500" />
          <input
            ref={searchRef}
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.currentTarget.value)}
            placeholder="Search or #tag…"
            className="min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-neutral-600"
          />
          {searchQuery && (
            <button onClick={() => setSearchQuery("")} title="Clear search">
              <X className="h-3.5 w-3.5 text-neutral-500 hover:text-neutral-300" />
            </button>
          )}
        </div>
      </div>
      <div className="vt-scroll min-h-0 flex-1 overflow-y-auto px-2 pb-3">
        {visible.length === 0 ? (
          <p className="px-3 py-8 text-center text-xs text-neutral-500">
            {tasks.length === 0
              ? "No tasks yet — add one above."
              : "Nothing matches this view."}
          </p>
        ) : (
          <div className="flex flex-col gap-0.5">
            {visible.map((t) => (
              <TaskRow key={t.id} task={t} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

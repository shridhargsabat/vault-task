import { useEffect, useState } from "react";
import { ArrowLeft, Pin, PinOff, Trash2, X } from "lucide-react";
import { useVaultStore } from "../../store/useVaultStore";
import { useDebouncedField } from "../../hooks/useDebouncedField";
import { PrioritySelect } from "./PrioritySelect";
import { DueDatePicker } from "./DueDatePicker";
import { ReminderPicker } from "./ReminderPicker";
import { SubtaskList } from "./SubtaskList";
import { NoteEditor } from "./NoteEditor";
import type { Subtask, Task, TaskStatus } from "../../types/models";

const STATUSES: { id: TaskStatus; label: string }[] = [
  { id: "todo", label: "Todo" },
  { id: "in_progress", label: "Doing" },
  { id: "done", label: "Done" },
];

export function TaskDetailPane({
  task,
  mode,
  onClose,
}: {
  task: Task;
  mode: "drawer" | "docked";
  onClose?: () => void;
}) {
  const updateTask = useVaultStore((s) => s.updateTask);
  const deleteTask = useVaultStore((s) => s.deleteTask);
  const [tagDraft, setTagDraft] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);

  const [title, setTitle] = useDebouncedField(
    task.id,
    task.title,
    500,
    (id, v) => {
      const clean = v.trim();
      if (clean && clean !== task.title) void updateTask(id, { title: clean });
    },
  );

  // Reset transient UI when switching tasks.
  useEffect(() => {
    setTagDraft("");
    setConfirmDelete(false);
  }, [task.id]);

  const saveSubtasks = (next: Subtask[]) =>
    void updateTask(task.id, { subtasks: next });

  const saveNote = (id: string, markdown: string) =>
    void updateTask(id, { note_markdown: markdown });

  const addTag = () => {
    const tag = tagDraft.trim().replace(/^#+/, "").slice(0, 24);
    if (!tag || task.tags.includes(tag)) {
      setTagDraft("");
      return;
    }
    void updateTask(task.id, { tags: [...task.tags, tag] });
    setTagDraft("");
  };

  const removeTag = (tag: string) =>
    void updateTask(task.id, { tags: task.tags.filter((t) => t !== tag) });

  const onDelete = () => {
    if (!confirmDelete) {
      setConfirmDelete(true);
      window.setTimeout(() => setConfirmDelete(false), 3000);
      return;
    }
    void deleteTask(task.id);
    onClose?.();
  };

  return (
    <div
      className={
        mode === "drawer"
          ? "vt-drawer-enter absolute bottom-0 right-0 top-0 z-20 flex w-[320px] flex-col border-l border-neutral-800 bg-neutral-950 shadow-2xl"
          : "flex h-full min-h-0 flex-col bg-neutral-950"
      }
    >
      {/* Header: back/close + status + actions */}
      <div className="flex shrink-0 items-center gap-1.5 border-b border-neutral-800/70 px-2.5 py-2">
        {mode === "drawer" && (
          <button
            title="Back to list"
            onClick={onClose}
            className="grid h-7 w-7 shrink-0 place-items-center rounded-md text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100"
          >
            <ArrowLeft className="h-4 w-4" />
          </button>
        )}
        <div className="flex rounded-md bg-neutral-900 p-0.5">
          {STATUSES.map((s) => (
            <button
              key={s.id}
              onClick={() => void updateTask(task.id, { status: s.id })}
              className={`rounded px-2 py-1 text-[11px] font-medium ${
                task.status === s.id
                  ? s.id === "done"
                    ? "bg-emerald-600 text-white"
                    : s.id === "in_progress"
                      ? "bg-amber-500/80 text-white"
                      : "bg-neutral-700 text-neutral-100"
                  : "text-neutral-500 hover:text-neutral-300"
              }`}
            >
              {s.label}
            </button>
          ))}
        </div>
        <span className="flex-1" />
        <button
          title={task.is_pinned ? "Unpin task" : "Pin task"}
          onClick={() => void updateTask(task.id, { is_pinned: !task.is_pinned })}
          className={`grid h-7 w-7 place-items-center rounded-md ${
            task.is_pinned
              ? "bg-emerald-600/30 text-emerald-300"
              : "text-neutral-500 hover:bg-neutral-800 hover:text-neutral-200"
          }`}
        >
          {task.is_pinned ? (
            <PinOff className="h-3.5 w-3.5" />
          ) : (
            <Pin className="h-3.5 w-3.5" />
          )}
        </button>
        <button
          title="Delete task"
          onClick={onDelete}
          className={`grid h-7 place-items-center rounded-md px-1.5 text-[11px] font-medium ${
            confirmDelete
              ? "bg-red-600 px-2.5 text-white"
              : "w-7 text-neutral-500 hover:bg-neutral-800 hover:text-red-300"
          }`}
        >
          {confirmDelete ? "Confirm?" : <Trash2 className="h-3.5 w-3.5" />}
        </button>
      </div>

      {/* Body */}
      <div className="vt-scroll flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-3">
        <input
          value={title}
          onChange={(e) => setTitle(e.currentTarget.value)}
          placeholder="Task title"
          className="w-full bg-transparent text-base font-semibold outline-none placeholder:text-neutral-600"
        />

        <div className="flex flex-wrap items-center gap-1.5">
          <PrioritySelect
            value={task.priority}
            onChange={(p) => void updateTask(task.id, { priority: p })}
          />
          <DueDatePicker
            value={task.due_date}
            onChange={(iso) => void updateTask(task.id, { due_date: iso })}
          />
          <ReminderPicker
            value={task.reminder_at}
            fired={task.reminder_fired}
            onChange={(iso) => void updateTask(task.id, { reminder_at: iso })}
          />
        </div>

        <div>
          {task.tags.length > 0 && (
            <div className="mb-1.5 flex flex-wrap gap-1">
              {task.tags.map((tag) => (
                <span
                  key={tag}
                  className="flex items-center gap-1 rounded-full bg-neutral-800 px-2 py-0.5 text-[11px] text-neutral-300"
                >
                  #{tag}
                  <button
                    title={`Remove #${tag}`}
                    onClick={() => removeTag(tag)}
                    className="text-neutral-500 hover:text-red-300"
                  >
                    <X className="h-3 w-3" />
                  </button>
                </span>
              ))}
            </div>
          )}
          <input
            value={tagDraft}
            onChange={(e) => setTagDraft(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") addTag();
            }}
            placeholder="Add a tag… (Enter)"
            className="w-full rounded-md bg-neutral-900 px-2 py-1.5 text-xs outline-none placeholder:text-neutral-600"
          />
        </div>

        <SubtaskList subtasks={task.subtasks} onChange={saveSubtasks} />

        <div className="flex min-h-[12rem] flex-1 flex-col">
          <NoteEditor
            taskId={task.id}
            initial={task.note_markdown}
            onSave={saveNote}
          />
        </div>

        <p className="text-[10px] text-neutral-600">
          Created {new Date(task.created_at).toLocaleString()} · Updated{" "}
          {new Date(task.updated_at).toLocaleString()}
        </p>
      </div>
    </div>
  );
}

import { useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  Check,
  ListChecks,
  Plus,
  X,
} from "lucide-react";
import type { Subtask } from "../../types/models";

function uid(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `st-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

export function SubtaskList({
  subtasks,
  onChange,
}: {
  subtasks: Subtask[];
  onChange: (next: Subtask[]) => void;
}) {
  const [draft, setDraft] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editText, setEditText] = useState("");
  const doneCount = subtasks.filter((s) => s.done).length;

  const add = () => {
    const title = draft.trim();
    if (!title) return;
    onChange([...subtasks, { id: uid(), title, done: false }]);
    setDraft("");
  };

  const toggle = (id: string) => {
    onChange(
      subtasks.map((s) => (s.id === id ? { ...s, done: !s.done } : s)),
    );
  };

  const remove = (id: string) => {
    onChange(subtasks.filter((s) => s.id !== id));
  };

  const move = (id: string, dir: -1 | 1) => {
    const i = subtasks.findIndex((s) => s.id === id);
    const j = i + dir;
    if (i < 0 || j < 0 || j >= subtasks.length) return;
    const next = [...subtasks];
    [next[i], next[j]] = [next[j], next[i]];
    onChange(next);
  };

  const commitEdit = () => {
    if (editingId === null) return;
    const title = editText.trim();
    onChange(
      title
        ? subtasks.map((s) =>
            s.id === editingId ? { ...s, title } : s,
          )
        : subtasks.filter((s) => s.id !== editingId),
    );
    setEditingId(null);
    setEditText("");
  };

  return (
    <section>
      <p className="mb-1.5 flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-neutral-500">
        <ListChecks className="h-3.5 w-3.5" />
        Subtasks
        {subtasks.length > 0 && (
          <span className="text-neutral-600">
            {doneCount}/{subtasks.length}
          </span>
        )}
      </p>
      <div className="flex flex-col gap-1">
        {subtasks.map((s) => (
          <div
            key={s.id}
            className="group flex items-center gap-1.5 rounded-md bg-neutral-900 px-2 py-1.5"
          >
            <button
              title={s.done ? "Mark open" : "Mark done"}
              onClick={() => toggle(s.id)}
              className={`grid h-[18px] w-[18px] shrink-0 place-items-center rounded border ${
                s.done
                  ? "border-emerald-600 bg-emerald-600 text-white"
                  : "border-neutral-600 hover:border-emerald-500"
              }`}
            >
              {s.done && <Check className="h-3 w-3" />}
            </button>
            {editingId === s.id ? (
              <input
                autoFocus
                value={editText}
                onChange={(e) => setEditText(e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") commitEdit();
                  if (e.key === "Escape") {
                    setEditingId(null);
                    setEditText("");
                  }
                }}
                onBlur={commitEdit}
                className="min-w-0 flex-1 bg-transparent text-xs outline-none"
              />
            ) : (
              <button
                onClick={() => {
                  setEditingId(s.id);
                  setEditText(s.title);
                }}
                title="Click to edit"
                className={`min-w-0 flex-1 truncate text-left text-xs ${
                  s.done ? "text-neutral-500 line-through" : "text-neutral-200"
                }`}
              >
                {s.title}
              </button>
            )}
            <span className="flex shrink-0 items-center opacity-0 transition-opacity group-hover:opacity-100">
              <button
                title="Move up"
                onClick={() => move(s.id, -1)}
                className="rounded p-0.5 text-neutral-500 hover:bg-neutral-800 hover:text-neutral-200"
              >
                <ArrowUp className="h-3 w-3" />
              </button>
              <button
                title="Move down"
                onClick={() => move(s.id, 1)}
                className="rounded p-0.5 text-neutral-500 hover:bg-neutral-800 hover:text-neutral-200"
              >
                <ArrowDown className="h-3 w-3" />
              </button>
              <button
                title="Delete subtask"
                onClick={() => remove(s.id)}
                className="rounded p-0.5 text-neutral-500 hover:bg-neutral-800 hover:text-red-300"
              >
                <X className="h-3 w-3" />
              </button>
            </span>
          </div>
        ))}
      </div>
      <div className="mt-1.5 flex items-center gap-1.5 rounded-md bg-neutral-900 px-2 py-1.5">
        <Plus className="h-3.5 w-3.5 shrink-0 text-neutral-500" />
        <input
          value={draft}
          onChange={(e) => setDraft(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") add();
          }}
          placeholder="Add a subtask…"
          className="min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-neutral-600"
        />
      </div>
    </section>
  );
}

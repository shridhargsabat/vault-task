import { useMemo, useState } from "react";
import { CalendarDays, Plus } from "lucide-react";
import { useVaultStore } from "../../store/useVaultStore";
import { parseQuickAdd } from "../../utils/quickAdd";
import { formatDue } from "../../utils/dates";
import { priorityMeta } from "../../utils/priorities";

export function QuickAddBar() {
  const createTask = useVaultStore((s) => s.createTask);
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);

  const parsed = useMemo(() => parseQuickAdd(value), [value]);
  const showHints =
    value.trim().length > 0 &&
    (parsed.priority !== undefined || parsed.due_date !== undefined);

  const submit = async () => {
    if (!parsed.title || busy) return;
    setBusy(true);
    try {
      await createTask({
        title: parsed.title,
        priority: parsed.priority,
        due_date: parsed.due_date ?? null,
      });
      setValue("");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="shrink-0 px-3 pb-2">
      <div className="flex items-center gap-2 rounded-lg bg-neutral-800 px-3 py-2 ring-emerald-500 focus-within:ring-2">
        <Plus className="h-4 w-4 shrink-0 text-neutral-500" />
        <input
          value={value}
          onChange={(e) => setValue(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void submit();
          }}
          placeholder="Add a task… (!high @today)"
          className="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-neutral-500"
        />
        {showHints && (
          <span className="flex shrink-0 items-center gap-1.5">
            {parsed.priority !== undefined && (
              <span
                className={`h-2 w-2 rounded-full ${priorityMeta(parsed.priority).dot}`}
                title={priorityMeta(parsed.priority).label}
              />
            )}
            {parsed.due_date !== undefined && (
              <span
                className="flex items-center gap-1 text-[11px] text-neutral-400"
                title={parsed.due_date}
              >
                <CalendarDays className="h-3 w-3" />
                {formatDue(parsed.due_date)}
              </span>
            )}
          </span>
        )}
      </div>
    </div>
  );
}

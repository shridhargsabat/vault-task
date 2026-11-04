import { useState } from "react";
import { Check, ChevronDown } from "lucide-react";
import { priorityMeta } from "../../utils/priorities";
import type { TaskPriority } from "../../types/models";

/** App-native priority dropdown (no OS-native select). */
export function PrioritySelect({
  value,
  onChange,
}: {
  value: number;
  onChange: (p: TaskPriority) => void;
}) {
  const [open, setOpen] = useState(false);
  const meta = priorityMeta(value);

  return (
    <div className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        className={`flex items-center gap-1.5 rounded-md px-2 py-1.5 text-xs font-medium ${meta.badge}`}
      >
        <span className={`h-2 w-2 rounded-full ${meta.dot}`} />
        {meta.label}
        <ChevronDown className="h-3 w-3 opacity-70" />
      </button>
      {open && (
        <>
          <div
            className="fixed inset-0 z-10 cursor-default"
            onClick={() => setOpen(false)}
          />
          <div className="absolute left-0 z-20 mt-1 w-36 rounded-lg border border-neutral-800 bg-neutral-900 p-1 shadow-xl">
            {([1, 2, 3, 4] as TaskPriority[]).map((p) => {
              const m = priorityMeta(p);
              const active = p === value;
              return (
                <button
                  key={p}
                  onClick={() => {
                    onChange(p);
                    setOpen(false);
                  }}
                  className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-xs hover:bg-neutral-800 ${m.text}`}
                >
                  <span className={`h-2 w-2 rounded-full ${m.dot}`} />
                  <span className="flex-1 text-left">{m.label}</span>
                  {active && <Check className="h-3 w-3" />}
                </button>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}

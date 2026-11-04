import { useMemo, useState } from "react";
import { CalendarDays, ChevronLeft, ChevronRight, X } from "lucide-react";
import { formatDue, isOverdue, todayISO } from "../../utils/dates";

const DOW = ["S", "M", "T", "W", "T", "F", "S"];

function monthCells(year: number, month: number): (number | null)[] {
  const firstDow = new Date(year, month, 1).getDay();
  const days = new Date(year, month + 1, 0).getDate();
  const cells: (number | null)[] = [];
  for (let i = 0; i < firstDow; i += 1) cells.push(null);
  for (let d = 1; d <= days; d += 1) cells.push(d);
  return cells;
}

/** App-native mini-calendar popover (no OS-native date picker popup). */
export function DueDatePicker({
  value,
  onChange,
}: {
  value: string | null;
  onChange: (iso: string | null) => void;
}) {
  const [open, setOpen] = useState(false);
  const [cursor, setCursor] = useState(() => value ?? todayISO());
  const overdue = isOverdue(value);

  const { year, month, cells, monthLabel } = useMemo(() => {
    const y = Number(cursor.slice(0, 4));
    const m = Number(cursor.slice(5, 7)) - 1;
    return {
      year: y,
      month: m,
      cells: monthCells(y, m),
      monthLabel: new Date(y, m, 1).toLocaleDateString(undefined, {
        month: "long",
        year: "numeric",
      }),
    };
  }, [cursor]);

  const shiftMonth = (delta: number) => {
    const d = new Date(year, month + delta, 1);
    setCursor(
      `${d.getFullYear()}-${`${d.getMonth() + 1}`.padStart(2, "0")}-01`,
    );
  };

  const pick = (day: number) => {
    const iso = `${year}-${`${month + 1}`.padStart(2, "0")}-${`${day}`.padStart(2, "0")}`;
    onChange(iso);
    setOpen(false);
  };

  return (
    <div className="relative">
      <button
        onClick={() => {
          setCursor(value ?? todayISO());
          setOpen((o) => !o);
        }}
        title={value ? `Due ${value}` : "Set due date"}
        className={`flex items-center gap-1.5 rounded-md px-2 py-1.5 text-xs font-medium ${
          overdue
            ? "bg-red-500/15 text-red-300"
            : value
              ? "bg-neutral-800 text-neutral-200"
              : "bg-neutral-800 text-neutral-500 hover:text-neutral-300"
        }`}
      >
        <CalendarDays className="h-3.5 w-3.5" />
        {formatDue(value)}
      </button>
      {open && (
        <>
          <div
            className="fixed inset-0 z-10 cursor-default"
            onClick={() => setOpen(false)}
          />
          <div className="absolute left-0 z-20 mt-1 w-60 rounded-lg border border-neutral-800 bg-neutral-900 p-2 shadow-xl">
            <div className="mb-1 flex items-center justify-between">
              <button
                onClick={() => shiftMonth(-1)}
                className="rounded p-1 hover:bg-neutral-800"
                title="Previous month"
              >
                <ChevronLeft className="h-4 w-4" />
              </button>
              <span className="text-xs font-medium text-neutral-200">
                {monthLabel}
              </span>
              <button
                onClick={() => shiftMonth(1)}
                className="rounded p-1 hover:bg-neutral-800"
                title="Next month"
              >
                <ChevronRight className="h-4 w-4" />
              </button>
            </div>
            <div className="grid grid-cols-7 gap-0.5">
              {DOW.map((d, i) => (
                <span
                  key={`${d}${i}`}
                  className="py-1 text-center text-[10px] text-neutral-600"
                >
                  {d}
                </span>
              ))}
              {cells.map((day, i) =>
                day === null ? (
                  <span key={`blank${i}`} />
                ) : (
                  <button
                    key={day}
                    onClick={() => pick(day)}
                    className={`rounded py-1 text-center text-xs ${
                      value ===
                      `${year}-${`${month + 1}`.padStart(2, "0")}-${`${day}`.padStart(2, "0")}`
                        ? "bg-emerald-600 font-medium text-white"
                        : "text-neutral-300 hover:bg-neutral-800"
                    }`}
                  >
                    {day}
                  </button>
                ),
              )}
            </div>
            <div className="mt-2 flex gap-1.5 border-t border-neutral-800 pt-2">
              <button
                onClick={() => {
                  onChange(todayISO());
                  setOpen(false);
                }}
                className="flex-1 rounded-md bg-neutral-800 py-1 text-[11px] text-neutral-300 hover:bg-neutral-700"
              >
                Today
              </button>
              <button
                onClick={() => {
                  onChange(null);
                  setOpen(false);
                }}
                className="flex flex-1 items-center justify-center gap-1 rounded-md bg-neutral-800 py-1 text-[11px] text-neutral-400 hover:bg-neutral-700"
              >
                <X className="h-3 w-3" /> Clear
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}

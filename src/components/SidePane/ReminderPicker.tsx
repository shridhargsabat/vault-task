import { useState } from "react";
import { Bell, BellOff, ChevronLeft, ChevronRight, X } from "lucide-react";
import { formatReminder, todayISO } from "../../utils/dates";

const DOW = ["S", "M", "T", "W", "T", "F", "S"];

function monthCells(year: number, month: number): (number | null)[] {
  const firstDow = new Date(year, month, 1).getDay();
  const days = new Date(year, month + 1, 0).getDate();
  const cells: (number | null)[] = [];
  for (let i = 0; i < firstDow; i += 1) cells.push(null);
  for (let d = 1; d <= days; d += 1) cells.push(d);
  return cells;
}

function inMinutes(mins: number): string {
  return new Date(Date.now() + mins * 60_000).toISOString();
}

function tomorrowAt(hour: number): string {
  const d = new Date();
  d.setDate(d.getDate() + 1);
  d.setHours(hour, 0, 0, 0);
  return d.toISOString();
}

const clampInt = (raw: string, max: number, fallback: number): number => {
  const n = Number.parseInt(raw, 10);
  if (!Number.isFinite(n) || n < 0) return fallback;
  return Math.min(n, max);
};

/** App-native reminder picker: presets + mini-calendar + HH:MM (no OS-native popups). */
export function ReminderPicker({
  value,
  fired,
  onChange,
}: {
  value: string | null;
  fired: boolean;
  onChange: (iso: string | null) => void;
}) {
  const [open, setOpen] = useState(false);
  const [cursor, setCursor] = useState(() => todayISO());
  const [pickedDay, setPickedDay] = useState<string | null>(null);
  const [hour, setHour] = useState("09");
  const [minute, setMinute] = useState("00");

  const y = Number(cursor.slice(0, 4));
  const m = Number(cursor.slice(5, 7)) - 1;
  const cells = monthCells(y, m);
  const monthLabel = new Date(y, m, 1).toLocaleDateString(undefined, {
    month: "long",
    year: "numeric",
  });

  const shiftMonth = (delta: number) => {
    const d = new Date(y, m + delta, 1);
    setCursor(
      `${d.getFullYear()}-${`${d.getMonth() + 1}`.padStart(2, "0")}-01`,
    );
  };

  const openPicker = () => {
    const base = value ? new Date(value) : new Date();
    if (!Number.isNaN(base.getTime())) {
      setHour(`${base.getHours()}`.padStart(2, "0"));
      setMinute(`${base.getMinutes()}`.padStart(2, "0"));
      setCursor(
        `${base.getFullYear()}-${`${base.getMonth() + 1}`.padStart(2, "0")}-01`,
      );
      setPickedDay(
        `${base.getFullYear()}-${`${base.getMonth() + 1}`.padStart(2, "0")}-${`${base.getDate()}`.padStart(2, "0")}`,
      );
    } else {
      setPickedDay(null);
    }
    setOpen((o) => !o);
  };

  const applyDayTime = (dayISO: string) => {
    const h = clampInt(hour, 23, 9);
    const min = clampInt(minute, 59, 0);
    const d = new Date(
      Number(dayISO.slice(0, 4)),
      Number(dayISO.slice(5, 7)) - 1,
      Number(dayISO.slice(8, 10)),
      h,
      min,
      0,
      0,
    );
    onChange(d.toISOString());
    setOpen(false);
  };

  const preset = (iso: string) => {
    onChange(iso);
    setOpen(false);
  };

  return (
    <div className="relative">
      <button
        onClick={openPicker}
        title={
          fired && value
            ? `Reminder sent ${formatReminder(value)} — pick a new time to re-arm`
            : value
              ? `Reminder: ${formatReminder(value)}`
              : "Set reminder"
        }
        className={`flex items-center gap-1.5 rounded-md px-2 py-1.5 text-xs font-medium ${
          value
            ? fired
              ? "bg-emerald-600/20 text-emerald-300"
              : "bg-neutral-800 text-neutral-200"
            : "bg-neutral-800 text-neutral-500 hover:text-neutral-300"
        }`}
      >
        {fired && value ? (
          <BellOff className="h-3.5 w-3.5" />
        ) : (
          <Bell className="h-3.5 w-3.5" />
        )}
        {formatReminder(value)}
      </button>
      {open && (
        <>
          <div
            className="fixed inset-0 z-10 cursor-default"
            onClick={() => setOpen(false)}
          />
          <div className="absolute left-0 z-20 mt-1 w-60 rounded-lg border border-neutral-800 bg-neutral-900 p-2 shadow-xl">
            <div className="mb-2 grid grid-cols-3 gap-1.5">
              <button
                onClick={() => preset(inMinutes(15))}
                className="rounded-md bg-neutral-800 py-1 text-[11px] text-neutral-300 hover:bg-neutral-700"
              >
                In 15 mins
              </button>
              <button
                onClick={() => preset(inMinutes(60))}
                className="rounded-md bg-neutral-800 py-1 text-[11px] text-neutral-300 hover:bg-neutral-700"
              >
                In 1 hour
              </button>
              <button
                onClick={() => preset(tomorrowAt(9))}
                className="rounded-md bg-neutral-800 py-1 text-[11px] text-neutral-300 hover:bg-neutral-700"
              >
                Tomorrow 9:00 AM
              </button>
            </div>
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
                    onClick={() => {
                      const iso = `${y}-${`${m + 1}`.padStart(2, "0")}-${`${day}`.padStart(2, "0")}`;
                      setPickedDay(iso);
                      applyDayTime(iso);
                    }}
                    className={`rounded py-1 text-center text-xs ${
                      pickedDay ===
                      `${y}-${`${m + 1}`.padStart(2, "0")}-${`${day}`.padStart(2, "0")}`
                        ? "bg-emerald-600 font-medium text-white"
                        : "text-neutral-300 hover:bg-neutral-800"
                    }`}
                  >
                    {day}
                  </button>
                ),
              )}
            </div>
            <div className="mt-2 flex items-center gap-1.5 border-t border-neutral-800 pt-2">
              <input
                value={hour}
                onChange={(e) => setHour(e.currentTarget.value.replace(/[^\d]/g, "").slice(0, 2))}
                inputMode="numeric"
                title="Hour (0-23)"
                className="w-10 rounded-md bg-neutral-800 py-1 text-center text-xs outline-none"
              />
              <span className="text-xs text-neutral-500">:</span>
              <input
                value={minute}
                onChange={(e) => setMinute(e.currentTarget.value.replace(/[^\d]/g, "").slice(0, 2))}
                inputMode="numeric"
                title="Minute (0-59)"
                className="w-10 rounded-md bg-neutral-800 py-1 text-center text-xs outline-none"
              />
              <span className="flex-1" />
              <button
                onClick={() => {
                  onChange(null);
                  setOpen(false);
                }}
                className="flex items-center gap-1 rounded-md bg-neutral-800 px-2 py-1 text-[11px] text-neutral-400 hover:bg-neutral-700"
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

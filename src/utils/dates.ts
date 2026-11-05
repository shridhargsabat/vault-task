/** Local-date helpers. Due dates are stored as `YYYY-MM-DD` strings. */

function pad(n: number): string {
  return n < 10 ? `0${n}` : `${n}`;
}

export function toISODate(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function todayISO(): string {
  return toISODate(new Date());
}

export function addDaysISO(days: number): string {
  const d = new Date();
  d.setDate(d.getDate() + days);
  return toISODate(d);
}

function parseISO(iso: string): Date | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  if (!m) return null;
  const d = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
  return Number.isNaN(d.getTime()) ? null : d;
}

export function isToday(iso: string | null | undefined): boolean {
  return !!iso && iso === todayISO();
}

export function isOverdue(iso: string | null | undefined): boolean {
  if (!iso) return false;
  return iso < todayISO();
}

/** Short human label: Today / Tomorrow / Nov 10 / Overdue Nov 3. */
export function formatDue(iso: string | null | undefined): string {
  if (!iso) return "No date";
  if (iso === todayISO()) return "Today";
  if (iso === addDaysISO(1)) return "Tomorrow";
  const d = parseISO(iso);
  if (!d) return iso;
  const label = d.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
  return isOverdue(iso) ? `Overdue ${label}` : label;
}

/** Reminder label: "No reminder" / "Today 2:30 PM" / "Tomorrow 9:00 AM" / "Nov 10, 9:00 AM". */
export function formatReminder(iso: string | null | undefined): string {
  if (!iso) return "No reminder";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const time = d.toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });
  const day = toISODate(d);
  if (day === todayISO()) return `Today ${time}`;
  if (day === addDaysISO(1)) return `Tomorrow ${time}`;
  const date = d.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
  return `${date}, ${time}`;
}

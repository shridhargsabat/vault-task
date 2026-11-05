import type { TaskPriority } from "../types/models";
import { addDaysISO, todayISO } from "./dates";

export interface ParsedQuickAdd {
  title: string;
  priority?: TaskPriority;
  due_date?: string;
  reminder_at?: string;
}

const PRIORITY_TOKENS: Record<string, TaskPriority> = {
  "!low": 1,
  "!1": 1,
  "!med": 2,
  "!medium": 2,
  "!2": 2,
  "!high": 3,
  "!3": 3,
  "!urgent": 4,
  "!4": 4,
};

/**
 * Parse `@remind(...)`: relative `15m`/`1h`/`2d`, `tomorrow` (9:00 AM),
 * or `HH:MM` (today, else tomorrow). Returns an ISO instant or null.
 */
function parseRemind(inner: string): string | null {
  const lower = inner.trim().toLowerCase();
  const rel = /^(\d+)(m|h|d)$/.exec(lower);
  if (rel) {
    const n = Number(rel[1]);
    const mult = rel[2] === "m" ? 60_000 : rel[2] === "h" ? 3_600_000 : 86_400_000;
    return new Date(Date.now() + n * mult).toISOString();
  }
  if (lower === "tomorrow") {
    const d = new Date();
    d.setDate(d.getDate() + 1);
    d.setHours(9, 0, 0, 0);
    return d.toISOString();
  }
  const hm = /^(\d{1,2}):(\d{2})$/.exec(lower);
  if (hm) {
    const h = Number(hm[1]);
    const min = Number(hm[2]);
    if (h > 23 || min > 59) return null;
    const d = new Date();
    d.setHours(h, min, 0, 0);
    if (d.getTime() <= Date.now()) d.setDate(d.getDate() + 1);
    return d.toISOString();
  }
  return null;
}

/**
 * Parse quick-add shortcuts: `!high`/`!urgent`/`!low` priority,
 * `@today`/`@tomorrow`/`@YYYY-MM-DD` due dates, and `@remind(15m)` /
 * `@remind(1h)` / `@remind(tomorrow)` / `@remind(17:30)` reminders.
 * Tokens are stripped from the title wherever they appear.
 */
export function parseQuickAdd(raw: string): ParsedQuickAdd {
  const out: ParsedQuickAdd = { title: "" };
  const kept: string[] = [];
  for (const tok of raw.split(/\s+/)) {
    const lower = tok.toLowerCase();
    if (lower in PRIORITY_TOKENS) {
      out.priority = PRIORITY_TOKENS[lower];
      continue;
    }
    if (lower === "@today") {
      out.due_date = todayISO();
      continue;
    }
    if (lower === "@tomorrow") {
      out.due_date = addDaysISO(1);
      continue;
    }
    const dateMatch = /^@(\d{4}-\d{2}-\d{2})$/.exec(tok);
    if (dateMatch) {
      out.due_date = dateMatch[1];
      continue;
    }
    const remindMatch = /^@remind\(([^)]+)\)$/i.exec(tok);
    if (remindMatch) {
      const at = parseRemind(remindMatch[1]);
      if (at) {
        out.reminder_at = at;
        continue;
      }
      // Unparseable: keep the token as literal title text.
    }
    kept.push(tok);
  }
  out.title = kept.join(" ").replace(/\s+/g, " ").trim();
  return out;
}

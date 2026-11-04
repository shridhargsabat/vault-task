import type { TaskPriority } from "../types/models";
import { addDaysISO, todayISO } from "./dates";

export interface ParsedQuickAdd {
  title: string;
  priority?: TaskPriority;
  due_date?: string;
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
 * Parse quick-add shortcuts: `!high`/`!urgent`/`!low` priority and
 * `@today`/`@tomorrow`/`@YYYY-MM-DD` due dates. Tokens are stripped
 * from the title wherever they appear.
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
    kept.push(tok);
  }
  out.title = kept.join(" ").replace(/\s+/g, " ").trim();
  return out;
}

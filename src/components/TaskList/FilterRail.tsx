import { useMemo } from "react";
import {
  CalendarDays,
  CheckCircle2,
  Flag,
  ListTodo,
  Lock,
  Settings,
  Tag,
  Unlock,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useVaultStore } from "../../store/useVaultStore";
import { isToday } from "../../utils/dates";
import type { ActiveFilter } from "../../types/models";

const FILTERS: { id: ActiveFilter; label: string; icon: LucideIcon }[] = [
  { id: "all", label: "All", icon: ListTodo },
  { id: "today", label: "Today", icon: CalendarDays },
  { id: "important", label: "Starred", icon: Flag },
  { id: "completed", label: "Done", icon: CheckCircle2 },
];

export function FilterRail({
  orientation,
}: {
  orientation: "pills" | "sidebar";
}) {
  const tasks = useVaultStore((s) => s.tasks);
  const activeFilter = useVaultStore((s) => s.activeFilter);
  const setActiveFilter = useVaultStore((s) => s.setActiveFilter);
  const searchQuery = useVaultStore((s) => s.searchQuery);
  const setSearchQuery = useVaultStore((s) => s.setSearchQuery);
  const setSettingsOpen = useVaultStore((s) => s.setSettingsOpen);
  const status = useVaultStore((s) => s.status);

  const counts = useMemo(() => {
    const c: Record<ActiveFilter, number> = {
      all: tasks.length,
      today: 0,
      important: 0,
      completed: 0,
    };
    for (const t of tasks) {
      if (isToday(t.due_date)) c.today += 1;
      if (t.priority >= 3) c.important += 1;
      if (t.status === "done") c.completed += 1;
    }
    return c;
  }, [tasks]);

  const tags = useMemo(() => {
    const m = new Map<string, number>();
    for (const t of tasks) {
      for (const tag of t.tags) m.set(tag, (m.get(tag) ?? 0) + 1);
    }
    return [...m.entries()].sort((a, b) => b[1] - a[1]).slice(0, 20);
  }, [tasks]);

  if (orientation === "pills") {
    return (
      <nav className="vt-scroll vt-no-bar flex shrink-0 gap-1 overflow-x-auto px-3 py-2">
        {FILTERS.map((f) => {
          const Icon = f.icon;
          const active = activeFilter === f.id;
          return (
            <button
              key={f.id}
              onClick={() => setActiveFilter(f.id)}
              className={`flex shrink-0 items-center gap-1 rounded-full px-2 py-1 text-[11px] font-medium transition-colors ${
                active
                  ? "bg-emerald-600 text-white"
                  : "bg-neutral-800 text-neutral-300 hover:bg-neutral-700"
              }`}
            >
              <Icon className="h-3 w-3" />
              {f.label}
              <span
                className={`rounded-full px-1 text-[10px] ${
                  active ? "bg-white/20" : "bg-neutral-700 text-neutral-400"
                }`}
              >
                {counts[f.id]}
              </span>
            </button>
          );
        })}
        <button
          title="Security settings"
          onClick={() => setSettingsOpen(true)}
          className="grid h-7 w-7 shrink-0 place-items-center self-center rounded-full bg-neutral-800 text-neutral-400 hover:bg-neutral-700 hover:text-neutral-200"
        >
          <Settings className="h-3.5 w-3.5" />
        </button>
      </nav>
    );
  }

  const locked = status?.locked ?? true;
  return (
    <nav className="vt-scroll flex h-full flex-col gap-4 overflow-y-auto p-3">
      <section>
        <p className="mb-1.5 px-2 text-[11px] font-semibold uppercase tracking-wider text-neutral-500">
          Smart lists
        </p>
        <div className="flex flex-col gap-0.5">
          {FILTERS.map((f) => {
            const Icon = f.icon;
            const active = activeFilter === f.id;
            return (
              <button
                key={f.id}
                onClick={() => setActiveFilter(f.id)}
                className={`flex items-center gap-2.5 rounded-md px-2.5 py-2 text-sm transition-colors ${
                  active
                    ? "bg-emerald-600/20 text-emerald-200"
                    : "text-neutral-300 hover:bg-neutral-800"
                }`}
              >
                <Icon className="h-4 w-4 shrink-0" />
                <span className="flex-1 text-left">{f.label}</span>
                <span className="rounded-full bg-neutral-800 px-1.5 text-[11px] text-neutral-400">
                  {counts[f.id]}
                </span>
              </button>
            );
          })}
        </div>
      </section>

      <section>
        <p className="mb-1.5 px-2 text-[11px] font-semibold uppercase tracking-wider text-neutral-500">
          Tags
        </p>
        {tags.length === 0 ? (
          <p className="px-2.5 text-xs text-neutral-500">
            No tags yet — add some in a task.
          </p>
        ) : (
          <div className="flex flex-col gap-0.5">
            {tags.map(([tag, n]) => {
              const active = searchQuery === `#${tag}`;
              return (
                <button
                  key={tag}
                  onClick={() => setSearchQuery(active ? "" : `#${tag}`)}
                  className={`flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm transition-colors ${
                    active
                      ? "bg-emerald-600/20 text-emerald-200"
                      : "text-neutral-300 hover:bg-neutral-800"
                  }`}
                >
                  <Tag className="h-3.5 w-3.5 shrink-0 text-neutral-500" />
                  <span className="flex-1 truncate text-left">{tag}</span>
                  <span className="text-[11px] text-neutral-500">{n}</span>
                </button>
              );
            })}
          </div>
        )}
      </section>

      <section className="mt-auto rounded-lg bg-neutral-900 p-3">
        <div className="mb-1 flex items-center gap-1.5">
          <p className="flex flex-1 items-center gap-1.5 text-xs font-medium text-neutral-300">
            {locked ? (
              <Lock className="h-3.5 w-3.5 text-amber-400" />
            ) : (
              <Unlock className="h-3.5 w-3.5 text-emerald-400" />
            )}
            {locked ? "Vault locked" : "Vault unlocked"}
          </p>
          <button
            title="Security settings"
            onClick={() => setSettingsOpen(true)}
            className="grid h-6 w-6 place-items-center rounded-md text-neutral-500 hover:bg-neutral-800 hover:text-neutral-200"
          >
            <Settings className="h-3.5 w-3.5" />
          </button>
        </div>
        <p className="text-[11px] leading-relaxed text-neutral-500">
          {tasks.length} task(s) · AES-256-GCM at rest
          {status && ` · auto-lock ${Math.round(status.auto_lock_seconds / 60)}m`}
        </p>
      </section>
    </nav>
  );
}

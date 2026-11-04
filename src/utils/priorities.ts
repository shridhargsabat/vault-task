import type { TaskPriority } from "../types/models";

export interface PriorityMeta {
  label: string;
  dot: string;
  text: string;
  badge: string;
}

const META: Record<TaskPriority, PriorityMeta> = {
  1: {
    label: "Low",
    dot: "bg-zinc-400",
    text: "text-zinc-300",
    badge: "bg-zinc-500/15 text-zinc-300",
  },
  2: {
    label: "Medium",
    dot: "bg-sky-400",
    text: "text-sky-300",
    badge: "bg-sky-500/15 text-sky-300",
  },
  3: {
    label: "High",
    dot: "bg-amber-400",
    text: "text-amber-300",
    badge: "bg-amber-500/15 text-amber-300",
  },
  4: {
    label: "Urgent",
    dot: "bg-red-400",
    text: "text-red-300",
    badge: "bg-red-500/15 text-red-300",
  },
};

export function priorityMeta(p: number): PriorityMeta {
  if (p === 1 || p === 2 || p === 3 || p === 4) return META[p];
  return META[2];
}

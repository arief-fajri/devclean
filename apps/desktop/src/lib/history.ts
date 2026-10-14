import type { CleanupRun, HistoryEntry } from "./tauri";

/**
 * Pure view logic for the cleanup-history dialog.
 *
 * History is presented per **cleanup run**, not per resource: one run can
 * be hundreds of resources, so an entry-level view either floods the UI or
 * (with an entry limit) hides the runs that came before.
 */

export function when(unix: number): string {
  const d = new Date(unix * 1000);
  return Number.isNaN(d.getTime()) ? "—" : d.toLocaleString();
}

export type RunFilter = "all" | "clean" | "problems";

export interface HistoryDay {
  label: string;
  runs: CleanupRun[];
  freed: number;
  requested: number;
  resources: number;
}

function dayKey(unix: number): string {
  const d = new Date(unix * 1000);
  if (Number.isNaN(d.getTime())) return "Unknown";
  return d.toDateString();
}

function dayLabel(unix: number): string {
  const d = new Date(unix * 1000);
  if (Number.isNaN(d.getTime())) return "Unknown";
  const today = new Date();
  if (
    d.getFullYear() === today.getFullYear() &&
    d.getMonth() === today.getMonth() &&
    d.getDate() === today.getDate()
  ) {
    return "Today";
  }
  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);
  if (
    d.getFullYear() === yesterday.getFullYear() &&
    d.getMonth() === yesterday.getMonth() &&
    d.getDate() === yesterday.getDate()
  ) {
    return "Yesterday";
  }
  return d.toLocaleDateString();
}

/** A run counts as a problem when any resource failed or only partly ran. */
export function runHasProblems(run: CleanupRun): boolean {
  return run.failed > 0 || run.partial > 0;
}

/** Group runs by calendar day, newest day first. Order inside a day is
 * preserved (the backend already returns newest run first). */
export function groupRunsByDay(runs: CleanupRun[]): HistoryDay[] {
  const byDay = new Map<string, CleanupRun[]>();
  for (const r of runs) {
    const key = dayKey(r.finished_at_unix);
    const list = byDay.get(key);
    if (list) list.push(r);
    else byDay.set(key, [r]);
  }
  const days: HistoryDay[] = [];
  for (const list of byDay.values()) {
    days.push({
      label: dayLabel(Math.max(...list.map((r) => r.finished_at_unix))),
      runs: list,
      freed: list.reduce((a, r) => a + r.freed_bytes, 0),
      requested: list.reduce((a, r) => a + r.requested_bytes, 0),
      resources: list.reduce((a, r) => a + r.entry_count, 0),
    });
  }
  days.sort((a, b) => {
    const at = Math.max(...a.runs.map((r) => r.finished_at_unix));
    const bt = Math.max(...b.runs.map((r) => r.finished_at_unix));
    return bt - at;
  });
  return days;
}

export function filterRuns(runs: CleanupRun[], filter: RunFilter): CleanupRun[] {
  if (filter === "all") return runs;
  return runs.filter((r) =>
    filter === "problems" ? runHasProblems(r) : !runHasProblems(r),
  );
}

export function countRunsByFilter(runs: CleanupRun[], filter: RunFilter): number {
  return filterRuns(runs, filter).length;
}

export interface RunTotals {
  freed: number;
  requested: number;
  runs: number;
  resources: number;
  problems: number;
}

/** Totals over the runs actually shown — never over a truncated window. */
export function runTotals(runs: CleanupRun[]): RunTotals {
  return {
    freed: runs.reduce((a, r) => a + r.freed_bytes, 0),
    requested: runs.reduce((a, r) => a + r.requested_bytes, 0),
    runs: runs.length,
    resources: runs.reduce((a, r) => a + r.entry_count, 0),
    problems: runs.reduce((a, r) => a + r.failed + r.partial, 0),
  };
}

/** `true` when the run carries entries written before contract v0.6 (no
 * `run_id`), so the UI can say the grouping is inferred. */
export function isLegacyRun(run: CleanupRun): boolean {
  return !run.run_id;
}

export function entryCountLabel(n: number): string {
  return `${n} ${n === 1 ? "resource" : "resources"}`;
}

/** `all` / `clean` / `problems` chip counts for the toolbar. */
export function filterCounts(runs: CleanupRun[]): Record<RunFilter, number> {
  return {
    all: runs.length,
    clean: runs.filter((r) => !runHasProblems(r)).length,
    problems: runs.filter((r) => runHasProblems(r)).length,
  };
}

export function statusColor(status: HistoryEntry["status"]): string {
  return status === "success"
    ? "text-emerald-600 dark:text-emerald-400"
    : status === "partial"
      ? "text-amber-600 dark:text-amber-400"
      : status === "failed"
        ? "text-red-600 dark:text-red-400"
        : "text-zinc-400";
}

export function statusBadge(status: HistoryEntry["status"]): string {
  return status === "success"
    ? "bg-emerald-100 text-emerald-800 dark:bg-emerald-900 dark:text-emerald-200"
    : status === "partial"
      ? "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200"
      : status === "failed"
        ? "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200"
        : "bg-zinc-200 text-zinc-600 dark:bg-zinc-700 dark:text-zinc-300";
}

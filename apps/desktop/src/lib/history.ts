import { describe, expect, it } from "vitest";
import type { CleanupStatus, HistoryEntry } from "./tauri";

function when(unix: number): string {
  const d = new Date(unix * 1000);
  return Number.isNaN(d.getTime()) ? "—" : d.toLocaleString();
}

export type StatusFilter = "all" | "success" | "problems";

export interface HistoryDay {
  label: string;
  entries: HistoryEntry[];
  freed: number;
  requested: number;
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
  const isToday =
    d.getFullYear() === today.getFullYear() &&
    d.getMonth() === today.getMonth() &&
    d.getDate() === today.getDate();
  if (isToday) return "Today";
  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);
  const isYesterday =
    d.getFullYear() === yesterday.getFullYear() &&
    d.getMonth() === yesterday.getMonth() &&
    d.getDate() === yesterday.getDate();
  if (isYesterday) return "Yesterday";
  return d.toLocaleDateString();
}

/** Group the audit log by calendar day, newest day first. */
export function groupHistory(entries: HistoryEntry[]): HistoryDay[] {
  const byDay = new Map<string, HistoryEntry[]>();
  for (const e of entries) {
    const key = dayKey(e.finished_at_unix);
    const list = byDay.get(key);
    if (list) list.push(e);
    else byDay.set(key, [e]);
  }
  const days: HistoryDay[] = [];
  for (const list of byDay.values()) {
    const newest = Math.max(...list.map((e) => e.finished_at_unix));
    days.push({
      label: dayLabel(newest),
      entries: [...list].sort((a, b) => b.finished_at_unix - a.finished_at_unix),
      freed: list.reduce((a, e) => a + e.freed_bytes, 0),
      requested: list.reduce((a, e) => a + e.requested_bytes, 0),
    });
  }
  days.sort(
    (a, b) =>
      Math.max(...b.entries.map((e) => e.finished_at_unix)) -
      Math.max(...a.entries.map((e) => e.finished_at_unix)),
  );
  return days;
}

function isProblem(s: CleanupStatus): boolean {
  return s === "partial" || s === "failed";
}

export function filterHistory(entries: HistoryEntry[], filter: StatusFilter): HistoryEntry[] {
  if (filter === "all") return entries;
  return entries.filter((e) =>
    filter === "success" ? !isProblem(e.status) : isProblem(e.status),
  );
}

export function historyTotals(entries: HistoryEntry[]): {
  freed: number;
  requested: number;
  problems: number;
} {
  return {
    freed: entries.reduce((a, e) => a + e.freed_bytes, 0),
    requested: entries.reduce((a, e) => a + e.requested_bytes, 0),
    problems: entries.filter((e) => isProblem(e.status)).length,
  };
}

export { when };

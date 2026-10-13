import { describe, expect, it } from "vitest";
import { filterHistory, groupHistory, historyTotals, when } from "./history";
import type { CleanupStatus, HistoryEntry } from "./tauri";

describe
describe("groupHistory", () => {
  const base = 1_700_000_000;
  const entry = (unix: number, bytes: number): HistoryEntry => ({
    finished_at_unix: unix,
    resource_id: `id-${unix}`,
    provider: "node_modules",
    name: "proj/node_modules",
    requested_bytes: bytes,
    freed_bytes: bytes,
    status: "success",
  });

  it("groups by calendar day, newest first, with per-day totals", () => {
    const baseMs = 1_700_000_000_000;
    const sameDay = baseMs;
    const nextDay = baseMs + 86_400_000;
    const days = groupHistory([
      entry(sameDay / 1000, 100),
      entry(nextDay / 1000, 50),
      entry(nextDay / 1000 + 60, 25),
    ]);
    expect(days.length).toBe(2);
    expect(days[0].freed).toBe(75);
    expect(days[1].freed).toBe(100);
  });

  it("sorts entries inside a day newest first", () => {
    const t = 1_700_000_000;
    const days = groupHistory([entry(t, 10), entry(t + 120, 20)]);
    expect(days[0].entries.map((e) => e.finished_at_unix)).toEqual([t + 120, t]);
  });

  it("handles an empty log and invalid timestamps", () => {
    expect(groupHistory([])).toEqual([]);
    expect(when(Number.NaN)).toBe("—");
  });
});

describe("filterHistory / historyTotals", () => {
  function entry(status: CleanupStatus, bytes = 10): HistoryEntry {
    return {
      finished_at_unix: 1_700_000_000,
      resource_id: `id-${status}`,
      provider: "docker",
      name: "n",
      requested_bytes: bytes,
      freed_bytes: bytes,
      status,
    };
  }

  const entries = [
    entry("success"),
    entry("failed"),
    entry("partial"),
    entry("cancelled"),
  ];

  it("keeps every entry for 'all'", () => {
    expect(filterHistory(entries, "all").length).toBe(4);
  });

  it("treats partial + failed as problems, cancelled as neither", () => {
    expect(filterHistory(entries, "problems").map((e) => e.status)).toEqual([
      "failed",
      "partial",
    ]);
    expect(filterHistory(entries, "success").map((e) => e.status)).toEqual([
      "success",
      "cancelled",
    ]);
  });

  it("sums freed bytes and counts problems", () => {
    const t = historyTotals(entries);
    expect(t.freed).toBe(40);
    expect(t.requested).toBe(40);
    expect(t.problems).toBe(2);
  });
});
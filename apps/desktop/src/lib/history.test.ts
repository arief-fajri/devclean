import { describe, expect, it } from "vitest";
import {
  countRunsByFilter,
  entryCountLabel,
  filterRuns,
  groupRunsByDay,
  isLegacyRun,
  runHasProblems,
  runTotals,
  when,
} from "./history";
import type { CleanupRun, HistoryEntry } from "./tauri";

const T = 1_700_000_000;

function entry(id: string, bytes = 100): HistoryEntry {
  return {
    run_id: "r1",
    finished_at_unix: T,
    resource_id: id,
    provider: "python_cache",
    name: `${id}/__pycache__`,
    requested_bytes: bytes,
    freed_bytes: bytes,
    moved_to_trash: false,
    sizing: "allocated",
    status: "success",
  };
}

function run(o: Partial<CleanupRun> = {}): CleanupRun {
  const base: CleanupRun = {
    run_id: "r1",
    finished_at_unix: T,
    entry_count: 1,
    requested_bytes: 100,
    freed_bytes: 100,
    success: 1,
    partial: 0,
    failed: 0,
    cancelled: 0,
    providers: ["python_cache"],
    pre_allocated_sizing: false,
    entries: [entry("a")],
  };
  return { ...base, ...o };
}

describe("runHasProblems / filterRuns", () => {
  it("treats failed and partial runs as problems, cancelled as clean", () => {
    expect(runHasProblems(run())).toBe(false);
    expect(runHasProblems(run({ failed: 1 }))).toBe(true);
    expect(runHasProblems(run({ partial: 1 }))).toBe(true);
    expect(runHasProblems(run({ cancelled: 5 }))).toBe(false);
  });

  it("filters and counts consistently", () => {
    const runs = [run(), run({ run_id: "r2", failed: 1 }), run({ run_id: "r3" })];
    expect(filterRuns(runs, "all").length).toBe(3);
    expect(filterRuns(runs, "problems").length).toBe(1);
    expect(filterRuns(runs, "clean").length).toBe(2);
    expect(countRunsByFilter(runs, "problems")).toBe(1);
  });
});

describe("runTotals", () => {
  it("sums across runs — the bug was totals taken from a truncated window", () => {
    // One huge run + one small old run: both must be counted.
    const huge = run({
      run_id: "new",
      entry_count: 289,
      requested_bytes: 28_900,
      freed_bytes: 28_900,
      success: 289,
    });
    const old = run({
      run_id: "old",
      entry_count: 1,
      requested_bytes: 10_800_000_000,
      freed_bytes: 10_800_000_000,
    });
    const t = runTotals([huge, old]);
    expect(t.freed).toBe(10_800_028_900);
    expect(t.requested).toBe(10_800_028_900);
    expect(t.runs).toBe(2);
    expect(t.resources).toBe(290);
    expect(t.problems).toBe(0);
  });

  it("counts problems across runs", () => {
    expect(runTotals([run({ failed: 2 }), run({ partial: 1 })]).problems).toBe(3);
  });

  it("handles an empty history", () => {
    expect(runTotals([])).toEqual({
      freed: 0,
      requested: 0,
      runs: 0,
      resources: 0,
      problems: 0,
    });
  });
});

describe("groupRunsByDay", () => {
  it("groups by day and sums per day", () => {
    const base = new Date(T * 1000);
    const nextDay = base.getTime() + 86_400_000;
    const days = groupRunsByDay([
      run({ run_id: "a", finished_at_unix: base.getTime() / 1000, freed_bytes: 10 }),
      run({ run_id: "b", finished_at_unix: nextDay / 1000, freed_bytes: 20 }),
      run({ run_id: "c", finished_at_unix: (nextDay + 1000) / 1000, freed_bytes: 5 }),
    ]);
    expect(days.length).toBe(2);
    expect(days[0].runs.length).toBe(2);
    expect(days[0].freed).toBe(25);
    expect(days[0].resources).toBe(2);
    expect(days[1].freed).toBe(10);
  });

  it("returns nothing for an empty history", () => {
    expect(groupRunsByDay([])).toEqual([]);
  });
});

describe("misc", () => {
  it("flags pre-v0.6 runs (no run_id) as legacy", () => {
    expect(isLegacyRun(run())).toBe(false);
    expect(isLegacyRun(run({ run_id: "" }))).toBe(true);
  });

  it("formats resource counts and invalid timestamps", () => {
    expect(entryCountLabel(1)).toBe("1 resource");
    expect(entryCountLabel(289)).toBe("289 resources");
    expect(when(Number.NaN)).toBe("—");
  });
});

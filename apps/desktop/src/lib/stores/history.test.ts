import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CleanupRun } from "$lib/tauri";

/**
 * History store: runs, not raw entries.
 *
 * The log is append-only and never truncated, so an entry-level window would
 * let one large cleanup hide every run before it. What matters here:
 * - `refresh` asks for a bounded number of runs and says so honestly via
 *   `truncated` when the page is showing the cap rather than everything;
 * - `ensureLoaded` reads once per visit and never stacks concurrent loads;
 * - a failure is surfaced as a string, never a throw into the page.
 */
vi.mock("$lib/tauri", async () => {
  const { tauriMock } = await import("./tauri-mock");
  return tauriMock;
});

import { resetTauriMock, tauriMock } from "./tauri-mock";
import { historyStore } from "./history.svelte";

function run(id: string): CleanupRun {
  return {
    pre_allocated_sizing: false,
    run_id: id,
    finished_at_unix: 1_700_000_000,
    entry_count: 1,
    requested_bytes: 100,
    freed_bytes: 100,
    success: 1,
    partial: 0,
    failed: 0,
    cancelled: 0,
    providers: ["node_modules"],
    entries: [],
  };
}

beforeEach(() => {
  resetTauriMock();
  historyStore.runs = [];
  historyStore.truncated = false;
  historyStore.loading = false;
  historyStore.error = null;
});

describe("refresh", () => {
  it("loads runs and marks the list complete under the cap", async () => {
    tauriMock.getHistoryRuns.mockResolvedValueOnce([run("r1"), run("r2")]);
    await historyStore.refresh();
    expect(historyStore.runs.map((r) => r.run_id)).toEqual(["r1", "r2"]);
    expect(historyStore.truncated).toBe(false);
    expect(historyStore.loading).toBe(false);
    expect(historyStore.error).toBeNull();
  });

  it("marks the list truncated when it hits the requested cap", async () => {
    // The store asks for 50; a full page means the backend may have more.
    const full = Array.from({ length: 50 }, (_, i) => run(`r${i}`));
    tauriMock.getHistoryRuns.mockResolvedValueOnce(full);
    await historyStore.refresh();
    expect(historyStore.runs).toHaveLength(50);
    expect(historyStore.truncated).toBe(true);
    expect(tauriMock.getHistoryRuns).toHaveBeenCalledWith(50);
  });

  it("surfaces a failure as an error string", async () => {
    tauriMock.getHistoryRuns.mockRejectedValueOnce(new Error("log unreadable"));
    await historyStore.refresh();
    expect(historyStore.error).toBe("Error: log unreadable");
    expect(historyStore.loading).toBe(false);
    expect(historyStore.runs).toEqual([]);
  });
});

describe("ensureLoaded", () => {
  it("loads once, then short-circuits on later visits", async () => {
    tauriMock.getHistoryRuns.mockResolvedValue([run("r1")]);
    await historyStore.ensureLoaded();
    await historyStore.ensureLoaded();
    expect(tauriMock.getHistoryRuns).toHaveBeenCalledTimes(1);
  });

  it("does not start a second load while one is in flight", async () => {
    let release!: (r: CleanupRun[]) => void;
    tauriMock.getHistoryRuns.mockImplementationOnce(
      () => new Promise<CleanupRun[]>((r) => (release = r)),
    );
    const first = historyStore.ensureLoaded();
    const second = historyStore.ensureLoaded(); // must no-op, not queue
    release([run("r1")]);
    await Promise.all([first, second]);
    expect(tauriMock.getHistoryRuns).toHaveBeenCalledTimes(1);
  });
});

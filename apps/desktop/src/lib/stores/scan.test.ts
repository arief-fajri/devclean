import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Resource, ScanSummary } from "$lib/tauri";

/**
 * Scan store: selection, summary application, targeted re-scan merging.
 *
 * The behavioural invariants pinned here:
 * - a targeted re-scan must NOT flip the whole list into `scanning` (that
 *   disables selection and dims every card — the bug that made a post-cleanup
 *   refresh feel like a freeze);
 * - `scan_completed` is ignored while a targeted re-scan is in flight, so
 *   the event cannot clobber the merge with one provider's rows;
 * - `pruneCleaned` drops cleaned ids immediately and deterministically,
 *   without waiting for any rescan;
 * - a selection never survives a summary that no longer contains the id.
 */
vi.mock("$lib/tauri", async () => {
  const { tauriMock } = await import("./tauri-mock");
  return tauriMock;
});

import { listeners, resetTauriMock, tauriMock } from "./tauri-mock";
import { scanStore } from "./scan.svelte";

function res(id: string, over: Partial<Resource> = {}): Resource {
  return {
    id,
    provider: "node_modules",
    name: id,
    path: `/tmp/${id}`,
    size_bytes: 100,
    reclaimable_bytes: 100,
    safety: "safe",
    blocked_reason: null,
    ...over,
  };
}

function summary(resources: Resource[], unique: number | null = 1000): ScanSummary {
  const total = resources.reduce((a, r) => a + r.size_bytes, 0);
  return {
    total_bytes: total,
    reclaimable_bytes: total,
    blocked_bytes: 0,
    unique_bytes: unique,
    providers: [
      {
        provider: "node_modules",
        name: "node_modules",
        total_bytes: total,
        reclaimable_bytes: total,
        resource_count: resources.length,
      },
    ],
    resources,
  };
}

beforeEach(async () => {
  // Let any pending targeted-scan guard release (`setTimeout(0)` inside the
  // store) fire before this test starts, so `targetedScans` is back to zero.
  // Without this, a previous test's in-flight guard leaks into the next one
  // and the event handlers legitimately — but confusingly — no-op.
  await new Promise((r) => setTimeout(r, 0));
  resetTauriMock();
  scanStore.scanning = false;
  scanStore.cancelling = false;
  scanStore.progress = null;
  scanStore.summary = null;
  scanStore.selected = [];
  scanStore.detail = null;
  scanStore.rescanning = null;
  scanStore.partialScan = null;
  scanStore.error = null;
  scanStore.lastScannedAt = null;
  scanStore.fromCache = false;
  scanStore.activeProvider = null;
  scanStore.doneProviders = [];
});

describe("selection", () => {
  it("toggle adds then removes an id", () => {
    scanStore.toggle("a");
    expect(scanStore.selected).toEqual(["a"]);
    scanStore.toggle("a");
    expect(scanStore.selected).toEqual([]);
  });

  it("selectMany deduplicates and unions", () => {
    scanStore.selected = ["a"];
    scanStore.selectMany(["a", "b", "b"]);
    expect(scanStore.selected).toEqual(["a", "b"]);
  });

  it("toggleProvider selects all of a provider, then deselects all", () => {
    scanStore.summary = summary([res("a"), res("b"), res("c", { provider: "docker" })]);
    scanStore.toggleProvider("node_modules");
    expect(scanStore.selected).toEqual(["a", "b"]);

    scanStore.toggleProvider("node_modules");
    expect(scanStore.selected).toEqual([]);
  });

  it("selectedBytes sums the reclaimable bytes of the selected ids only", () => {
    scanStore.summary = summary([
      res("a", { reclaimable_bytes: 100 }),
      res("b", { reclaimable_bytes: 40 }),
    ]);
    scanStore.selected = ["b"];
    expect(scanStore.selectedBytes()).toBe(40);
  });

  it("applySummary drops selections that no longer exist", () => {
    scanStore.selected = ["a", "ghost"];
    scanStore.applySummary(summary([res("a")]));
    expect(scanStore.selected).toEqual(["a"]);
  });

  it("resourcesByProvider filters the current summary", () => {
    scanStore.summary = summary([res("a"), res("b", { provider: "docker" })]);
    expect(scanStore.resourcesByProvider("docker").map((r) => r.id)).toEqual(["b"]);
  });
});

describe("pruneCleaned", () => {
  it("removes success results immediately and prunes the selection", () => {
    scanStore.summary = summary([res("a"), res("b")]);
    scanStore.selected = ["a", "b"];
    scanStore.pruneCleaned([
      {
        resource_id: "a",
        requested_bytes: 100,
        freed_bytes: 100,
        moved_to_trash: false,
        duration_ms: 1,
        status: "success",
      },
      {
        resource_id: "b",
        requested_bytes: 100,
        freed_bytes: 0,
        moved_to_trash: false,
        duration_ms: 1,
        status: "failed",
        error: "boom",
      },
    ]);
    // Only the successful one is gone; the failed one stays (and keeps its
    // selection so the user can retry it).
    expect(scanStore.summary!.resources.map((r) => r.id)).toEqual(["b"]);
    expect(scanStore.selected).toEqual(["b"]);
  });

  it("is a no-op for an empty summary", () => {
    scanStore.pruneCleaned([]);
    expect(scanStore.summary).toBeNull();
  });
});

describe("loadCached", () => {
  it("applies the cache and stamps the scan's real age", async () => {
    const savedAt = 1_700_000_000;
    tauriMock.getCachedScan.mockResolvedValueOnce({
      summary: summary([res("a")]),
      saved_at_unix: savedAt,
    });
    await scanStore.loadCached();
    expect(scanStore.summary!.resources).toHaveLength(1);
    expect(scanStore.fromCache).toBe(true);
    expect(scanStore.lastScannedAt!.getTime()).toBe(savedAt * 1000);
  });

  it("leaves the first-run state alone when there is no cache", async () => {
    tauriMock.getCachedScan.mockResolvedValueOnce(null);
    await scanStore.loadCached();
    expect(scanStore.summary).toBeNull();
    expect(scanStore.fromCache).toBe(false);
    expect(scanStore.lastScannedAt).toBeNull();
  });

  it("never throws, even when the bridge rejects", async () => {
    tauriMock.getCachedScan.mockRejectedValueOnce(new Error("nope"));
    await expect(scanStore.loadCached()).resolves.toBeUndefined();
    expect(scanStore.summary).toBeNull();
  });
});

describe("start", () => {
  it("applies the full summary and leaves scanning false", async () => {
    tauriMock.scanAll.mockResolvedValueOnce(summary([res("a"), res("b")]));
    await scanStore.start();
    expect(scanStore.summary!.resources).toHaveLength(2);
    expect(scanStore.scanning).toBe(false);
    expect(scanStore.fromCache).toBe(false);
    expect(scanStore.lastScannedAt).toBeInstanceOf(Date);
  });

  it("surfaces a bridge failure as an error string, not a throw", async () => {
    tauriMock.scanAll.mockRejectedValueOnce(new Error("scan exploded"));
    await scanStore.start();
    expect(scanStore.error).toBe("Error: scan exploded");
    expect(scanStore.scanning).toBe(false);
    expect(scanStore.summary).toBeNull();
  });
});

describe("targeted re-scan", () => {
  it("merges one provider without flipping the global scanning flag", async () => {
    scanStore.summary = summary([res("keep", { provider: "docker" })]);
    tauriMock.scanProvider.mockResolvedValueOnce(
      summary([res("fresh", { provider: "node_modules" })], null),
    );

    // Observe the flag while the re-scan is actually in flight.
    const racing = scanStore.rescanProvider("node_modules");
    const sawScanning = scanStore.scanning;
    await racing;

    expect(sawScanning, "a targeted rescan must not dim the whole list").toBe(false);
    const ids = scanStore.summary!.resources.map((r) => r.id).sort();
    expect(ids, "the other provider's rows survive").toEqual(["fresh", "keep"]);
    expect(scanStore.rescanning).toBeNull();
  });

  it("rescanProviders records the partial badge and never sets scanning", async () => {
    scanStore.summary = summary([res("keep", { provider: "docker" })]);
    tauriMock.scanProvider.mockResolvedValueOnce(
      summary([res("fresh", { provider: "node_modules" })], null),
    );

    const racing = scanStore.rescanProviders(["node_modules"]);
    expect(scanStore.scanning).toBe(false);
    await racing;

    expect(scanStore.partialScan).toEqual(["node_modules"]);
    expect(scanStore.fromCache).toBe(false);
    expect(scanStore.scanning).toBe(false);
  });

  it("an empty provider list falls back to a full scan", async () => {
    tauriMock.scanAll.mockResolvedValueOnce(summary([res("a")]));
    await scanStore.rescanProviders([]);
    expect(scanStore.summary!.resources).toHaveLength(1);
    expect(tauriMock.scanAll).toHaveBeenCalledTimes(1);
    expect(tauriMock.scanProvider).not.toHaveBeenCalled();
  });

  it("scan_completed is ignored while a targeted re-scan is in flight", async () => {
    scanStore.listen();
    scanStore.summary = summary([res("keep", { provider: "docker" })]);
    tauriMock.scanProvider.mockResolvedValueOnce(
      summary([res("fresh", { provider: "node_modules" })], null),
    );

    const racing = scanStore.rescanProviders(["node_modules"]);
    // The menu bar's background scan finishes mid-flight: its summary must
    // not replace the merge-in-progress.
    listeners.scanCompleted?.(summary([res("imposter")]));
    await racing;

    const ids = scanStore.summary!.resources.map((r) => r.id).sort();
    expect(ids).toEqual(["fresh", "keep"]);
  });
});

describe("events", () => {
  it("a global scan_started flips scanning; a targeted one does not", async () => {
    scanStore.listen();
    listeners.scanStarted?.();
    expect(scanStore.scanning).toBe(true);

    // Now with a targeted scan in flight the event must be ignored.
    scanStore.scanning = false;
    const racing = scanStore.rescanProvider("node_modules");
    listeners.scanStarted?.();
    expect(scanStore.scanning).toBe(false);
    await racing;
  });

  it("progress tracks the doneProviders set without duplicates", () => {
    scanStore.listen();
    listeners.scanProgress?.({ provider: "docker", done: 1, total: 2 });
    listeners.scanProgress?.({ provider: "docker", done: 2, total: 2 });
    listeners.scanProgress?.({ provider: "node_modules", done: 1, total: 1 });
    expect(scanStore.doneProviders).toEqual(["docker", "node_modules"]);
    expect(scanStore.progress).toEqual({ provider: "node_modules", done: 1, total: 1 });
  });

  it("scan_completed applies the summary when no targeted scan is in flight", () => {
    scanStore.listen();
    listeners.scanCompleted?.(summary([res("from-event")]));
    expect(scanStore.summary!.resources.map((r) => r.id)).toEqual(["from-event"]);
    expect(scanStore.scanning).toBe(false);
  });

  it("scan_provider_started drives the per-card loader", () => {
    scanStore.listen();
    listeners.scanProviderStarted?.("docker");
    expect(scanStore.activeProvider).toBe("docker");
    // A finished provider clears its own loader.
    listeners.scanProgress?.({ provider: "docker", done: 3, total: 3 });
    expect(scanStore.activeProvider).toBeNull();
  });
});

describe("openDetail", () => {
  it("resolves the id server-side and stores the resource", async () => {
    tauriMock.getResourceDetails.mockResolvedValueOnce(res("detail-me"));
    await scanStore.openDetail("detail-me");
    expect(scanStore.detail?.id).toBe("detail-me");
    scanStore.closeDetail();
    expect(scanStore.detail).toBeNull();
  });

  it("surfaces a failure without throwing", async () => {
    tauriMock.getResourceDetails.mockRejectedValueOnce(new Error("gone"));
    await scanStore.openDetail("ghost");
    expect(scanStore.error).toBe("Error: gone");
    expect(scanStore.detail).toBeNull();
  });
});

describe("cancel", () => {
  it("is a no-op when nothing is scanning", async () => {
    await scanStore.cancel();
    expect(tauriMock.cancelScan).not.toHaveBeenCalled();
  });

  it("drops the scanning flag even if the bridge rejects", async () => {
    scanStore.scanning = true;
    tauriMock.cancelScan.mockRejectedValueOnce(new Error("late"));
    await scanStore.cancel();
    expect(scanStore.scanning).toBe(false);
    expect(scanStore.error).toBe("Error: late");
  });
});

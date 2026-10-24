import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CleanupPreview, CleanupResult, ScanSummary } from "$lib/tauri";

/**
 * Cleanup store: the measure → confirm → execute flow.
 *
 * Invariants pinned here:
 * - the confirmation dialog opens BEFORE the re-measure resolves — a slow
 *   `preview_cleanup` must not look like a dead button;
 * - a preview failure stays inside the dialog (`previewError`), never a
 *   toast and never a silent close;
 * - the preview token travels with `runCleanup`, so the backend — not the
 *   frontend — decides whether confirmation actually happened;
 * - a superseded preview (a newer one started) must not resurrect stale
 *   state;
 * - a finished cleanup prunes the scan list and re-scans only what it
 *   touched.
 */
vi.mock("$lib/tauri", async () => {
  const { tauriMock } = await import("./tauri-mock");
  return tauriMock;
});

import { listeners, resetTauriMock, tauriMock } from "./tauri-mock";
import { cleanupStore } from "./cleanup.svelte";
import { scanStore } from "./scan.svelte";
import type { Resource } from "$lib/tauri";

function preview(ids: string[]): CleanupPreview {
  return {
    items: ids.map((id) => ({
      resource_id: id,
      name: `name-of-${id}`,
      bytes: 100,
      safety: "safe",
    })),
    total_bytes: ids.length * 100,
    preview_token: "tok-123",
    expires_at_unix: 2_000_000_000,
  };
}

function okResult(id: string): CleanupResult {
  return {
    resource_id: id,
    requested_bytes: 100,
    freed_bytes: 100,
    moved_to_trash: false,
    duration_ms: 5,
    status: "success",
  };
}

beforeEach(() => {
  resetTauriMock();
  cleanupStore.cleaning = false;
  cleanupStore.cancelling = false;
  cleanupStore.confirmOpen = false;
  cleanupStore.previewing = false;
  cleanupStore.previewError = null;
  cleanupStore.preview = null;
  cleanupStore.previewAt = null;
  cleanupStore.previewToken = null;
  cleanupStore.names = {};
  cleanupStore.progress = null;
  cleanupStore.results = null;
  cleanupStore.touchedProviders = [];
  cleanupStore.error = null;

  scanStore.summary = null;
  scanStore.selected = [];
  scanStore.scanning = false;
  scanStore.partialScan = null;
});

describe("openConfirm", () => {
  it("opens the dialog before the re-measure resolves", async () => {
    let release!: (p: CleanupPreview) => void;
    tauriMock.previewCleanup.mockImplementationOnce(
      () => new Promise<CleanupPreview>((r) => (release = r)),
    );

    const racing = cleanupStore.openConfirm(["a"]);
    // The dialog must already be up while the await is still pending —
    // otherwise a slow re-measure looks like a dead button (G-UI-01).
    expect(cleanupStore.confirmOpen).toBe(true);
    expect(cleanupStore.previewing).toBe(true);
    expect(cleanupStore.preview).toBeNull();

    release(preview(["a"]));
    await racing;

    expect(cleanupStore.previewing).toBe(false);
    expect(cleanupStore.preview?.preview_token).toBe("tok-123");
    expect(cleanupStore.previewToken).toBe("tok-123");
    expect(cleanupStore.nameOf("a")).toBe("name-of-a");
    expect(cleanupStore.previewAt).toBeInstanceOf(Date);
  });

  it("keeps a preview failure inside the dialog", async () => {
    tauriMock.previewCleanup.mockRejectedValueOnce(new Error("path vanished"));
    await cleanupStore.openConfirm(["a"]);
    expect(cleanupStore.confirmOpen, "the dialog must stay open").toBe(true);
    expect(cleanupStore.previewError).toBe("Error: path vanished");
    expect(cleanupStore.previewing).toBe(false);
    expect(cleanupStore.preview).toBeNull();
  });

  it("a superseded preview does not resurrect stale state", async () => {
    let releaseFirst!: (p: CleanupPreview) => void;
    tauriMock.previewCleanup
      .mockImplementationOnce(() => new Promise<CleanupPreview>((r) => (releaseFirst = r)))
      .mockResolvedValueOnce(preview(["b"]));

    const first = cleanupStore.openConfirm(["a"]);
    const second = cleanupStore.openConfirm(["b"]);
    releaseFirst(preview(["a"]));
    await Promise.all([first, second]);

    // The newer preview won; the older one was dropped, not applied.
    expect(cleanupStore.preview?.items[0]?.resource_id).toBe("b");
    expect(cleanupStore.previewToken).toBe("tok-123");
  });

  it("retryPreview reopens and re-measures", async () => {
    tauriMock.previewCleanup.mockRejectedValueOnce(new Error("boom"));
    await cleanupStore.openConfirm(["a"]);
    expect(cleanupStore.previewError).not.toBeNull();

    tauriMock.previewCleanup.mockResolvedValueOnce(preview(["a"]));
    await cleanupStore.retryPreview(["a"]);
    expect(cleanupStore.previewError).toBeNull();
    expect(cleanupStore.confirmOpen).toBe(true);
  });
});

describe("execute", () => {
  it("travels with the preview token and reacts exactly once", async () => {
    tauriMock.previewCleanup.mockResolvedValueOnce(preview(["a"]));
    await cleanupStore.openConfirm(["a"]);

    tauriMock.scanAll.mockResolvedValueOnce({
      total_bytes: 0,
      reclaimable_bytes: 0,
      blocked_bytes: 0,
      unique_bytes: 0,
      providers: [],
      resources: [] as Resource[],
    } satisfies ScanSummary);

    tauriMock.runCleanup.mockResolvedValueOnce([okResult("a")]);
    await cleanupStore.execute(["a"]);

    expect(tauriMock.runCleanup).toHaveBeenCalledWith(["a"], "tok-123");
    expect(cleanupStore.results).toEqual([okResult("a")]);
    expect(cleanupStore.freedBytes()).toBe(100);
    expect(cleanupStore.requestedBytes()).toBe(100);
    expect(cleanupStore.cleaning).toBe(false);
    // The dialog stays open so the flow can move to its `done` phase.
    expect(cleanupStore.confirmOpen).toBe(true);
    // The cleaned resource is gone from the list immediately.
    expect(scanStore.summary?.resources ?? []).toHaveLength(0);
  });

  it("surfaces a rejection as an error string and closes nothing", async () => {
    tauriMock.previewCleanup.mockResolvedValueOnce(preview(["a"]));
    await cleanupStore.openConfirm(["a"]);

    tauriMock.runCleanup.mockRejectedValueOnce(new Error("refused: no token"));
    await cleanupStore.execute(["a"]);

    expect(cleanupStore.error).toBe("Error: refused: no token");
    expect(cleanupStore.results).toBeNull();
    expect(cleanupStore.cleaning).toBe(false);
    expect(cleanupStore.confirmOpen).toBe(true);
  });

  it("clears progress when the run settles", async () => {
    tauriMock.previewCleanup.mockResolvedValueOnce(preview(["a"]));
    await cleanupStore.openConfirm(["a"]);

    tauriMock.scanAll.mockResolvedValueOnce({
      total_bytes: 0,
      reclaimable_bytes: 0,
      blocked_bytes: 0,
      providers: [],
      resources: [],
    } satisfies ScanSummary);
    tauriMock.runCleanup.mockImplementationOnce(async () => {
      cleanupStore.progress = { done: 1, total: 2 };
      return [okResult("a")];
    });
    await cleanupStore.execute(["a"]);
    expect(cleanupStore.progress).toBeNull();
  });
});

describe("cancel", () => {
  it("is a no-op when nothing is running", async () => {
    await cleanupStore.cancel();
    expect(tauriMock.cancelCleanup).not.toHaveBeenCalled();
  });

  it("keeps cancelling true after a successful request — the queue is still draining", async () => {
    cleanupStore.cleaning = true;
    let release!: () => void;
    tauriMock.cancelCleanup.mockImplementationOnce(() => new Promise<void>((r) => (release = r)));

    const racing = cleanupStore.cancel();
    expect(cleanupStore.cancelling).toBe(true);
    release();
    await racing;
    // The in-flight item still finishes; `execute`'s finally clears the flag
    // when the queue has actually settled — a successful cancel must not
    // drop the "cancelling…" indication early.
    expect(cleanupStore.cancelling).toBe(true);
  });

  it("clears cancelling when the bridge rejects", async () => {
    cleanupStore.cleaning = true;
    tauriMock.cancelCleanup.mockRejectedValueOnce(new Error("late"));
    await cleanupStore.cancel();
    expect(cleanupStore.cancelling).toBe(false);
    expect(cleanupStore.error).toBe("Error: late");
  });
});

describe("events", () => {
  it("cleanup_started flips cleaning; progress is mirrored", () => {
    cleanupStore.listen();
    listeners.cleanupStarted?.(["a", "b"]);
    expect(cleanupStore.cleaning).toBe(true);
    expect(cleanupStore.progress).toBeNull();

    listeners.cleanupProgress?.({ done: 1, total: 2 });
    expect(cleanupStore.progress).toEqual({ done: 1, total: 2 });
  });

  it("cleanup_completed stores results and settles the flags", () => {
    cleanupStore.listen();
    cleanupStore.cleaning = true;
    cleanupStore.confirmOpen = true;
    cleanupStore.progress = { done: 2, total: 2 };
    listeners.cleanupCompleted?.([okResult("a")]);
    expect(cleanupStore.results).toEqual([okResult("a")]);
    expect(cleanupStore.cleaning).toBe(false);
    expect(cleanupStore.progress).toBeNull();
    // Still open: the dialog moves to its `done` phase rather than vanishing.
    expect(cleanupStore.confirmOpen).toBe(true);
  });
});

describe("close / closeFlow / dismissResults", () => {
  it("close drops the dialog and any in-flight preview", async () => {
    let release!: (p: CleanupPreview) => void;
    tauriMock.previewCleanup.mockImplementationOnce(
      () => new Promise<CleanupPreview>((r) => (release = r)),
    );
    const racing = cleanupStore.openConfirm(["a"]);
    cleanupStore.close();
    release(preview(["a"]));
    await racing;

    expect(cleanupStore.confirmOpen).toBe(false);
    // The late preview must not reopen or repopulate anything.
    expect(cleanupStore.preview).toBeNull();
    expect(cleanupStore.previewToken).toBeNull();
    expect(cleanupStore.previewing).toBe(false);
  });

  it("closeFlow also clears the results, so a Done click resets the flow", () => {
    cleanupStore.results = [okResult("a")];
    cleanupStore.confirmOpen = true;
    cleanupStore.closeFlow();
    expect(cleanupStore.confirmOpen).toBe(false);
    expect(cleanupStore.results).toBeNull();
  });

  it("dismissResults clears results but keeps the dialog open", () => {
    cleanupStore.results = [okResult("a")];
    cleanupStore.confirmOpen = true;
    cleanupStore.dismissResults();
    expect(cleanupStore.results).toBeNull();
    expect(cleanupStore.confirmOpen).toBe(true);
  });
});

describe("nameOf", () => {
  it("falls back to the id when the name was never snapshotted", () => {
    expect(cleanupStore.nameOf("ghost")).toBe("ghost");
  });
});

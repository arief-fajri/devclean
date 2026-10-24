/// <reference types="node" />
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

/**
 * A finished cleanup must re-scan only the providers it touched, in the
 * background, without flipping the global scanning state.
 *
 * It used to call `scanStore.start()` on every `cleanup_completed`: a full
 * sweep of all providers that set `scanning`, dimmed the cards and disabled
 * selection — the app felt frozen right after a delete. The behaviour is now
 * covered by the store tests (`stores/scan.test.ts`); these rules pin the
 * *wiring* between the two stores, like `startup.test.ts`.
 */
const here = dirname(fileURLToPath(import.meta.url));
const scan = readFileSync(join(here, "stores", "scan.svelte.ts"), "utf8");
const cleanup = readFileSync(join(here, "stores", "cleanup.svelte.ts"), "utf8");

describe("post-cleanup rescan is targeted", () => {
  it("no longer runs a full scan when a cleanup completes", () => {
    // The scan store must not react to cleanup at all any more.
    expect(
      scan.includes("onCleanupCompleted"),
      "scanStore still listens to cleanup_completed",
    ).toBe(false);
  });

  it("re-scans only the providers that were touched", () => {
    expect(cleanup).toContain("scanStore.rescanProviders(this.touchedProviders)");
  });

  it("removes the cleaned items immediately, without waiting for the rescan", () => {
    // Deterministic: the backend reported the ids, so the list drops them at
    // once. Otherwise a slow or failed rescan leaves the rows on screen. The
    // reaction is driven from the awaited result, not only the event.
    expect(cleanup).toContain("scanStore.pruneCleaned(this.results)");
    const prune = cleanup.indexOf("scanStore.pruneCleaned(this.results)");
    const rescan = cleanup.indexOf("scanStore.rescanProviders(this.touchedProviders)");
    expect(prune, "prune must run before the rescan").toBeGreaterThan(-1);
    expect(prune).toBeLessThan(rescan);
  });

  it("suppresses global scan state but keeps the per-card loader", () => {
    // Only the global handlers are guarded: `scanning` must not flip (that is
    // what froze the app), and `scan_completed` must not clobber the merge.
    // `onScanProviderStarted`/`onScanProgress` stay live so the tile loader
    // shows on the provider being rescanned.
    const guards = scan.match(/if \(this\.targetedScans > 0\) return;/g) ?? [];
    expect(guards.length, "exactly onScanStarted and onScanCompleted are guarded").toBe(2);
    expect(scan).toContain("onScanProviderStarted((provider) => {");
    expect(scan).toContain("this.activeProvider = provider;");
  });
});

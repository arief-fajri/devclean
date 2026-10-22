import { describe, expect, it } from "vitest";
import { mergeProviderSummary, pruneResources } from "./scan-merge";
import type { Resource, ScanSummary } from "./tauri";

function res(o: Partial<Resource> & Pick<Resource, "id" | "provider">): Resource {
  return {
    name: o.id,
    path: `/tmp/${o.id}`,
    size_bytes: 100,
    reclaimable_bytes: 100,
    safety: "safe",
    blocked_reason: null,
    ...o,
  };
}

function summary(o: Partial<ScanSummary> = {}): ScanSummary {
  const resources = o.resources ?? [
    res({ id: "nm:0", provider: "node_modules", reclaimable_bytes: 400, size_bytes: 400 }),
    res({ id: "go:0", provider: "go_cache", reclaimable_bytes: 300, size_bytes: 300 }),
  ];
  return {
    total_bytes: resources.reduce((a, r) => a + r.size_bytes, 0),
    reclaimable_bytes: resources.reduce((a, r) => a + r.reclaimable_bytes, 0),
    blocked_bytes: 0,
    providers: [
      { provider: "node_modules", name: "node_modules", total_bytes: 400, reclaimable_bytes: 400, resource_count: 1 },
      { provider: "go_cache", name: "Go cache", total_bytes: 300, reclaimable_bytes: 300, resource_count: 1 },
    ],
    resources,
    ...o,
  };
}

describe("mergeProviderSummary", () => {
  it("replaces the target provider's rows and keeps the others", () => {
    const partial: ScanSummary = {
      total_bytes: 50,
      reclaimable_bytes: 50,
      blocked_bytes: 0,
      providers: [
        { provider: "node_modules", name: "node_modules", total_bytes: 50, reclaimable_bytes: 50, resource_count: 1 },
      ],
      resources: [res({ id: "nm:new", provider: "node_modules", reclaimable_bytes: 50, size_bytes: 50 })],
    };

    const merged = mergeProviderSummary(summary(), "node_modules", partial);

    expect(merged.resources.map((r) => r.id).sort()).toEqual(["go:0", "nm:new"]);
    expect(merged.providers.map((p) => p.provider)).toEqual(["go_cache", "node_modules"]);
    expect(merged.providers.find((p) => p.provider === "go_cache")?.reclaimable_bytes).toBe(300);
    expect(merged.providers.find((p) => p.provider === "node_modules")?.reclaimable_bytes).toBe(50);
    expect(merged.reclaimable_bytes).toBe(350);
    expect(merged.total_bytes).toBe(350);
  });

  it("drops the provider's rows when the rescan found nothing", () => {
    const empty: ScanSummary = {
      total_bytes: 0,
      reclaimable_bytes: 0,
      blocked_bytes: 0,
      providers: [],
      resources: [],
    };
    const merged = mergeProviderSummary(summary(), "node_modules", empty);
    expect(merged.resources.map((r) => r.id)).toEqual(["go:0"]);
    expect(merged.providers.map((p) => p.provider)).toEqual(["go_cache"]);
    expect(merged.reclaimable_bytes).toBe(300);
  });

  it("recomputes blocked_bytes from the merged rows", () => {
    const partial: ScanSummary = {
      total_bytes: 200,
      reclaimable_bytes: 200,
      blocked_bytes: 200,
      providers: [
        { provider: "node_modules", name: "node_modules", total_bytes: 200, reclaimable_bytes: 200, resource_count: 1 },
      ],
      resources: [
        res({ id: "nm:blocked", provider: "node_modules", reclaimable_bytes: 200, size_bytes: 200, blocked_reason: "protected" }),
      ],
    };
    const merged = mergeProviderSummary(summary(), "node_modules", partial);
    expect(merged.blocked_bytes).toBe(200);
  });

  // Wave D6: one provider's rows cannot measure a whole-scan dedup figure.
  // The frontend carries `unique_bytes`; a merge must null it, never
  // recompute it (the inode keys never leave the backend).
  it("clears unique_bytes — a targeted merge cannot measure it", () => {
    const before = summary({ unique_bytes: 600 });
    expect(before.unique_bytes).toBe(600);
    const partial: ScanSummary = {
      total_bytes: 50,
      reclaimable_bytes: 50,
      blocked_bytes: 0,
      providers: [],
      resources: [res({ id: "nm:new", provider: "node_modules", reclaimable_bytes: 50, size_bytes: 50 })],
    };
    expect(mergeProviderSummary(before, "node_modules", partial).unique_bytes).toBeNull();
  });
});

describe("pruneResources", () => {
  it("drops the removed ids and keeps everything else", () => {
    const pruned = pruneResources(summary(), ["nm:0"]);
    expect(pruned.resources.map((r) => r.id)).toEqual(["go:0"]);
    expect(pruned.providers.map((p) => p.provider)).toEqual(["go_cache"]);
    expect(pruned.reclaimable_bytes).toBe(300);
    expect(pruned.total_bytes).toBe(300);
  });

  it("drops a provider once its last resource is gone", () => {
    const pruned = pruneResources(summary(), ["nm:0", "go:0"]);
    expect(pruned.resources).toEqual([]);
    expect(pruned.providers).toEqual([]);
    expect(pruned.reclaimable_bytes).toBe(0);
  });

  it("is a no-op when nothing matches (same reference, no needless render)", () => {
    const s = summary();
    expect(pruneResources(s, ["does-not-exist"])).toBe(s);
    expect(pruneResources(s, [])).toBe(s);
  });

  it("recomputes blocked_bytes after the prune", () => {
    const s = summary({
      resources: [
        res({ id: "nm:0", provider: "node_modules", reclaimable_bytes: 400, size_bytes: 400 }),
        res({
          id: "go:0",
          provider: "go_cache",
          reclaimable_bytes: 300,
          size_bytes: 300,
          blocked_reason: "protected",
        }),
      ],
    });
    const pruned = pruneResources(s, ["go:0"]);
    expect(pruned.blocked_bytes).toBe(0);
  });

  // Wave D6: rows changed, so the whole-scan figure no longer holds.
  it("clears unique_bytes — the rows behind it changed", () => {
    const pruned = pruneResources(summary({ unique_bytes: 600 }), ["nm:0"]);
    expect(pruned.unique_bytes).toBeNull();
  });
});

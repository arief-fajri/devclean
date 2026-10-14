import { describe, expect, it } from "vitest";
import {
  addableCandidates,
  alreadyAddedCount,
  applyView,
  countBySafety,
  DEFAULT_VIEW,
  matchesQuery,
  needsReview,
  OVERVIEW_PROVIDER_LIMIT,
  pruneExpanded,
  topProviders,
  type ViewOptions,
} from "./view";
import type { Resource, ScanSummary } from "./tauri";

function res(o: Partial<Resource> & Pick<Resource, "id">): Resource {
  return {
    provider: "docker",
    name: o.id,
    path: `/tmp/${o.id}`,
    size_bytes: 100,
    reclaimable_bytes: 100,
    safety: "safe",
    // The backend omits these when the resource is cleanable; the TS shape
    // mirrors that (absent === selectable).
    blocked_reason: null,
    ...o,
  };
}

function summary(): ScanSummary {
  return {
    total_bytes: 1000,
    reclaimable_bytes: 900,
    retention_blocked_bytes: 0,
    providers: [
      { provider: "docker", name: "Docker", total_bytes: 500, reclaimable_bytes: 500, resource_count: 1 },
      { provider: "go_cache", name: "Go cache", total_bytes: 400, reclaimable_bytes: 400, resource_count: 2 },
    ],
    resources: [
      res({ id: "dk:0", provider: "docker", name: "Docker build cache", reclaimable_bytes: 500 }),
      res({ id: "go:0", provider: "go_cache", name: "Go build cache", reclaimable_bytes: 100 }),
      res({
        id: "go:1",
        provider: "go_cache",
        name: "Go module cache",
        reclaimable_bytes: 300,
        safety: "caution",
        detail: "· 200d old",
      }),
    ],
  };
}

function opts(o: Partial<ViewOptions> = {}): ViewOptions {
  return { ...DEFAULT_VIEW, ...o };
}

describe("applyView", () => {
  it("returns no groups without a summary", () => {
    expect(applyView(null, [], opts())).toEqual([]);
  });

  it("sorts providers and rows largest-first by default", () => {
    const groups = applyView(summary(), [], opts());
    expect(groups.map((g) => g.summary.provider)).toEqual(["docker", "go_cache"]);
    expect(groups[1].resources.map((r) => r.id)).toEqual(["go:1", "go:0"]);
  });

  it("matches name, provider name, and path basename case-insensitively", () => {
    const s = summary();
    expect(applyView(s, [], opts({ query: "module" }))[0].resources.map((r) => r.id)).toEqual([
      "go:1",
    ]);
    expect(
      applyView(s, [], opts({ query: "DOCKER" })).map((g) => g.summary.provider),
    ).toEqual(["docker"]);
    // path basename: resource whose dir name matches, display name does not
    const withPath: ScanSummary = {
      ...s,
      resources: [res({ id: "x:0", provider: "docker", name: "zzz", path: "/code/my-app/node_modules" })],
    };
    expect(applyView(withPath, [], opts({ query: "my-app" }))[0].resources.map((r) => r.id)).toEqual([
      "x:0",
    ]);
  });

  it("filters by safety, with dangerous counted as review", () => {
    const s = summary();
    const safe = applyView(s, [], opts({ safety: "safe" }));
    expect(safe.flatMap((g) => g.resources).map((r) => r.id).sort()).toEqual(["dk:0", "go:0"]);
    const review = applyView(s, [], opts({ safety: "review" }));
    expect(review.flatMap((g) => g.resources).map((r) => r.id)).toEqual(["go:1"]);
    const withDanger: ScanSummary = {
      ...s,
      resources: [...s.resources, res({ id: "dg:0", safety: "dangerous" })],
    };
    expect(
      applyView(withDanger, [], opts({ safety: "review" }))
        .flatMap((g) => g.resources)
        .map((r) => r.id)
        .sort(),
    ).toEqual(["dg:0", "go:1"]);
  });

  it("filters stale signals and selection", () => {
    const s = summary();
    expect(
      applyView(s, [], opts({ staleOnly: true }))
        .flatMap((g) => g.resources)
        .map((r) => r.id),
    ).toEqual(["go:1"]);
    expect(applyView(s, ["go:0"], opts({ selectedOnly: true }))[0].resources.map((r) => r.id)).toEqual([
      "go:0",
    ]);
  });

  it("drops providers left with zero visible rows", () => {
    const groups = applyView(summary(), [], opts({ query: "docker build" }));
    expect(groups.map((g) => g.summary.provider)).toEqual(["docker"]);
  });

  it("sorts by name alphabetically", () => {
    const groups = applyView(summary(), [], opts({ sort: "name" }));
    expect(groups.map((g) => g.summary.provider)).toEqual(["docker", "go_cache"]);
    expect(groups[1].resources.map((r) => r.name)).toEqual(["Go build cache", "Go module cache"]);
  });

  it("sorts review-first, then largest", () => {
    const s = summary();
    const withBigSafe: ScanSummary = {
      ...s,
      providers: [
        ...s.providers,
        { provider: "zz", name: "Zed", total_bytes: 1, reclaimable_bytes: 1, resource_count: 1 },
      ],
      resources: [...s.resources, res({ id: "zz:0", provider: "zz", reclaimable_bytes: 10_000 })],
    };
    const groups = applyView(withBigSafe, [], opts({ sort: "safety" }));
    // go_cache holds the caution row, so it surfaces first despite docker
    // being bigger — and the caution row leads within its group.
    expect(groups[0].summary.provider).toBe("go_cache");
    expect(groups[0].resources[0].id).toBe("go:1");
  });
});

describe("countBySafety / needsReview / matchesQuery", () => {
  it("counts safe vs review", () => {
    expect(countBySafety(summary().resources)).toEqual({ safe: 2, review: 1 });
  });

  it("treats caution and dangerous as review", () => {
    expect(needsReview("safe")).toBe(false);
    expect(needsReview("caution")).toBe(true);
    expect(needsReview("dangerous")).toBe(true);
  });

  it("blank query matches everything", () => {
    expect(matchesQuery(res({ id: "a" }), "Docker", "   ")).toBe(true);
  });
});

describe("addableCandidates", () => {
  const c = (path: string, already = false) => ({
    path,
    already_configured: already,
  });

  it("drops candidates already configured on the backend", () => {
    const list = [c("/a", true), c("/b")];
    expect(addableCandidates(list, []).map((x) => x.path)).toEqual(["/b"]);
  });

  it("drops candidates already present in the unsaved draft", () => {
    // Backend says "not configured" but the user just added it in the dialog.
    const list = [c("/a"), c("/b")];
    expect(addableCandidates(list, ["/a"]).map((x) => x.path)).toEqual(["/b"]);
  });

  it("never lists the same path twice — the reported Settings duplication", () => {
    const list = [c("/a", true), c("/a"), c("/b")];
    const addable = addableCandidates(list, ["/a"]);
    expect(addable.map((x) => x.path)).toEqual(["/b"]);
    expect(alreadyAddedCount(list, ["/a"])).toBe(2);
  });

  it("handles an empty candidate list", () => {
    expect(addableCandidates([], [])).toEqual([]);
    expect(alreadyAddedCount([], [])).toBe(0);
  });
});

describe("pruneExpanded", () => {
  it("keeps providers that are still present", () => {
    expect(pruneExpanded(["a", "b"], ["a", "b", "c"])).toEqual(["a", "b"]);
  });

  it("drops a provider whose last item was cleaned", () => {
    expect(pruneExpanded(["a", "gone"], ["a", "b"])).toEqual(["a"]);
  });

  it("never invents an id — a cleaned card must not shift onto another", () => {
    // Regression for: expand A → clean A → another card appeared open.
    const expanded = ["python_cache"];
    const afterClean = pruneExpanded(expanded, ["docker", "node_modules"]);
    expect(afterClean).toEqual([]);
    expect(afterClean).not.toContain("docker");
    expect(afterClean).not.toContain("node_modules");
  });

  it("keeps the stored array untouched", () => {
    const expanded = ["a", "b"];
    pruneExpanded(expanded, ["a"]);
    expect(expanded).toEqual(["a", "b"]);
  });

  it("handles an empty list", () => {
    expect(pruneExpanded([], ["a"])).toEqual([]);
  });
});


describe("topProviders", () => {
  /** `n` providers of `bytes` each, named in order. */
  function withProviders(n: number, bytes: (i: number) => number, count = 1) {
    const providers = Array.from({ length: n }, (_, i) => ({
      provider: `p${i}`,
      name: `Provider ${i}`,
      total_bytes: bytes(i),
      reclaimable_bytes: bytes(i),
      resource_count: count,
    }));
    return {
      total_bytes: 1000,
      reclaimable_bytes: providers.reduce((a, p) => a + p.reclaimable_bytes, 0),
      retention_blocked_bytes: 0,
      providers,
      resources: [],
    };
  }

  it("lists the biggest reclaimers first", () => {
    const rows = topProviders(withProviders(3, (i) => (i + 1) * 100));
    expect(rows.map((r) => r.bytes)).toEqual([300, 200, 100]);
    expect(rows.map((r) => r.provider)).toEqual(["p2", "p1", "p0"]);
  });

  it("drops providers holding nothing reclaimable", () => {
    const rows = topProviders(withProviders(3, (i) => (i === 0 ? 0 : 500)));
    expect(rows.map((r) => r.provider)).toEqual(["p1", "p2"]);
  });

  it("stays within the list length the Overview was designed for", () => {
    // Feedback round: the card must not leave half its height empty, and must
    // not push the other cards off the fold.
    expect(OVERVIEW_PROVIDER_LIMIT).toBeGreaterThanOrEqual(8);
    expect(OVERVIEW_PROVIDER_LIMIT).toBeLessThanOrEqual(12);
  });

  it("caps the list and names the tail instead of hiding it", () => {
    const n = OVERVIEW_PROVIDER_LIMIT + 4;
    const rows = topProviders(withProviders(n, (i) => (n - i) * 100));
    expect(rows).toHaveLength(OVERVIEW_PROVIDER_LIMIT + 1);
    const other = rows[rows.length - 1];
    expect(other.isOther).toBe(true);
    expect(other.label).toBe("Other · 4 providers");
  });

  it("the aggregate reconciles with the headline", () => {
    const n = OVERVIEW_PROVIDER_LIMIT + 4;
    const summary = withProviders(n, (i) => (n - i) * 100);
    const rows = topProviders(summary);
    // every reclaimable byte is either on a row or inside "Other"
    expect(rows.reduce((a, r) => a + r.bytes, 0)).toBe(summary.reclaimable_bytes);
    expect(rows.slice(0, OVERVIEW_PROVIDER_LIMIT).reduce((a, r) => a + r.bytes, 0)).toBeGreaterThan(0);
  });

  it("the aggregate carries the resources it stands for", () => {
    const n = OVERVIEW_PROVIDER_LIMIT + 3;
    const tail = 3;
    const rows = topProviders(withProviders(n, (i) => (n - i) * 10, 2));
    const other = rows[rows.length - 1];
    expect(other.isOther).toBe(true);
    expect(other.resourceCount).toBe(2 * tail);
  });

  it("adds no aggregate when everything fits", () => {
    const rows = topProviders(withProviders(4, (i) => (i + 1) * 10));
    expect(rows).toHaveLength(4);
    expect(rows.some((r) => r.isOther)).toBe(false);
  });

  it("names the single tail provider instead of counting it", () => {
    const n = OVERVIEW_PROVIDER_LIMIT + 1;
    const rows = topProviders(withProviders(n, (i) => (n - i) * 10));
    const other = rows[rows.length - 1];
    expect(other.isOther).toBe(true);
    expect(other.label).toBe(`Other · Provider ${n - 1}`);
    expect(other.provider).toBeNull();
  });

  it("honours an explicit limit", () => {
    const rows = topProviders(withProviders(10, (i) => (10 - i) * 10), 3);
    expect(rows).toHaveLength(4);
    expect(rows.slice(0, 3).map((r) => r.provider)).toEqual(["p0", "p1", "p2"]);
  });

  it("returns nothing when the scan found nothing", () => {
    expect(topProviders(withProviders(0, () => 0))).toEqual([]);
  });
});

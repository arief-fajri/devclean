/**
 * Pure merge of one provider's fresh scan into an existing `ScanSummary`.
 *
 * A targeted rescan (`scan_provider`) returns only one provider's resources.
 * Replacing the whole summary with it would drop every other provider, which
 * is exactly what happened when the `scan_completed` event handler overwrote
 * the store before this merge ran. Keeping the merge pure makes it testable
 * and gives the store one place to get it right.
 *
 * No Svelte, no Tauri — unit-tested with vitest, like `$lib/view`.
 */
import type { Resource, ScanSummary } from "./tauri";

/** Recompute totals and `blocked_bytes` from a resource set. */
function totals(resources: Resource[]): Pick<
  ScanSummary,
  "total_bytes" | "reclaimable_bytes" | "blocked_bytes"
> {
  let total_bytes = 0;
  let reclaimable_bytes = 0;
  let blocked_bytes = 0;
  for (const r of resources) {
    total_bytes += r.size_bytes;
    reclaimable_bytes += r.reclaimable_bytes;
    if (r.blocked_reason != null) blocked_bytes += r.reclaimable_bytes;
  }
  return { total_bytes, reclaimable_bytes, blocked_bytes };
}

/**
 * Replace `provider`'s rows and per-provider summary with `partial`'s, keeping
 * every other provider untouched. `partial` must contain only that provider's
 * resources (which is what `scan_provider` returns).
 */
export function mergeProviderSummary(
  summary: ScanSummary,
  provider: string,
  partial: ScanSummary,
): ScanSummary {
  const resources = [
    ...summary.resources.filter((r) => r.provider !== provider),
    ...partial.resources,
  ];
  const providers = [
    ...summary.providers.filter((p) => p.provider !== provider),
    ...partial.providers,
  ].sort((a, b) => b.reclaimable_bytes - a.reclaimable_bytes);

  return { ...totals(resources), providers, resources };
}

/**
 * Remove resources by id and rebuild the per-provider summaries, dropping
 * providers left with no rows.
 *
 * This is the deterministic half of the post-cleanup update: the backend
 * already told us which ids succeeded, so the list can drop them the moment
 * the cleanup finishes, without waiting for (or depending on) a rescan. A
 * targeted rescan then reconciles anything that reappeared.
 */
export function pruneResources(summary: ScanSummary, removedIds: Iterable<string>): ScanSummary {
  const removed = new Set(removedIds);
  if (removed.size === 0) return summary;
  const resources = summary.resources.filter((r) => !removed.has(r.id));
  if (resources.length === summary.resources.length) return summary;

  const names = new Map(summary.providers.map((p) => [p.provider, p.name]));
  const providers = summarize(resources, names);
  return { ...totals(resources), providers, resources };
}

/** Per-provider totals for a resource set, biggest reclaimable first. */
function summarize(resources: Resource[], names: Map<string, string>): ScanSummary["providers"] {
  const byProvider = new Map<string, Resource[]>();
  for (const r of resources) {
    const list = byProvider.get(r.provider);
    if (list) list.push(r);
    else byProvider.set(r.provider, [r]);
  }
  return [...byProvider.entries()]
    .map(([provider, rs]) => ({
      provider,
      name: names.get(provider) ?? provider,
      total_bytes: rs.reduce((a, r) => a + r.size_bytes, 0),
      reclaimable_bytes: rs.reduce((a, r) => a + r.reclaimable_bytes, 0),
      resource_count: rs.length,
    }))
    .sort((a, b) => b.reclaimable_bytes - a.reclaimable_bytes);
}

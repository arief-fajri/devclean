/**
 * Pure list-view logic: search / filter / sort over a `ScanSummary`.
 * No Svelte, no Tauri — unit-tested with vitest.
 *
 * Selection is intentionally filter-independent: filtering only changes
 * *visibility*. Provider checkboxes then act on the visible rows only,
 * and the tooltip always names the affected count.
 */
import { isStaleSignal } from "./format";
import type { ProviderSummary, Resource, SafetyLevel, ScanSummary } from "./tauri";

export type SafetyFilter = "all" | "safe" | "review";
export type SortKey = "largest" | "name" | "safety";

export interface ViewOptions {
  query: string;
  safety: SafetyFilter;
  staleOnly: boolean;
  selectedOnly: boolean;
  sort: SortKey;
  /** Display preference (not a filter): show the path line per row. */
  showPaths: boolean;
}

export const DEFAULT_VIEW: ViewOptions = {
  query: "",
  safety: "all",
  staleOnly: false,
  selectedOnly: false,
  sort: "largest",
  showPaths: true,
};

export interface ProviderGroup {
  summary: ProviderSummary;
  resources: Resource[];
}

export function needsReview(safety: SafetyLevel): boolean {
  return safety === "caution" || safety === "dangerous";
}

function safetyRank(safety: SafetyLevel): number {
  if (safety === "dangerous") return 2;
  if (safety === "caution") return 1;
  return 0;
}

/**
 * Last two path segments, e.g. `"my-app/node_modules"`. Project caches
 * live one level below the project dir, so the bare basename
 * (`"node_modules"`) alone can never match a project search.
 */
function searchablePath(path: string): string {
  return path
    .split(/[/\\]/)
    .filter(Boolean)
    .slice(-2)
    .join("/")
    .toLowerCase();
}

export function matchesQuery(resource: Resource, providerName: string, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (!needle) return true;
  return (
    resource.name.toLowerCase().includes(needle) ||
    providerName.toLowerCase().includes(needle) ||
    searchablePath(resource.path).includes(needle)
  );
}

export function applyView(
  summary: ScanSummary | null,
  selectedIds: string[],
  opts: ViewOptions,
): ProviderGroup[] {
  if (!summary) return [];
  const selected = new Set(selectedIds);
  const providerName = new Map(summary.providers.map((p) => [p.provider, p.name]));
  const groups: ProviderGroup[] = [];
  for (const p of summary.providers) {
    let list = summary.resources.filter((r) => r.provider === p.provider);
    list = list.filter((r) => {
      if (opts.safety === "safe" && r.safety !== "safe") return false;
      if (opts.safety === "review" && !needsReview(r.safety)) return false;
      if (opts.staleOnly && !isStaleSignal(r.detail)) return false;
      if (opts.selectedOnly && !selected.has(r.id)) return false;
      if (!matchesQuery(r, providerName.get(r.provider) ?? "", opts.query)) return false;
      return true;
    });
    if (list.length === 0) continue;
    if (opts.sort === "largest") {
      list = [...list].sort((a, b) => b.reclaimable_bytes - a.reclaimable_bytes);
    } else if (opts.sort === "name") {
      list = [...list].sort((a, b) => a.name.localeCompare(b.name));
    } else {
      list = [...list].sort(
        (a, b) =>
          safetyRank(b.safety) - safetyRank(a.safety) ||
          b.reclaimable_bytes - a.reclaimable_bytes,
      );
    }
    groups.push({ summary: p, resources: list });
  }
  if (opts.sort === "name") {
    groups.sort((a, b) => a.summary.name.localeCompare(b.summary.name));
  } else if (opts.sort === "safety") {
    // Providers holding review items surface first — otherwise a huge
    // all-safe provider would keep burying the risky rows below it.
    const rank = (g: ProviderGroup) =>
      Math.max(...g.resources.map((r) => safetyRank(r.safety)));
    groups.sort(
      (a, b) => rank(b) - rank(a) || b.summary.reclaimable_bytes - a.summary.reclaimable_bytes,
    );
  } else {
    groups.sort((a, b) => b.summary.reclaimable_bytes - a.summary.reclaimable_bytes);
  }
  return groups;
}

export function countBySafety(resources: Resource[]): { safe: number; review: number } {
  let safe = 0;
  let review = 0;
  for (const r of resources) {
    if (r.safety === "safe") safe += 1;
    else review += 1;
  }
  return { safe, review };
}

/**
 * Scan-root candidates that are still worth offering.
 *
 * The backend flags `already_configured` against the *saved* roots, while
 * the dialog edits a draft — so both must be checked. Rendering an
 * already-added path in the suggestion list is what made every active
 * root appear twice in Settings.
 */
export function addableCandidates<T extends { path: string; already_configured: boolean }>(
  candidates: T[],
  draftRoots: string[],
): T[] {
  const roots = new Set(draftRoots);
  return candidates.filter((c) => !c.already_configured && !roots.has(c.path));
}

export function alreadyAddedCount<T extends { path: string; already_configured: boolean }>(
  candidates: T[],
  draftRoots: string[],
): number {
  return candidates.length - addableCandidates(candidates, draftRoots).length;
}

/**
 * Drop expanded provider ids that no longer exist (e.g. their last item was
 * cleaned, so the card disappears).
 *
 * Without this the ids accumulate forever and a provider that later shows up
 * again pops open by itself — the "another card opened itself" report.
 */
export function pruneExpanded(expanded: string[], present: string[]): string[] {
  const alive = new Set(present);
  return expanded.filter((p) => alive.has(p));
}

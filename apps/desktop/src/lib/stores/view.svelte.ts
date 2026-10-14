/**
 * List-view store (Svelte 5 runes): global search / filter / sort.
 * Pure matching lives in `$lib/view` so it stays unit-testable.
 */
import { DEFAULT_VIEW, pruneExpanded, type SafetyFilter, type SortKey, type ViewOptions } from "$lib/view";

class ViewStore {
  query = $state(DEFAULT_VIEW.query);
  safety = $state<SafetyFilter>(DEFAULT_VIEW.safety);
  staleOnly = $state(DEFAULT_VIEW.staleOnly);
  selectedOnly = $state(DEFAULT_VIEW.selectedOnly);
  sort = $state<SortKey>(DEFAULT_VIEW.sort);
  showPaths = $state(DEFAULT_VIEW.showPaths);

  get options(): ViewOptions {
    return {
      query: this.query,
      safety: this.safety,
      staleOnly: this.staleOnly,
      selectedOnly: this.selectedOnly,
      sort: this.sort,
      showPaths: this.showPaths,
    };
  }

  /** Any narrowing active (used for the "clear filters" affordance). */
  get active(): boolean {
    return (
      this.query.trim() !== "" ||
      this.safety !== "all" ||
      this.staleOnly ||
      this.selectedOnly
    );
  }

  /** Expanded provider ids. Session-only: collapsed shows the summary,
   * expanded reveals the item rows. */
  expanded = $state<string[]>([]);

  toggleExpand(provider: string) {
    this.expanded = this.expanded.includes(provider)
      ? this.expanded.filter((p) => p !== provider)
      : [...this.expanded, provider];
  }

  /**
   * Forget expanded ids whose provider no longer exists (e.g. its last
   * item was cleaned). Pure logic lives in `$lib/view` for testing.
   */
  pruneExpanded(present: string[]) {
    const next = pruneExpanded(this.expanded, present);
    if (next.length !== this.expanded.length) this.expanded = next;
  }

  reset() {
    this.query = DEFAULT_VIEW.query;
    this.safety = DEFAULT_VIEW.safety;
    this.staleOnly = DEFAULT_VIEW.staleOnly;
    this.selectedOnly = DEFAULT_VIEW.selectedOnly;
    this.sort = DEFAULT_VIEW.sort;
    this.showPaths = DEFAULT_VIEW.showPaths;
  }
}

export const viewStore = new ViewStore();

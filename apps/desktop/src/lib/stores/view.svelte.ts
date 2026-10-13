/**
 * List-view store (Svelte 5 runes): global search / filter / sort.
 * Pure matching lives in `$lib/view` so it stays unit-testable.
 */
import { DEFAULT_VIEW, type SafetyFilter, type SortKey, type ViewOptions } from "$lib/view";

class ViewStore {
  query = $state(DEFAULT_VIEW.query);
  safety = $state<SafetyFilter>(DEFAULT_VIEW.safety);
  staleOnly = $state(DEFAULT_VIEW.staleOnly);
  selectedOnly = $state(DEFAULT_VIEW.selectedOnly);
  sort = $state<SortKey>(DEFAULT_VIEW.sort);

  get options(): ViewOptions {
    return {
      query: this.query,
      safety: this.safety,
      staleOnly: this.staleOnly,
      selectedOnly: this.selectedOnly,
      sort: this.sort,
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

  reset() {
    this.query = DEFAULT_VIEW.query;
    this.safety = DEFAULT_VIEW.safety;
    this.staleOnly = DEFAULT_VIEW.staleOnly;
    this.selectedOnly = DEFAULT_VIEW.selectedOnly;
    this.sort = DEFAULT_VIEW.sort;
  }
}

export const viewStore = new ViewStore();

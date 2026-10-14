/**
 * Cleanup-history store: reads the backend run log via `get_history_runs`.
 *
 * Runs, not raw entries — the log is append-only and never truncated, so
 * an entry-level window would let one large cleanup hide every run before
 * it (see `get_history_runs` in the contract).
 */
import { getHistoryRuns } from "$lib/tauri";
import type { CleanupRun } from "$lib/tauri";

class HistoryStore {
  runs = $state<CleanupRun[]>([]);
  /** True when the backend may have more runs than we loaded. */
  truncated = $state(false);
  loading = $state(false);
  error = $state<string | null>(null);

  /** One read per visit: the page asks for data on entry, and the Overview
   *  only needs the newest run. */
  async ensureLoaded() {
    if (this.loading || this.runs.length > 0) return;
    await this.refresh();
  }

  async refresh() {
    this.loading = true;
    this.error = null;
    try {
      const limit = 50;
      this.runs = await getHistoryRuns(limit);
      // The backend caps at 200; we ask for fewer so the page stays
      // scannable, and say so honestly rather than implying "everything".
      this.truncated = this.runs.length >= limit;
    } catch (e) {
      this.error = String(e);
    } finally {
      this.loading = false;
    }
  }
}

export const historyStore = new HistoryStore();

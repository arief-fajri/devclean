/**
 * Cleanup history store: reads the backend JSONL log via `get_history`.
 * The log is append-only and written server-side after every cleanup.
 */
import { getHistory } from "$lib/tauri";
import type { HistoryEntry } from "$lib/tauri";

class HistoryStore {
  entries = $state<HistoryEntry[]>([]);
  open = $state(false);
  loading = $state(false);
  error = $state<string | null>(null);

  async openDialog() {
    this.open = true;
    await this.refresh();
  }

  close() {
    this.open = false;
  }

  async refresh() {
    this.loading = true;
    this.error = null;
    try {
      this.entries = await getHistory(100);
    } catch (e) {
      this.error = String(e);
    } finally {
      this.loading = false;
    }
  }
}

export const historyStore = new HistoryStore();

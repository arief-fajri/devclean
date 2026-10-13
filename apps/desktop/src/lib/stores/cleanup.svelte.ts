/**
 * Cleanup store: preview → confirm → execute → result.
 * The confirmation dialog (G-UI-01) is driven by `confirmOpen` + `preview`.
 */
import { previewCleanup, runCleanup } from "$lib/tauri";
import type { CleanupPreview, CleanupResult } from "$lib/tauri";

class CleanupStore {
  cleaning = $state(false);
  confirmOpen = $state(false);
  preview = $state<CleanupPreview | null>(null);
  results = $state<CleanupResult[] | null>(null);
  error = $state<string | null>(null);

  async openConfirm(ids: string[]) {
    this.error = null;
    this.results = null;
    try {
      this.preview = await previewCleanup(ids);
      this.confirmOpen = true;
    } catch (e) {
      this.error = String(e);
    }
  }

  async execute(ids: string[]) {
    this.cleaning = true;
    this.error = null;
    try {
      this.results = await runCleanup(ids);
    } catch (e) {
      this.error = String(e);
    } finally {
      this.cleaning = false;
      this.confirmOpen = false;
    }
  }

  freedBytes(): number {
    return (this.results ?? []).reduce((acc, r) => acc + r.freed_bytes, 0);
  }

  close() {
    this.confirmOpen = false;
    this.preview = null;
  }

  dismissResults() {
    this.results = null;
  }
}

export const cleanupStore = new CleanupStore();

/**
 * Cleanup store: preview → confirm → execute → result.
 * The confirmation dialog (G-UI-01) is driven by `confirmOpen` + `preview`.
 */
import {
  cancelCleanup,
  onCleanupCompleted,
  onCleanupProgress,
  onCleanupStarted,
  previewCleanup,
  runCleanup,
} from "$lib/tauri";
import type { CleanupPreview, CleanupProgress, CleanupResult } from "$lib/tauri";

class CleanupStore {
  cleaning = $state(false);
  cancelling = $state(false);
  confirmOpen = $state(false);
  preview = $state<CleanupPreview | null>(null);
  previewAt = $state<Date | null>(null);
  /** Proof the backend issued for this id set (v0.5.0). `cleanup` refuses
   * to run without it while confirmation is on. */
  previewToken = $state<string | null>(null);
  /** Resource names snapshotted at preview time, so the result table can
   * show names even after the backend prunes cleaned ids from state. */
  names = $state<Record<string, string>>({});
  progress = $state<CleanupProgress | null>(null);
  results = $state<CleanupResult[] | null>(null);
  /** Providers touched by the last cleanup, so the post-cleanup re-scan can
   * be targeted instead of sweeping every provider. */
  touchedProviders = $state<string[]>([]);
  error = $state<string | null>(null);

  async openConfirm(ids: string[]) {
    this.error = null;
    this.results = null;
    try {
      this.preview = await previewCleanup(ids);
      this.previewAt = new Date();
      this.previewToken = this.preview.preview_token;
      this.names = Object.fromEntries(this.preview.items.map((i) => [i.resource_id, i.name]));
      this.confirmOpen = true;
    } catch (e) {
      this.error = String(e);
    }
  }

  async execute(ids: string[]) {
    this.cleaning = true;
    this.cancelling = false;
    this.progress = null;
    this.error = null;
    try {
      // The token travels with the call: the backend decides whether the
      // confirmation actually happened, not this frontend.
      this.results = await runCleanup(ids, this.previewToken ?? undefined);
    } catch (e) {
      this.error = String(e);
    } finally {
      this.cleaning = false;
      this.cancelling = false;
      // NOTE: confirmOpen stays true — the flow dialog moves to its `done`
      // phase instead of vanishing. Close via closeFlow() (Done/Rescan).
      this.progress = null;
    }
  }

  /** Cancel the remaining cleanup queue (I5). The in-flight item
   * finishes; the rest report `cancelled` (I8). */
  async cancel() {
    if (!this.cleaning) return;
    this.cancelling = true;
    try {
      await cancelCleanup();
    } catch (e) {
      this.error = String(e);
      this.cancelling = false;
    }
  }

  listen() {
    onCleanupStarted(() => {
      this.cleaning = true;
      this.progress = null;
    });
    onCleanupProgress((p) => {
      this.progress = p;
    });
    onCleanupCompleted((r) => {
      this.results = r;
      this.cleaning = false;
      this.cancelling = false;
      // confirmOpen stays true so the flow dialog shows the `done` phase.
      this.progress = null;
    });
  }

  freedBytes(): number {
    return (this.results ?? []).reduce((acc, r) => acc + r.freed_bytes, 0);
  }

  requestedBytes(): number {
    return (this.results ?? []).reduce((acc, r) => acc + r.requested_bytes, 0);
  }

  nameOf(id: string): string {
    return this.names[id] ?? id;
  }

  close() {
    this.confirmOpen = false;
    this.preview = null;
    this.previewToken = null;
  }

  /** Close the whole flow dialog (Done/Rescan): dialog + preview + results. */
  closeFlow() {
    this.confirmOpen = false;
    this.preview = null;
    this.previewToken = null;
    this.results = null;
  }

  dismissResults() {
    this.results = null;
  }
}

export const cleanupStore = new CleanupStore();

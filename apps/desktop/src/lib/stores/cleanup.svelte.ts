/**
 * Cleanup store: measure → confirm → execute → result.
 *
 * The dialog (G-UI-01) is open from the moment `openConfirm` is called, not
 * from the moment the preview returns — `previewing` covers the re-measure,
 * `previewError` keeps a failure inside the dialog where the user can read it
 * and retry. Which phase is showing is decided by `$lib/cleanup-view`.
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
import { settingsStore } from "$lib/stores/settings.svelte";

class CleanupStore {
  cleaning = $state(false);
  cancelling = $state(false);
  confirmOpen = $state(false);
  /** `preview_cleanup` is re-measuring every selected path. The dialog is
   *  open during this: without it, clicking "clean" looks dead. */
  previewing = $state(false);
  /** The re-measure failed. Shown INSIDE the dialog, never swallowed. */
  previewError = $state<string | null>(null);
  /** Guards against a cancelled preview landing after a newer one started. */
  previewRun = 0;
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

  /**
   * Open the confirmation and start re-measuring.
   *
   * The dialog opens *before* the await: `preview_cleanup` walks the selected
   * paths, which can take a while, and showing nothing during that time is
   * what made this flow feel broken. The dialog then shows the re-measure
   * phase, and settles into review or into an error it can display.
   */
  async openConfirm(ids: string[]) {
    const run = ++this.previewRun;
    this.error = null;
    this.results = null;
    this.preview = null;
    this.previewAt = null;
    this.previewToken = null;
    this.previewError = null;
    this.previewing = true;
    this.confirmOpen = true;
    try {
      const preview = await previewCleanup(ids);
      // A cancelled or superseded preview must not resurrect stale state.
      if (run !== this.previewRun) return;
      this.preview = preview;
      this.previewAt = new Date();
      this.previewToken = preview.preview_token;
      this.names = Object.fromEntries(preview.items.map((i) => [i.resource_id, i.name]));
    } catch (e) {
      if (run !== this.previewRun) return;
      this.previewError = String(e);
    } finally {
      if (run === this.previewRun) this.previewing = false;
    }
  }

  /** Re-measure the same selection after a failure, without closing first. */
  async retryPreview(ids: string[]) {
    this.confirmOpen = true;
    await this.openConfirm(ids);
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
      // Cleanup freed space on the disk; re-measure so the dashboard card
      // does not keep quoting the number from before the deletion.
      void settingsStore.refreshDiskFree();
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
    // Bump the run so an in-flight preview landing afterwards is dropped.
    this.previewRun += 1;
    this.confirmOpen = false;
    this.previewing = false;
    this.previewError = null;
    this.preview = null;
    this.previewToken = null;
  }

  /** Close the whole flow dialog (Done/Rescan): dialog + preview + results. */
  closeFlow() {
    this.previewRun += 1;
    this.confirmOpen = false;
    this.previewing = false;
    this.previewError = null;
    this.preview = null;
    this.previewToken = null;
    this.results = null;
  }

  dismissResults() {
    this.results = null;
  }
}

export const cleanupStore = new CleanupStore();

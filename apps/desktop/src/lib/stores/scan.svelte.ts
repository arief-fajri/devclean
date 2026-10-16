/**
 * Scan store (Svelte 5 runes). Holds the last ScanSummary, progress, and the
 * user's selection of resource ids. Selection stores ids only — never paths.
 */
import {
  cancelScan,
  getCachedScan,
  getResourceDetails,
  onScanCompleted,
  onScanProgress,
  onScanProviderStarted,
  onScanStarted,
  scanAll,
  scanProvider,
} from "$lib/tauri";
import type { CleanupResult, Resource, ScanProgress, ScanSummary } from "$lib/tauri";
import { mergeProviderSummary, pruneResources } from "$lib/scan-merge";

class ScanStore {
  scanning = $state(false);
  cancelling = $state(false);
  progress = $state<ScanProgress | null>(null);
  summary = $state<ScanSummary | null>(null);
  selected = $state<string[]>([]);
  detail = $state<Resource | null>(null);
  rescanning = $state<string | null>(null);
  /** Providers refreshed by the last targeted re-scan; `null` after a full
   * scan. Drives the "partial re-scan" badge so stale figures elsewhere are
   * never presented as fresh. */
  partialScan = $state<string[] | null>(null);
  error = $state<string | null>(null);
  lastScannedAt = $state<Date | null>(null);
  /** True while the shown summary comes from the on-disk cache. */
  fromCache = $state(false);
  /** Provider currently scanning (v0.5.0). Null when nothing is running. */
  activeProvider = $state<string | null>(null);
  /** Providers that have already finished this run — drives the per-card
   * check/spinner/dot state while a scan is in flight. */
  doneProviders = $state<string[]>([]);
  /**
   * Depth of targeted rescans in flight. While it is non-zero the scan events
   * are ignored for global state: a per-provider rescan must not flip the
   * whole list into `scanning` (which disables selection and dims the cards),
   * and its `scan_completed` must not overwrite the merged summary with a
   * single provider's — the caller's `mergeProviderSummary` is authoritative.
   */
  private targetedScans = 0;

  /** Instant paint from the last scan (if roots match). The caller runs a
   * scan only when the user asked for one (`scan_automatically` or a click),
   * so this is the resting state, not a paint before a guaranteed rescan.
   * Never throws. */
  async loadCached() {
    try {
      const cached = await getCachedScan();
      if (cached) {
        this.applySummary(cached.summary);
        // The scan's real age, not the moment we launched — otherwise the
        // dashboard would claim a week-old cache was measured just now.
        this.lastScannedAt = new Date(cached.saved_at_unix * 1000);
        this.fromCache = true;
      }
    } catch {
      // No cache — the first-run state stands until the user scans.
    }
  }

  resourcesByProvider(provider: string): Resource[] {
    return (this.summary?.resources ?? []).filter((r) => r.provider === provider);
  }

  selectedBytes(): number {
    const map = new Map((this.summary?.resources ?? []).map((r) => [r.id, r]));
    return this.selected.reduce((acc, id) => acc + (map.get(id)?.reclaimable_bytes ?? 0), 0);
  }

  toggle(id: string) {
    this.selected = this.selected.includes(id)
      ? this.selected.filter((s) => s !== id)
      : [...this.selected, id];
  }

  clearSelection() {
    this.selected = [];
  }

  /** Add ids to the selection (used by "Select safe items"). */
  selectMany(ids: string[]) {
    this.selected = [...new Set([...this.selected, ...ids])];
  }

  toggleProvider(provider: string) {
    const ids = this.resourcesByProvider(provider).map((r) => r.id);
    const all = ids.every((id) => this.selected.includes(id));
    this.selected = all
      ? this.selected.filter((s) => !ids.includes(s))
      : [...new Set([...this.selected, ...ids])];
  }

  /** Drill-down for one resource id (I1: ids only, resolved server-side). */
  async openDetail(id: string) {
    try {
      this.detail = await getResourceDetails(id);
    } catch (e) {
      this.error = String(e);
    }
  }

  closeDetail() {
    this.detail = null;
  }

  /** Re-scan one provider and merge into the current summary, so the
   * other providers' rows are preserved (contract: full-scan-filter). */
  async rescanProvider(provider: string) {
    if (this.scanning || this.rescanning) return;
    this.rescanning = provider;
    this.targetedScans += 1;
    this.error = null;
    try {
      await this.rescanOne(provider);
    } catch (e) {
      this.error = String(e);
    } finally {
      this.rescanning = null;
      this.releaseTargeted();
    }
  }

  /**
   * Re-scan only `providers`, merging each result into the current summary so
   * the other providers' rows are preserved. The scan events are suppressed
   * for the duration (see `targetedScans`), so the list stays interactive and
   * the `scan_completed` event cannot overwrite the merge.
   */
  async rescanProviders(providers: string[]) {
    const list = [...new Set(providers)];
    if (list.length === 0) {
      await this.start();
      return;
    }
    this.targetedScans += 1;
    this.error = null;
    try {
      for (const p of list) {
        // A full scan was started while we were walking: stand down.
        if (this.scanning) break;
        this.rescanning = p;
        await this.rescanOne(p);
      }
      // Only these providers were refreshed; the rest keep their last known
      // figures, which the UI must say out loud (see SummaryPanel badge).
      this.partialScan = list;
      this.lastScannedAt = new Date();
      this.fromCache = false;
    } catch (e) {
      this.error = String(e);
    } finally {
      this.rescanning = null;
      this.releaseTargeted();
    }
  }

  /** One provider's scan, merged into the summary. No event handling. */
  private async rescanOne(provider: string) {
    try {
      const f = await scanProvider(provider);
      if (!this.summary) {
        this.applySummary(f);
        return;
      }
      this.summary = mergeProviderSummary(this.summary, provider, f);
      this.lastScannedAt = new Date();
      const alive = new Set(this.summary.resources.map((r) => r.id));
      this.selected = this.selected.filter((id) => alive.has(id));
    } finally {
      // Never leave the per-card loader spinning if the progress event never
      // arrived (e.g. the scan errored).
      if (this.activeProvider === provider) this.activeProvider = null;
    }
  }

  /**
   * Drop the targeted-scan guard only after the queued `scan_completed` for
   * the last provider has settled — otherwise that event would overwrite the
   * merge with the single provider's summary.
   */
  private releaseTargeted() {
    setTimeout(() => {
      this.targetedScans = Math.max(0, this.targetedScans - 1);
    }, 0);
  }

  /**
   * Drop the resources a finished cleanup reported as `success` from the shown
   * summary, immediately.
   *
   * Deterministic and independent of the rescan: the backend already told us
   * which ids were removed, so the list must not keep showing them while a
   * rescan runs — or if none lands. The targeted rescan that follows then
   * reconciles anything still on disk.
   */
  pruneCleaned(results: CleanupResult[]) {
    if (!this.summary) return;
    const removed = results
      .filter((r) => r.status === "success")
      .map((r) => r.resource_id);
    if (removed.length === 0) return;
    this.summary = pruneResources(this.summary, removed);
    const alive = new Set(this.summary.resources.map((r) => r.id));
    this.selected = this.selected.filter((id) => alive.has(id));
  }

async start() {
    this.scanning = true;
    this.cancelling = false;
    this.error = null;
    this.progress = null;
    this.fromCache = false;
    this.partialScan = null;
    this.activeProvider = null;
    this.doneProviders = [];
    try {
      const s = await scanAll();
      this.applySummary(s);
    } catch (e) {
      this.error = String(e);
    } finally {
      this.scanning = false;
      this.cancelling = false;
      this.activeProvider = null;
    }
  }

  /** Cancel the in-flight scan (I5). The backend flag settles fast;
   * the UI also drops `scanning` immediately so it never hangs. */
  async cancel() {
    if (!this.scanning) return;
    this.cancelling = true;
    try {
      await cancelScan();
    } catch (e) {
      this.error = String(e);
      this.scanning = false;
      this.cancelling = false;
    }
    this.activeProvider = null;
  }

  applySummary(s: ScanSummary) {
    this.summary = s;
    this.lastScannedAt = new Date();
    this.scanning = false;
    this.cancelling = false;
    this.activeProvider = null;
    this.doneProviders = [];
    // Drop selections that no longer exist (e.g. after cleanup).
    const alive = new Set(s.resources.map((r) => r.id));
    this.selected = this.selected.filter((id) => alive.has(id));
  }

  listen() {
    onScanStarted(() => {
      // A targeted rescan must not put the whole list into `scanning` — that
      // disables selection and dims every card, which is what made a cleanup
      // feel like the app had frozen.
      if (this.targetedScans > 0) return;
      this.scanning = true;
      this.cancelling = false;
      this.progress = null;
      this.activeProvider = null;
      this.doneProviders = [];
    });
    onScanProviderStarted((provider) => {
      // Always reflected: this drives the per-card loader, and a targeted
      // rescan must show one on the provider it is walking.
      this.activeProvider = provider;
    });
    onScanProgress((p) => {
      // A targeted rescan drives only the per-card loader (set by
      // `onScanProviderStarted`); it must not touch the global progress bar or
      // leave a lingering "done" checkmark, so those are skipped while it runs.
      if (this.targetedScans === 0) {
        this.progress = p;
        if (!this.doneProviders.includes(p.provider)) {
          this.doneProviders = [...this.doneProviders, p.provider];
        }
      }
      // The finished provider is no longer the running one.
      if (this.activeProvider === p.provider) this.activeProvider = null;
    });
    // `scan_completed` is only for scans the store did not initiate (the menu
    // bar's background scan). A targeted rescan merges its own result, and a
    // store-initiated full scan applies the awaited summary; letting the event
    // through here would clobber a targeted merge with one provider's rows.
    onScanCompleted((s) => {
      if (this.targetedScans > 0) return;
      this.applySummary(s);
    });
  }
}

export const scanStore = new ScanStore();

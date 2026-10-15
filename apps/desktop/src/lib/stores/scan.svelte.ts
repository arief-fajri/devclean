/**
 * Scan store (Svelte 5 runes). Holds the last ScanSummary, progress, and the
 * user's selection of resource ids. Selection stores ids only — never paths.
 */
import {
  cancelScan,
  getCachedScan,
  getResourceDetails,
  onCleanupCompleted,
  onScanCompleted,
  onScanProgress,
  onScanProviderStarted,
  onScanStarted,
  scanAll,
  scanProvider,
} from "$lib/tauri";
import type { Resource, ScanProgress, ScanSummary } from "$lib/tauri";

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
    this.error = null;
    try {
      const f = await scanProvider(provider);
      if (!this.summary) {
        this.applySummary(f);
        return;
      }
      const resources = [
        ...this.summary.resources.filter((r) => r.provider !== provider),
        ...f.resources,
      ];
      const providers = [...this.summary.providers.filter((p) => p.provider !== provider), ...f.providers].sort(
        (a, b) => b.reclaimable_bytes - a.reclaimable_bytes,
      );
      this.summary = {
        total_bytes: resources.reduce((acc, r) => acc + r.size_bytes, 0),
        reclaimable_bytes: resources.reduce((acc, r) => acc + r.reclaimable_bytes, 0),
        blocked_bytes: resources
          .filter((r) => r.blocked_reason != null)
          .reduce((acc, r) => acc + r.reclaimable_bytes, 0),
        providers,
        resources,
      };
      this.lastScannedAt = new Date();
      const alive = new Set(resources.map((r) => r.id));
      this.selected = this.selected.filter((id) => alive.has(id));
    } catch (e) {
      this.error = String(e);
    } finally {
      this.rescanning = null;
    }
  }

  async rescanProviders(providers: string[]) {
  const list = [...new Set(providers)];
  if (list.length === 0) {
    await this.start();
    return;
  }
  for (const p of list) {
    await this.rescanProvider(p);
  }
  // Only these providers were refreshed; the rest keep their last known
  // figures, which the UI must say out loud (see SummaryPanel badge).
  this.partialScan = list;
  this.lastScannedAt = new Date();
  this.fromCache = false;
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
      this.scanning = true;
      this.cancelling = false;
      this.progress = null;
      this.activeProvider = null;
      this.doneProviders = [];
    });
    onScanProviderStarted((provider) => {
      this.activeProvider = provider;
    });
    onScanProgress((p) => {
      this.progress = p;
      if (!this.doneProviders.includes(p.provider)) {
        this.doneProviders = [...this.doneProviders, p.provider];
      }
      // The finished provider is no longer the running one.
      if (this.activeProvider === p.provider) this.activeProvider = null;
    });
    onScanCompleted((s) => this.applySummary(s));
    onCleanupCompleted(() => {
      // State already pruned backend-side; UI re-renders from summary.
      // A background rescan keeps numbers honest.
      void this.start();
    });
  }
}

export const scanStore = new ScanStore();

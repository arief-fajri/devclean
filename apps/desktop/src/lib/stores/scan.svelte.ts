/**
 * Scan store (Svelte 5 runes). Holds the last ScanSummary, progress, and the
 * user's selection of resource ids. Selection stores ids only — never paths.
 */
import {
  cancelScan,
  getResourceDetails,
  onCleanupCompleted,
  onScanCompleted,
  onScanProgress,
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
  expanded = $state<string[]>([]);
  detail = $state<Resource | null>(null);
  rescanning = $state<string | null>(null);
  error = $state<string | null>(null);
  lastScannedAt = $state<Date | null>(null);

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

  toggleProvider(provider: string) {
    const ids = this.resourcesByProvider(provider).map((r) => r.id);
    const all = ids.every((id) => this.selected.includes(id));
    this.selected = all
      ? this.selected.filter((s) => !ids.includes(s))
      : [...new Set([...this.selected, ...ids])];
  }

  toggleExpand(provider: string) {
    this.expanded = this.expanded.includes(provider)
      ? this.expanded.filter((p) => p !== provider)
      : [...this.expanded, provider];
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

  async start() {
    this.scanning = true;
    this.cancelling = false;
    this.error = null;
    this.progress = null;
    try {
      const s = await scanAll();
      this.applySummary(s);
    } catch (e) {
      this.error = String(e);
    } finally {
      this.scanning = false;
      this.cancelling = false;
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
  }

  applySummary(s: ScanSummary) {
    this.summary = s;
    this.lastScannedAt = new Date();
    this.scanning = false;
    this.cancelling = false;
    // Drop selections that no longer exist (e.g. after cleanup).
    const alive = new Set(s.resources.map((r) => r.id));
    this.selected = this.selected.filter((id) => alive.has(id));
  }

  listen() {
    onScanStarted(() => {
      this.scanning = true;
      this.cancelling = false;
      this.progress = null;
    });
    onScanProgress((p) => {
      this.progress = p;
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

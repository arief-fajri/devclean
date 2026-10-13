/**
 * Scan store (Svelte 5 runes). Holds the last ScanSummary, progress, and the
 * user's selection of resource ids. Selection stores ids only — never paths.
 */
import { onCleanupCompleted, onScanCompleted, onScanProgress, scanAll } from "$lib/tauri";
import type { Resource, ScanProgress, ScanSummary } from "$lib/tauri";

class ScanStore {
  scanning = $state(false);
  progress = $state<ScanProgress | null>(null);
  summary = $state<ScanSummary | null>(null);
  selected = $state<string[]>([]);
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

  async start() {
    this.scanning = true;
    this.error = null;
    this.progress = null;
    try {
      const s = await scanAll();
      this.applySummary(s);
    } catch (e) {
      this.error = String(e);
    } finally {
      this.scanning = false;
    }
  }

  applySummary(s: ScanSummary) {
    this.summary = s;
    this.lastScannedAt = new Date();
    // Drop selections that no longer exist (e.g. after cleanup).
    const alive = new Set(s.resources.map((r) => r.id));
    this.selected = this.selected.filter((id) => alive.has(id));
  }

  listen() {
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

/**
 * Settings store: scan roots + confirmation toggles.
 * Backend persistence is in-memory (file persistence = M3, per contract);
 * the safety validator still re-checks every path (I2, I7).
 */
import {
  detectScanRoots,
  getDiskFree,
  getProviders,
  getSettings,
  updateSettings,
} from "$lib/tauri";
import type { DiskFree, ProviderInfo, ScanRootCandidate, Settings, ThemeMode } from "$lib/tauri";

class SettingsStore {
  settings = $state<Settings | null>(null);
  /** Backend registry + enabled flags (contract v0.4.0 `get_providers`). */
  providers = $state<ProviderInfo[]>([]);
  open = $state(false);
  saving = $state(false);
  error = $state<string | null>(null);
  /** `detect_scan_roots` result (contract v0.5.0). */
  candidates = $state<ScanRootCandidate[]>([]);
  detecting = $state(false);
  /** Free space on the data volume — tells the user up front whether an
   * AI-database backup would even fit. */
  diskFree = $state<DiskFree | null>(null);

  async load() {
    try {
      this.settings = await getSettings();
      this.providers = await getProviders();
      this.diskFree = await getDiskFree().catch(() => null);
    } catch (e) {
      this.error = String(e);
    }
  }

  /** Discover candidate scan roots under $HOME. Read-only and bounded. */
  async detect() {
    if (this.detecting) return;
    this.detecting = true;
    try {
      this.candidates = await detectScanRoots();
    } catch (e) {
      this.error = String(e);
    } finally {
      this.detecting = false;
    }
  }

  /** Refresh candidates from the saved roots (after a save). */
  async refreshCandidates() {
    try {
      this.candidates = await detectScanRoots();
    } catch {
      // Non-fatal: the user can still type a path.
    }
  }

  async openDialog() {
    await this.load();
    this.error = null;
    this.open = true;
  }

  close() {
    this.open = false;
  }

  /** Save; returns true when the caller should rescan (roots, exclusions
   * or provider set changed). */
  async save(next: Settings): Promise<boolean> {
    this.saving = true;
    this.error = null;
    try {
      const key = (xs: string[]) => [...xs].sort().join("\n");
      const beforeRoots = key(this.settings?.scan_roots ?? []);
      const beforeExcluded = key(this.settings?.excluded_paths ?? []);
      const beforeDisabled = key(this.settings?.disabled_providers ?? []);
      this.settings = await updateSettings(next);
      this.providers = await getProviders();
      const changed =
        beforeRoots !== key(next.scan_roots) ||
        beforeExcluded !== key(next.excluded_paths) ||
        beforeDisabled !== key(next.disabled_providers) ||
        this.settings.retention_days !== next.retention_days;
      if (beforeRoots !== key(next.scan_roots)) await this.refreshCandidates();
      return changed;
    } catch (e) {
      this.error = String(e);
      return false;
    } finally {
      this.saving = false;
      this.open = false;
    }
  }

  /** Persist an explicit theme choice without touching the dialog. */
  async setTheme(theme: ThemeMode) {
    if (!this.settings) return;
    try {
      this.settings = await updateSettings({ ...this.settings, theme });
    } catch (e) {
      this.error = String(e);
    }
  }
}

export const settingsStore = new SettingsStore();

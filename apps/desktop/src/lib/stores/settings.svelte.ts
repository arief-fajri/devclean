/**
 * Settings store: scan roots + confirmation toggles.
 * Backend persistence is in-memory (file persistence = M3, per contract);
 * the safety validator still re-checks every path (I2, I7).
 */
import { getSettings, updateSettings } from "$lib/tauri";
import type { Settings } from "$lib/tauri";

class SettingsStore {
  settings = $state<Settings | null>(null);
  open = $state(false);
  saving = $state(false);
  error = $state<string | null>(null);

  async load() {
    try {
      this.settings = await getSettings();
    } catch (e) {
      this.error = String(e);
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

  /** Save; returns true when scan roots changed (caller should rescan). */
  async save(next: Settings): Promise<boolean> {
    this.saving = true;
    this.error = null;
    try {
      const before = [...(this.settings?.scan_roots ?? [])].sort().join("\n");
      this.settings = await updateSettings(next);
      const after = [...next.scan_roots].sort().join("\n");
      return before !== after;
    } catch (e) {
      this.error = String(e);
      return false;
    } finally {
      this.saving = false;
      this.open = false;
    }
  }
}

export const settingsStore = new SettingsStore();

/**
 * Typed Tauri bridge — the ONLY module allowed to call `invoke`/`listen`.
 * Mirrors `docs/reference/tauri-contract.md` v0.2.0. Contract change here
 * requires a contract doc update (G-API-01).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type SafetyLevel = "safe" | "caution" | "dangerous";
export type CleanupStatus = "success" | "partial" | "failed" | "cancelled";

export interface Resource {
  id: string;
  provider: string;
  name: string;
  path: string;
  size_bytes: number;
  reclaimable_bytes: number;
  safety: SafetyLevel;
  detail?: string;
}

export interface ProviderSummary {
  provider: string;
  name: string;
  total_bytes: number;
  reclaimable_bytes: number;
  resource_count: number;
}

export interface ScanSummary {
  total_bytes: number;
  reclaimable_bytes: number;
  providers: ProviderSummary[];
  resources: Resource[];
}

export interface CleanupPreviewItem {
  resource_id: string;
  name: string;
  bytes: number;
  safety: SafetyLevel;
  warning?: string;
}

export interface CleanupPreview {
  items: CleanupPreviewItem[];
  total_bytes: number;
}

export interface CleanupResult {
  resource_id: string;
  requested_bytes: number;
  freed_bytes: number;
  duration_ms: number;
  status: CleanupStatus;
  error?: string;
}

export interface Settings {
  scan_roots: string[];
  excluded_paths: string[];
  require_confirmation: boolean;
  show_preview: boolean;
  allow_privileged_operations: boolean;
  scan_automatically: boolean;
  theme: ThemeMode;
}

export type ThemeMode = "system" | "dark" | "light";

export interface HistoryEntry {
  finished_at_unix: number;
  resource_id: string;
  provider: string;
  name: string;
  requested_bytes: number;
  freed_bytes: number;
  status: CleanupStatus;
  error?: string;
}

export interface CleanupProgress {
  done: number;
  total: number;
}

export interface ScanProgress {
  provider: string;
  done: number;
  total: number;
}

export const scanAll = () => invoke<ScanSummary>("scan_all");
export const scanProvider = (provider: string) =>
  invoke<ScanSummary>("scan_provider", { provider });
export const getResourceDetails = (id: string) =>
  invoke<Resource | null>("get_resource_details", { id });
export const previewCleanup = (ids: string[]) =>
  invoke<CleanupPreview>("preview_cleanup", { ids });
export const runCleanup = (ids: string[]) =>
  invoke<CleanupResult[]>("cleanup", { ids });
export const cancelScan = () => invoke<void>("cancel_scan");
export const cancelCleanup = () => invoke<void>("cancel_cleanup");
export const getSettings = () => invoke<Settings>("get_settings");
export const updateSettings = (settings: Settings) =>
  invoke<Settings>("update_settings", { settings });
export const getCachedScan = () => invoke<ScanSummary | null>("get_cached_scan");
export const getHistory = (limit?: number) =>
  invoke<HistoryEntry[]>("get_history", { limit });

export const onScanProgress = (cb: (p: ScanProgress) => void): Promise<UnlistenFn> =>
  listen<ScanProgress>("scan_progress", (e) => cb(e.payload));
export const onScanStarted = (cb: () => void): Promise<UnlistenFn> =>
  listen<null>("scan_started", () => cb());
export const onScanCompleted = (cb: (s: ScanSummary) => void): Promise<UnlistenFn> =>
  listen<ScanSummary>("scan_completed", (e) => cb(e.payload));
export const onCleanupStarted = (cb: (ids: string[]) => void): Promise<UnlistenFn> =>
  listen<string[]>("cleanup_started", (e) => cb(e.payload));
export const onCleanupProgress = (cb: (p: CleanupProgress) => void): Promise<UnlistenFn> =>
  listen<CleanupProgress>("cleanup_progress", (e) => cb(e.payload));
export const onCleanupCompleted = (cb: (r: CleanupResult[]) => void): Promise<UnlistenFn> =>
  listen<CleanupResult[]>("cleanup_completed", (e) => cb(e.payload));

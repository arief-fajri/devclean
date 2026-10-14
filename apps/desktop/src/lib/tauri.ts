/**
 * Typed Tauri bridge — the ONLY module allowed to call `invoke`/`listen`.
 * Mirrors `docs/reference/tauri-contract.md` v0.7.0. Contract change here
 * requires a contract doc update (G-API-01).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type SafetyLevel = "safe" | "caution" | "dangerous";
export type CleanupStatus = "success" | "partial" | "failed" | "cancelled";

export interface ResourceImpact {
  consequence: string;
  cost?: string;
  command?: string;
}

export interface Resource {
  id: string;
  provider: string;
  name: string;
  path: string;
  size_bytes: number;
  reclaimable_bytes: number;
  safety: SafetyLevel;
  detail?: string;
  impact?: ResourceImpact;
  /** Age of the item in whole days. Null for global caches, whose age is
   * meaningless — that is why retention never holds them back. */
  age_days?: number | null;
  /** False when the retention policy keeps this resource. Reported, but
   * the backend refuses it even if its id is sent directly. */
  retention_eligible: boolean;
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
  /** Bytes held back by the retention policy — reported separately so the
   * headline number never silently shrinks. */
  retention_blocked_bytes: number;
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
  /** Proof this id set was measured and shown. `cleanup` requires it
   * while confirmation is on, so the gate is enforced by the backend. */
  preview_token: string;
  expires_at_unix: number;
}

export interface CleanupResult {
  resource_id: string;
  requested_bytes: number;
  freed_bytes: number;
  duration_ms: number;
  status: CleanupStatus;
  error?: string;
}

/** How far Docker cleanup may go. Anything past `build-cache` also
 * requires `allow_privileged_operations`. */
export type DockerPruneScope =
  | "off"
  | "build-cache"
  | "build-cache+images"
  | "all";

export const DOCKER_SCOPE_LABELS: Record<DockerPruneScope, string> = {
  off: "Off — never touch Docker",
  "build-cache": "Build cache only",
  "build-cache+images": "Build cache + unused images",
  all: "Everything, including volumes",
};

export const DOCKER_SCOPE_CONSEQUENCE: Record<DockerPruneScope, string> = {
  off: "Docker is left completely alone.",
  "build-cache": "Only the build cache goes. Images rebuild from your Dockerfile.",
  "build-cache+images": "Unused images go too — including ones you built by hand.",
  all: "Unused volumes are REMOVED. A volume can hold database state that no image references any more.",
};

export const DOCKER_SCOPE_NEEDS_PRIVILEGED: Record<DockerPruneScope, boolean> = {
  off: false,
  "build-cache": false,
  "build-cache+images": true,
  all: true,
};

export interface Settings {
  scan_roots: string[];
  excluded_paths: string[];
  require_confirmation: boolean;
  show_preview: boolean;
  allow_privileged_operations: boolean;
  scan_automatically: boolean;
  theme: ThemeMode;
  /** Provider ids skipped during scans (empty = everything active). */
  disabled_providers: string[];
  /** Keep anything younger than this many days out of cleanup.
   * `null` = no age policy. */
  retention_days?: number | null;
  docker_prune_scope: DockerPruneScope;
  /** Copy the AI database aside before a destructive SQL cleanup. Needs
   * roughly the database size in free space. Default off. */
  sqlite_backup: boolean;
}

/** A folder DevClean suggests as a scan root. */
export interface ScanRootCandidate {
  path: string;
  approximate_bytes: number | null;
  cache_count: number;
  already_configured: boolean;
}

export interface DiskFree {
  available_bytes: number;
  total_bytes: number;
  /** False when the platform query failed — show "unknown", never "0". */
  known: boolean;
  /**
   * Mount point of the volume these numbers describe (e.g. `/Volumes/Work`),
   * when the platform can name it. macOS only; null elsewhere.
   */
  mount: string | null;
}

export interface ProviderInfo {
  id: string;
  name: string;
  enabled: boolean;
}

export type ThemeMode = "system" | "dark" | "light";

export interface HistoryEntry {
  /** Cleanup run this entry belongs to. Empty for entries written before
   * contract v0.6 — those are grouped by `finished_at_unix` instead. */
  run_id?: string;
  finished_at_unix: number;
  resource_id: string;
  provider: string;
  name: string;
  requested_bytes: number;
  freed_bytes: number;
  status: CleanupStatus;
  error?: string;
}

/** One cleanup run aggregated over its per-resource entries. The primary
 * unit of the history UI — a single run can be hundreds of resources. */
export interface CleanupRun {
  run_id: string;
  finished_at_unix: number;
  entry_count: number;
  requested_bytes: number;
  freed_bytes: number;
  success: number;
  partial: number;
  failed: number;
  cancelled: number;
  providers: string[];
  entries: HistoryEntry[];
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
export const runCleanup = (ids: string[], previewToken?: string) =>
  invoke<CleanupResult[]>("cleanup", { ids, previewToken });
export const cancelScan = () => invoke<void>("cancel_scan");
export const cancelCleanup = () => invoke<void>("cancel_cleanup");
export const getSettings = () => invoke<Settings>("get_settings");
export const updateSettings = (settings: Settings) =>
  invoke<Settings>("update_settings", { settings });
export const getCachedScan = () => invoke<ScanSummary | null>("get_cached_scan");
export const getHistory = (limit?: number) =>
  invoke<HistoryEntry[]>("get_history", { limit });
/** v0.6.0: newest cleanup **runs** (default 20, cap 200). Use this for any
 * UI — the entry-level `getHistory` limit lets one large cleanup hide every
 * earlier run. */
export const getHistoryRuns = (limit?: number) =>
  invoke<CleanupRun[]>("get_history_runs", { limit });
export const getProviders = () => invoke<ProviderInfo[]>("get_providers");
export const detectScanRoots = () => invoke<ScanRootCandidate[]>("detect_scan_roots");
export const getDiskFree = () => invoke<DiskFree>("get_disk_free");

export const onScanProgress = (cb: (p: ScanProgress) => void): Promise<UnlistenFn> =>
  listen<ScanProgress>("scan_progress", (e) => cb(e.payload));
/** v0.5.0: fired when a provider BEGINS, so the UI can show which one is
 * running right now instead of only which ones are already done. */
export const onScanProviderStarted = (cb: (p: string) => void): Promise<UnlistenFn> =>
  listen<string>("scan_provider_started", (e) => cb(e.payload));
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

/** v0.7.0: the menu bar asking the window to show a page. `route` is checked
 * against `$lib/nav` before use — an unknown id must never navigate. */
export interface TrayAction {
  route: string;
  filter?: string;
}

export const onTrayAction = (cb: (a: TrayAction) => void): Promise<UnlistenFn> =>
  listen<TrayAction>("tray_action", (e) => cb(e.payload));

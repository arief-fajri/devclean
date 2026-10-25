/**
 * Typed Tauri bridge — the ONLY module allowed to call `invoke`/`listen`.
 * Mirrors `docs/reference/tauri-contract.md` (the contract version lives in
 * that document's banner; do not restate it here to rot). Contract change
 * here requires a contract doc update (G-API-01) — enforced by
 * `contract-sync.test.ts`, which diffs the document's type block against
 * this file.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export {
  BLOCK_REASON_HINTS,
  BLOCK_REASON_LABELS,
  blockHint,
  blockLabel,
  blockedBytes,
  isSelectable,
  type BlockReason,
} from "./blocked";
import type { BlockReason } from "./blocked";

export type SafetyLevel = "safe" | "caution" | "dangerous";
export type CleanupStatus = "success" | "partial" | "failed" | "cancelled";

/** How recently a project-backed artifact was touched (contract v0.10.0).
 * Derived from the composite age — artifact mtime, last commit, project
 * directory mtime — not the filesystem mtime alone. */
export type Activity = "active" | "recent" | "stale" | "dormant";

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
  /** What deleting this frees: allocated blocks, not a sum of file lengths. */
  size_bytes: number;
  reclaimable_bytes: number;
  /** Logical size, present only when materially LARGER than `size_bytes`
   * (sparse files). Absent for ordinary trees. */
  apparent_bytes?: number | null;
  safety: SafetyLevel;
  detail?: string;
  impact?: ResourceImpact;
  /** Age of the item in whole days. Null for global caches, whose age is
   * meaningless — that is why retention never holds them back. v0.10.0: the
   * composite of artifact mtime, last commit and project directory mtime. */
  age_days?: number | null;
  /** How recently the artifact was touched (v0.10.0). Null when the scanner
   * could not classify it (a global cache, or a path outside any repository).
   * The toolbar filters on this directly, never by parsing `detail`. */
  activity?: Activity | null;
  /** Unix seconds of the containing repository's last commit (v0.10.0). */
  last_commit_unix?: number | null;
  /** Why this resource cannot be selected, or null when it can. The backend
   * refuses any resource carrying a reason, even if its id is sent directly. */
  blocked_reason?: BlockReason | null;
  /** One sentence explaining `blocked_reason`. */
  blocked_detail?: string | null;
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
  blocked_bytes: number;
  /** v0.13.0: distinct-inode bytes across the whole scan — `Some` after a
   * full scan, `null` whenever the figure can no longer be measured
   * (targeted rescan, post-cleanup prune, an older cache). The backend
   * measures it; the frontend carries it and never recomputes it. */
  unique_bytes?: number | null;
}

/** The last scan as read from the on-disk cache, with the moment it was taken
 * so the UI can show its real age rather than the launch time. */
export interface CachedScan {
  summary: ScanSummary;
  saved_at_unix: number;
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
  /** Bytes returned to the filesystem by this cleanup. Every cleanup is
   * permanent since 1.0.0, so this is always the space actually reclaimed. */
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
  /** Allow cleanup of AI conversation history. Default false. */
  ai_cleanup_enabled: boolean;
  /** Check GitHub Releases for a newer version at startup. Default false —
   * DevClean makes no routine outbound request unless asked. v0.12.0. */
  check_updates_automatically: boolean;
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
  /** Legacy pre-1.0 read-only flag: `true` when the artifact was moved to
   * the Trash. Never set by new cleanups; kept so old history lines can be
   * labelled as archived. Not part of the contract. */
  moved_to_trash?: boolean;
  /** Which sizing convention produced these figures. Absent = "apparent"
   * (every line written before v0.8.0). */
  sizing?: "apparent" | "allocated";
  status: CleanupStatus;
  error?: string;
}

/** One cleanup run aggregated over its per-resource entries. The primary
 * unit of the history UI — a single run can be hundreds of resources. */
export interface CleanupRun {
  /** True when any entry used the pre-0.1.0 sizing convention. */
  pre_allocated_sizing: boolean;
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
export const getCachedScan = () => invoke<CachedScan | null>("get_cached_scan");
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

/** Phase of the in-app updater (contract v0.12.0). The backend switches on
 * these strings, so they are part of the contract. */
export type UpdatePhase =
  | "idle"
  | "checking"
  | "up_to_date"
  | "available"
  | "downloading"
  | "installing"
  | "ready_to_restart"
  | "error";

/** State of the in-app updater (contract v0.12.0). One value, emitted whole
 * on every change, so the UI never stitches phases together. */
export interface UpdateStatus {
  /** The version this build reports. */
  current_version: string;
  phase: UpdatePhase;
  /** The newer version the channel offers, when one exists. */
  available_version?: string | null;
  /** Release notes from the manifest, when the channel provides them. */
  notes?: string | null;
  checked_at_unix?: number | null;
  /** Whole percent while downloading; null when the server sent no length. */
  progress_percent?: number | null;
  /** Why the last check or install failed. Never a silent no-op. */
  error?: string | null;
}

/** The last updater state, without a network call. */
export const getUpdateStatus = () => invoke<UpdateStatus>("get_update_status");
/** Ask the release channel whether a newer version exists (bounded). */
export const checkForUpdate = () => invoke<UpdateStatus>("check_for_update");
/** Download + install the pending update. Does NOT restart the app. */
export const installUpdate = () => invoke<UpdateStatus>("install_update");
/** Relaunch into the freshly installed version. */
export const restartApp = () => invoke<void>("restart_app");

/** v0.12.0: the whole updater state after any change. */
export const onUpdateStatus = (cb: (s: UpdateStatus) => void): Promise<UnlistenFn> =>
  listen<UpdateStatus>("update_status", (e) => cb(e.payload));

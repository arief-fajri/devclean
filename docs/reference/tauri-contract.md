# Tauri Contract (public behavior contract)

> The single document defining what the frontend may see. Changing this file
> = changing the contract → requires human approval (Level B) + a regression
> test. Version: **v0.6.0 (run-level history, targeted provider scan)**.

## Commands (15)

| Command | Args | Returns | Notes |
|---|---|---|---|
| `scan_all` | — | `ScanSummary` | Scans every enabled provider (4 concurrent); emits `scan_started → scan_provider_started* → scan_progress* → scan_completed` |
| `scan_provider` | `{provider: string}` | `ScanSummary` | Scans **only** that provider; merges into state so other providers' ids still resolve (v0.6.0) |
| `get_resource_details` | `{id: string}` | `Resource \| null` | Drill-down for a single resource |
| `preview_cleanup` | `{ids: string[]}` | `CleanupPreview` | Re-measures via the provider and issues a `preview_token`; **must** be called before `cleanup` when confirmation is on |
| `cleanup` | `{ids: string[], preview_token?: string}` | `CleanupResult[]` | Refuses without a valid token when `require_confirmation`; emits `cleanup_started → cleanup_progress* → cleanup_completed`; appends to history |
| `cancel_scan` | — | `()` | Sets the cancel flag; settles in <1s |
| `cancel_cleanup` | — | `()` | Cancels the remaining queue; the in-flight item finishes |
| `get_settings` | — | `Settings` | Loaded from disk at startup; empty `scan_roots` are re-seeded from the defaults |
| `update_settings` | `{settings: Settings}` | `Settings` | Persists to disk |
| `get_cached_scan` | — | `ScanSummary \| null` | Last scan iff roots match; `null` otherwise |
| `get_history` | `{limit?: number}` | `HistoryEntry[]` | Newest entries, oldest first; default 100, cap 1000 |
| `get_providers` | — | `ProviderInfo[]` | Every registered provider + its enabled state |
| `detect_scan_roots` | — | `ScanRootCandidate[]` | Candidate folders under `$HOME`, largest cache first (v0.5.0) |
| `get_disk_free` | — | `DiskFree` | Free/total bytes of the data volume; `known: false` when unavailable (v0.5.0) |
| `get_history_runs` | `{limit?: number}` | `CleanupRun[]` | Newest cleanup **runs** (default 20, cap 200) with their entries attached; tail-read (v0.6.0) |

## Events (7)

| Event | Payload | When |
|---|---|---|
| `scan_started` | `null` | Scan begins |
| `scan_provider_started` | `string` (provider id) | A provider **begins** — lets the UI show which one is running, not just which are done (v0.5.0) |
| `scan_progress` | `{provider, done, total}` | Each provider finishes; `done` counts completions and is monotonic |
| `scan_completed` | `ScanSummary` | All providers finished |
| `cleanup_started` | `string[]` (ids) | Cleanup begins |
| `cleanup_progress` | `{done, total}` | One per finished item |
| `cleanup_completed` | `CleanupResult[]` | All ids processed |

## Types (summary; canonical in `crates/core/src/types.rs`)

```ts
type SafetyLevel = "safe" | "caution" | "dangerous";
type CleanupStatus = "success" | "partial" | "failed" | "cancelled";
type DockerPruneScope = "off" | "build-cache" | "build-cache+images" | "all";
type ThemeMode = "system" | "dark" | "light";

interface ResourceImpact { consequence: string; cost?: string; command?: string }

interface Resource {
  id: string; provider: string; name: string; path: string;
  size_bytes: number; reclaimable_bytes: number; safety: SafetyLevel;
  detail?: string; impact?: ResourceImpact;
  /** Age in whole days; null for global caches, whose age is meaningless. */
  age_days?: number | null;
  /** False when the retention policy keeps it. The backend refuses such an
   *  id even if the frontend sends it directly. */
  retention_eligible: boolean;
}

interface ProviderSummary { provider: string; name: string; total_bytes: number;
  reclaimable_bytes: number; resource_count: number }

interface ScanSummary { total_bytes: number; reclaimable_bytes: number;
  providers: ProviderSummary[]; resources: Resource[];
  /** Bytes held back by the retention policy, reported separately. */
  retention_blocked_bytes: number }

interface CleanupPreviewItem { resource_id: string; name: string; bytes: number;
  safety: SafetyLevel; warning?: string }

interface CleanupPreview { items: CleanupPreviewItem[]; total_bytes: number;
  preview_token: string; expires_at_unix: number }

interface CleanupResult { resource_id: string; requested_bytes: number;
  freed_bytes: number; duration_ms: number; status: CleanupStatus; error?: string }

interface Settings {
  scan_roots: string[]; excluded_paths: string[];
  require_confirmation: boolean; show_preview: boolean;
  allow_privileged_operations: boolean; scan_automatically: boolean;
  theme: ThemeMode; disabled_providers: string[];
  /** Keep items younger than N days out of cleanup; null = no age policy. */
  retention_days?: number | null;
  docker_prune_scope: DockerPruneScope;
  sqlite_backup: boolean;
}

interface ProviderInfo { id: string; name: string; enabled: boolean }

interface ScanRootCandidate { path: string; approximate_bytes: number | null;
  cache_count: number; already_configured: boolean }

interface DiskFree { available_bytes: number; total_bytes: number; known: boolean }

interface HistoryEntry { run_id?: string; finished_at_unix: number;
  resource_id: string; provider: string; name: string; requested_bytes: number;
  freed_bytes: number; status: CleanupStatus; error?: string }
interface CleanupRun { run_id: string; finished_at_unix: number; entry_count: number;
  requested_bytes: number; freed_bytes: number; success: number; partial: number;
  failed: number; cancelled: number; providers: string[];
  entries: HistoryEntry[] }
```

Example request (the frontend never sends paths):

```json
{ "ids": ["nm:0003", "rs:0000", "go:0001"], "preview_token": "v1:1789...:1f:9ab2..." }
```

Not:

```json
{ "paths": ["/Users/you/Projects/app/node_modules"] }
```

## Behavioural notes (no type change, but caller-visible)

1. **`CleanupResult.freed_bytes` is now measured, never assumed.** For a
   `RunCommand` resource the engine reads the provider's `measure()` before
   and after the command. Before v0.5.0 it compared against the resource's
   sentinel path — never a directory — so `after` was always `0` and
   `freed` always equalled `requested`. A prune that reclaimed 3 GB of a
   10 GB cache reported 10 GB. When the after-state cannot be measured at
   all, `freed_bytes` is `0` and `error` says so.
2. **`scan_progress.done` is monotonic.** It counts providers that *finished*
   (was: their static position in the provider list, which could run
   backwards or report `N/N` with work pending).
3. `cleanup` returns `Err` (not a per-item failure) when a required
   `preview_token` is missing, expired, or covers a different id set.

## Contract changelog

- `v0.6.0` — run-level history + targeted provider scan.
  - `+get_history_runs` → `CleanupRun[]`; `HistoryEntry.run_id?`
    (`#[serde(default)]`, so every pre-v0.6 history line still reads — legacy
    entries are grouped by `finished_at_unix`).
  - `scan_provider` now scans **only** the requested provider instead of a
    full scan plus a filter, and merges the result into `AppState::resources`
    so the other providers' id→path entries survive.
  - Regression tests: `a_huge_run_no_longer_hides_earlier_runs`,
    `legacy_lines_without_run_id_are_grouped_by_timestamp`,
    `tail_read_handles_multi_chunk_files_and_straddling_lines`,
    `run_totals` (frontend).
- `v0.5.0` — retention, AI data, backend-enforced gates.
  **Commands:** `+detect_scan_roots`, `+get_disk_free`; `cleanup` gains
  `preview_token?`. **Events:** `+scan_provider_started`.
  **Types (all additive, `#[serde(default)]` so older `settings.json` and
  scan caches still load):** `Resource.age_days`, `Resource.retention_eligible`,
  `ScanSummary.retention_blocked_bytes`, `CleanupPreview.preview_token` +
  `expires_at_unix`, `Settings.retention_days`, `Settings.docker_prune_scope`,
  `Settings.sqlite_backup`.
  **Behaviour:** measured `freed_bytes`; monotonic `scan_progress.done`;
  backend now enforces confirmation, retention and Docker scope.
  Providers 9 → 14 (`+docker_pruned`, `+global_cache`, `+opencode_fs`,
  `+opencode_db`, `+claude`).
  Regression tests: `old_cached_scans_without_impact_still_deserialize`,
  `pre_v04_settings_file_without_disabled_providers_enables_everything`,
  `empty_scan_roots_are_reseeded_from_defaults`,
  `cleanup_rejects_without_a_valid_preview_token`,
  `cleanup_rejects_expired_preview_token`,
  `retention_blocked_resource_is_refused_by_the_engine`,
  `privileged_scope_is_refused_until_the_user_opts_in`,
  `runcommand_freed_bytes_is_measured_not_assumed`,
  `progress_done_counts_completions_and_is_monotonic`,
  `provider_started_fires_once_per_enabled_provider`,
  `the_cutoff_is_computed_in_milliseconds_to_match_the_schema`.
- `v0.4.0` — Provider toggles: `+get_providers` → `ProviderInfo[]`, and
  `Settings.disabled_providers`. `cleanup_progress` fires **once per
  finished item**.
- `v0.3.0` — UI polish: `Resource.impact?`, additive.
- `v0.2.0` — M3: `+get_history`, `+get_cached_scan`, `Settings.theme`.
- `v0.1.0` — initial 9 commands + 6 events.
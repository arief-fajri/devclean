# Tauri Contract (public behavior contract)

> The single document defining what the frontend may see. Changing this file
> = changing the contract → requires human approval (Level B) + a regression
> test. Version: **v0.3.0 (UI polish)**.

## Commands (11)

| Command | Args | Returns | Notes |
|---|---|---|---|
| `scan_all` | — | `ScanSummary` | Scans 4 providers in parallel; emits `scan_started → scan_progress* → scan_completed` |
| `scan_provider` | `{provider: string}` | `ScanSummary` | MVP: full scan, then filter to one provider |
| `get_resource_details` | `{id: string}` | `Resource \| null` | Drill-down for a single resource |
| `preview_cleanup` | `{ids: string[]}` | `CleanupPreview` | Re-measures + warnings; must be called before `cleanup` when preview is ON |
| `cleanup` | `{ids: string[]}` | `CleanupResult[]` | Emits `cleanup_started → cleanup_progress → cleanup_completed`; appends to history |
| `cancel_scan` | — | `()` | Sets the cancel flag; settles in <1s (target) |
| `cancel_cleanup` | — | `()` | Cancels the remaining queue; the in-flight item finishes |
| `get_settings` | — | `Settings` | Loaded from disk at startup (M3) |
| `update_settings` | `{settings: Settings}` | `Settings` | Persists to disk (M3) |
| `get_cached_scan` | — | `ScanSummary \| null` | Last scan iff roots match; `null` otherwise (M3) |
| `get_history` | `{limit?: number}` | `HistoryEntry[]` | Newest entries, oldest first; default 100, cap 1000 (M3) |

## Events (6)

| Event | Payload | When |
|---|---|---|
| `scan_started` | `null` | Scan begins |
| `scan_progress` | `{provider, done, total}` | Each provider finishes |
| `scan_completed` | `ScanSummary` | All providers finished |
| `cleanup_started` | `string[]` (ids) | Cleanup begins |
| `cleanup_progress` | `{done, total}` | Batch finished |
| `cleanup_completed` | `CleanupResult[]` | All ids processed |

## Types (summary; canonical in `crates/core/src/types.rs`)

```ts
type SafetyLevel = "safe" | "caution" | "dangerous";
interface ResourceImpact { consequence: string; cost?: string; command?: string }
interface Resource { id: string; provider: string; name: string; path: string;
  size_bytes: number; reclaimable_bytes: number; safety: SafetyLevel; detail?: string;
  impact?: ResourceImpact }
interface ProviderSummary { provider: string; name: string; total_bytes: number;
  reclaimable_bytes: number; resource_count: number }
interface ScanSummary { total_bytes: number; reclaimable_bytes: number;
  providers: ProviderSummary[]; resources: Resource[] }
interface CleanupPreviewItem { resource_id: string; name: string; bytes: number;
  safety: SafetyLevel; warning?: string }
interface CleanupPreview { items: CleanupPreviewItem[]; total_bytes: number }
type CleanupStatus = "success" | "partial" | "failed" | "cancelled";
interface CleanupResult { resource_id: string; requested_bytes: number; freed_bytes: number;
  duration_ms: number; status: CleanupStatus; error?: string }
interface Settings { scan_roots: string[]; excluded_paths: string[];
  require_confirmation: boolean; show_preview: boolean;
  allow_privileged_operations: boolean; scan_automatically: boolean;
  theme: "system" | "dark" | "light" }
interface HistoryEntry { finished_at_unix: number; resource_id: string;
  provider: string; name: string; requested_bytes: number; freed_bytes: number;
  status: CleanupStatus; error?: string }
```

Example request (the frontend never sends paths):

```json
{ "ids": ["nm:0003", "rs:0000", "go:0001"] }
```

Not:

```json
{ "paths": ["/Users/you/Projects/app/node_modules"] }
```

## Contract changelog

- `v0.3.0` — UI polish: `Resource.impact?` (`ResourceImpact {consequence, cost?, command?}`,
  additive, `#[serde(default)]` so v0.2.x on-disk caches still load). CLI
  text moved out of `detail` (age/signal only) into `impact.command`.
  Regression tests: `old_cached_scans_without_impact_still_deserialize`,
  per-provider `impact` assertions. No new commands or events.
- `v0.2.0` — M3: `+get_history`, `+get_cached_scan`, `Settings.theme`. Provider
  scans are cancellable per entry with a 60s timeout; cleanup subprocesses
  have a 120s timeout (both code+test enforced, no type changes).
- `v0.1.0` — initial 9 commands + 6 events. `scan_provider` is still full-scan-filter (optimize in M3).

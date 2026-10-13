# Tauri Contract (public behavior contract)

> The single document defining what the frontend may see. Changing this file
> = changing the contract → requires human approval (Level B) + a regression
> test. Version: **v0.1.0 (MVP)**.

## Commands (9)

| Command | Args | Returns | Notes |
|---|---|---|---|
| `scan_all` | — | `ScanSummary` | Scans 4 providers in parallel; emits `scan_started → scan_progress* → scan_completed` |
| `scan_provider` | `{provider: string}` | `ScanSummary` | MVP: full scan, then filter to one provider |
| `get_resource_details` | `{id: string}` | `Resource \| null` | Drill-down for a single resource |
| `preview_cleanup` | `{ids: string[]}` | `CleanupPreview` | Re-measures + warnings; must be called before `cleanup` when preview is ON |
| `cleanup` | `{ids: string[]}` | `CleanupResult[]` | Emits `cleanup_started → cleanup_progress → cleanup_completed` |
| `cancel_scan` | — | `()` | Sets the cancel flag; settles in <1s (target) |
| `cancel_cleanup` | — | `()` | Cancels the remaining queue; the in-flight item finishes |
| `get_settings` | — | `Settings` | — |
| `update_settings` | `{settings: Settings}` | `Settings` | File persistence = M3 (currently in-memory) |

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
interface Resource { id: string; provider: string; name: string; path: string;
  size_bytes: number; reclaimable_bytes: number; safety: SafetyLevel; detail?: string }
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
  allow_privileged_operations: boolean; scan_automatically: boolean }
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

- `v0.1.0` — initial 9 commands + 6 events. `scan_provider` is still full-scan-filter (optimize in M3).

# Tauri Contract (public behavior contract)

> The single document defining what the frontend may see. Changing this file
> = changing the contract → requires human approval (Level B) + a regression
> test. Version: **v0.9.0 (cache-first startup)**.

## Commands (15)

| Command | Args | Returns | Notes |
|---|---|---|---|
| `scan_all` | — | `ScanSummary` | Scans every enabled provider (4 concurrent); emits `scan_started → scan_provider_started* → scan_progress* → scan_completed` |
| `scan_provider` | `{provider: string}` | `ScanSummary` | Scans **only** that provider; merges into state so other providers' ids still resolve (v0.6.0) |
| `get_resource_details` | `{id: string}` | `Resource \| null` | Drill-down for a single resource |
| `preview_cleanup` | `{ids: string[]}` | `CleanupPreview` | Re-measures via the provider and issues a `preview_token`; **must** be called before `cleanup` when confirmation is on. **Fails after `PREVIEW_TIMEOUT` (120s)** instead of blocking (v0.7.2); ids the backend does not resolve are omitted from `items`, which the UI reports rather than presenting as "0 B" |
| `cleanup` | `{ids: string[], preview_token?: string}` | `CleanupResult[]` | Refuses without a valid token when `require_confirmation`; emits `cleanup_started → cleanup_progress* → cleanup_completed`; appends to history |
| `cancel_scan` | — | `()` | Sets the cancel flag; settles in <1s |
| `cancel_cleanup` | — | `()` | Cancels the remaining queue; the in-flight item finishes |
| `get_settings` | — | `Settings` | Loaded from disk at startup; empty `scan_roots` are re-seeded from the defaults |
| `update_settings` | `{settings: Settings}` | `Settings` | Persists to disk |
| `get_cached_scan` | — | `CachedScan \| null` | Last scan + its `saved_at_unix`, iff roots match; `null` otherwise. Also seeds the id registry from the cache, so a cleanup started before the first scan finishes still resolves its ids (v0.7.2). Return shape changed in v0.9.0 from `ScanSummary \| null` to carry the scan's age |
| `get_history` | `{limit?: number}` | `HistoryEntry[]` | Newest entries, oldest first; default 100, cap 1000 |
| `get_providers` | — | `ProviderInfo[]` | Every registered provider + its enabled state |
| `detect_scan_roots` | — | `ScanRootCandidate[]` | Candidate folders under `$HOME`, largest cache first (v0.5.0) |
| `get_disk_free` | — | `DiskFree` | Free/total bytes of the volume holding `$HOME`, plus that volume's `mount` name when the platform can supply one; `known: false` when unavailable. Measured with `statvfs`, **no subprocess** (v0.7.3) |
| `get_history_runs` | `{limit?: number}` | `CleanupRun[]` | Newest cleanup **runs** (default 20, cap 200) with their entries attached; tail-read (v0.6.0) |

## Events (8)

| Event | Payload | When |
|---|---|---|
| `scan_started` | `null` | Scan begins |
| `scan_provider_started` | `string` (provider id) | A provider **begins** — lets the UI show which one is running, not just which are done (v0.5.0) |
| `scan_progress` | `{provider, done, total}` | Each provider finishes; `done` counts completions and is monotonic |
| `scan_completed` | `ScanSummary` | All providers finished |
| `cleanup_started` | `string[]` (ids) | Cleanup begins |
| `cleanup_progress` | `{done, total}` | One per finished item |
| `cleanup_completed` | `CleanupResult[]` | All ids processed |
| `tray_action` | `{route: string, filter?: string}` | The menu bar asked the window to show a page (v0.7.0). Emitted **after** the window is brought forward. The UI validates `route` against its own page table and ignores anything else |

## Types (summary; canonical in `crates/core/src/types.rs`)

```ts
type SafetyLevel = "safe" | "caution" | "dangerous";
type CleanupStatus = "success" | "partial" | "failed" | "cancelled";
type DockerPruneScope = "off" | "build-cache" | "build-cache+images" | "all";
type ThemeMode = "system" | "dark" | "light";

interface ResourceImpact { consequence: string; cost?: string; command?: string }

interface Resource {
  id: string; provider: string; name: string; path: string;
  /** What deleting this frees: allocated blocks, not a sum of file lengths.
   *  Hard-linked inodes are counted once within the artifact. */
  size_bytes: number; reclaimable_bytes: number;
  /** Logical size, present only when materially LARGER than `size_bytes`
   *  (sparse files). Absent for ordinary trees. */
  apparent_bytes?: number | null;
  safety: SafetyLevel;
  detail?: string; impact?: ResourceImpact;
  /** Age in whole days; null for global caches, whose age is meaningless. */
  age_days?: number | null;
  /** Why this resource cannot be selected, or null when it can. The backend
   *  refuses any resource carrying a reason, even if its id is sent directly. */
  blocked_reason?: BlockReason | null;
  /** One sentence explaining `blocked_reason`, e.g. "kept by your 30-day
   *  retention policy (this is 12d old)". */
  blocked_detail?: string | null;
}

type BlockReason = "retention" | "ai_read_only" | "privileged_required"
  | "docker_scope" | "protected";

interface ProviderSummary { provider: string; name: string; total_bytes: number;
  reclaimable_bytes: number; resource_count: number }

interface ScanSummary { total_bytes: number; reclaimable_bytes: number;
  providers: ProviderSummary[]; resources: Resource[];
  /** Bytes the backend will refuse to clean, for ANY reason — retention,
   *  AI report-only, privileged opt-in, Docker scope, git protection. */
  blocked_bytes: number }

/** v0.9.0: the on-disk cache as the UI sees it. */
interface CachedScan { summary: ScanSummary; saved_at_unix: number }

interface CleanupPreviewItem { resource_id: string; name: string; bytes: number;
  safety: SafetyLevel; warning?: string }

interface CleanupPreview { items: CleanupPreviewItem[]; total_bytes: number;
  preview_token: string; expires_at_unix: number }

interface CleanupResult { resource_id: string; requested_bytes: number;
  freed_bytes: number; moved_to_trash: boolean; duration_ms: number;
  status: CleanupStatus; error?: string }

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

interface DiskFree { available_bytes: number; total_bytes: number; known: boolean;
  mount: string | null }   // v0.7.3: e.g. "/Volumes/Work"; macOS only, null elsewhere

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
3. **`preview_cleanup` is bounded (v0.7.2).** Re-measuring walks every
   selected path; over 120s it fails with an explanation instead of leaving the
   dialog waiting forever (G-REL-01, which previously covered scans and
   subprocesses but not this).
4. **A partial preview is reported, not hidden.** `preview_cleanup` silently
   omitted ids it could not resolve; the UI now shows how many of the selected
   resources were found, and refuses to present an empty preview as work.
5. **Closing the window hides it (v0.7.0).** The app keeps running in the menu
   bar; only `Quit DevClean` exits. Before this, closing the last window made
   the runtime exit (`tauri-runtime-writ` exits on `RunEvent::ExitRequested`
   when the last window is destroyed), which silently removed the tray icon
   and made every menu action unreachable.
6. **Navigation is frontend-local.** The page id lives in `$lib/nav` and is
   never sent to the backend; a reload always starts on Overview. No command
   was added to support the sidebar.
7. **The Dock icon opens the window (v0.7.3).** Closing the window hides it
   (note 5), which left the Dock icon on screen and inert: macOS sends
   `applicationShouldHandleReopen` on a Dock click, but `Builder::run` installs
   an *empty* event handler (`build()?.run(|_, _| {})`), so nothing acted on
   it. The shell now runs its own loop and handles `RunEvent::Reopen` by
   calling the same `show_main` the menu bar uses. The Dock, the tray icon and
   the menu all reveal the window through one function, so they cannot drift.
8. **Free space is re-measured where it moves (v0.7.3).** The dashboard read
   `get_disk_free` once at launch, so after a cleanup the card quoted the
   pre-deletion figure. Both the UI store and the menu bar's cached copy now
   refresh when a cleanup finishes. `get_disk_free` also stops spawning `df`
   on the boot path (v0.7.3): it is measured with `statvfs`, and a
   regression test compares the two so the substitution cannot silently drift.
9. **Startup paints from the cache and rests (v0.9.0).** The app used to run a
   full scan on every launch, unconditionally: `scan_automatically` (default
   OFF) was stored and rendered but never read, and the window opened onto a
   loader nobody asked for. Now the shell paints `get_cached_scan` and runs a
   scan **only when `scan_automatically` is on** (or when the user clicks Scan
   Now / Scan again, or a cleanup finished). With no cache the dashboard shows
   an explicit "You haven't scanned yet" warning instead of scanning on its own.
   Because the cache is now the resting state, `CachedScan.saved_at_unix` is
   surfaced so the summary can report the scan's real age and flag a stale
   cache rather than presenting old numbers as current.

## Menu bar surface (v0.7.0)

Not an IPC contract, but caller-visible: the menu bar is the only part of
DevClean a user can reach when the window is closed.

| Item | Action |
|---|---|
| `open` | Show and focus the window |
| `scan` | `run_scan` in the background — **never** a cleanup |
| `review` | Show the window on the Resources page with the safe filter applied |
| `history` | Show the window on the History page |
| `settings` | Show the window on the Settings page |
| `quit` | Exit the app |

One click of either button opens the menu (`show_menu_on_left_click`); the
info block above the actions is rebuilt from the on-disk scan cache, and
`open`/`review`/`history`/`settings` additionally emit `tray_action`. There
is no destructive item — see G-UI-02 in `docs/development/methodology/guardrails.md`.

## Contract changelog

- `v0.9.0` — cache-first startup. **One command re-typed; no command added or
  removed.**
  - `get_cached_scan` now returns `CachedScan | null` (`{ summary,
    saved_at_unix }`) instead of `ScanSummary | null`, so the UI can report the
    scan's real age. `devclean_core::CachedScan` and
    `devclean_scanner::load_cached_scan` are new; `load_cached_summary` stays
    for the menu bar, which reads the file's mtime for age.
  - **Behaviour:** a scan runs on launch only when `Settings.scan_automatically`
    is on. The shell paints the cache and rests; with no cache it shows a
    "You haven't scanned yet" warning. Previously the launch scan was
    unconditional and `scan_automatically` was a control with no effect.
  - **Behaviour:** the summary reports the cached scan's age (from
    `saved_at_unix`) and flags a cache older than 7 days, instead of stamping
    the paint with the launch time.
  - Regression tests: `the_cache_carries_the_time_it_was_saved` (Rust),
    `the_cached_summary_seeds_the_id_registry` (now asserts `saved_at_unix`),
    and `cache-first startup` (frontend: the launch scan is gated, the cached
    paint derives its age).
- `v0.8.0` — honest sizes, recoverable deletes, and gates that actually run.
  **No command added, removed or re-typed.** Three coupled changes; the third
  fixes a control that had never worked.

  **1. Sizes that match `du`.**
  - `Resource.size_bytes` now means **allocated blocks** (`st_blocks * 512`),
    i.e. what deleting the resource frees — it was a sum of file *lengths*,
    which read up to 19% low on trees full of small files and up to 145% high
    on trees full of hard links (measured: this repo's own `target/` reported
    26.6 GB against `du`'s 10.8 GB). Hard-linked inodes are counted once per
    artifact via `(dev, ino)`.
  - `+Resource.apparent_bytes?: number | null` (additive, `#[serde(default)]`)
    — the logical size, present only when materially larger than `size_bytes`,
    so sparse files can be shown as `8.6 GB (appears as 460 GB)`.
  - The on-disk scan cache is versioned `1 → 2`: a v1 cache holds logical
    sizes and is rejected rather than displayed.
  - Decision record: [docs/decisions/allocated-vs-apparent.md](../decisions/allocated-vs-apparent.md).

  **2. Recoverable deletes.**
  - `+Settings.delete_mode: "trash" | "permanent"` (additive,
    `#[serde(default)]` → `trash`). Path removal moves the artifact to
    `~/.Trash` by default instead of deleting it.
  - `+CleanupResult.moved_to_trash: boolean`, `+HistoryEntry.moved_to_trash`
    (additive). **Under `trash`, `freed_bytes` is 0**: the space is still on
    disk and returns only when the user empties the Trash.
  - The mode governs path removal only — `RunCommand` and `RunSql` have no
    undo and are never reported as trashed.
  - Decision record: [docs/decisions/trash-by-default.md](../decisions/trash-by-default.md).

  **3. Blocked reasons — a control that never ran, fixed.**
  - `Resource.retention_eligible: boolean` is **replaced** by
    `blocked_reason?: BlockReason | null` + `blocked_detail?: string | null`
    (breaking to the type; nothing else consumed the old field).
  - **`retention_eligible` was never set to `false` in production code**, so
    `Settings.retention_days` had no effect on any cleanup and
    `ScanSummary.blocked_bytes` was permanently zero. Measured after
    the fix with a 10-year policy: **106/106 resources blocked, 69.6 GB held
    back**; before it, the same policy blocked **zero**.
  - `+Settings.ai_cleanup_enabled: boolean` (additive, `#[serde(default)]` →
    `false`). AI conversation history is reported with sizes but not
    selectable until the user opts in (`blocked_reason: "ai_read_only"`).
  - `+HistoryEntry.sizing?: "apparent" | "allocated"` and
    `+CleanupRun.pre_allocated_sizing: boolean` (additive). Rows written
    before v0.8.0 read as `"apparent"`; the history page labels those runs
    "before 0.1.0".
  - The scan stamps the reason; the engine refuses what is stamped. Both call
    the same `devclean_core::policy::block_reason_for`, so the explanation the
    user reads is the one that is enforced (I11).
  - Decision record: [docs/decisions/blocked-reason.md](../decisions/blocked-reason.md).

  **Regression tests:** `allocated_counts_blocks_not_payload`,
  `an_empty_directory_measures_zero`,
  `a_hardlinked_file_is_counted_once_within_an_artifact`,
  `apparent_is_reported_only_when_materially_larger`,
  `a_v1_cache_with_logical_sizes_is_rejected`,
  `trash_mode_moves_aside_and_reports_pending_not_freed`,
  `permanent_mode_removes_and_reports_freed`,
  `command_backed_actions_are_never_marked_as_trashed`,
  the `devclean_core::policy` suite, `retention_and_ai_gates_now_reach_the_scan`,
  `blocked_ids_are_collected_for_the_engine` (Rust); `cleanupOutcome` and
  `blocked` suites (frontend).
- `v0.7.2` — bounded cleanup preview. **No command added, removed or
  re-typed.**
  - `preview_cleanup` now fails with an explanation after `PREVIEW_TIMEOUT`
    (120s) rather than blocking indefinitely; `PREVIEW_TIMEOUT` is a new
    `devclean_core::limits` constant.
  - `get_cached_scan` seeds the id registry from the cache, so the launch
    window between the cached paint and the first scan can no longer produce
    an empty preview.
  - Regression tests: `work_that_overruns_its_budget_fails_instead_of_hanging`,
    `work_that_finishes_in_time_is_returned_untouched`,
    `a_failed_preview_reports_its_own_error`,
    `the_cached_summary_seeds_the_id_registry` (Rust);
    `previewPhase`/`previewCoverage`/`coverageNote` (frontend).
  - G-REL-01's "no external operation may block forever" now covers the
    preview as well as scans and cleanup subprocesses.
- `v0.7.0` — menu bar quick actions + page navigation. **No command was
  added or changed.**
  - `+tray_action` event (`{route, filter?}`), emitted only after the window
    is shown; the frontend validates the route id.
  - Closing the window hides it instead of exiting (behavioural note 5).
  - Menu bar: one click opens a read-only info block (reclaimable, kept,
    top three providers, stale count, low-disk warning) plus the six actions
    above. Numbers come from the scan cache and from free space measured once
    per scan — opening the menu never spawns a process or starts a scan.
  - Regression tests: `the_menu_can_only_do_the_six_reviewed_actions`,
    `chips_are_square_and_actually_paint_something`, `headline_reports_reclaimable_and_kept`,
    `low_disk_only_speaks_up_when_the_disk_is_actually_low` (Rust);
    `keeps destructive calls out of the tray module`,
    `keeps destructive calls out of every page`, `stays out of the way while
    the user types`, `navigates on ⌘1…⌘4` (frontend).
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
  `ScanSummary.blocked_bytes`, `CleanupPreview.preview_token` +
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

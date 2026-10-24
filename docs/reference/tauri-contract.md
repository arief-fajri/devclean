# Tauri Contract (public behavior contract)

> The single document defining what the frontend may see. Changing this file = changing the contract → requires a deliberate review and a regression test. Version: **v0.13.0 (Wave D coverage)**.

## Commands (19)

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
| `get_update_status` | — | `UpdateStatus` | Last updater state, **no network call** — lets Settings render on mount (v0.12.0) |
| `check_for_update` | — | `UpdateStatus` | Asks the release channel for a newer version; **fails after `UPDATE_CHECK_TIMEOUT` (15s)** instead of spinning. A failure is reported as `phase: "error"` (v0.12.0) |
| `install_update` | — | `UpdateStatus` | Downloads + installs the pending update (signature-verified by the backend); emits `update_status` as it progresses. **Refused while a scan or cleanup is running.** Does **not** restart (v0.12.0) |
| `restart_app` | — | `()` | Relaunches into the freshly installed version. The process is replaced, so it normally never returns (v0.12.0) |

## Events (9)

| Event | Payload | When |
|---|---|---|
| `scan_started` | `null` | Scan begins |
| `scan_provider_started` | `string` (provider id) | A provider **begins** — lets the UI show which one is running, not just which are done (v0.5.0). The project-tree providers (the original three at v0.11.0, **seven since v0.13.0**) share one traversal, so they emit together when it begins (v0.11.0) |
| `scan_progress` | `{provider, done, total}` | Each provider finishes; `done` counts completions and is monotonic. The project-tree providers tick together when the shared traversal settles (v0.11.0) |
| `scan_completed` | `ScanSummary` | All providers finished |
| `cleanup_started` | `string[]` (ids) | Cleanup begins |
| `cleanup_progress` | `{done, total}` | One per finished item |
| `cleanup_completed` | `CleanupResult[]` | All ids processed |
| `tray_action` | `{route: string, filter?: string}` | The menu bar asked the window to show a page (v0.7.0). Emitted **after** the window is brought forward. The UI validates `route` against its own page table and ignores anything else |
| `update_status` | `UpdateStatus` | The whole updater state changed — check result, download progress, install, error (v0.12.0). One event, one payload, so the UI never stitches phases together |

## Types (summary; canonical in `crates/core/src/types.rs`)

```ts
type SafetyLevel = "safe" | "caution" | "dangerous";
type CleanupStatus = "success" | "partial" | "failed" | "cancelled";
type DockerPruneScope = "off" | "build-cache" | "build-cache+images" | "all";
/** How a cleanup removes a whole artifact (v0.8.0). */
type DeleteMode = "trash" | "permanent";
type ThemeMode = "system" | "dark" | "light";

/** v0.12.0: phase of the in-app updater. */
type UpdatePhase =
  | "idle" | "checking" | "up_to_date" | "available"
  | "downloading" | "installing" | "ready_to_restart" | "error";

/** v0.12.0: the whole updater state, emitted on every change. */
interface UpdateStatus {
  current_version: string;
  phase: UpdatePhase;
  available_version?: string | null;
  notes?: string | null;
  checked_at_unix?: number | null;
  progress_percent?: number | null;   // whole percent while downloading
  error?: string | null;              // never a silent no-op
}

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
  /** Age in whole days; null for global caches, whose age is meaningless.
   *  v0.10.0: the COMPOSITE age — the most recent of the artifact's mtime,
   *  the repository's last commit and the project directory's mtime. */
  age_days?: number | null;
  /** v0.10.0: how recently the artifact was touched, as a tier. Null when
   *  the scanner could not classify it (a global cache, or a path outside
   *  any repository). The toolbar filters on this, never on `detail`. */
  activity?: Activity | null;
  /** v0.10.0: unix seconds of the containing repository's last commit. */
  last_commit_unix?: number | null;
  /** Why this resource cannot be selected, or null when it can. The backend
   *  refuses any resource carrying a reason, even if its id is sent directly. */
  blocked_reason?: BlockReason | null;
  /** One sentence explaining `blocked_reason`, e.g. "kept by your 30-day
   *  retention policy (this is 12d old)". */
  blocked_detail?: string | null;
}

type Activity = "active" | "recent" | "stale" | "dormant";
// < 7d      7–30      30–90     90+

type BlockReason = "retention" | "ai_read_only" | "privileged_required"
  | "docker_scope" | "protected" | "browser_running";   // +browser_running (v0.13.0)

interface ProviderSummary { provider: string; name: string; total_bytes: number;
  reclaimable_bytes: number; resource_count: number }

interface ScanSummary { total_bytes: number; reclaimable_bytes: number;
  providers: ProviderSummary[]; resources: Resource[];
  /** Bytes the backend will refuse to clean, for ANY reason — retention,
   *  AI report-only, privileged opt-in, Docker scope, git protection,
   *  a running browser. */
  blocked_bytes: number;
  /** v0.13.0: distinct-inode bytes across the WHOLE scan — what deleting
   *  every reported artifact would return, with a hard-linked inode counted
   *  once. Present after a full scan; null whenever it can no longer be
   *  measured (targeted rescan, post-cleanup prune, an older cache).
   *  `total_bytes` keeps meaning "sum of the rows". Carried, never
   *  recomputed here — the inode keys that define it never leave the
   *  backend. */
  unique_bytes?: number | null }

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
  /** How a cleanup removes a whole artifact (v0.8.0, default "trash"). */
  delete_mode: DeleteMode;
  /** Allow cleanup of AI conversation history (v0.8.0, default false). */
  ai_cleanup_enabled: boolean;
  /** Check GitHub Releases at startup (v0.12.0, default false). */
  check_updates_automatically: boolean;
}

interface ProviderInfo { id: string; name: string; enabled: boolean }

interface ScanRootCandidate { path: string; approximate_bytes: number | null;
  cache_count: number; already_configured: boolean }

interface DiskFree { available_bytes: number; total_bytes: number; known: boolean;
  mount: string | null }   // v0.7.3: e.g. "/Volumes/Work"; macOS only, null elsewhere

interface HistoryEntry { run_id?: string; finished_at_unix: number;
  resource_id: string; provider: string; name: string; requested_bytes: number;
  freed_bytes: number; moved_to_trash?: boolean;
  sizing?: "apparent" | "allocated";
  status: CleanupStatus; error?: string }
interface CleanupRun { pre_allocated_sizing: boolean;
  run_id: string; finished_at_unix: number; entry_count: number;
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

1. **`CleanupResult.freed_bytes` is now measured, never assumed.** For a `RunCommand` resource the engine reads the provider's `measure()` before and after the command. Before v0.5.0 it compared against the resource's sentinel path — never a directory — so `after` was always `0` and `freed` always equalled `requested`. A prune that reclaimed 3 GB of a 10 GB cache reported 10 GB. When the after-state cannot be measured at all, `freed_bytes` is `0` and `error` says so.
2. **`scan_progress.done` is monotonic.** It counts providers that *finished* (was: their static position in the provider list, which could run backwards or report `N/N` with work pending).
3. `cleanup` returns `Err` (not a per-item failure) when a required `preview_token` is missing, expired, or covers a different id set.
4. **`preview_cleanup` is bounded (v0.7.2).** Re-measuring walks every selected path; over 120s it fails with an explanation instead of leaving the dialog waiting forever (G-REL-01, which previously covered scans and subprocesses but not this).
5. **A partial preview is reported, not hidden.** `preview_cleanup` silently omitted ids it could not resolve; the UI now shows how many of the selected resources were found, and refuses to present an empty preview as work.
6. **Closing the window hides it (v0.7.0).** The app keeps running in the menu bar; only `Quit DevClean` exits. Before this, closing the last window made the runtime exit (`tauri-runtime-writ` exits on `RunEvent::ExitRequested` when the last window is destroyed), which silently removed the tray icon and made every menu action unreachable.
7. **Navigation is frontend-local.** The page id lives in `$lib/nav` and is never sent to the backend; a reload always starts on Overview. No command was added to support the sidebar.
8. **The Dock icon opens the window (v0.7.3).** Closing the window hides it (note 6), which left the Dock icon on screen and inert: macOS sends `applicationShouldHandleReopen` on a Dock click, but `Builder::run` installs an *empty* event handler (`build()?.run(|_, _| {})`), so nothing acted on it. The shell now runs its own loop and handles `RunEvent::Reopen` by calling the same `show_main` the menu bar uses. The Dock, the tray icon and the menu all reveal the window through one function, so they cannot drift.
9. **Free space is re-measured where it moves (v0.7.3).** The dashboard read `get_disk_free` once at launch, so after a cleanup the card quoted the pre-deletion figure. Both the UI store and the menu bar's cached copy now refresh when a cleanup finishes. `get_disk_free` also stops spawning `df` on the boot path (v0.7.3): it is measured with `statvfs`, and a regression test compares the two so the substitution cannot silently drift.
10. **Startup paints from the cache and rests (v0.9.0).** The app used to run a full scan on every launch, unconditionally: `scan_automatically` (default OFF) was stored and rendered but never read, and the window opened onto a loader nobody asked for. Now the shell paints `get_cached_scan` and runs a scan **only when `scan_automatically` is on** (or when the user clicks Scan Now / Scan again, or a cleanup finished, which re-scans only the providers it touched — see note 12). With no cache the dashboard shows an explicit "You haven't scanned yet" warning instead of scanning on its own. Because the cache is now the resting state, `CachedScan.saved_at_unix` is surfaced so the summary can report the scan's real age and flag a stale cache rather than presenting old numbers as current.
11. **`age_days` is a composite, and a new gate can appear (v0.10.0).** `Resource.age_days` no longer means "project directory age": it is the most recent of the artifact's mtime, the repository's last commit and the project directory's mtime, so a project committed to yesterday stops looking dormant. `Resource.activity` is the tier derived from it. And `blocked_reason: "protected"` can now appear: an artifact that a repository tracks (or a new file it does not yet track) while that repository has uncommitted changes is refused — deleting it would lose work that is committed nowhere. A **gitignored** artifact (`node_modules`) stays cleanable even in a dirty repository. Paths outside any repository, and machines without git, are left exactly as before: `activity` is null and nothing is protected.
12. **A finished cleanup re-scans only the providers it touched (v0.10.0).** It used to run a full scan of every provider, which dimmed the list, disabled selection and made the app feel frozen right after a delete. Now the shell removes the cleaned resources from the list immediately (the backend reported which ids succeeded), so the update does not wait on a scan, and then re-scans only the providers behind those resources, in the background: the list stays interactive, the other providers keep their last-known figures (flagged by the existing "partial re-scan" badge), and the touched providers' rows and the on-disk cache are refreshed. If nothing identifiable was touched, it falls back to a full scan. A full scan is still available from Scan Now / the menu bar.
13. **The project-tree providers share one traversal (v0.11.0).** `node_modules`, `rust_target` and `python_cache` used to walk every scan root apiece and re-walk each artifact for its size. They now contribute rules to one shared walk that matches and sizes in the same pass. **No command, event or type changed** — the rows and their sizes are identical (equivalence-tested) — but the *timing* of their events did: all three emit `scan_provider_started` together when the shared walk begins, and all three emit `scan_progress` together when it settles, rather than each firing as its own walk finished. `done` stays monotonic and `total` still counts every active provider once, so the bar cannot stall (G-SCAN-05). One edge case changes: a `target/` that is itself a symlink is no longer reported (G-SCAN-03 — symlinks are never followed). Decision record: [docs/decisions/single-pass-walk.md](../decisions/single-pass-walk.md).
14. **Updates are in-app, signed, and opt-in (v0.12.0).** Settings gains an **Updates** tab: the running version, a Check for updates button, release notes, a download progress bar, Download & install, and Restart to apply. The payload is verified against a minisign public key committed in `tauri.conf.json` **before** it is installed; the download is signature-checked and the endpoint is HTTPS. Nothing is downloaded until the user asks. `install_update` **refuses while a scan or cleanup is running** — restarting mid-deletion is the one dangerous interaction — and it never restarts on its own; that is a separate button. `Settings.check_updates_automatically` (default **false**) adds a startup check, spawned and bounded, never on the window's critical path. A failed check is reported as `phase: "error"` with the reason. Decision record: [docs/decisions/in-app-updates.md](../decisions/in-app-updates.md).
15. **Seven new provider ids, four of them on the shared walk (v0.13.0).** `get_providers` and scan results now carry `browser_temp`, `llm_models`, `ruby`, `go_vendor`, `flutter`, `android` and `pods` (Providers 14 → 21). The four tree rules ride the Wave C shared walk — one traversal however many rules — so no command or event shape changed; their rows appear under their own ids with their own prefixes (`gv`, `fl`, `ad`, `pd`, `bt`, `lm`, `rb`). Every fixed path list and every exclusion in the wave is recorded in [docs/decisions/wave-d-coverage.md](../decisions/wave-d-coverage.md).
16. **`blocked_reason: "browser_running"` can now appear, and a browser-temp row's safety reports what the probe knows (v0.13.0).** The seven Chromium-family browsers clone their `.app` bundle into `/var/folders/…/X/<bundle-id>.code_sign_clone/` when codesign re-signs a helper; leftovers of a crashed or automated session are reported as `browser_temp`. A **confirmed-running** browser stamps the new reason at scan time, and the cleanup path **re-probes and refuses live** — a stale scan cannot delete the clone of a browser that started meanwhile. A **failed** liveness probe never blocks, but floors the row's safety to `caution`; `NotRunning` is the only state that yields `safe` (G-SAFE-07). Also new under existing ids: node's rule matches `.next`/`.turbo`/`.svelte-kit`/`.expo`, python's matches `.ruff_cache`/`.tox`/`__pypackages__`, `global_cache` grows six targets, and `xcode_data` becomes four rows — including **Archives as the first `dangerous`** (its dSYMs die with it), with `CoreSimulator/Devices` deliberately excluded. Decision records: [browser-temp-probe.md](../decisions/browser-temp-probe.md), [wave-d-coverage.md](../decisions/wave-d-coverage.md).
17. **`ScanSummary.unique_bytes` reports the deduplicated figure (v0.13.0).** After a full scan it is the distinct-inode total: an inode hard-linked into two artifacts counts once, while `total_bytes` keeps summing rows as before. It becomes `null` whenever the figure can no longer be measured — a targeted rescan, a post-cleanup prune, an older scan cache — and the frontend carries it without ever recomputing it (the inode keys never leave the backend). No cache version bump: absence is the honest default.

## Menu bar surface (v0.7.0)

Not an IPC contract, but caller-visible: the menu bar is the only part of DevClean a user can reach when the window is closed.

| Item | Action |
|---|---|
| `open` | Show and focus the window |
| `scan` | `run_scan` in the background — **never** a cleanup |
| `review` | Show the window on the Resources page with the safe filter applied |
| `history` | Show the window on the History page |
| `settings` | Show the window on the Settings page |
| `quit` | Exit the app |

One click of either button opens the menu (`show_menu_on_left_click`); the info block above the actions is rebuilt from the on-disk scan cache, and `open`/`review`/`history`/`settings` additionally emit `tray_action`. There is no destructive item — see G-UI-02 in `docs/development/methodology/guardrails.md`.

## Contract changelog

Contract versions are their own sequence, independent of the product version in `Cargo.toml` — the frontend and backend always ship together, so the contract changes on its own schedule. The two are aligned at 1.0.

- `v0.13.0` — Wave D coverage. **No command or event added, removed or re-typed.** **Types (additive):** `+BlockReason: "browser_running"`; `+ScanSummary.unique_bytes?: number | null` (`#[serde(default)]` → absent is `null`, no `CACHE_VERSION` bump). **Providers 14 → 21:** `+browser_temp`, `+llm_models`, `+ruby`, `+go_vendor`, `+flutter`, `+android`, `+pods`.
  - **Behaviour:** the four new tree rules ride the shared walk, so their rows appear without any command or event change; the new own-scanners surface through `get_providers` and scan results as every provider does. Every path choice and exclusion in the wave is recorded in [docs/decisions/wave-d-coverage.md](../decisions/wave-d-coverage.md).
  - **Behaviour:** expanded rules on existing ids — node matches `.next`/`.turbo`/`.svelte-kit`/`.expo` beside `package.json`; python matches `.ruff_cache`/`.tox`/`__pypackages__`; `global_cache` grows CocoaPods, Yarn, pip, deno, the Dart pub cache and the Dart analysis server cache; `xcode_data` becomes a target list whose **Archives row is the first `dangerous`** in the product (`CoreSimulator/Devices` is deliberately excluded — simulator data is not a cache).
  - **Behaviour:** a confirmed-running Chromium-family browser stamps `blocked_reason: "browser_running"` through `block_reason_for` at scan time, and `cleanup` **re-probes and refuses live** through `refusal_for` (G-SAFE-07): a stale scan cannot delete the clone of a browser that started meanwhile. A FAILED probe never blocks but floors the row's safety to `caution` — `NotRunning` is the only state that yields `safe`. The gate is provider-declared (`Provider::held_while_browsers_run`), never id-matched, so it cannot touch other providers.
  - **Behaviour:** `ScanSummary.unique_bytes` is the distinct-inode total after a FULL scan (`total_bytes` keeps summing rows), and `null` after a targeted rescan, a post-cleanup prune or from an older cache. The frontend carries it and never recomputes it — the inode keys never leave the backend.
  - Regression tests: `the_tree_rules_report_the_expected_resource_set`, `the_shared_walk_reports_exactly_the_sum_of_the_solo_walks`, `no_two_resources_in_one_scan_share_a_path` (equivalence); `every_provider_prefix_is_unique`, `the_tree_cancel_fixture_actually_matches`, `browser_temp_stops_when_cancelled`, `llm_models_stops_when_cancelled`, `ruby_stops_when_cancelled`; `a_running_browser_blocks_the_resource`, `browser_running_outranks_retention`, `every_reason_has_a_label` (core policy); `a_running_browser_refuses_browser_temp_even_after_a_clean_scan`, `the_browser_gate_never_touches_other_providers` (engine); `the_browser_gate_reaches_the_scan`, `the_browser_gate_never_reaches_a_provider_without_the_declaration` (scan policy); `unknown_never_yields_safe`, `not_running_is_the_only_state_that_yields_safe`, `the_floor_never_lowers_a_more_cautious_verdict` (G-SAFE-07); `unique_bytes_counts_a_shared_inode_once_and_a_targeted_scan_reports_none`, `a_cache_written_before_unique_bytes_loads_none`, `a_targeted_rescan_replaces_one_provider_and_keeps_the_rest` (Rust); `blocked.test.ts`, `providerMeta.test.ts`, `scan-merge.test.ts` (frontend).
  - Decision records: [docs/decisions/browser-temp-probe.md](../decisions/browser-temp-probe.md), [docs/decisions/wave-d-coverage.md](../decisions/wave-d-coverage.md).
- `v0.12.0` — in-app updates. **Commands:** `+get_update_status`, `+check_for_update`, `+install_update`, `+restart_app`. **Events:** `+update_status`. **Types (additive):** `UpdateStatus` + `UpdatePhase`; `Settings.check_updates_automatically` (`#[serde(default)]` → `false`).
  - **Behaviour:** the payload is verified against the minisign public key in `tauri.conf.json` before install; the endpoint is HTTPS and the insecure-transport flag is never set (G-UPD-01).
  - **Behaviour:** the check is bounded at 15s and the download at 300s; the opt-in startup check is spawned, never awaited, so it never holds the window (G-UPD-02).
  - **Behaviour:** `install_update` is refused while a scan or cleanup is running, and does not restart the app — `restart_app` is a separate, explicit command (G-UPD-03).
  - **Behaviour:** a failed check or install surfaces `phase: "error"` with the reason, never a silent no-op (G-UPD-04).
  - Regression tests: `update_phase_serializes_to_the_contract_strings`, `idle_update_status_is_empty_but_versioned`, `pre_v012_settings_file_does_not_check_for_updates`, `busy_guard_clears_the_flag_on_drop`, `busy_guard_clears_the_flag_when_unwound`, `an_update_check_that_overruns_is_bounded_and_says_so`, `a_failed_update_check_reports_its_reason` (Rust); `update.test.ts` (frontend).
  - Decision record: [docs/decisions/in-app-updates.md](../decisions/in-app-updates.md).
- `v0.11.0` — single-pass walk. **No command, event or type changed.**
  - **Behaviour:** the three project-tree providers (`node_modules`, `rust_target`, `python_cache`) share one traversal that matches and sizes in the same pass, instead of three walks plus a per-artifact re-walk. The rows and their sizes are identical (equivalence-tested against the pre-refactor output).
  - **Behaviour:** those three now emit `scan_provider_started` together when the shared walk begins and `scan_progress` together when it settles. `done` stays monotonic and `total` still counts every active provider once (behavioural note 13).
  - **Behaviour:** a `target/` that is itself a symlink is no longer reported — matching is `lstat`-only (G-SCAN-03).
  - The scan cache format is unchanged (the rows are the same); no cache version bump.
  - Decision record: [docs/decisions/single-pass-walk.md](../decisions/single-pass-walk.md).
  - Regression tests: the `equivalence` suite (`the_tree_rules_report_the_expected_resource_set`, `measurement_semantics_are_preserved`, `folded_sizing_matches_measuring_each_artifact_directly`, `an_artifact_is_sized_past_the_match_depth_boundary`, `the_shared_walk_reports_exactly_the_sum_of_the_solo_walks`, the two G-SCAN-03 tests), `the_shared_walk_reports_once_per_tree_provider`, `a_cancelled_shared_walk_still_ticks_every_tree_provider`, `own_and_tree_providers_share_one_progress_total`, and the per-provider `cancel_contract` rows.
- `v0.10.0` — git-aware classification. **No command added, removed or re-typed.**
  - `+Resource.activity?: "active" | "recent" | "stale" | "dormant" | null` (additive, `#[serde(default)]`), derived from the composite age. The toolbar filters on it instead of parsing the display string; `lib/format.ts` loses `parseAgeDays`/`isStaleSignal`.
  - `+Resource.last_commit_unix?: number | null` (additive, `#[serde(default)]`) — the containing repository's last commit.
  - **Behaviour:** `age_days` becomes the composite of artifact mtime, last commit and project directory mtime. A project committed to daily no longer looks dormant, so `retention_days` stops offering an active project's cache.
  - **Behaviour:** `blocked_reason: "protected"` can now appear. `BlockReason::Protected` (present since v0.8.0 with no producer) is stamped when an artifact is tracked — or untracked and not ignored — in a repository with uncommitted changes, and refused by the engine. It is checked **first**, ahead of the AI and retention gates. A gitignored artifact stays cleanable even in a dirty repository.
  - **Behaviour:** the scan cache is versioned `2 → 3`; a v2 cache (mtime-only age) is rejected rather than shown.
  - **Behaviour:** a finished cleanup re-scans only the providers it touched instead of every provider, in the background and without flipping the global scanning state. `scan_provider` also updates the on-disk cache for that provider (preserving the cache's timestamp), so a restart does not resurrect cleaned resources.
  - Decision record: [docs/decisions/gitignore-aware-protection.md](../decisions/gitignore-aware-protection.md).
  - Regression tests: `a_tracked_artifact_in_a_dirty_repo_is_protected`, `a_gitignored_artifact_stays_cleanable_in_a_dirty_repo`, `a_clean_repo_protects_nothing`, `an_untracked_file_in_a_dirty_repo_is_protected`, `last_commit_beats_a_stale_mtime`, `activity_thresholds_at_the_boundaries`, `a_path_outside_any_repo_is_not_classified`, `one_check_ignore_call_per_repo` (Rust); `isStaleActivity` (frontend).
- `v0.9.0` — cache-first startup. **One command re-typed; no command added or removed.**
  - `get_cached_scan` now returns `CachedScan | null` (`{ summary, saved_at_unix }`) instead of `ScanSummary | null`, so the UI can report the scan's real age. `devclean_core::CachedScan` and `devclean_scanner::load_cached_scan` are new; `load_cached_summary` stays for the menu bar, which reads the file's mtime for age.
  - **Behaviour:** a scan runs on launch only when `Settings.scan_automatically` is on. The shell paints the cache and rests; with no cache it shows a "You haven't scanned yet" warning. Previously the launch scan was unconditional and `scan_automatically` was a control with no effect.
  - **Behaviour:** the summary reports the cached scan's age (from `saved_at_unix`) and flags a cache older than 7 days, instead of stamping the paint with the launch time.
  - Regression tests: `the_cache_carries_the_time_it_was_saved` (Rust), `the_cached_summary_seeds_the_id_registry` (now asserts `saved_at_unix`), and `cache-first startup` (frontend: the launch scan is gated, the cached paint derives its age).
- `v0.8.0` — honest sizes, recoverable deletes, and gates that actually run. **No command added, removed or re-typed.** Three coupled changes; the third fixes a control that had never worked.

**1. Sizes that match `du`.**
  - `Resource.size_bytes` now means **allocated blocks** (`st_blocks * 512`), i.e. what deleting the resource frees — it was a sum of file *lengths*, which read up to 19% low on trees full of small files and up to 145% high on trees full of hard links (measured: this repo's own `target/` reported 26.6 GB against `du`'s 10.8 GB). Hard-linked inodes are counted once per artifact via `(dev, ino)`.
  - `+Resource.apparent_bytes?: number | null` (additive, `#[serde(default)]`) — the logical size, present only when materially larger than `size_bytes`, so sparse files can be shown as `8.6 GB (appears as 460 GB)`.
  - The on-disk scan cache is versioned `1 → 2`: a v1 cache holds logical sizes and is rejected rather than displayed.
  - Decision record: [docs/decisions/allocated-vs-apparent.md](../decisions/allocated-vs-apparent.md).

**2. Recoverable deletes.**
  - `+Settings.delete_mode: "trash" | "permanent"` (additive, `#[serde(default)]` → `trash`). Path removal moves the artifact to `~/.Trash` by default instead of deleting it.
  - `+CleanupResult.moved_to_trash: boolean`, `+HistoryEntry.moved_to_trash` (additive). **Under `trash`, `freed_bytes` is 0**: the space is still on disk and returns only when the user empties the Trash.
  - The mode governs path removal only — `RunCommand` and `RunSql` have no undo and are never reported as trashed.
  - Decision record: [docs/decisions/trash-by-default.md](../decisions/trash-by-default.md).

**3. Blocked reasons — a control that never ran, fixed.**
  - `Resource.retention_eligible: boolean` is **replaced** by `blocked_reason?: BlockReason | null` + `blocked_detail?: string | null` (breaking to the type; nothing else consumed the old field).
  - **`retention_eligible` was never set to `false` in production code**, so `Settings.retention_days` had no effect on any cleanup and `ScanSummary.blocked_bytes` was permanently zero. With a 10-year policy the gate now blocks every resource it should; before it, the same policy blocked **none**.
  - `+Settings.ai_cleanup_enabled: boolean` (additive, `#[serde(default)]` → `false`). AI conversation history is reported with sizes but not selectable until the user opts in (`blocked_reason: "ai_read_only"`).
  - `+HistoryEntry.sizing?: "apparent" | "allocated"` and `+CleanupRun.pre_allocated_sizing: boolean` (additive). Rows written before v0.8.0 read as `"apparent"`; the history page labels those runs "before 0.1.0".
  - The scan stamps the reason; the engine refuses what is stamped. Both call the same `devclean_core::policy::block_reason_for`, so the explanation the user reads is the one that is enforced (I11).
  - Decision record: [docs/decisions/blocked-reason.md](../decisions/blocked-reason.md).

**Regression tests:** `allocated_counts_blocks_not_payload`, `an_empty_directory_measures_zero`, `a_hardlinked_file_is_counted_once_within_an_artifact`, `apparent_is_reported_only_when_materially_larger`, `a_v1_cache_with_logical_sizes_is_rejected`, `trash_mode_moves_aside_and_reports_pending_not_freed`, `permanent_mode_removes_and_reports_freed`, `command_backed_actions_are_never_marked_as_trashed`, the `devclean_core::policy` suite, `retention_and_ai_gates_now_reach_the_scan`, `blocked_ids_are_collected_for_the_engine` (Rust); `cleanupOutcome` and `blocked` suites (frontend).
- `v0.7.2` — bounded cleanup preview. **No command added, removed or re-typed.**
  - `preview_cleanup` now fails with an explanation after `PREVIEW_TIMEOUT` (120s) rather than blocking indefinitely; `PREVIEW_TIMEOUT` is a new `devclean_core::limits` constant.
  - `get_cached_scan` seeds the id registry from the cache, so the launch window between the cached paint and the first scan can no longer produce an empty preview.
  - Regression tests: `work_that_overruns_its_budget_fails_instead_of_hanging`, `work_that_finishes_in_time_is_returned_untouched`, `a_failed_preview_reports_its_own_error`, `the_cached_summary_seeds_the_id_registry` (Rust); `previewPhase`/`previewCoverage`/`coverageNote` (frontend).
  - G-REL-01's "no external operation may block forever" now covers the preview as well as scans and cleanup subprocesses.
- `v0.7.0` — menu bar quick actions + page navigation. **No command was added or changed.**
  - `+tray_action` event (`{route, filter?}`), emitted only after the window is shown; the frontend validates the route id.
  - Closing the window hides it instead of exiting (behavioural note 6).
  - Menu bar: one click opens a read-only info block (reclaimable, kept, top three providers, stale count, low-disk warning) plus the six actions above. Numbers come from the scan cache and from free space measured once per scan — opening the menu never spawns a process or starts a scan.
  - Regression tests: `the_menu_can_only_do_the_six_reviewed_actions`, `chips_are_square_and_actually_paint_something`, `headline_reports_reclaimable_and_kept`, `low_disk_only_speaks_up_when_the_disk_is_actually_low` (Rust); `keeps destructive calls out of the tray module`, `keeps destructive calls out of every page`, `stays out of the way while the user types`, `navigates on ⌘1…⌘4` (frontend).
- `v0.6.0` — run-level history + targeted provider scan.
  - `+get_history_runs` → `CleanupRun[]`; `HistoryEntry.run_id?` (`#[serde(default)]`, so every pre-v0.6 history line still reads — legacy entries are grouped by `finished_at_unix`).
  - `scan_provider` now scans **only** the requested provider instead of a full scan plus a filter, and merges the result into `AppState::resources` so the other providers' id→path entries survive.
  - Regression tests: `a_huge_run_no_longer_hides_earlier_runs`, `legacy_lines_without_run_id_are_grouped_by_timestamp`, `tail_read_handles_multi_chunk_files_and_straddling_lines`, `run_totals` (frontend).
- `v0.5.0` — retention, AI data, backend-enforced gates. **Commands:** `+detect_scan_roots`, `+get_disk_free`; `cleanup` gains `preview_token?`. **Events:** `+scan_provider_started`. **Types (all additive, `#[serde(default)]` so older `settings.json` and scan caches still load):** `Resource.age_days`, `Resource.retention_eligible`, `ScanSummary.blocked_bytes`, `CleanupPreview.preview_token` + `expires_at_unix`, `Settings.retention_days`, `Settings.docker_prune_scope`, `Settings.sqlite_backup`. **Behaviour:** measured `freed_bytes`; monotonic `scan_progress.done`; backend now enforces confirmation, retention and Docker scope. Providers 9 → 14 (`+docker_pruned`, `+global_cache`, `+opencode_fs`, `+opencode_db`, `+claude`). Regression tests: `old_cached_scans_without_impact_still_deserialize`, `pre_v04_settings_file_without_disabled_providers_enables_everything`, `empty_scan_roots_are_reseeded_from_defaults`, `cleanup_rejects_without_a_valid_preview_token`, `cleanup_rejects_expired_preview_token`, `retention_blocked_resource_is_refused_by_the_engine`, `privileged_scope_is_refused_until_the_user_opts_in`, `runcommand_freed_bytes_is_measured_not_assumed`, `progress_done_counts_completions_and_is_monotonic`, `provider_started_fires_once_per_enabled_provider`, `the_cutoff_is_computed_in_milliseconds_to_match_the_schema`.
- `v0.4.0` — Provider toggles: `+get_providers` → `ProviderInfo[]`, and `Settings.disabled_providers`. `cleanup_progress` fires **once per finished item**.
- `v0.3.0` — UI polish: `Resource.impact?`, additive.
- `v0.2.0` — `+get_history`, `+get_cached_scan`, `Settings.theme`.
- `v0.1.0` — initial 9 commands + 6 events.

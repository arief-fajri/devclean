# Guardrails (lean edition)

> Constraints that keep DevClean out of unacceptable states.
> - A **test** answers: *did we observe the expected behavior?*
> - A **guardrail** answers: *what behavior is never allowed?*
>
> Every entry names its **enforcement mechanism** — without enforcement, a
> guardrail is just a suggestion. System model: [system-model.md](./system-model.md).

## Reading the enforcement column

- **code** — enforced by program logic · **config** — enforced by shipped defaults ·
- **test** — covered by an automated test · **CI** — enforced by a CI job ·
- **process** — enforced by the DoD/review workflow · `(gap)` — missing, tracked honestly

## 1. Safety (G-SAFE) — the life of this product

```text
G-SAFE-01  The frontend must not supply paths for deletion; resource_id only.
           Enforcement: code — commands resolve id→path server-side via state;
           test — resolve_rejects_unknown_id ✅
G-SAFE-02  No shell commands from user input; only fixed provider (program, args) pairs.
           Enforcement: code — CleanupAction::RunCommand enum; review ✅
G-SAFE-03  Significant destructive operations are not defaults (volumes, venvs, images).
           Enforcement: config+code — `docker_prune_scope` defaults to `build-cache`
           and anything wider additionally requires `allow_privileged_operations`; the
           CLEANUP ENGINE refuses it (not just the UI) ✅ ·
           test — privileged_scope_is_refused_until_the_user_opts_in,
           docker_scope_off_refuses_even_without_the_privileged_flag ✅
G-SAFE-05  Configuration and credentials are never deleted by an age-based prune,
           whatever the glob matches (I10).
           Enforcement: code — `CleanupAction::PruneOlderThan.skip_names` ·
           test — a_skip_list_beats_a_broad_pattern ✅
G-SAFE-04  Protected paths are untouchable from any direction (==, inside, ancestor).
           Enforcement: code+test — SafetyValidator, 7 tests ✅
```

## 2. Scan (G-SCAN)

```text
G-SCAN-01  Scan concurrency is bounded (max 4 providers in parallel).
           Enforcement: code — tokio Semaphore in ScanManager ✅
G-SCAN-02  Scan & cleanup are cancellable, settling in <1s.
             Enforcement: code — cooperative AtomicBool checks per walk entry ✅ ·
             test — cancel_settles_in_under_one_second ✅
G-SCAN-03  Symlinks are never followed during traversal/measurement.
             Enforcement: code — follow_links(false) ✅
G-SCAN-05  Progress counters count COMPLETIONS, never list positions, and fire on
            every exit path (success, cancel, timeout, panic).
            Enforcement: code — `AtomicUsize` completion counter in `ScanManager` +
            a `report` closure that always fires ✅ ·
            test — progress_done_counts_completions_and_is_monotonic,
            cancelled_providers_still_emit_progress ✅
G-SCAN-06  A scan that silently reports less than the truth is a bug, not a
            cosmetic one. Every parser used for a reported size returns an
            explicit "unmeasurable" rather than defaulting to zero.
            Enforcement: code — `Provider::measure() -> Option<u64>` ✅ ·
            test — unparseable_lines_are_skipped_not_fatal,
            parses_the_percentage_suffix_docker_actually_emits ✅
G-SCAN-04  The UI thread is never blocked by filesystem work (scan re-measure,
            deletion, provider CLI). Enforcement: code — `cleanup` and
            `preview_cleanup` are async + `spawn_blocking`, so no sync command
            touches the Tauri main thread ✅ · manual — UI stays responsive
            and the cleanup dialog animates progress ✅
```

## 3. Contract (G-API)

```text
G-API-01  Command/event/response changes require a tauri-contract.md update + test.
           Enforcement: process — DoD step 2, review ✅
G-API-02  Cleanup status/error codes are deterministic per resource (I8).
           Enforcement: code — CleanupResult {status, freed, error} ✅
```

## 4. Reliability (G-REL)

```text
G-REL-01  No external operation may block forever.
            Enforcement: code — spawn_blocking + stdin-less commands ✅ ·
            explicit timeouts: PROVIDER_SCAN_TIMEOUT (60s) + CLEANUP_COMMAND_TIMEOUT
            (120s, child killed) ✅ · test — provider_timeout + command_timeout ✅
G-REL-02  Partial failure is transparent to the user.
           Enforcement: code — engine computes freed = before−after per resource ✅
G-REL-03  `freed_bytes` is a MEASUREMENT, never an assertion (I12). Every branch of
           the engine derives it from `Provider::measure()` before and after; when
           the after-state is unmeasurable the result is 0 plus a reason.
           Enforcement: code+test ✅ ·
           test — runcommand_freed_bytes_is_measured_not_assumed,
           runcommand_that_frees_nothing_reports_zero_not_the_requested_figure,
           unmeasurable_after_state_reports_zero_and_a_reason,
           refused_path_reports_zero_freed_not_the_requested_figure ✅
G-REL-04  No gate lives only in the UI (I11). Confirmation, retention and the
           Docker scope are refused by the cleanup engine.
           Enforcement: code — `engine.execute_gated` + `CleanupPolicy` ·
           test — retention_blocked_resource_is_refused_by_the_engine,
           privileged_scope_is_refused_until_the_user_opts_in,
           ordinary_providers_are_never_touched_by_the_docker_gates ✅
```

## 5. UI (G-UI)

```text
G-UI-01  Significant cleanup requires a confirmation dialog + re-measured preview.
           Enforcement: code — `preview_cleanup` issues a `preview_token` bound to
           the id set with a 300s TTL; `cleanup` REFUSES without a valid one while
           `require_confirmation` is on ✅ ·
           test — authorize_rejects_missing/expired/mismatched tokens ✅
G-UI-02  The menu bar may read and navigate, never clean. "Review safe items" opens
           the list with a filter; it does not select, preview or delete anything.
           Enforcement: code — `on_menu_event` refuses any id outside
           `MENU_ACTION_IDS` (tray.rs), and the list is read-only by construction:
           the numbers come from the scan cache, free space from one probe per scan ·
           test — the_menu_can_only_do_the_six_reviewed_actions (Rust) and
           keeps_destructive_calls_out_of_the_tray_module (frontend) ✅
G-UI-03  The cleanup confirmation is an interruption, not a page. It is mounted by
           the shell, over every page, and swallows shell shortcuts while it is open.
           Enforcement: code — `App.svelte` blocks `resolveHotkey` while the flow is
           open; test — keeps_destructive_calls_out_of_every_page and
           mounts_the_confirmation_flow_from_the_shell (frontend) ✅
```

### What changed in v0.5.0, and why the claims above changed

Every entry above names a mechanism that a test can fail against. Three of them
previously named mechanisms that did not exist:

| Was | Now | Trigger |
|---|---|---|
| "G-UI-01 enforced by code" | enforced by `preview_token` in the engine | the old enforcement was a `tracing::debug!` |
| v0.7.0 UI shell revamp (G-UI-02/03) | the menu-bar id allowlist + the page-level scan of `routes/*.svelte` | converting the dialogs into pages could have put a cleanup call one click from the list |

## Open gaps

- **G-SAFE-06 — the confirmation token is a workflow gate, not an
  authentication boundary.** A fully compromised frontend can mint its own
  preview and then call `cleanup`. What the token buys is real: an id set
  that was never shown to the user is refused, and numbers cannot go stale
  between preview and execute. What it cannot buy is protection from a
  hostile UI, which would need an out-of-process frontend.

- **G-REL-05 — scan-time CLI probes have no timeout.** `run_tool` (provider
  queries) and `go_env` spawn processes without a deadline, and
  `tokio::time::timeout` around a `spawn_blocking` task does not kill the
  blocking thread — it only stops awaiting. Bounded in practice by
  `PROVIDER_SCAN_TIMEOUT` returning, but a hung probe leaves a thread parked.

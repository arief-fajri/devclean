# Guardrails

> An engineering note: the constraints that keep DevClean out of unacceptable states. It describes how the project is built rather than how to use it.
> - A **test** answers: *did we observe the expected behavior?*
> - A **guardrail** answers: *what behavior is never allowed?* Every entry names its **enforcement mechanism** — without enforcement, a guardrail is just a suggestion. System model: [system-model.md](./system-model.md).

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
G-SAFE-07  A failed liveness probe fails toward MORE caution, never less
           (Wave D4). `ProcState::Unknown` from the browser probe never
           reads as `NotRunning`: it floors the browser-temp row's safety
           to `caution` — never `safe` — and `NotRunning` is the only
           state that yields `safe`. A failed probe also never REFUSES
           (only a confirmed `Running` does — an over-eager refusal would
           make the feature useless); the caution half is the floor.
           Enforcement: code — `browser_temp::safety_for` /
           `floor_safety` (a pure function the scan stamps through) ·
           test — unknown_never_yields_safe,
           not_running_is_the_only_state_that_yields_safe,
           the_floor_never_lowers_a_more_cautious_verdict ✅
```

## 2. Scan (G-SCAN)

```text
G-SCAN-01  Scan concurrency is bounded (max 4 providers in parallel).
           The shared project-tree walk counts as ONE task, however many
           rules ride it.
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
            cancelled_providers_still_emit_progress,
            the_shared_walk_reports_once_per_tree_provider,
            a_cancelled_shared_walk_still_ticks_every_tree_provider ✅
G-SCAN-07  The project-tree providers share ONE traversal. A rule matches only
            within its own max_depth, but a matched artifact is always sized in
            full — depth gates the match, never the size — and away from any
            match the walk prunes at the deepest rule's max_depth.
            Enforcement: code — depth gate + `Scope` override + prune in
            `walk_tree_rules` ✅ ·
            test — the_tree_rules_report_the_expected_resource_set,
            an_artifact_is_sized_past_the_match_depth_boundary,
            folded_sizing_matches_measuring_each_artifact_directly,
            the_shared_walk_reports_exactly_the_sum_of_the_solo_walks ✅
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
G-REL-05  Scan-time CLI probes carry a deadline (closed in Wave B).
           `run_tool`, `go_env`, `pgrep` and the git calls all go through
           `devclean_providers::exec::run_with_deadline`, which kills the child
           when `PROVIDER_COMMAND_TIMEOUT` (30s) fires. Before this they spawned
           with no timeout and a hung probe parked a thread.
           Enforcement: code — one helper every provider subprocess uses ·
           test — a_hung_child_is_killed_at_the_deadline,
           a_missing_program_is_none_not_a_panic ✅
```

## 5. Publication (G-PUB)

```text
G-PUB-01  A platform claim in README.md must be backed by CI, and
           tauri.conf.json must build no artifact for an untested platform.
           Enforcement: test — apps/desktop/src/lib/platform-claim.test.ts ✅ ·
           CI — the frontend job runs `npm test` ✅
G-PUB-04  Every local image in README.md comes from `demo/` and exists on disk.
           A screenshot of a real machine leaks project names and sizes into a
           public repository; a reference to a file nobody generated renders a
           broken image.
           Enforcement: test — platform-claim.test.ts ·
           script — demo/make-fixture.sh produces the environment ✅
```

## 6. UI (G-UI)

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
G-UI-04  The menu bar icon is a real template image: it must carry an alpha
           channel, stay centred, and never fill its frame. macOS paints a
           template image from alpha alone — a PNG without one is a solid block,
           which is exactly how the tray icon shipped as a white square.
           Enforcement: asset — `tools/make_tray_icon.py` writes the PNG *and* the
           SVG from one geometry, so they cannot drift;
           test — tray-icon.test.ts decodes the shipped PNG and asserts an alpha
           channel, transparent corners, a non-slab coverage, fill and centring ✅
G-UI-05  A clickable surface must look clickable. Tailwind v4 dropped v3's
           preflight rule for pointer cursors, so every button inherited the
           OS default arrow and the app gave no affordance at all.
           Enforcement: code — one `:where(...)` rule in `app.css` under
           `@layer base` (layered, so a utility such as `cursor-default` can
           still opt out; `:where` keeps specificity at 0 so `.control` keeps
           winning); disabled controls get `cursor: not-allowed` instead of a
           silent dead arrow;
           test — architecture.test.ts asserts the rule is still declared, that
           no surface smuggles `cursor-default` in (only the modal backdrop may),
           that nobody re-adds a hand-written `cursor-pointer`, and that every
           button either has a handler or `disabled`, so the blanket selector is
           not advertising a promise the app does not keep ✅
G-UI-06  Every affordance the OS gives us must work. Hiding the window on
           close keeps DevClean in the menu bar, but it also means the Dock
           icon is on screen doing nothing — a bug with no error, no log and
           no failing test, because the event simply had no listener.
           Enforcement: code — `show_main` is the single reveal path
           (`pub(crate)`), called by the menu bar, the tray icon and the Dock;
           `lib.rs` matches `RunEvent::Reopen`;
           test — tests::the_dock_reopen_event_is_handled fails if the arm is
           deleted *or* if the shell goes back to `Builder::run`;
           tests::every_reveal_path_goes_through_one_function fails if anyone
           reaches for the window outside `show_main` ✅
G-REL-06  Nothing on the boot path may spawn a process without a deadline.
           `get_disk_free` used to run `df` while the window loaded its
           settings: no timeout, no way to report progress, and a hang there
           would have held the whole startup. It is measured with `statvfs`
           instead. The general form of this — unbounded provider probes — is
           now closed too as G-REL-05 (Wave B).
           Enforcement: code — `disk::report()` is the only free-space source
           and it makes no `Command`; test — `statvfs_agrees_with_df` keeps the
           substitute honest against the `df` it replaced ✅
```

## 7. CI & AI (G-CI / G-AI)

```text
G-CI-01  Merging requires: cargo test + clippy 0 warnings + clean fmt +
           svelte-check 0 errors + vitest.
           Enforcement: CI — the `rust` job runs the Rust half on macOS and
           Ubuntu; the `frontend` job runs svelte-check and vitest on Ubuntu ✅
G-CI-02  Every guardrail enforced by a test must run in CI. A test nobody runs
           is a suggestion.
           Enforcement: CI — the `frontend` job runs `npm test`, so
           G-UI-02/03/04/05 and G-PUB-01/04 are actually executed ✅ ·
           test — platform-claim.test.ts asserts that `npm test` is in the
           workflow ✅
G-CI-03  A test may not depend on the machine's state: the contents of $HOME,
           whether an external tool is installed, the user's git configuration,
           or filesystem case-sensitivity. A fixture states everything it needs
           rather than reading it from the machine.
           Enforcement: test — test-hygiene.test.ts scans crates/*/tests and
           fails on `default_scan_roots()`, `app_data_dir()`, `home_dir()` or
           `default_trash_dir()` outside a named exception ✅ ·
           test — disk::the_app_data_directory_really_is_on_the_home_volume
           resolves to the nearest EXISTING ancestor instead of assuming the
           directory exists ✅ · test — policy_evidence.rs uses stub providers
           rather than scanning the real machine ✅
G-AI-01  An automated change is for internal & reversible work only. Contract,
            security, destructive and dependency changes require explicit human
            approval before implementation, not after. See AGENTS.md.
            Enforcement: process ✅
```

## 8. Updates (G-UPD)

```text
G-UPD-01  Update payloads are signature-verified before install, over HTTPS.
            The minisign public key is committed; the private key is a repo
            secret; `dangerousInsecureTransportProtocol` is never set, so a
            plaintext endpoint cannot be used.
            Enforcement: config — `plugins.updater.pubkey` + an HTTPS endpoint
            in tauri.conf.json, no insecure-transport flag ✅ ·
            code — verification lives in tauri-plugin-updater and cannot be
            disabled ✅ ·
            test — update.test.ts keeps the frontend off the plugin API ✅
G-UPD-02  The update check and download are bounded, and an opt-in startup
            check never blocks the window.
            Enforcement: code — `UPDATE_CHECK_TIMEOUT` (15s) via
            `bounded_update`, `UPDATE_DOWNLOAD_TIMEOUT` (300s) around the
            download; the startup check is spawned, never awaited ✅ ·
            test — an_update_check_that_overruns_is_bounded_and_says_so ✅
G-UPD-03  Installing an update is an explicit user action, is refused while a
            scan or cleanup is running, and never restarts the app on its own.
            Enforcement: code — `install_update` refuses while `op_running`
            (an RAII `BusyGuard` set by `run_scan`/`cleanup`); install and
            `restart_app` are separate commands ✅ ·
            test — busy_guard_clears_the_flag_on_drop,
            busy_guard_clears_the_flag_when_unwound (Rust) ·
            update.test.ts — install does not restart (frontend) ✅
G-UPD-04  A failed check or install is reported, never a silent no-op.
            Enforcement: code — every failure becomes `UpdatePhase::Error`
            carrying the reason ✅ ·
            test — a_failed_update_check_reports_its_reason ✅
```

## Open gaps

- **G-SAFE-06 — the confirmation token is a workflow gate, not an authentication boundary.** A fully compromised frontend can mint its own preview and then call `cleanup`. What the token buys is real: an id set that was never shown to the user is refused, and numbers cannot go stale between preview and execute. What it cannot buy is protection from a hostile UI, which would need an out-of-process frontend. Accepted as a limitation rather than chased — see the [roadmap](../roadmap.md).

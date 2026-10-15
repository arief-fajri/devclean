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
           still open as G-REL-05; what is closed is the app's own boot path.
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

### What G-CI-03 is for, and how to satisfy it

Twice a test passed locally and failed on a clean CI runner, because it asserted something about *the machine* while its name claimed to assert something about the product:

| Test | What it really asserted |
|---|---|
| `disk::the_app_data_directory_really_is_on_the_home_volume` | "this machine has run DevClean" — `app_data_dir()` does not exist until it has |
| `policy_evidence::retention_and_ai_gates_now_reach_the_scan` | "this machine has AI history" — a runner has never run opencode or Claude |

Neither was visible from a green local run. Both are only found on a machine that has never been used.

**Writing a test that satisfies it:**

- Build the thing you are testing. `ScanManager::with_config` takes any `Provider`, so a stub can exercise the scan pipeline without a real directory existing.
- Compare against a fixture you created in this test, never against what happens to be on disk.
- When a path may not exist yet, resolve to the nearest existing ancestor rather than assuming.

**Git fixtures (Wave B of the roadmap):** every setting a fixture depends on must be passed explicitly, never read from the machine's configuration. A runner has no global git identity, and a user's `core.excludesFile` or `core.fileMode` changes what `check-ignore` and `status` report.

```rust
git -c init.defaultBranch=main init
git -c user.name=test -c user.email=test@example.invalid commit -m "..."
git -c core.excludesFile=/dev/null -c core.fileMode=false check-ignore <path>
```

### Why these guardrails exist

Every entry above names a mechanism a test can fail against. That is the whole point of the file, and it was learned the hard way: several controls shipped with prose that named an enforcement they did not have.

| Was | Now | What caught it |
|---|---|---|
| "G-UI-01 enforced by code" | enforced by `preview_token` in the engine | the old enforcement was a `tracing::debug!` |
| the UI shell revamp could put a cleanup call one click from the list (G-UI-02/03) | the menu-bar id allowlist + the page-level scan of `routes/*.svelte` | reading the diff |
| the tray icon painted a white square (G-UI-04) | the asset is decoded and measured in CI, and generated from one geometry | the earlier PNG was written as greyscale *without* an alpha channel, so macOS painted the whole frame; nothing in Rust, the build or a screenshot diff caught it |
| `retention_eligible` was never set to `false` in production, so `Settings.retention_days` had no effect and `retention_blocked_bytes` was permanently 0 | `blocked_reason` stamped at scan time by `devclean_core::policy::block_reason_for`, refused by the engine from the same function | a 10-year policy now blocks every resource it should; before, it blocked none |
| `freed_bytes` for command-backed cleanup always equalled `requested` | `before − after`, both measured; unmeasurable reports 0 with a reason | `runcommand_freed_bytes_is_measured_not_assumed` |
| the Trash mode did not exist, so every delete was permanent | `delete_mode` defaults to `trash`; `freed_bytes` is 0 for a trashed artifact | `trash_mode_moves_aside_and_reports_pending_not_freed` |
| the frontend tests — including G-UI-02/03/05 — were never run by CI | the `frontend` job runs `npm test` | `platform-claim.test.ts` asserts it |

G-CI-01 and G-AI-01 were dropped once while this file was restructured, and nothing noticed — a guardrail document is not covered by a test that fails when an entry disappears. They are restored above. It is the same failure class as everything else here: a control that existed only as prose stopped existing, and the only reason it was found is that a CI job failed and sent someone looking.

## Open gaps

- **G-SAFE-06 — the confirmation token is a workflow gate, not an authentication boundary.** A fully compromised frontend can mint its own preview and then call `cleanup`. What the token buys is real: an id set that was never shown to the user is refused, and numbers cannot go stale between preview and execute. What it cannot buy is protection from a hostile UI, which would need an out-of-process frontend. Accepted as a limitation rather than chased — see the [roadmap](../roadmap.md).

- **G-REL-05 — scan-time CLI probes have no timeout.** `run_tool` (provider queries) and `go_env` spawn processes without a deadline, and `tokio::time::timeout` around a `spawn_blocking` task does not kill the blocking thread — it only stops awaiting. Bounded in practice by `PROVIDER_SCAN_TIMEOUT` returning, but a hung probe leaves a thread parked. Planned to close in Wave B of the [roadmap](../roadmap.md), which adds git subprocesses that need a deadline helper anyway.

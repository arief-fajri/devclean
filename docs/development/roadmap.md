# DevClean Roadmap

> MVP focus: **M0 → M1+M2**. M3+ is backlog — don't start it before M2 is
> stable (stability rule: no new providers before the scanner & cleanup are
> stable).

## M0 — Foundation ✅ (done at init)

```text
[x] Cargo workspace (core/providers/scanner/cleanup) + Tauri 2 + Svelte 5 + Tailwind
[x] SafetyValidator + protected paths + 7 tests
[x] Provider trait + opaque-id registry
[x] Concurrent ScanManager + CleanupEngine + 9 commands + 6 events
[x] Docs: README, AGENTS, system-model, guardrails, tauri-contract, developing, roadmap, security
[x] CI workflow (clippy + test + svelte-check)
```

Exit: `cargo tauri dev` opens, `cargo test` is green, docs are complete.

## M1+M2 — Scanner + safe cleanup ✅ (done)

```text
[x] Svelte dashboard: total reclaimable + per-provider bars + scan progress
[x] Per-provider drill-down (project list + age/stale signal)
[x] Preview → ConfirmDialog → cleanup → result flow (bytes freed + before/after)
[x] Transparent per-resource errors + scan/cleanup cancel from the UI
[x] Validate the 4 providers on a real machine (node, rust, go, docker-builder)
[x] CI workflow + svelte-check gate
```

Exit: the user can reclaim real storage safely end-to-end, with no crashes
and no deletions outside scan roots (evidence: demo + tests).

Evidence: node fixture found + freed 3002/3002 B with source intact;
rust_target found; go build/module caches read (~4.0/4.3 GB); docker
build cache read (~10.8 GB). Gates: `cargo test` 13 passed, clippy 0
warnings, fmt clean, `svelte-check` 0 errors 0 warnings, `vite build` ok.
Contract unchanged (still 9 commands + 6 events v0.1.0).

## M3 — Safety hardening + UX ✅ (done)

```text
[x] Explicit per-provider timeouts + cancel <1s test (closes gaps G-SCAN-02/G-REL-01)
[x] Persistent settings + scan cache file + cleanup history (JSONL, not SQLite)
[x] System tray + restrained notifications (never adware)
[x] Final dark/light toggle, project ignore list
[x] More providers: npm/pnpm/yarn cache, Python, Cargo registry, Gradle, Xcode
[x] Smart stale detection (>90 days) — a signal only, never a safety verdict
```

Exit: no operation blocks forever, state survives restarts, 9 providers
cover the common stacks, tray + history round out daily use.

Evidence: cancel settles <1s (test), provider timeout 60s + command
timeout 120s with kill (tests); settings/cache/history live under the
per-platform app data dir; 9/9 providers scanned live (npm, python
fixture with venv invisible, cargo registry ~443 MB Caution, xcode,
gradle absent → empty); contract v0.2.0 (`+get_history`,
`+get_cached_scan`, `Settings.theme`).

## UI polish — Phase 1+2 ✅ (done)

```text
[x] Summary panel (safe-to-clean split, share %, Scan again as primary) + stat tiles
[x] lucide-svelte icon set, tabular numerals, max-w-5xl (xl:max-w-7xl), 2-line resource rows
[x] Global search/filter/sort toolbar + selection-aware cleanup bar
[x] Resource.impact (contract v0.3.0, additive, old caches still load) +
    ConfirmDialog + ResultPanel
[x] vitest: 20 frontend tests (view, format, safety, providerMeta)
```

Evidence: `cargo test` 41 passed, clippy 0 warnings, fmt clean,
`svelte-check` 0 errors 0 warnings, `vite build` ok, `vitest` 20 passed.

## UI fixes — 12 reported issues ✅ (done)

```text
[x] History modal refresh icon → lucide; no emoji left in src/
[x] Provider expand fixed: collapsed = summary only, real accordion toggle
    (root cause: preview === resources made expand a visual no-op)
[x] Removed the redundant "Up to date" pill (summary panel already shows it)
[x] Uniform control height (.control token) so select/buttons align
[x] Toolbar compacted from 3 rows to 2
[x] Filesystem path shown per item (+ Paths toggle) — what actually gets deleted
[x] Settings fully in English; guardrail codes (G-*/I*) removed from UI copy
[x] Provider on/off toggles (contract v0.4.0: +get_providers,
    Settings.disabled_providers, ScanManager filter)
[x] Sticky header
[x] Full-width content on wide windows (window default 1120x780)
[x] Cleanup no longer freezes: cleanup + preview_cleanup are async +
    spawn_blocking, cleanup_progress now fires per finished item
[x] Cleanup is a single 3-phase dialog (review → running → done)
[x] History enriched: grouped by day, totals, status filter, lucide status icons
```

Evidence: `cargo test` 46 passed (incl. progress-callback + provider-toggle
regression tests), clippy 0 warnings, fmt clean, `svelte-check` 0 errors
0 warnings, `vite build` ok, `vitest` 29 passed.

## v0.5.0 — Coverage + honest numbers ✅

```text
[x] Fix freed_bytes for subprocess-backed cleanup (was: always == requested)
[x] Monotonic scan progress; new scan_provider_started event
[x] Auto-detected scan roots + `detect_scan_roots`; empty-root migration
[x] Age-based retention (`Settings.retention_days`, enforced by the engine)
[x] AI history providers: opencode fs, opencode.db (SQLite + VACUUM), Claude
[x] Docker images/volumes behind `docker_prune_scope` + privileged opt-in
[x] Global system caches (Chrome, Homebrew, bun, VS Code, gopls, node-gyp, uv)
[x] Backend-enforced confirmation (preview token), not a UI checkbox
[x] UI: Settings in 4 sections, scan progress replacing the safe bar,
      app/tray icon, sticky modal headers with a close button
      (the modal Settings became a page in v0.7.0)
```

Evidence: 122 Rust tests + 30 frontend tests green; clippy 0 warnings; fmt
clean; svelte-check 0 errors 0 warnings; vite build ok. Reclaimable went
from **11.79 GB** to **82.60 GB** on the development machine — the gap was
almost entirely the empty `scan_roots` array plus the missing AI and Docker
providers.

Three latent correctness bugs found and fixed along the way, all of the same
shape ("a parse failure silently became 0"): the `docker system df` parser
aborting on one bad line, the size parser rejecting Docker's `(83%)` suffix,
and the SQLite cutoff computed in seconds against a millisecond column.

## UI fixes round 2 ✅ (done)

```text
[x] History is run-level (contract v0.6.0): HistoryEntry.run_id, CleanupRun,
    get_history_runs with tail-read. Fixed: one 289-resource cleanup consumed
    the 100-entry budget and hid every earlier run, and the totals were taken
    from that truncated window (132 KB shown instead of 10.8 GB).
[x] scan_provider is genuinely targeted (ScanManager::scan_one) and merges into
    state so other providers' ids still resolve; post-cleanup rescan touches
    only the cleaned providers, flagged in the summary.
[x] Settings: no more duplicate scan-root paths; Retention controls share one
    size token; the disabled "Show filesystem path" row is gone (toolbar toggle
    is the single control).
[x] Provider expand can no longer open itself after a cleanup (pruned ids +
    regression test); cleaned-to-empty providers show a short note.
[x] Confirmation and result dialogs show the filesystem path, not just the name.
[x] Search field has its own bordered box, padding and spacing.
```

Evidence: `cargo test` 46 passed (incl. the 289-vs-1 history regression),
clippy 0 warnings, fmt clean, `svelte-check` 0 errors 0 warnings,
`vite build` ok, `vitest` 42 passed.

## M4 — UI shell revamp + menu bar quick actions ✅ (v0.7.0, done)

```text
[x] Sidebar shell replaces the header; four pages instead of four dialogs
    (Overview · Resources · History · Settings), landing on Overview
[x] Resource drill-down became a non-modal right-hand drawer, not a page
[x] Cleanup confirmation deliberately stays the ONLY modal (G-UI-03)
[x] ⌘1…⌘4 pages, ⌘R scan, ⌘, settings, ⌘F focus search, per-page scroll restore
[x] Menu bar: one click (either button) opens a Docker-style menu with a
    read-only info block — reclaimable, kept, top three providers, stale count,
    low-disk warning — plus six actions, none of which can delete (G-UI-02)
[x] Closing the window hides it; the app stays in the menu bar (Quit exits)
[x] Tray artwork re-framed: 36×36 (18pt @2x), glyph centred at ~89% of the canvas
[x] Free space measured once per scan — opening the menu never spawns a process
```

Fixed along the way: the tray icon was 44×44 with the glyph at 64% of the frame
and 6px below centre (it looked small and sunken); the item was also inert —
left-clicking a visible window only re-showed it, and closing the window exited
the app, so no menu action was ever reachable.

Evidence: 140 Rust tests + 65 frontend tests green; clippy 0 warnings; fmt
clean; svelte-check 0 errors 0 warnings; vite build ok.

## v0.7.1 — UI polish (done)

```text
[x] Overview: "Where the space is" lists up to OVERVIEW_PROVIDER_LIMIT providers
    with the tail aggregated as a named "Other · N providers" row, and the CTA
    pinned to the card's bottom edge. Sorting, capping and the aggregate live in
    `topProviders` (view.ts), unit-tested — including a check that the rows
    always reconcile with the headline figure.
[x] Settings: sections are a horizontal tab strip again (roving tabindex, arrow /
    Home / End keys), and the Save bar is `fixed` instead of `sticky` — `sticky`
    only pins while the page is taller than the window, so on a short section
    the bar floated in the middle of the window.
[x] One `--spacing-sidebar` token now drives the sidebar and both fixed bars, so
    they cannot drift; the selection bar no longer runs underneath the sidebar.
[x] Tray icon rewritten: the PNG had been written as greyscale *without* an alpha
    channel, and macOS paints a template image from alpha — the menu bar showed a
    solid white square. Now a bolder, simplified sprout, generated together with
    its SVG by tools/make_tray_icon.py, and guarded by tray-icon.test.ts (G-UI-04).
```

## M5 — UI regression coverage (not started)

```text
[ ] Component tests for the shell: sidebar navigation, page switch, drawer
    focus/close, cleanup-flow keyboard blocking.
    Blocked on: a DOM environment (jsdom / @testing-library/svelte). v0.7.0 kept
    the dependency surface unchanged and moved the rules that could be tested
    without a DOM into `lib/nav.ts` and `architecture.test.ts`, but a visual
    regression (a stretched icon, a collapsed row) is still invisible to CI.
```

## V1.0 (recommended scope)

Dashboard, filesystem scanner, node_modules, npm/pnpm cache, Go cache,
Docker build cache, Rust target, scan progress, cleanup preview, safe cleanup,
cleanup history, dark mode, tray, Windows + macOS + Linux.

## V2.0 (ideas, not committed)

Scheduled cleanup, Docker image analysis, duplicate detection, disk trend,
scheduled recommendations. Monetization (Pro: analytics/scheduled/rules) only
after product-market fit — never put a subscription into core cleanup.## v0.7.1 — UI polish (done)

```text
[x] Overview: "Where the space is" lists up to OVERVIEW_PROVIDER_LIMIT providers
    with the tail aggregated as a named "Other · N providers" row, and the CTA
    pinned to the card's bottom edge. Sorting, capping and the aggregate live in
    `topProviders` (view.ts), unit-tested — including a check that the rows
    always reconcile with the headline figure.
[x] Settings: sections are a horizontal tab strip again (roving tabindex, arrow /
    Home / End keys), and the Save bar is `fixed` instead of `sticky` — `sticky`
    only pins while the page is taller than the window, so on a short section
    the bar floated in the middle of the window.
[x] One `--spacing-sidebar` token now drives the sidebar and both fixed bars, so
    they cannot drift; the selection bar no longer runs underneath the sidebar.
[x] Tray icon rewritten: the PNG had been written as greyscale *without* an alpha
    channel, and macOS paints a template image from alpha — the menu bar showed a
    solid white square. Now a bolder, simplified sprout, generated together with
    its SVG by tools/make_tray_icon.py, and guarded by tray-icon.test.ts (G-UI-04).
```

## M5 — UI regression coverage (not started)

```text
[ ] Component tests for the shell: sidebar navigation, page switch, drawer
    focus/close, cleanup-flow keyboard blocking.
    Blocked on: a DOM environment (jsdom / @testing-library/svelte). v0.7.0 kept
    the dependency surface unchanged and moved the rules that could be tested
    without a DOM into `lib/nav.ts` and `architecture.test.ts`, but a visual
    regression (a stretched icon, a collapsed row) is still invisible to CI.
```

## V1.0 (recommended scope)

Dashboard, filesystem scanner, node_modules, npm/pnpm cache, Go cache,
Docker build cache, Rust target, scan progress, cleanup preview, safe cleanup,
cleanup history, dark mode, tray, Windows + macOS + Linux.

## V2.0 (ideas, not committed)

Scheduled cleanup, Docker image analysis, duplicate detection, disk trend,
scheduled recommendations. Monetization (Pro: analytics/scheduled/rules) only
after product-market fit — never put a subscription into core cleanup.

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

## M3 — Safety hardening + UX (later)

```text
[ ] Explicit per-provider timeouts + cancel <1s test (closes gaps G-SCAN-02/G-REL-01)
[ ] Persistent settings + scan cache file + cleanup history (SQLite optional)
[ ] System tray + restrained notifications (never adware)
[ ] Final dark/light toggle, project ignore list
[ ] More providers: npm/pnpm/yarn cache, Python, Cargo registry, Gradle, Xcode
[ ] Smart stale detection (>90 days) — a signal only, never a safety verdict
```

## V1.0 (recommended scope)

Dashboard, filesystem scanner, node_modules, npm/pnpm cache, Go cache,
Docker build cache, Rust target, scan progress, cleanup preview, safe cleanup,
cleanup history, dark mode, tray, Windows + macOS + Linux.

## V2.0 (ideas, not committed)

Scheduled cleanup, Docker image analysis, duplicate detection, disk trend,
scheduled recommendations. Monetization (Pro: analytics/scheduled/rules) only
after product-market fit — never put a subscription into core cleanup.

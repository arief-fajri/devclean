# DevClean Development Guide

> Step-by-step guide to running, testing, and contributing. Written for every level — if any step confuses you or fails, open an issue.

## Quick start (TL;DR, ~5 minutes)

```bash
git clone <repo> && cd devclean

# Backend: build + test
cargo test --workspace

# Frontend
npm --prefix apps/desktop ci

# Run
cargo tauri dev
```

Open the DevClean window. The app is a sidebar shell with four pages — **Overview** (the landing page: reclaimable total, free space on the volume, the three biggest reclaimers, stale count, last cleanup), **Resources** (search, filter, sort, provider cards, selection), **History** and **Settings**. The sidebar footer always says what the backend is doing, so a running scan is visible from any page. `⌘1`…`⌘4` switch pages, `⌘R` scans, `⌘,` opens Settings, `⌘F` (or `/`) focuses search; leaving and returning to a page keeps its scroll position.

Overview → **Scan now** → up to 22 providers appear on Resources with byte counts and share percentages. Providers you switched off in Settings are skipped entirely. The toolbar above the list offers global search, All/Safe/Review/Stale/Selected filters, sorting (largest first by default), and a Paths toggle; the stat tiles toggle the safety filter. A provider card starts collapsed (summary only) — click its row to reveal the items, each with its cleanup consequence and filesystem path. Clicking an item opens a right-hand drawer (Esc or Close to dismiss, focus returns to the row). Selecting items raises the selection bar, which follows you across pages (safe/review split, Clear, Review & clean selected).

Cleanup runs in one modal across three phases — the only modal left in the app, on purpose (G-UI-03): **review** (re-measured, review items first, acknowledgement required) → **running** (live progress bar + per-item state + Cancel remaining) → **done** (freed vs requested, per-resource before/after). The app stays responsive throughout — `cleanup` and `preview_cleanup` run off the UI thread. Scan roots, ignore list, provider toggles and the rest live on the Settings page behind a sticky Save bar (persisted to disk); its **Updates** tab drives the in-app updater (`get_update_status`, `check_for_update`, `install_update`, `restart_app` — see `tauri-contract.md`). After a cleanup the app re-scans **only the providers it cleaned** — the summary shows a "partial re-scan" note because the other providers keep their last known figures. Use **Scan again** for a full sweep. Past cleanups live on the History page, grouped by cleanup **run** (per-resource rows collapsed behind a chevron), so one large cleanup can never hide the runs before it.

Closing the window does not quit: DevClean stays in the menu bar, where one click (either button) opens a menu showing what is reclaimable, what retention keeps, the three biggest reclaimers, the stale count and — when the disk is under 10% free — a low-space warning. From there: Open DevClean / Scan now / Review safe items / History / Settings / Quit. None of them can delete anything (G-UI-02); a cleanup always goes through the confirmation modal. Notifications fire only for completed scans/cleanups while the window is unfocused.

Limits: each provider scan is bounded by 60s (`PROVIDER_SCAN_TIMEOUT`), each cleanup subprocess by 120s (`CLEANUP_COMMAND_TIMEOUT`, child killed on expiry). Cancel settles in <1s (tested).

**Your setup works if:**

- `cargo tauri dev` opens a window (Vite at `http://localhost:1420`)
- No `curl` needed — verify via the UI: Scan lists the enabled providers (up to 21 — `node_modules`, Rust `target/`, Go cache, Docker, npm/pnpm/yarn caches, Python caches, Cargo registry, Gradle caches, Xcode data, opencode and Claude data, Go vendoring, Flutter, Android/Gradle, CocoaPods, LLM model stores, Ruby caches and gated browser temp files; absent tools simply report empty, and providers disabled in Settings never appear)
- `cargo test --workspace` is green, `cargo clippy` has 0 warnings, `npm run check` has 0 errors, `npm test` (vitest) is green

## Prerequisites

| Tool | Minimum | Used for | Check |
|---|---|---|---|
| Rust | 1.90+ | backend + Tauri | `rustc --version` |
| Node.js | 22+ | Svelte frontend | `node --version` |
| Tauri CLI | 2.x | `cargo tauri dev/build` | `cargo tauri --version` |
| Go | 1.21+ (optional) | testing the Go provider | `go version` |
| Docker | 24+ (optional) | testing the Docker provider | `docker info` |

> Go/Docker are optional: their providers return empty when the tool is missing. That is normal behavior, not an error.

Install the Tauri CLI once:

```bash
cargo install tauri-cli --version "^2" --locked
```

## Daily commands

```bash
cargo test --workspace                  # all Rust tests
cargo test -p devclean-core             # one crate only
cargo clippy --workspace --all-targets  # must be 0 warnings
cargo fmt --all                         # format (check: append -- --check)

npm --prefix apps/desktop run check     # svelte-check + tsc
npm --prefix apps/desktop run build     # build frontend (output ../dist)
npm --prefix apps/desktop test          # vitest (stores, components, guardrails)

cargo tauri dev                         # dev mode (from the repo root)
cargo tauri build                       # native bundle (DMG/MSI/deb)
```

## Adding a new provider

See [CONTRIBUTING.md](../../CONTRIBUTING.md#adding-a-provider) — the steps live there so there is one place to keep them current.

## Troubleshooting

| Symptom | Common cause | Fix |
|---|---|---|
| `cargo tauri dev` blank window | Vite not ready / port conflict | Wait 5s, make sure port 1420 is free |
| Scan finds 0 bytes everywhere | `~/Projects\|Code\|Workspace` don't exist | Add a scan root under Settings |
| Empty Docker provider | Docker daemon is down | `docker info` must succeed |
| Docker **and** Go empty in the installed app | Finder launch inherits launchd's minimal `PATH` | Run `/Applications/DevClean.app/Contents/MacOS/devclean` from a terminal, or install 1.1.0+, which widens `PATH` at startup (`system::augment_process_path`) |
| `svelte-check` event errors | event payload changed | Align with `tauri-contract.md` |
| `no such command: tauri` | CLI not installed / Rust too old | Update Rust (`rustup update`), then install the CLI |

## Debugging layout

- Rust logs: the `cargo tauri dev` terminal panel (tauri-plugin-log, Info level).
- Tauri events: open the frontend devtools → Console tab → `listen()` logs (add when needed, never commit console.log).
- Byte counts: `preview_cleanup` is the final truth (re-measured), not the scan numbers.

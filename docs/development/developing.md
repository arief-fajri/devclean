# DevClean Development Guide

> Step-by-step guide to running, testing, and contributing. Written for every
> level — if any step confuses you or fails, open an issue.

## Quick start (TL;DR, ~5 minutes)

```bash
git clone <repo> && cd devclean

# Backend: build + test
cargo test --workspace

# Frontend
cd apps/desktop && npm install && cd ../..

# Run
cargo tauri dev
```

Open the DevClean window → click **Scan** → 4 providers appear with byte counts.

**Your setup works if:**

- `cargo tauri dev` opens a window (Vite at `http://localhost:1420`)
- No `curl` needed — verify via the UI: Scan lists `node_modules`, `Rust target`, `Go cache`, `Docker`
- `cargo test --workspace` is green, `cargo clippy` has 0 warnings, `npm run check` has 0 errors

## Prerequisites

| Tool | Minimum | Used for | Check |
|---|---|---|---|
| Rust | 1.90+ | backend + Tauri | `rustc --version` |
| Node.js | 22+ | Svelte frontend | `node --version` |
| Tauri CLI | 2.x | `cargo tauri dev/build` | `cargo tauri --version` |
| Go | 1.21+ (optional) | testing the Go provider | `go version` |
| Docker | 24+ (optional) | testing the Docker provider | `docker info` |

> Go/Docker are optional: their providers return empty when the tool is
> missing. That is normal behavior, not an error.

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

cargo tauri dev                         # dev mode (from the repo root)
cargo tauri build                       # native bundle (DMG/MSI/deb)
```

## Adding a new provider (example: Python)

1. Create `crates/providers/src/python.rs` implementing the `Provider` trait
   (`id`, `name`, `default_safety`, `scan`, `cleanup_action`).
2. Register it in `all_providers()` + add an id prefix in `ScanManager`.
3. Write a test: dummy folder → scan finds it → `SafetyValidator` rejects
   paths outside scan roots.
4. Contract update? Not needed for a provider alone (types are unchanged) —
   but note it in the roadmap + PR.

## Troubleshooting

| Symptom | Common cause | Fix |
|---|---|---|
| `cargo tauri dev` blank window | Vite not ready / port conflict | Wait 5s, make sure port 1420 is free |
| Scan finds 0 bytes everywhere | `~/Projects\|Code\|Workspace` don't exist | Add a scan root under Settings |
| Empty Docker provider | Docker daemon is down | `docker info` must succeed |
| `svelte-check` event errors | event payload changed | Align with `tauri-contract.md` |
| `no such command: tauri` | CLI not installed / Rust too old | Update Rust (`rustup update`), then install the CLI |

## Debugging layout

- Rust logs: the `cargo tauri dev` terminal panel (tauri-plugin-log, Info level).
- Tauri events: open the frontend devtools → Console tab → `listen()` logs (add when needed, never commit console.log).
- Byte counts: `preview_cleanup` is the final truth (re-measured), not the scan numbers.

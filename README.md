# DevClean

> **Clean the junk created by your development tools.**

DevClean monitors disk-hungry development resources (Docker build cache,
`node_modules`, Go cache, Rust `target/`, …) and cleans them up safely —
**diagnosis + safety + cleanup** in a single desktop app.

Stack: **Rust + Tauri 2 + Svelte 5 + Tailwind** (no React, no heavy UI kit).

```text
Disk usage
    ↓
DevClean scan
    ↓
82.6 GB reclaimable

node_modules         14.8 GB
Rust target          18.7 GB
Docker images/vol    16.7 GB
Go cache              8.4 GB
System caches         5.8 GB
opencode history     14.2 GB
    ↓
[ Review & clean selected ]
```

(Measured on the development machine, where `~/VOLANTIS` and `~/EXP` are
auto-detected as scan roots.)

## Quick start (5 minutes)

```bash
# 1. Prereqs: Rust 1.90+, Node 22+, Tauri CLI 2
cargo install tauri-cli --version "^2" --locked

# 2. Backend + frontend deps
cargo build -p devclean
cd apps/desktop && npm install && cd ../..

# 3. Run in dev mode
cargo tauri dev
```

**Your setup works if:**

- The DevClean window opens (Vite at `http://localhost:1420`)
- **Overview** shows a reclaimable total and the **Scan now** button lists 14
  providers without any setup (see below)
- `cargo test --workspace` is green · `cargo clippy` has 0 warnings · `npm run check` has 0 errors

Full details: [docs/development/developing.md](docs/development/developing.md).

## Platform support

| Platform | Status |
|---|---|
| **macOS** | **Tested.** The shipped build target (`app`, `dmg`), and the only platform in CI. |
| Linux | **Not tested.** The code paths are Unix-shaped (allocated blocks, `~/.Trash`), but nothing builds or runs it. |
| Windows | **Not tested.** Not a build target. Path handling, Trash and block sizing are all unverified there. |

This table is enforced: `apps/desktop/src/lib/platform-claim.test.ts` fails if
the README claims a platform that has no CI job, or if `tauri.conf.json`
declares a bundle target for one. It exists because a support matrix drifts
away from reality the moment nothing checks it.

## Repo layout

```text
devclean/
├── apps/desktop/          # Tauri app: Svelte 5 UI + thin src-tauri Rust layer
│   ├── src/               # App shell, routes, components, stores (runes), bridge
│   └── src-tauri/src/     # main.rs, commands.rs, events.rs, state.rs, tray.rs
├── crates/
│   ├── core/              # types, SafetyValidator, settings, preview tokens
│   ├── providers/         # Provider trait + 14 resource providers
│   ├── scanner/           # ScanManager (concurrent, cancellable) + cache
│   └── cleanup/           # preview + gated executor, retention, SQLite
└── docs/
    ├── architecture/overview.md
    ├── development/{developing.md, roadmap.md, methodology/{system-model.md, guardrails.md}}
    ├── reference/tauri-contract.md   # commands/events contract — never change silently
    └── security.md
```

One-way dependencies: `core ← providers ← scanner/cleanup ← src-tauri ← UI`.

## Ground rules (summary)

1. **Frontend = untrusted.** The UI only sends `{provider, resource_id}` — never a raw path.
2. **Backend = authority.** Every deletion goes through `SafetyValidator`; paths are resolved server-side.
3. **Stable contract.** Adding/changing a Tauri command or event requires a `docs/reference/tauri-contract.md` update + test.
4. **Safety first.** Read [docs/security.md](docs/security.md) and the `G-SAFE-*` guardrails before touching cleanup code.
5. **Numbers are measured.** `freed_bytes` is `before − after` for every branch, including subprocess-backed cleanup. When the after-state cannot be measured, the result says `0` and why — it never asserts the requested figure was freed.

## The window, and what is left when you close it

The app is a sidebar with four pages — **Overview** (reclaimable total, free
space, biggest reclaimers, stale count, last cleanup), **Resources** (the
working list), **History** and **Settings**. `⌘1`…`⌘4` switch pages,
`⌘R` scans, `⌘F` focuses search.

Closing the window does not quit. DevClean stays in the **menu bar**, where one
click opens a menu that answers the same questions without opening the window:
how much is reclaimable, how much retention keeps, the three biggest
reclaimers, the stale count, and a warning when the volume drops below 10%
free. From there: Open DevClean · Scan now · Review safe items · History ·
Settings · Quit.

**No menu item can delete anything.** A cleanup always opens the
confirmation modal in the window, with a re-measured preview — the menu bar is
diagnostics and navigation only, and two guardrails enforce it
([G-UI-02](docs/development/methodology/guardrails.md),
[G-UI-03](docs/development/methodology/guardrails.md)).

## Scan roots are detected, not typed

Default roots cover the usual folders (`~/Projects`, `~/Code`, `~/Workspace`,
`~/VOLANTIS`, `~/EXP`, `~/src`, `~/repos`, `~/dev`, `~/work`), filtered to those
that exist. **Settings → Scan → Detect folders** discovers the rest — it walks
`$HOME` at most two levels deep, never descends into a cache, and reports an
estimated size per candidate so you can pick without opening anything.

A pre-v0.5.0 `settings.json` with an empty `scan_roots` array is re-seeded from
the defaults on load; otherwise the app would stay permanently blind to
`node_modules` on a machine whose work lives outside `~/Projects`.

## Relationship to the legacy `~/scripts/cleanup.sh`

The two are **independent by design** and safe to run together — both are
idempotent, and neither depends on the other:

| Area | DevClean | `cleanup.sh` |
|---|---|---|
| Report | ✅ main UI | ✅ `check` |
| Safe caches | ✅ | ✅ |
| `node_modules` | ✅ | ✅ |
| Docker images/volumes | ✅ via **Settings → Cleanup → Docker scope** | ✅ manual, asks first |
| AI history (`opencode.db`, `claude`) | ✅ with a retention policy | ✅ `clean-ai --keep-days N` |
| Scheduling | GUI only | cron/launchd |

There is no bridge between them: DevClean never invokes the script, and the
script never reads DevClean's state.

New AI agents / developers start at [AGENTS.md](AGENTS.md).

## MVP roadmap

- **M0** Foundation: workspace, Tauri+Svelte running, green CI ✅
- **M1+M2** Scanner + safe cleanup ✅
- **M3** Safety hardening + UX: timeouts, settings, history, tray ✅
- **v0.5.0** Coverage + honest numbers: 14 providers, auto-detected scan roots,
  age-based retention, AI history (opencode + Claude), configurable Docker
  scope, backend-enforced confirmation ✅ (you are here)

See [docs/development/roadmap.md](docs/development/roadmap.md).

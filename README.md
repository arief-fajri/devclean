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
- The **Scan** button lists 14 providers and finds your projects without any setup (see below)
- `cargo test --workspace` is green · `cargo clippy` has 0 warnings · `npm run check` has 0 errors

Full details: [docs/development/developing.md](docs/development/developing.md).

## Repo layout

```text
devclean/
├── apps/desktop/          # Tauri app: Svelte 5 UI + thin src-tauri Rust layer
│   ├── src/               # components, stores (runes), tauri bridge
│   └── src-tauri/src/     # main.rs, commands.rs, events.rs, state.rs
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

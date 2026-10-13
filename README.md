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
47.8 GB reclaimable

Docker             15.2 GB
node_modules       12.8 GB
Go cache             8.4 GB
Rust target          6.1 GB
    ↓
[ Clean Safe Resources ]
```

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
- The **Scan** button lists 4 providers (node, rust, go, docker)
- `cargo test --workspace` is green · `cargo clippy` has 0 warnings · `npm run check` has 0 errors

Full details: [docs/development/developing.md](docs/development/developing.md).

## Repo layout

```text
devclean/
├── apps/desktop/          # Tauri app: Svelte 5 UI + thin src-tauri Rust layer
│   ├── src/               # components, stores (runes), tauri bridge
│   └── src-tauri/src/     # main.rs, commands.rs, events.rs, state.rs
├── crates/
│   ├── core/              # types, SafetyValidator, settings, system helpers
│   ├── providers/         # Provider trait + node/go/docker/rust_target
│   ├── scanner/           # ScanManager (concurrent, cancellable) + cache
│   └── cleanup/           # preview + safety-gated executor
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

New AI agents / developers start at [AGENTS.md](AGENTS.md).

## MVP roadmap

- **M0** Foundation: workspace, Tauri+Svelte running, green CI ✅
- **M1+M2** Scanner + safe cleanup: 4 providers, dashboard, preview → confirm → cleanup → result ✅ (you are here)
- **M3** Safety hardening + UX: tray, persistent settings, history, more providers

See [docs/development/roadmap.md](docs/development/roadmap.md).

# AGENTS.md — instructions for AI agents & developers

DevClean is a **Rust + Tauri 2 + Svelte 5** desktop app for monitoring and
cleaning development resources. This file is the **single entry point for
every work session**. Read it first, before any other file.

Public behavior contract: `docs/reference/tauri-contract.md`. Caller-visible
changes (commands/events/responses) must be deliberate, tested, and written
there.

---

## System thinking loop (required for non-trivial tasks)

```text
MODEL → GUARDRAIL → IMPLEMENT → OBSERVE → TEST → EVALUATE → RECORD
```

Details: [system-model.md](docs/development/methodology/system-model.md) (what
system we are building),
[guardrails.md](docs/development/methodology/guardrails.md) (what must never
happen).

### Definition of Done (6 steps, answered before & after coding)

1. **Which system property changes?** (scan / cleanup / safety / UI)
2. **Which invariant must stay true?** (I-* in system-model.md)
3. **Which guardrail applies?** (G-* in guardrails.md)
4. **How is the behavior observed?** (`tracing` logs, Tauri events, byte counts)
5. **How is it tested?** (`cargo test`, `svelte-check`, manual scenario)
6. **What evidence proves it works?** (test output / before-after bytes freed)

Answers 1–3 are written **before** coding (PR comment/summary), 4–6 attached
**after**. Without them, the task is not done.

### Failure classification — classify BEFORE fixing

| Type | Pattern | Response |
|---|---|---|
| **A — Implementation failure** | Design ✓, guardrail ✓, implementation ✗ | Fix the implementation |
| **B — Design failure** | Implementation ✓, design was lacking | Update system-model.md |
| **C — Missing guardrail** | System entered a forbidden state | Add a guardrail |
| **D — Missing observability** | Failed but the operator can't tell why | Add measurement/log/event |
| **E — Wrong acceptance** | Checklist passed but behavior was unsafe | Fix the criteria + test |

Never jump from a failing test straight to a patch without recording the
classification.

### Decision authority

| Level | What | Action |
|---|---|---|
| **A — Decide & execute** | Internal, reversible, touches no guardrail/contract | Do it, record it |
| **B — Ask a human** | Contract, security, destructive, new dependency, cleanup behavior | Write a proposal, **stop until a human agrees**. Silence ≠ approval. |

In doubt → escalate to B. Never downgrade B to A.

---

## Read first (per task)

- Architecture: [docs/architecture/overview.md](docs/architecture/overview.md)
- Contract: [docs/reference/tauri-contract.md](docs/reference/tauri-contract.md)
- Roadmap: [docs/development/roadmap.md](docs/development/roadmap.md)
- Dev guide: [docs/development/developing.md](docs/development/developing.md) (source of truth for build/test commands)
- Model & invariants: [system-model.md](docs/development/methodology/system-model.md)
- Guardrails: [guardrails.md](docs/development/methodology/guardrails.md)
- Security: [docs/security.md](docs/security.md)

## Build / Test / Lint

```bash
cargo test --workspace                 # Rust: all crates
cargo clippy --workspace --all-targets # must be 0 warnings
cargo fmt --all -- --check             # must be clean

npm --prefix apps/desktop run check    # svelte-check + tsc
npm --prefix apps/desktop run build    # build frontend

cargo tauri dev                        # run from root (frontendDist ../dist)
```

## Code layout

- `crates/core/` — types, `SafetyValidator`, settings, system helpers. Must not depend on any other crate.
- `crates/providers/` — `Provider` trait + 14 providers (`node_modules`, `rust_target`, `go`, `docker`, `python_cache`, …). Depends only on `core`.
- `crates/scanner/` — concurrent `ScanManager` + last-scan cache.
- `crates/cleanup/` — `preview` + safety-validated executor.
- `apps/desktop/src-tauri/src/` — thin layer: `commands.rs` (15 commands), `events.rs`, `state.rs`. No filesystem logic beyond wiring.
- `apps/desktop/src/` — Svelte 5 (runes) + Tailwind only:
  - `App.svelte` — the shell: sidebar, page switch, selection bar, resource
    drawer, and the cleanup-flow modal (the only modal; G-UI-03).
  - `lib/routes/` — one file per page (Overview, Resources, History,
    Settings). Pages may not call the cleanup API (`architecture.test.ts`).
  - `lib/nav.ts` — pure page ids, titles, ⌘-shortcuts, and the validation the
    `tray_action` event is checked against. Pure + unit-tested, like
    `lib/view.ts`.
  - `lib/tauri.ts`, `lib/stores/`, `lib/components/` (incl. `Sidebar`).

## Hard rules

1. The frontend must never send a raw path for deletion — only an opaque `resource_id`.
2. Every directory deletion must go through `SafetyValidator::validate_path`.
3. `RunCommand` is only for fixed `(program, args)` pairs from providers — never via shell, never from user input.
4. Caller-visible changes without a `tauri-contract.md` update + regression test are rejected.
5. `allow_privileged_operations` defaults to OFF; significant destructive operations require a confirmation dialog (G-UI-01).
6. Every guardrail must have an enforcement mechanism (code/test/CI) — a guardrail without enforcement is just a suggestion.

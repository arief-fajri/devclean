# Documentation

Start with the [README](../README.md) if you want to install or use DevClean.
This page is the index for everything under `docs/`.

## Reference

| Document | What it is |
|---|---|
| [reference/tauri-contract.md](reference/tauri-contract.md) | **The public behaviour contract.** Every command, event and field the frontend may see, with a changelog. Changing it requires human approval and a regression test. |

## Architecture

| Document | What it is |
|---|---|
| [architecture/overview.md](architecture/overview.md) | How the crates fit together and which way dependencies point. |
| [development/methodology/system-model.md](development/methodology/system-model.md) | What system this is, where its boundaries are, and the invariants that must always hold. |

## Decisions

Why things are the way they are, and what was rejected. Each record states a
decision someone could reasonably have made differently — if it could not be
rejected, it is a note, not a decision.

| Record | Decision |
|---|---|
| [decisions/allocated-vs-apparent.md](decisions/allocated-vs-apparent.md) | `size_bytes` is allocated blocks, not a sum of file lengths. |
| [decisions/trash-by-default.md](decisions/trash-by-default.md) | Cleanups move to the Trash by default, and `freed_bytes` is 0 when they do. |
| [decisions/blocked-reason.md](decisions/blocked-reason.md) | One `blocked_reason`, stamped at scan time, enforced by the engine. |

## Development

| Document | What it is |
|---|---|
| [development/developing.md](development/developing.md) | Build, test and lint commands. The source of truth for how to run this. |
| [development/roadmap.md](development/roadmap.md) | What is left, plus known gaps and non-goals. |
| [development/methodology/guardrails.md](development/methodology/guardrails.md) | What behaviour is never allowed, and the mechanism that enforces each one. |

## Operations

| Document | What it is |
|---|---|
| [operations/recovery.md](operations/recovery.md) | Restoring something from the Trash, reading a partial failure, and what each blocked badge means. |
| [operations/screenshots.md](operations/screenshots.md) | Regenerating the README screenshots from the fixture. |

## Security

| Document | What it is |
|---|---|
| [security.md](security.md) | The design rationale: threat model, deletion pipeline, why each check exists. |
| [../SECURITY.md](../SECURITY.md) | The reporting policy: what counts as a vulnerability, how to report it. |

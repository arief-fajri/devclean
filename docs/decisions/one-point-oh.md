# ADR: 1.0 locks the contract — breaking means 2.0

**Status:** accepted (product 1.0.0, contract v1.0.0)

## Context

DevClean carries two version numbers that move independently: the product
version a user installs, and the contract version of the IPC surface
between the bundled frontend and backend
([tauri-contract.md](../reference/tauri-contract.md)). For every wave up
to 0.13.0 the contract moved freely — commands were added, one was
re-typed (`get_cached_scan` in v0.9.0), and v0.8.0 replaced a field —
because 0.x is exactly where that is allowed.

The roadmap's version ladder promised the two numbers align at 1.0. An
alignment nobody committed to in writing would be a number change, not a
promise. This ADR is the writing-down.

## Decision

From **contract v1.0.0**:

1. **A caller-visible breaking change requires contract 2.0.0.** Breaking
   means: removing or renaming a command or event, removing or renaming a
   field, or changing what an existing field means. Adding a command,
   event, optional field or enum member is not breaking and increments the
   contract minor.
2. **The contract document is the source of truth, and it is enforced.**
   The Types block in `tauri-contract.md` is diffed against
   `apps/desktop/src/lib/tauri.ts` by `contract-sync.test.ts`; a change
   that touches one without the other fails CI. The document's behavioural
   notes are contract too — they describe what a caller may rely on.
3. **The bundled frontend and backend always ship together.** The
   commitment is not about independent deployment (there is none); it is
   about the discipline that made the document trustworthy, and about any
   outside reader — a script author, a future alternative frontend — being
   able to rely on what is written.
4. **Stability covers the documented surface only**: the 19 commands, 9
   events, and the types and notes in `tauri-contract.md`. Internal crate
   APIs, cache file formats and UI text are not part of it.

## Consequences

- Post-1.0 work that would break the surface is either done additively or
  waits for 2.0. Nothing in the current backlog needs a break.
- The contract changelog keeps its own sequence (v1.x), independent of the
  product patch cadence.
- `contract-sync.test.ts`, the G-API-01 DoD step and the contract banner
  stay permanently — they are the enforcement, not the ceremony.

## Rejected alternatives

- **"1.0 is just another number."** Rejected: the ladder already promised
  alignment, and a version boundary that changes nothing is not worth
  crossing — this ADR is what makes crossing it mean something.
- **Lock only commands, allow type drift.** Rejected: the frontend is
  compiled against those types; a silently re-typed field breaks it
  exactly as hard as a removed command, and `contract-sync.test.ts` exists
  because document drift actually happened (Wave E3 found six fields the
  document had lost).

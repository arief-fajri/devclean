# ADR: permanent-only cleanup (supersedes trash-by-default pre-1.0)

**Status:** accepted (1.0.0, pre-release amendment — no contract major bump).

## Context

Trash-by-default made a wrong click recoverable, but it broke the core
expectation of a developer cleanup tool: reclaimed space must return
immediately. A `2.3 GB SUCCESS` that leaves the Trash full reads as a bug
(`go_cache` already bypassed Trash via `go clean`, so the UI was
inconsistent: some providers reclaimed now, others reported `0 B freed` with
bytes pending). The Trash also added a whole platform matrix (macOS flat vs
freedesktop sidecars, EXDEV copies, `libc` date formatting) for a safety net
that most cleanups never needed — almost every artifact is regenerable
(`npm install`, `cargo build`, `go build`, Docker layer rebuild). The
exception: items classified **Dangerous** (Xcode Archives with shipped
dSYMs, opencode conversation history, Docker local volumes) are not —
they carry their own consequence copy and a separate confirmation.

## Decision

1. **Every cleanup is permanent.** Path removal uses `remove_dir_all`;
   `freed_bytes` always reports the measured `before − after`.
2. **Remove the choice.** `Settings.delete_mode`, `type DeleteMode`,
   `CleanupResult.moved_to_trash`, `crates/cleanup/src/trash.rs` and the
   Settings radiogroup are gone.
3. **History stays readable.** `HistoryEntry.moved_to_trash?` remains as a
   legacy pre-1.0 read-only flag so old runs can be labelled `archived in
   Trash (pre-1.0)`. New entries always write `false` (omitted in practice).
4. **Preview + confirmation stay mandatory.** With no undo, the re-measured
   preview and backend-enforced `preview_token` are the safety net (G-UI-01,
   I6, I11). The confirmation dialog states `Permanent deletion has no undo`
   before the click (the done screen repeats it), and Dangerous items
   demand a second acknowledgement spelling out their consequences.

## Consequences

- Space reclaims immediately; there is no `pending` figure anymore.
- A wrong click cannot be undone — the confirmation copy must stay explicit.
- Old `settings.json` files containing `"delete_mode"` still load (serde
  ignores the unknown field).
- Old history lines with `"moved_to_trash": true` still parse and render as
  archived, never as active pending work.

## Rejected alternatives

- **Keep Trash as opt-in.** Rejected: two code paths, two meanings of
  `freed_bytes`, and the same confusion that motivated this change.
- **Trash everything including vendor commands.** Already rejected —
  `RunCommand`/`RunSql` never had an undo.

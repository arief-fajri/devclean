# ADR: cleanups move to the Trash by default

**Status:** accepted (v0.8.0)

## Context

Every cleanup used `std::fs::remove_dir_all`. A mistake — selecting the wrong project, mis-reading a provider row — was unrecoverable. That is a poor default for a tool that runs against directories the user did not hand-pick one by one, and it is the single largest source of "I lost work to a cleanup script".

Two figures also got conflated. `freed_bytes` claimed space had been returned, but a move to the Trash does not return space — it relocates it. Reporting `freed_bytes = requested` after a Trash move would be exactly the class of unmeasured claim that G-REL-03 exists to prevent.

## Decision

1. **`Settings.delete_mode` defaults to `trash`.** `permanent` is an explicit opt-out for users who want the space back immediately.

2. **`freed_bytes` is 0 for a trashed artifact.** `CleanupResult.moved_to_trash` carries the fact, and the summary says *"moved to Trash — space returns when you empty it"*. The two figures are computed by a pure function (`cleanupOutcome`) so the distinction is unit-tested rather than left to a template.

3. **The delete mode governs path removal only.** `RunCommand` (a vendor prune) and `RunSql` (a `DELETE FROM`) have no undo, so they can never be reported as trashed. `command_backed_actions_are_never_marked_as_trashed` pins this.

4. **Trash is implemented in-tree, not via the `trash` crate.**

## Why not the `trash` crate

The crate is the obvious choice and was the first implementation. On macOS it shells out to AppleScript to ask Finder to perform the move. Measured from a normal process:

```text
Error during a `trash` operation: The AppleScript exited with error.
Finder got an error: The operation can't be completed because "" is in use. (-47)
```

A delete that fails when Finder is busy is worse than no delete at all: the user chose Trash specifically so a mistake would be recoverable, and the failure arrives as an opaque AppleScript error. `crates/cleanup/src/trash.rs` does the move itself — `os.Rename` into `~/.Trash`, with a recursive copy-then-remove fallback when the source is on another filesystem (`EXDEV`). The original is removed only after the copy fully succeeds.

## Consequences

- A trashed artifact lands in `~/.Trash` under its own name and can be dragged back out. macOS "Put Back" metadata is **not** written, so Finder offers no one-click undo; the file is still recoverable, just manually.
- Collisions are handled by suffixing (`node_modules`, `node_modules.1`, …) so a second cleanup never clobbers a previous one.
- Symlinks are recreated as links, never followed, in the cross-device copy.
- `CleanupResult.moved_to_trash` and `HistoryEntry.moved_to_trash` are additive contract fields.
- **Linux/XDG Trash: implemented for 1.0, unverified in a desktop file manager.** The freedesktop format needs `.trashinfo` files for the artifact to appear in a file manager's Trash. At v0.8.0 the shipped targets were macOS only, so implementing half of it would have been worse than not claiming it. Wave E1 (1.0.0) implemented it: `move_to_trash` dispatches to a freedesktop layout on Linux (`files/<name>` plus `info/<name>.trashinfo`, metadata written **before** the move so a crash never leaves an untrashed file) and the flat `~/.Trash` layout elsewhere. CI pins the sidecar bytes and the encoding, but cannot prove the file shows up in a real GNOME or KDE Trash — that is stated as "implemented, unverified" in the README support table rather than hidden.

## Rejected alternatives

- **Keep `remove_dir_all`, add an "are you sure?" checkbox.** Rejected: a checkbox does not restore a directory.
- **Trash everything including pruned files.** Rejected: `PruneOlderThan` removes hundreds of individual files, and filling the Trash with log files makes the Trash useless for the one thing it is good at.
- **Report `freed_bytes` under Trash and label it "pending".** Rejected: the field means "returned to the filesystem". A separate `pending` figure in the summary is clearer than overloading the one the engine measures.

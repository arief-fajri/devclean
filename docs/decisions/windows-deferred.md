# ADR: Windows stays unshipped after 1.0 — deferred, not rejected

**Status:** accepted (product 1.0.0); reopen when someone owns the port

## Context

The support table in the README has always said Windows is **Not tested**:
there is no Windows CI runner, no `msi`/`nsis` bundle target, and
`platform-claim.test.ts` fails if either appears without the other. Before
1.0 the Wave E plan forced the underlying question: is Windows a platform
this project *does not do* (a permanent no), or one it has not done *yet*
(a deferral)? "Undecided" cannot ship in a 1.0 support table.

The honest state of the code:

- The crates are Unix-shaped. The providers assume POSIX behaviour
  (`inode_key` deliberately over-counts off-Unix as `(0, 0)`), and free space
  is measured with `statvfs`. (Pre-1.0 the Trash module also imported
  Unix-only fs APIs; it was removed with the permanent-only switch.)
- Nothing has ever compiled there. "Probably portable" without a runner is
  exactly the claim G-PUB-01 exists to forbid.

## Decision

**Defer.** Windows is recorded on the post-1.0 roadmap as a horizon item
with a known entry cost, not claimed in any document, and not declared
permanently unsupported either. The README row stays exactly as it is:
"Not tested. Not a build target."

Reopening costs, stated so the next person does not have to rediscover
them: cfg-gate the Unix-only disk paths (Windows equivalent:
`GetDiskFreeSpaceExW`),
re-verify path handling and block sizing, add an `msi` or `nsis` target,
and stand up a `windows-latest` job **before** the support table changes —
in that order, because the table follows the CI job, never leads it.

## Consequences

- A Windows user cannot install DevClean and is told so in one line of the
  README rather than by a crash.
- The port, when it comes, is a normal feature branch: nothing in the
  1.0 surface has to change first (the contract is platform-neutral — no
  command exposes an OS-specific path format to the frontend).
- `platform-claim.test.ts` keeps refusing any half-claim: a Windows bundle
  target without a runner fails, and a runner without the README row
  fails.

## Rejected alternatives

- **Permanently unsupported.** Rejected: the codebase is closer to
  portable than not (no macOS-only APIs in the scan or engine paths, the
  platform-specific parts are contained in `disk.rs`), and
  a permanent no would have to be reversed later for exactly the cost
  deferring costs now — plus a broken promise.
- **Claim it for 1.0 without a runner.** Rejected: G-PUB-01. A support
  table that lies is worse than one that says "Not tested", and the table
  is enforced by a test precisely so this cannot regress quietly.

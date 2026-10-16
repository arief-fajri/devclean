# ADR: gitignore-aware protection

**Status:** accepted (v0.10.0) **Fixes:** nothing stopped a cleanup from deleting a directory a repository tracks

## Context

Activity came from filesystem mtime alone, and nothing knew whether an artifact
was committed. Two consequences:

- A project whose cache had not been touched in months looked stale even when
  its repository was committed to daily, so `retention_days` offered an active
  project's cache for deletion.
- Nothing stopped a cleanup from removing a directory a repository actually
  tracks. If a project ever commits `dist/` or `target/`, it was offered for
  deletion like any other cache. `BlockReason::Protected` had existed since the
  first release with **nothing producing it** — the same failure class as
  `retention_eligible` before v0.8.0: a documented reason no code path could
  reach.

## Decision

1. **Classification is a scanner pass, not a provider concern.** After
   providers return their resources, the scanner groups them by repository
   root and asks git once per repository: `git log -1 --format=%ct` for the
   last commit, `git status --porcelain` for dirtiness, and — **only for dirty
   repositories** — one `git check-ignore --stdin -z` for all their paths. A
   clean repository cannot produce `Protected`, so it never needs
   `check-ignore`; that is what keeps the pass affordable.

2. **`repo_root_for` spawns nothing.** `git rev-parse --show-toplevel` per
   artifact would mean eighty subprocesses for eighty `node_modules`. Walking
   the path's ancestors for a `.git` entry — file or directory, because
   worktrees and submodules use a file — is zero subprocesses and correct for
   the normal case.

3. **The gate is `tracked (or untracked, non-ignored) + dirty`, checked
   first.** `BlockReason::Protected` gets a producer inside `block_reason_for`,
   ahead of the AI and retention gates: deleting a tracked file is data loss
   regardless of what every other setting says. The one row that keeps the tool
   usable is that a **gitignored** artifact (`node_modules`) stays cleanable
   even in a dirty repository — refusing every artifact in a dirty repository
   would make the tool useless in exactly the repositories people work in.

4. **Age becomes a composite.** `Resource.age_days` is now the most recent of
   the artifact's mtime, the repository's last commit and the project
   directory's mtime, and `Resource.activity` is the tier derived from it
   (`Active` < 7d, `Recent` 7–30, `Stale` 30–90, `Dormant` 90+). A project
   committed to yesterday stops looking dormant. The frontend filters on
   `activity`, not on a string parsed out of `detail`.

5. **Every git call carries a deadline, and a machine without git is
   unclassified.** The git calls needed a timeout helper anyway, which closes
   G-REL-05: `run_tool`, `go_env` and `pgrep` now go through the same
   `run_with_deadline`. When git is absent the scanner skips classification
   entirely, so those machines behave exactly as before Wave B.

## Consequences

- `PolicyInputs.protected` is a new input, set by the scanner from the git
  pass. The engine already refuses any `blocked_reason` stamped at scan time,
  so no engine change was needed.
- `ScanSummary.blocked_bytes` now includes protected artifacts — they are
  reported, with sizes, and cannot be selected.
- The scan cache is versioned `2 → 3`: a v2 cache holds the old mtime-only age,
  so a project committed to daily would still look dormant until a rescan.
- `Provider::project_root` tells the scanner which directory's age describes an
  artifact, keeping classification provider-driven rather than a table of
  provider ids in the scanner (P1). Global caches return `None` and stay
  unclassified (I9).

## Tests

Every git fixture sets its own identity and configuration explicitly — a
runner has no global git identity, and a user's `core.excludesFile` would
change what `check-ignore` answers. Production does **not** override
`core.excludesFile`: whether a path is ignored must respect the user's own
global ignore; the fixture is what is made explicit. A machine without git
skips the git tests with a message rather than failing for an absent tool.

## Rejected alternatives

- **`git rev-parse --show-toplevel` per artifact.** Rejected: one subprocess
  per artifact, on every scan, to learn something a directory walk answers for
  free.
- **`check-ignore` for every artifact in every repository.** Rejected: a clean
  repository cannot produce `Protected`, so the call would be pure overhead.
- **Refuse every artifact in a dirty repository.** Rejected: it would block
  `node_modules` in every repository a user is actively working in, which is
  precisely where the tool is most useful.
- **Fail open when `git status` cannot be read.** Rejected: treating an
  unreadable repository as clean is how a tracked artifact gets deleted. The
  status is fail-safe (dirty) once git is known to be present.

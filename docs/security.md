# Security design (safety is a core feature)

> DevClean can delete data. Security here means one thing: making sure the
> Clean button never deletes what it shouldn't. Read alongside the G-SAFE-*
> guardrails.
>
> **This file is the design rationale.** The reporting policy — what counts as
> a vulnerability, how to report one, and response times — is in
> [SECURITY.md](../SECURITY.md).

## Threat model (summary)

```text
Frontend = untrusted       → can be bypassed, never trust its input
Backend = authority        → the only side allowed to resolve paths
Provider = controlled capability → declares HOW to delete, not WHAT may be deleted
Safety layer = gatekeeper  → decides allow/deny, always default-deny
```

## Deletion pipeline

```text
User request (ids)
  ↓
CleanupProvider.cleanup_action()  → RemoveDir(path) | RunCommand(prog, args)
  ↓
SafetyValidator (for RemoveDir)
  ├─ path == protected or inside protected? → DENY
  ├─ path an ancestor of protected (e.g. $HOME)? → DENY
  └─ path outside scan roots? → DENY
  ↓
Execute → measure freed = before − after → report per resource
```

## Strictly forbidden

```text
✗ Command::new("sh").arg("-c").arg(<anything>)
✗ accepting paths from the frontend for deletion
✗ silent privilege escalation
✗ destructive defaults (volumes, venvs, images)
✗ hidden cleanup (everything must pass preview + appear in the result)
```

The correct pattern for official-tool commands (Go example):

```rust
// ✅ constant program + args from the provider, no shell, no user input
Command::new("go").args(["clean", "-cache"]).output()
```

## Protected paths (minimal set, `crates/core/src/safety.rs`)

```text
/  ~/Documents  ~/Desktop  ~/Downloads  ~/Pictures  ~/Videos
~/.ssh  ~/.gnupg  ~/.config
```

The `/` exemption from the "inside protected" check is documented in code:
every absolute path is "inside" `/`, so `/` only rejects exact matches —
while the ancestor check still refuses to delete `$HOME` and friends.

## Privileged operations

`allow_privileged_operations` defaults to **OFF**, and the cleanup **engine**
consults it — it is not a stored flag that nothing reads.
`Settings.docker_prune_scope` separately caps what Docker cleanup may
touch, and the two combine:

| `docker_prune_scope` | Needs privileged | What actually runs |
|---|---|---|
| `off` | no | nothing |
| `build-cache` (default) | no | `docker builder prune -af` |
| `build-cache+images` | **yes** | + `docker image prune -af` |
| `all` | **yes** | + `docker volume prune -af` |

Selecting a wider scope without enabling the privileged flag does not widen
anything: the engine falls back to build-cache and the Settings dialog says so
in as many words.

## Confirmation token

`preview_cleanup` measures the exact id set and returns a `preview_token`
(digest of the sorted ids + issue time + nonce, 300s TTL). While
`require_confirmation` is on, `cleanup` refuses without a valid token
covering exactly those ids.

**What this buys**

- A caller cannot delete a set of resources that was never shown to it.
- Sizes cannot silently change between preview and execute.

**What it does NOT buy — and this matters**

This is a *workflow* gate, not an authentication boundary. The frontend runs in
the same process tree and can read any token it is handed, so a fully
compromised frontend can mint its own preview and then call `cleanup`.
Defending against that needs an out-of-process UI, which is out of scope.

The real defences remain `SafetyValidator`, the protected-path list, and the
fixed `(program, args)` pairs. The token makes `require_confirmation` mean
something; it does not make the frontend trustworthy.

## AI data (`opencode`, `claude`)

Two categories, handled differently:

- **Filesystem history** (logs, session diffs, snapshots, tool output, plans,
  conversation transcripts) — age-pruned only. `CleanupAction::PruneOlderThan`
  takes a fixed glob AND a `skip_names` list; the skip list wins over the glob
  at any depth, so `history.jsonl`, `settings.json` and credentials survive
  even under `pattern: "*"` (I10).
- **The SQLite history** — `opencode.db` is often the single largest item on a
  long-lived machine. It is gated by:
  1. a cheap structural probe (open + read the schema catalog);
  2. an **exclusive-lock test** — `BEGIN EXCLUSIVE` with a 0s busy timeout.
     If another process holds the file the lock fails and the cleanup is
     refused;
  3. `PRAGMA foreign_keys = OFF` (opencode's schema declares
     `REFERENCES session(id)` with no `ON DELETE CASCADE`, so enforcing it
     would make the DELETE fail outright);
  4. fixed `&'static str` statements with the age cutoff as a **bound
     parameter**, never interpolated;
  5. `wal_checkpoint(TRUNCATE)` before `VACUUM`, without which freed pages stay
     claimed by the WAL and the file never shrinks.

`sqlite_backup` defaults to **off**: a backup is a full second copy, and
requiring as much free space as the database itself would make the feature
unusable on smaller disks. With it off, the confirmation dialog states the size
that would be needed and the risk, rather than failing at the last moment.

### Why `pgrep` is not the idleness gate

A `pgrep -f opencode` guard looks reasonable and does not work. Measured from a
process launched inside a sandbox:

```text
$ pgrep -fl opencode            # exit 1, nothing — while opencode IS running
$ pgrep -fl <own exact exe path> # exit 1, nothing — cannot see its own process
```

A probe that answers "nothing is running" when it simply cannot see anything is
worse than no probe. DevClean uses the SQLite exclusive lock as the
authoritative gate, which needs no process-table access at all, and keeps
`pgrep` only as a secondary hint whose "unknown" answer is never read as
"safe".

### Why there is no `PRAGMA quick_check`

Measured: `quick_check(1)` on a multi-gigabyte `opencode.db` takes **minutes**.
A multi-minute freeze before a confirmation dialog is not acceptable, and the
check is not needed for safety — SQLite's `DELETE` and `VACUUM` are both
transactional, so a damaged database fails the operation and leaves the
original file untouched.

## Reporting

See [SECURITY.md](../SECURITY.md) for how to report a vulnerability and what
counts as one. The short version for maintainers: a safety bug is P0, never
fixed silently, and always ships with a guardrail that has a real enforcement
mechanism.

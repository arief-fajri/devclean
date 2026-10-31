# Security design (safety is a core feature)

> DevClean can delete data. Security here means one thing: making sure the Clean button never deletes what it shouldn't. Read alongside the G-SAFE-* guardrails. **This file is the design rationale.** The reporting policy — what counts as a vulnerability, how to report one, and response times — is in [SECURITY.md](../SECURITY.md).

## Threat model (summary)

```text
Frontend = untrusted       → can be bypassed, never trust its input
Backend = authority        → the only side allowed to resolve paths
Provider = controlled capability → declares HOW to delete, not WHAT may be deleted
Safety layer = gatekeeper  → decides allow/deny, always default-deny
```

## Webview hardening (defense in depth)

The threat model says the frontend is untrusted. The backend enforces every destructive gate for that reason — but the webview itself is also locked down, so a hypothetical injection has nowhere to go:

```text
default-src 'self'; connect-src ipc: http://ipc.localhost; style-src 'self' 'unsafe-inline'
```

- **No remote code, no network.** `default-src 'self'` means the page may load only its own bundled script, style and assets; `connect-src` is narrowed to the Tauri IPC channel (`ipc:` on macOS, `http://ipc.localhost` elsewhere), so an injected script cannot exfiltrate anything over the network — it can only call the commands the app already exposes.
- **The one relaxation is styles.** `style-src 'unsafe-inline'` is required by the progress bars that set `style="width: …%"` inline. Script has **no** inline or `eval` allowance; the production bundle is a single `'self'` script.
- **Dev is deliberately not locked.** `devCsp` is `null`, because Vite's HMR needs to connect to the dev server and inject modules. The policy above is therefore verified against a **production** build, not the dev server.

Honest scope: this is hardening, not the security boundary. A compromised webview can still invoke the IPC surface — which is exactly why the boundary lives in Rust (`SafetyValidator`, `block_reason_for`, the preview token) and not in the page. The residual gap is recorded as G-SAFE-06 in the [guardrails](development/methodology/guardrails.md).

The same bundle config pins the webview floor: **macOS 12.0 (Monterey)**, `bundle.macOS.minimumSystemVersion`.

## Deletion pipeline

```text
User request (ids)
  ↓
CleanupProvider.cleanup_action()  → RemoveDir(path) | RunCommand(prog, args)
  ↓
SafetyValidator (for RemoveDir)
  ├─ path == protected or inside protected? → DENY
  ├─ path an ancestor of protected (e.g. $HOME)? → DENY
  ├─ path outside scan roots? → DENY
  └─ every check repeats on the canonical (symlink-resolved) form,
     so a scan root or path component that is a symlink into
     protected territory is refused too
  ↓
Execute → measure freed = before − after → report per resource
```

Every branch runs twice: once on the lexical spelling the provider reported, and once on the canonical form (`canonicalize`, best-effort up to the nearest existing ancestor). A `~/Dev → ~/Documents/Dev` symlink used to pass the lexical check and delete real files under `~/Documents`; the canonical pass refuses it.

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

The `/` exemption from the "inside protected" check is documented in code: every absolute path is "inside" `/`, so `/` only rejects exact matches — while the ancestor check still refuses to delete `$HOME` and friends.

## Privileged operations

`allow_privileged_operations` defaults to **OFF**, and the cleanup **engine** consults it — it is not a stored flag that nothing reads. `Settings.docker_prune_scope` separately caps what Docker cleanup may touch, and the two combine:

| `docker_prune_scope` | Needs privileged | What actually runs |
|---|---|---|
| `off` | no | nothing |
| `build-cache` (default) | no | `docker builder prune -af` |
| `build-cache+images` | **yes** | + `docker image prune -af` |
| `all` | **yes** | + `docker volume prune -af` |

Selecting a wider scope without enabling the privileged flag does not widen anything: the engine falls back to build-cache and the Settings dialog says so in as many words.

## Confirmation token

`preview_cleanup` measures the exact id set and returns a `preview_token` (digest of the sorted ids + issue time + nonce, 300s TTL). While `require_confirmation` is on, `cleanup` refuses without a valid token covering exactly those ids.

**What this buys**

- A caller cannot delete a set of resources that was never shown to it.
- Sizes cannot silently change between preview and execute.

**What it does NOT buy — and this matters**

This is a *workflow* gate, not an authentication boundary. The frontend runs in the same process tree and can read any token it is handed, so a fully compromised frontend can mint its own preview and then call `cleanup`. Defending against that needs an out-of-process UI, which is out of scope.

The real defences remain `SafetyValidator`, the protected-path list, and the fixed `(program, args)` pairs. The token makes `require_confirmation` mean something; it does not make the frontend trustworthy.

## AI data (`opencode`, `claude`)

Two categories, handled differently:

- **Filesystem history** (logs, session diffs, snapshots, tool output, plans, conversation transcripts) — age-pruned only. `CleanupAction::PruneOlderThan` takes a fixed glob AND a `skip_names` list; the skip list wins over the glob at any depth, so `history.jsonl`, `settings.json` and credentials survive even under `pattern: "*"` (I10).
- **The SQLite history** — `opencode.db` is often the single largest item on a long-lived machine. It is gated by:
  1. a cheap structural probe (open + read the schema catalog);
  2. an **exclusive-lock test** — `BEGIN EXCLUSIVE` with a 0s busy timeout. If another process holds the file the lock fails and the cleanup is refused;
  3. `PRAGMA foreign_keys = OFF` (opencode's schema declares `REFERENCES session(id)` with no `ON DELETE CASCADE`, so enforcing it would make the DELETE fail outright);
  4. fixed `&'static str` statements with the age cutoff as a **bound parameter**, never interpolated;
  5. `wal_checkpoint(TRUNCATE)` before `VACUUM`, without which freed pages stay claimed by the WAL and the file never shrinks.

`sqlite_backup` defaults to **off**: a backup is a full second copy, and requiring as much free space as the database itself would make the feature unusable on smaller disks. With it off, the confirmation dialog states the size that would be needed and the risk, rather than failing at the last moment.

### Why `pgrep` is not the idleness gate

A `pgrep -f opencode` guard looks reasonable and does not work. Measured from a process launched inside a sandbox:

```text
$ pgrep -fl opencode            # exit 1, nothing — while opencode IS running
$ pgrep -fl <own exact exe path> # exit 1, nothing — cannot see its own process
```

A probe that answers "nothing is running" when it simply cannot see anything is worse than no probe. DevClean uses the SQLite exclusive lock as the authoritative gate, which needs no process-table access at all, and keeps `pgrep` only as a secondary hint whose "unknown" answer is never read as "safe".

### Why a running browser DOES block — the other direction of the same rule

`browser_temp` (Wave D) deletes the code-sign clones Chromium-family browsers leave in `/var/folders/…/X/`. There is no exclusive lock to take on a directory tree, so the liveness probe IS the gate — which looks like it contradicts the section above. It does not: the two halves of a probe answer differently.

- **A POSITIVE `pgrep` match is trustworthy.** Seeing the browser's process proves it is running, the same way a failed `BEGIN EXCLUSIVE` proves someone holds the database. So a confirmed `Running` stamps `blocked_reason: "browser_running"` at scan time (through `block_reason_for`, I11) and the cleanup path **re-probes and refuses live** — a stale scan cannot delete the clone of a browser that started meanwhile.
- **A NEGATIVE match is not.** The sandbox case above — exit 1 because the process table is invisible — must never read as "safe to delete". So `ProcState::Unknown` (no `pgrep`, a timeout, an invisible table) never blocks, never unblocks, and floors the row's safety to `caution`. `NotRunning` is the only state that yields `safe` (**G-SAFE-07**).
- **The gate asks the provider, never an id.** `Provider::held_while_browsers_run()` (true only for `browser_temp`) arms the gate, so a running Chrome cannot block `node_modules` — the same probe value reaching any other provider is inert, and a test pins that.

### Why there is no `PRAGMA quick_check`

Measured: `quick_check(1)` on a multi-gigabyte `opencode.db` takes **minutes**. A multi-minute freeze before a confirmation dialog is not acceptable, and the check is not needed for safety — SQLite's `DELETE` and `VACUUM` are both transactional, so a damaged database fails the operation and leaves the original file untouched.

## Reporting

See [SECURITY.md](../SECURITY.md) for how to report a vulnerability and what counts as one. The short version for maintainers: a safety bug is P0, never fixed silently, and always ships with a guardrail that has a real enforcement mechanism.

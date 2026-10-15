# Roadmap

What is left, not what happened. Shipped work is in [CHANGELOG.md](../../CHANGELOG.md); the reasoning behind it is in [docs/decisions](../decisions/).

## Where we are

Product **0.1.0** is the first public release: macOS only, sizes that match `du`, deletions that move to the Trash, and every gate enforced by the backend rather than by a disabled checkbox. The contract sits at **v0.9.0** after the cache-first startup change.

Two numbers move independently — the product version a user installs, and the contract version of the IPC surface between the bundled frontend and backend. A contract bump does not imply a product release, and the reverse is also true. At 1.0 they are aligned deliberately.

## Version ladder

| Wave | Content | Product | Contract |
|---|---|---|---|
| **A** | First public release | `0.1.0` | `v0.8.0` |
| — | Cache-first startup (shipped as a fix) | `0.1.0` | `v0.9.0` |
| **B** | Git-aware classification | `0.2.0` | `v0.10.0` |
| **C** | Single-pass walk | `0.3.0` | `v0.11.0` |
| **D** | Coverage | `0.4.0` | `v0.12.0` |
| **E** | Stabilisation and documentation | `1.0.0` | `v1.0.0` |

Rules that apply to every wave:

- Every new gate goes through `devclean_core::policy::block_reason_for`. Four controls in this project have already shipped documented and inert; none of them took that route, and no test caught it because the test asserted the control existed rather than that it fired.
- Caller-visible changes update [docs/reference/tauri-contract.md](../reference/tauri-contract.md) and add a regression test.
- A decision someone could reasonably have made differently gets an ADR in `docs/decisions/`.
- The fixture and README screenshots are refreshed when the dashboard's shape changes.
- Full gate before a version bump: `cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace`, and the frontend's `check`, `test` and `build`.
- Changing what may be cleaned needs a human decision before implementation, not after.

---

## Wave B — Git-aware classification

Today "activity" comes from filesystem mtime alone, and nothing knows whether an artifact is committed. Two consequences:

- A project whose cache has not been touched in months looks stale even when its repository is committed to daily, so `retention_days` offers an active project's cache for deletion.
- Nothing stops a cleanup from removing a directory a repository actually tracks. If a project ever commits `dist/` or `target/`, it is offered for deletion like any other cache.

### What changes

A new `crates/providers/src/git.rs`:

```rust
/// Last commit time of the repository containing `root`, or None.
pub fn last_commit_unix(root: &Path) -> Option<u64>;

/// Repository root containing `path`, or None. No subprocess.
pub fn repo_root_for(path: &Path) -> Option<PathBuf>;

/// True when the repository has uncommitted changes.
pub fn is_dirty(root: &Path) -> bool;

/// Which of `candidates` are ignored, in one call per repository.
pub fn ignored_paths(root: &Path, candidates: &[PathBuf]) -> HashSet<PathBuf>;
```

Every call uses `Command::new("git").args(["-C", root, …])` with fixed arguments, never a shell, and carries a deadline.

`repo_root_for` deliberately spawns nothing. `git rev-parse --show-toplevel` per artifact would mean eighty subprocesses for eighty `node_modules`; walking the path's ancestors looking for a `.git` entry — file or directory, because worktrees and submodules use a file — is zero subprocesses and correct for the normal case.

### Activity becomes four tiers

```rust
pub enum Activity { Active, Recent, Stale, Dormant }
//                    < 7d     7–30      30–90     90+
```

Age is the **most recent** of artifact mtime, last commit and project directory mtime. A committed-to-yesterday project stops looking dormant. `Resource.age_days` changes meaning from "project directory age" to this composite, which is the point: `retention_days` then protects projects that are actively worked on.

### The gate

`BlockReason::Protected` has existed since the first release with nothing producing it. This gives it a producer, placed **first** in `block_reason_for`, ahead of the AI and retention gates — deleting a tracked file is data loss regardless of what every other setting says.

| Artifact | Repository | Result |
|---|---|---|
| gitignored | dirty | **deletable** — this is the `node_modules` case |
| gitignored | clean | deletable |
| tracked | clean | deletable — nothing uncommitted is lost |
| tracked | dirty | `protected` |
| untracked, not ignored | dirty | `protected` — a new file is still a file |
| anything | not a repo | unclassified, behaves as today |

The first row is what keeps this from being useless. Refusing every artifact in a dirty repository would make the tool unusable in exactly the repositories people work in.

### Scanner

```
1. group resources by repository root        no subprocess
2. per repository, pool of min(NumCPU, 8):
     git log -1 --format=%ct     → activity
     git status --porcelain      → dirty?
3. only the dirty ones: one `check-ignore --stdin -z` for all their paths
4. stamp activity, last_commit_unix, tracked_in_dirty_repo
```

Step 3 is why this is affordable: a clean repository cannot produce `Protected`, so it never needs `check-ignore`. Stamping is applied serially so results do not depend on worker completion order.

The UI loses `parseAgeDays` and `isStaleSignal` in `lib/format.ts`, which recover the age by parsing the display string; the toolbar filters on `activity` directly.

Closes **G-REL-05** (no timeout on `run_tool`/`go_env`), since the git calls need a deadline helper anyway.

### Tests

Every git setting the fixture depends on is set explicitly, never inherited — the runner has no global git identity, and a user's `core.excludesFile` would otherwise change what `check-ignore` answers. Production does **not** override `core.excludesFile`: whether a path is ignored must respect the user's own global ignore. The fixture is what is made explicit.

| Test | What it proves |
|---|---|
| `a_tracked_artifact_in_a_dirty_repo_is_protected` | the gate fires |
| `a_gitignored_artifact_stays_cleanable_in_a_dirty_repo` | one dirty file does not block `node_modules` |
| `a_clean_repo_protects_nothing` | no false positives |
| `an_untracked_file_in_a_dirty_repo_is_protected` | new files count |
| `last_commit_beats_a_stale_mtime` | composite age, not mtime alone |
| `activity_thresholds_at_the_boundaries` | 7 / 30 / 90 exactly |
| `a_path_outside_any_repo_is_not_classified` | global caches untouched |
| `one_check_ignore_call_per_repo` | batching actually batches |

### Contract

`v0.10.0`, additive: `Resource.activity?`, `Resource.last_commit_unix?`. Behavioural notes without a type change: `age_days` becomes composite, and `blocked_reason: "protected"` can now appear. ADR: `docs/decisions/gitignore-aware-protection.md`.

**Level B** — this changes what may be cleaned.

---

## Wave C — Single-pass walk

### Problem

`node_modules`, Rust `target/`, Python caches, `npm_cache`, `cargo_registry` and `gradle_cache` each walk the scan root independently. One scan over a large dev tree traverses it several times, and the cost grows with every provider Wave D adds.

### The trait split

`Provider` currently mixes two concerns: identity/cleanup, and how to find resources. They are separated.

```rust
/// Identity, cleanup, measurement, policy flags. Knows nothing about scanning.
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn default_safety(&self) -> SafetyLevel;
    fn cleanup_action(&self, path: &Path) -> CleanupAction;
    fn measure(&self, path: &Path) -> Option<u64>;
    // is_user_data, retention_days, required_docker_scope, extra_allowed_roots
}

/// Providers that own their traversal: docker, opencode_db, global_cache, …
pub trait OwnScanner: Provider {
    fn scan(&self, roots: &[PathBuf], cancel: &AtomicBool) -> Vec<Resource>;
}

/// Providers that contribute RULES to one shared walk: node, rust_target, python, …
pub trait TreeRule: Provider {
    fn spec(&self) -> TreeRuleSpec;
    fn materialize(&self, m: &Match, cancel: &AtomicBool) -> Option<Resource>;
}
```

`TreeRuleSpec` carries the per-provider differences that are easy to lose when merging walks: the directory name(s) to match, a sibling file that must exist (`package.json` for `node_modules`), the maximum depth (node uses 6), whether to skip nested matches of the same kind, and additional prune directories.

### What must not regress

- cancellation settles in < 1s (I5, G-SCAN-02)
- symlinks are never followed (G-SCAN-03)
- a scan never silently reports less than the truth (G-SCAN-06)

### Tests

Equivalence is the main mitigation: for the same fixture, the set of resources before and after the refactor must be identical. Plus one test per rule row, so a broken table row fails one test rather than every ecosystem.

This is the largest refactor of the five. One traversal bug breaks every project-tree provider at once.

Contract `v0.11.0`: most likely no type change; internal only.

---

## Wave D — Coverage

Depends on Wave C. Without the shared walk this adds roughly eight full traversals.

### New providers

- `browser_temp` — zombie Chromium code-sign clones (observed at 92 copies / 156 GB in the wild)
- `llm_models` — LM Studio, Hugging Face, Ollama, llamafile
- `ruby`, `flutter`, `android`

### Expanded rules

- node: `.next`, `.turbo`, `.svelte-kit`, React Native
- Python: `.ruff_cache`, `.tox`, `__pypackages__`
- Go: `vendor/` as `caution`
- system caches; Xcode `Archives`, DeviceSupport, CoreSimulator

### The browser_temp hazard

`pgrep` cannot see the process table from a sandboxed process. A failed probe must **raise** the safety level to `caution`, never lower it to `safe`. The inverse deletes the cache of a browser that is currently running.

### Also here

Cross-artifact deduplication for the *total*. Per-artifact figures already deduplicate hard links within one tree; the total does not.

Contract `v0.12.0`, additive: new provider ids in `get_providers` and in scan results.

**Level B** — new providers change what is offered for deletion, and `browser_temp` is destructive.

---

## Wave E — Stabilisation and documentation

The gate for `1.0.0`. Two halves: close what is open, then make the documentation true.

### Close the recorded gaps

- **Component tests for the shell** — Sidebar navigation, page switching, drawer focus, cleanup-dialog keyboard blocking. Needs a DOM environment.
- **XDG Trash** — `trash.rs` currently states Linux is "deliberately not implemented". It becomes implemented but **not verified**: `default_trash_dir()` turns platform-conditional (`$XDG_DATA_HOME/Trash` on Linux, with `files/` and `info/` and one `.trashinfo` per artifact, percent-encoded `Path=` and a `DeletionDate=`), and the per-mount `.Trash-$UID` case is best-effort. CI can test the deterministic part — the bytes of the `.trashinfo` and the encoding — but not the file appearing in a real GNOME or KDE Trash. That is stated rather than hidden, and the platform table is not affected: Linux still builds and tests, and nothing is shipped for it.
- **G-REL-05** — closed in Wave B; verify it did not regress.
- **Cross-artifact deduplication** — if not finished in Wave D.

### Documentation

The repository is public, so the documentation is the product as much as the code is. Before 1.0:

- `roadmap.md` is rewritten again for the post-1.0 horizon, so it stays forward-looking rather than accumulating shipped work.
- Executed plan documents are removed. A plan is an ephemeral artifact: once it ships, the plan goes and its decisions live in `docs/decisions/`.
- `guardrails.md` is reduced to current state; its history belongs in `CHANGELOG.md`.
- Every document is checked against reality — the contract, the contract changelog, the versions in `Cargo.toml` and `tauri.conf.json`, the support table, and the commands in `developing.md`.

### The 1.0 commitment

`1.0.0` promises IPC stability; after it, a breaking change needs `2.0`. The contract reaches `1.0.0` with the product. If that promise cannot be honoured, staying at `0.x` is more honest than making it.

Decisions required before 1.0: whether Windows is supported or permanently unsupported, and whether signing and notarization are pursued (they need an Apple Developer account; until then the first-launch steps are documented).

**Level B** — the stability commitment and the XDG Trash change both alter the cleanup path.

---

## Accepted limitations

Recorded rather than hidden. Each names why it is not fixed.

- **G-SAFE-06 — the confirmation token is a workflow gate, not authentication.** The frontend runs in the same process tree and can read any token it is handed, so a fully compromised frontend can mint its own preview. What it does buy is real: an id set that was never shown to the user is refused. What it cannot buy needs an out-of-process UI. Accepted and documented rather than chased, because full authentication against a frontend in the same process tree is a false promise without OS-level isolation.
- **No bundle is shipped for Linux.** The workspace builds and the full suite runs on Ubuntu in CI, which exercises the Unix-specific code on a second OS, but no `.deb`, `.rpm` or AppImage is produced and the platform is not supported.

## Non-goals

Things this project will deliberately not do, so they are not re-litigated:

- **Permanent deletion by default.** `trash` is the default; `permanent` is an explicit opt-out. A cleanup tool that cannot be undone is one mistake away from losing a week of work.
- **Deleting AI conversation history by default.** It is reported with its size but stays unselectable until the user enables it in Settings.
- **Deleting a database it cannot take exclusively.** No `VACUUM` on a file another process holds open.
- **Touching config or credentials.** `history.jsonl`, `settings.json`, `auth.json` and friends are hard-preserved, whatever pattern matches them.
- **Reporting a number it did not measure.** `freed_bytes` is `before − after`; when the after-state is unmeasurable the result says `0` and why.
- **Claiming a platform it does not test.** See the support table in the README.

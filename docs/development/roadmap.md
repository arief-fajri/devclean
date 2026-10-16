# Roadmap

What is left, not what happened. Shipped work is in [CHANGELOG.md](../../CHANGELOG.md); the reasoning behind it is in [docs/decisions](../decisions/).

## Where we are

Product **0.2.0** is the first release after 0.1.0: macOS only, sizes that match `du`, deletions that move to the Trash, every gate enforced by the backend, and now git-aware classification. Activity is a composite of artifact mtime, last commit and project directory mtime, and a repository's uncommitted work is protected. The contract sits at **v0.10.0**.

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

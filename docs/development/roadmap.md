# Roadmap

What is left, not what happened. Shipped work is in
[CHANGELOG.md](../../CHANGELOG.md).

## Phases

### 1 — Release `0.1.0` (in progress)

The product works end to end. What is missing is the packaging a public
release needs: `LICENSE`, `SECURITY.md`, this changelog, contribution
guidance, a release workflow that turns a tag into a downloadable artifact,
and screenshots taken from a fixture rather than a real machine.

### 2 — Git-aware classification

Today "activity" comes from filesystem mtime alone, and nothing knows whether
an artifact is committed. Two consequences:

- A project whose cache has not been touched in months looks stale even when
  its repository is committed to daily.
- Nothing stops a cleanup from removing a directory a repository actually
  tracks — if a project ever commits `dist/` or `target/`, it is offered for
  deletion like any other cache.

Both are fixed by reading `git log -1 --format=%ct` for the last commit and
`git check-ignore` for whether the artifact is ignored. A **tracked** artifact
in a repository with uncommitted changes becomes `blocked_reason: protected`;
a gitignored one stays deletable, so one dirty file cannot block a cleanup.

Activity becomes four tiers (`active` < 7d, `recent` 7–30, `stale` 30–90,
`dormant` 90+), taken as the most recent of artifact mtime, last commit and
project directory mtime.

Must go through `devclean_core::policy::block_reason_for` like every other
gate. Four controls in this project have already shipped documented and inert;
none of them took that route.

### 3 — Single-pass walk

`node_modules`, Rust `target/` and Python caches each walk the scan root
independently, so a scan over `~/VOLANTIS` traverses it three times. The three
become declarative rule tables executed by one shared traversal, which also
makes a new project ecosystem a table rather than a module.

This is the prerequisite for phase 4: without it, adding five more
project-tree providers means eight full traversals.

### 4 — Coverage

New providers: `browser_temp` (zombie Chromium code-sign clones — observed at
92 copies / 156 GB in the wild), `llm_models` (LM Studio, Hugging Face, Ollama,
llamafile), `ruby`, `flutter`, `android`. Expanded rules for node (`.next`,
`.turbo`, `.svelte-kit`, React Native), Python (`.ruff_cache`, `.tox`,
`__pypackages__`), Go (`vendor/` as caution), system caches and Xcode
(`Archives`, DeviceSupport, CoreSimulator).

`browser_temp` needs care: `pgrep` cannot see the process table from a
sandboxed process, so a failed probe must raise the safety level to `caution`,
never lower it to `safe`.

Also here: cross-artifact deduplication for the *total* (per-artifact figures
already deduplicate hard links within one artifact).

## Known gaps

Recorded rather than hidden. Each names why it is not fixed yet.

- **G-REL-05 — scan-time CLI probes have no timeout.** `run_tool` and `go_env`
  spawn processes without a deadline, and `tokio::time::timeout` around a
  `spawn_blocking` task does not kill the blocking thread — it only stops
  awaiting. Bounded in practice by `PROVIDER_SCAN_TIMEOUT` returning, but a
  hung probe leaves a thread parked.
- **G-SAFE-06 — the confirmation token is a workflow gate, not authentication.**
  The frontend runs in the same process tree and can read any token it is
  handed, so a fully compromised frontend can mint its own preview. What it does
  buy is real: an id set that was never shown to the user is refused. What it
  cannot buy needs an out-of-process UI.
- **Linux/XDG Trash.** macOS only by design. The freedesktop format needs
  `.trashinfo` files for the artifact to appear in a file manager's Trash, and
  half-implementing that is worse than not claiming it.
- **Component tests for the shell.** Sidebar navigation, page switching, drawer
  focus and cleanup-dialog keyboard blocking are not covered. Blocked on a DOM
  environment; the rules that could be tested without one live in `lib/nav.ts`
  and `architecture.test.ts`, but a visual regression is still invisible to CI.

## Non-goals

Things this project will deliberately not do, so they are not re-litigated:

- **Permanent deletion by default.** `trash` is the default; `permanent` is an
  explicit opt-out. A cleanup tool that cannot be undone is one mistake away
  from losing a week of work.
- **Deleting AI conversation history by default.** It is reported with its size
  but stays unselectable until the user enables it in Settings.
- **Deleting a database it cannot take exclusively.** No `VACUUM` on a file
  another process holds open.
- **Touching config or credentials.** `history.jsonl`, `settings.json`,
  `auth.json` and friends are hard-preserved, whatever pattern matches them.
- **Reporting a number it did not measure.** `freed_bytes` is `before − after`;
  when the after-state is unmeasurable the result says `0` and why.
- **Claiming a platform it does not test.** See the support table in the README.

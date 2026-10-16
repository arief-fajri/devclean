# ADR: one traversal for the project-tree providers

**Status:** accepted (product 0.3.0, contract v0.11.0) **Supersedes:** the per-provider walks of `node_modules`, `rust_target` and `python_cache`

## Context

Three providers answer the same question — "where are the regenerable artifacts under these roots?" — by walking the same trees:

- `node_modules`: a directory named `node_modules` whose parent holds `package.json`, to depth 6.
- `rust_target`: a directory named `target` whose parent holds `Cargo.toml`, to depth 5.
- `python_cache`: a directory named `__pycache__`, `.pytest_cache` or `.mypy_cache`, to depth 6.

Each walked every scan root independently, and each then measured its matches by walking **into** them again (`dir_size_detail_checked`). One scan of a large dev tree therefore traversed it three times, and every matched artifact a fourth time for its size. The cost grows linearly with each rule Wave D adds.

## Decision

`Provider` is split into three traits, and the three tree providers contribute **rules** to one shared traversal instead of walking on their own.

```rust
pub trait Provider: Send + Sync { /* identity, cleanup, measure, policy flags */ }
pub trait OwnScanner: Provider { fn scan(&self, roots, cancel) -> Vec<Resource>; }
pub trait TreeRule: Provider {
    fn spec(&self) -> TreeRuleSpec;
    fn materialize(&self, m: &Match, size: &DirSize) -> Option<Resource>;
}
```

`all_providers()` hands out `Scannable` (`Own(Arc<dyn OwnScanner>)` or `Tree(Arc<dyn TreeRule>)`); everything that never scans reaches the plain `Provider` view through `Scannable::provider()`.

`walk_tree_rules(roots, rules, cancel)` walks each root once and, per entry:

1. **Matches** every rule whose `dir_names` include the entry, at `depth <= spec.max_depth`, with the optional `sibling_file` present and `skip_nested` satisfied. `rust_target` is re-expressed from "a `Cargo.toml` file with a sibling `target/`" to "a `target/` directory with a sibling `Cargo.toml`" — siblings sit at the same walkdir depth, so every depth boundary is unchanged.
2. **Sizes** a match by opening a `Scope` (a `SizeScope`) at that directory, feeding it every file beneath, and closing it when the walk steps out. Scopes nest, so an artifact inside another is counted into both — the same double-count the old per-artifact measurements produced. A matched artifact is sized in **full**: the walk keeps descending past every rule's `max_depth`, because those bytes are the resource. Away from any match it **prunes** at `max(spec.max_depth)` — no rule can match deeper.
3. **Skips** symlinks (`follow_links(false)`, `lstat` matching, G-SCAN-03) and unreadable entries (G-SCAN-06), and checks `cancel` before every entry (G-SCAN-02).

Both the walk and `dir_size_detail_checked` feed the **same `SizeScope`**, so hard-link dedup, apparent accounting and symlink handling cannot drift between the two paths.

### Progress stays per provider

The UI still sees one provider at a time. The tree rules ride one task, but the manager fires one `scan_provider_started` per rule when the group enters the walk, and one `scan_progress` per rule when it settles — on every exit path (success, cancel, timeout, panic), so the bar can never stall below its total (G-SCAN-05). `total` counts own and tree providers alike. This batching is the only caller-visible change, and it is written down in `docs/reference/tauri-contract.md` (v0.11.0).

## Consequences

- One scan over a dev-tree-heavy root does one traversal instead of three plus per-match re-walks. `bench_tree_walk` isolates the tree rules from the fixed-path caches: **2.1× the old three walks, 1.6× the old three walks plus per-match re-measurement**. The whole-scan bench (`bench_scan`) is dominated by the real `$HOME` caches and barely moves, which is why the isolated bench exists.
- The reported rows and their sizes are unchanged: the equivalence suite needed no assertion changed — only its driver. Every row's bytes are asserted equal to a standalone `dir_size_detail`, and an artifact matched at depth 2 is asserted sized to depth 11.
- One deliberate edge change: a `target/` that is itself a symlink is no longer reported (the old `target.is_dir()` followed it). `lstat`-only matching is what G-SCAN-03 asks for; no fixture covered the old behaviour.
- The tree group takes **one** semaphore slot, not three (G-SCAN-01 still holds: at most four tasks in parallel).
- Wave D's new rules are `TreeRuleSpec`s and ride this walk; the traversal count does not grow with them.

## Rejected alternatives

- **Keep per-artifact measurement.** Rejected: it re-walks every matched artifact — the dominant cost — and is exactly what this removes.
- **One global depth limit for the shared walk.** Rejected: it would truncate artifact sizes. The match is depth-gated; the size is not.
- **A single global hard-link dedup set across all artifacts.** Rejected: an inode shared between two artifacts is freed when either is deleted, so each artifact must count it (same reasoning as [`allocated-vs-apparent.md`](allocated-vs-apparent.md)).
- **Move every provider onto the shared walk.** Rejected: Docker, git classification and the fixed-path caches have their own semantics (sentinel paths, subprocesses, single-dir measurements) and no shared traversal to join.
- **Emit progress on a timer to interleave the tree ticks.** Rejected: timing-dependent events are untestable; the group ticks together, deterministically, at settle.

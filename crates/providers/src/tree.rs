//! Rules for the shared project-tree walk (Wave C).
//!
//! `node_modules`, `rust_target` and `python_cache` used to walk the scan
//! roots once apiece, each re-traversing the same dev tree. They now
//! contribute a [`TreeRuleSpec`] — what directory names to match, what
//! sibling file must exist, how deep the match may sit — and the scanner
//! walks each root exactly once, offering every entry to every rule.
//!
//! Matching, skipping and depth gating live in [`walk_tree_rules`];
//! measurement stays inside [`TreeRule::materialize`] until sizing folds
//! into the same traversal in the next phase.

use crate::provider::Provider;
use devclean_core::Resource;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The per-provider differences the shared walk must not lose when it
/// merges three walks into one.
#[derive(Debug, Clone)]
pub struct TreeRuleSpec {
    /// Directory names this rule matches (`node_modules`; `target`;
    /// `__pycache__`, `.pytest_cache`, `.mypy_cache`).
    pub dir_names: &'static [&'static str],
    /// A file that must exist in the matched directory's PARENT for the
    /// match to count: `package.json` for node, `Cargo.toml` for rust.
    /// `None` matches on the directory name alone (python).
    pub sibling_file: Option<&'static str>,
    /// Maximum depth, counted from the scan root the way `walkdir` counts
    /// it (the root itself is depth 0): 6 for node and python, 5 for rust.
    pub max_depth: usize,
    /// A match nested inside another match of the same rule is not
    /// reported — its bytes still belong to the enclosing artifact
    /// (`node_modules` inside `node_modules`).
    pub skip_nested: bool,
    /// Directories the walk must never descend into for this rule.
    /// Empty today; reserved for rules Wave D adds.
    pub extra_prune_dirs: &'static [&'static str],
}

/// One match the walk offers to a rule: a directory that satisfied the
/// rule's spec.
#[derive(Debug, Clone)]
pub struct Match {
    /// The matched directory.
    pub dir: PathBuf,
    /// Its depth from the scan root (walkdir numbering: root = 0).
    pub depth: usize,
}

/// Providers that contribute rules to the shared walk instead of walking
/// the scan roots themselves.
pub trait TreeRule: Provider {
    fn spec(&self) -> TreeRuleSpec;

    /// Turn one match into a resource, or `None` to skip it (measurement
    /// failed, empty artifact, …). Returned [`Resource`]s carry
    /// `id == ""`, like every other scan path.
    fn materialize(&self, m: &Match, cancel: &AtomicBool) -> Option<Resource>;
}

/// Walk every `root` exactly once, offering every entry to every rule.
///
/// This replaces the three independent project-tree walks. The traversal
/// mirrors them entry for entry so the reported resource set cannot drift:
/// symlinks are never followed (`follow_links(false)`, G-SCAN-03), unreadable
/// entries are skipped rather than fatal (G-SCAN-06), `cancel` is checked
/// before every entry so a set flag settles in <1s (G-SCAN-02), and each
/// rule enforces its own `max_depth` even though the shared traversal goes
/// as deep as the deepest rule.
///
/// One group of resources is returned per rule, in input order — the
/// registry assigns ids per provider, so rows must stay grouped.
pub fn walk_tree_rules(
    roots: &[PathBuf],
    rules: &[Arc<dyn TreeRule>],
    cancel: &AtomicBool,
) -> Vec<Vec<Resource>> {
    let mut groups: Vec<Vec<Resource>> = vec![Vec::new(); rules.len()];
    if rules.is_empty() {
        return groups;
    }
    let specs: Vec<TreeRuleSpec> = rules.iter().map(|r| r.spec()).collect();
    let max_depth = specs.iter().map(|s| s.max_depth).max().unwrap_or(0);

    for root in roots {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let iter = walkdir::WalkDir::new(root)
            .follow_links(false)
            .max_depth(max_depth)
            .into_iter();
        for entry in iter {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            // An entry the walk cannot read is skipped, not fatal.
            let Ok(e) = entry else { continue };
            // Every current rule matches on a directory name; files are
            // only relevant as the sibling check below.
            if !e.file_type().is_dir() {
                continue;
            }
            for (i, spec) in specs.iter().enumerate() {
                if e.depth() > spec.max_depth {
                    continue;
                }
                if !spec
                    .dir_names
                    .contains(&e.file_name().to_str().unwrap_or(""))
                {
                    continue;
                }
                if spec.skip_nested
                    && e.path()
                        .parent()
                        .map(|p| {
                            p.components().any(|c| {
                                spec.dir_names
                                    .contains(&c.as_os_str().to_str().unwrap_or(""))
                            })
                        })
                        .unwrap_or(false)
                {
                    continue;
                }
                let sibling_ok = match spec.sibling_file {
                    None => true,
                    Some(file) => e.path().parent().unwrap_or(e.path()).join(file).is_file(),
                };
                if !sibling_ok {
                    continue;
                }
                let m = Match {
                    dir: e.path().to_path_buf(),
                    depth: e.depth(),
                };
                if let Some(resource) = rules[i].materialize(&m, cancel) {
                    groups[i].push(resource);
                }
            }
        }
    }
    groups
}

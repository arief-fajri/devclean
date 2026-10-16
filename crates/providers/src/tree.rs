//! Rules for the shared project-tree walk (Wave C).
//!
//! `node_modules`, `rust_target` and `python_cache` used to walk the scan
//! roots once apiece, each re-traversing the same dev tree. They now
//! contribute a [`TreeRuleSpec`] — what directory names to match, what
//! sibling file must exist, how deep the match may sit — and the scanner
//! walks each root exactly once, offering every entry to every rule.
//!
//! The walk itself lands with the shared-walk phase; this module holds the
//! vocabulary both phases share, so the rule implementations can be written
//! against a stable shape.

use crate::provider::Provider;
use devclean_core::Resource;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

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

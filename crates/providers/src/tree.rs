//! Rules for the shared project-tree walk (Wave C).
//!
//! `node_modules`, `rust_target` and `python_cache` used to walk the scan
//! roots once apiece, each re-traversing the same dev tree, and then each
//! measured its matches by walking the artifact AGAIN. They now contribute a
//! [`TreeRuleSpec`] — what directory names to match, what sibling file must
//! exist (any of a fixed list), how deep the match may sit — and the scanner
//! walks each root once,
//! offering every entry to every rule and sizing each match inside the same
//! traversal.
//!
//! Matching, skipping, depth gating and sizing all live in
//! [`walk_tree_rules`]; a rule only turns a [`Match`] plus its measured
//! [`DirSize`] into a [`Resource`].

use crate::provider::{DirSize, Provider, SizeScope};
use devclean_core::Resource;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The per-provider differences the shared walk must not lose when it
/// merges three walks into one.
#[derive(Debug, Clone)]
pub struct TreeRuleSpec {
    /// Directory names this rule matches (`node_modules`; `target`;
    /// `__pycache__`, `.pytest_cache`, `.mypy_cache`).
    pub dir_names: &'static [&'static str],
    /// Files of which AT LEAST ONE must exist in the matched directory's
    /// PARENT for the match to count: `package.json` for node, `Cargo.toml`
    /// for rust, any of the four gradle markers for android. An empty list
    /// matches on the directory name alone (python).
    pub sibling_files: &'static [&'static str],
    /// Maximum depth, counted from the scan root the way `walkdir` counts
    /// it (the root itself is depth 0): 6 for node and python, 5 for rust.
    /// Depth gates the MATCH only — a matched artifact is always sized in
    /// full, however deep its contents run.
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

    /// Turn one match and its measured size into a resource, or `None` to
    /// skip it (an empty artifact, for the rules that skip those).
    /// Returned [`Resource`]s carry `id == ""`, like every other scan path.
    fn materialize(&self, m: &Match, size: &DirSize) -> Option<Resource>;
}

/// An artifact currently being sized by the walk: opened when a rule
/// matches a directory, fed every file beneath it, closed when the walk
/// leaves the directory again.
struct Scope {
    rule: usize,
    dir: PathBuf,
    depth: usize,
    size: SizeScope,
}

/// Walk every `root` exactly once, offering every entry to every rule and
/// sizing each match in the same pass.
///
/// This replaces three walks plus three measurements. The traversal mirrors
/// the per-provider walks entry for entry so the reported rows and their
/// sizes cannot drift: symlinks are never followed (`follow_links(false)`,
/// G-SCAN-03), unreadable entries are skipped rather than fatal (G-SCAN-06),
/// `cancel` is checked before every entry and a partially sized artifact is
/// dropped rather than reported (G-SCAN-02), and each rule enforces its own
/// `max_depth` on the match.
///
/// A matched artifact is sized in FULL: the walk keeps descending into it
/// even past every rule's `max_depth`, because those bytes are the resource.
/// Away from any match it prunes at `max(spec.max_depth)` — no rule could
/// match deeper, so there is nothing left to look for.
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
        let mut scopes: Vec<Scope> = Vec::new();
        let mut iter = walkdir::WalkDir::new(root).follow_links(false).into_iter();
        while let Some(entry) = iter.next() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            // An entry the walk cannot read is skipped, not fatal.
            let Ok(e) = entry else { continue };
            let path = e.path();

            // Close every scope this entry has left. Depth-first order means
            // an open scope is closed exactly when the walk steps out of its
            // directory; the innermost is always the first to go.
            while let Some(scope) = scopes.last() {
                if path.starts_with(&scope.dir) {
                    break;
                }
                let scope = scopes.pop().expect("checked by last()");
                let m = Match {
                    dir: scope.dir,
                    depth: scope.depth,
                };
                if let Some(resource) = rules[scope.rule].materialize(&m, &scope.size.finish()) {
                    groups[scope.rule].push(resource);
                }
            }

            if e.file_type().is_dir() {
                for (i, spec) in specs.iter().enumerate() {
                    if e.depth() > spec.max_depth {
                        continue;
                    }
                    if !spec.dir_names.contains(&name_of(&e)) {
                        continue;
                    }
                    if spec.skip_nested && parent_has_dir_name(path, spec.dir_names) {
                        continue;
                    }
                    let parent = path.parent().unwrap_or(path);
                    let sibling_ok = spec.sibling_files.is_empty()
                        || spec
                            .sibling_files
                            .iter()
                            .any(|file| parent.join(file).is_file());
                    if !sibling_ok {
                        continue;
                    }
                    scopes.push(Scope {
                        rule: i,
                        dir: path.to_path_buf(),
                        depth: e.depth(),
                        size: SizeScope::default(),
                    });
                }
                // Past every rule's match depth with nothing open: no entry
                // below can match or belong to an artifact. Prune the subtree.
                if e.depth() >= max_depth && scopes.is_empty() {
                    iter.skip_current_dir();
                }
            } else if !scopes.is_empty() {
                // A file inside one or more artifacts: feed every scope it
                // belongs to (an artifact inside another counts in both).
                if let Ok(m) = e.metadata() {
                    for scope in &mut scopes {
                        scope.size.add(&m);
                    }
                }
            }
        }

        // A cancel mid-artifact drops it: a partial size is a wrong size,
        // and the caller already treats a cancelled run as incomplete.
        if cancel.load(Ordering::Relaxed) {
            scopes.clear();
        }
        for scope in scopes.drain(..) {
            let m = Match {
                dir: scope.dir,
                depth: scope.depth,
            };
            if let Some(resource) = rules[scope.rule].materialize(&m, &scope.size.finish()) {
                groups[scope.rule].push(resource);
            }
        }
    }
    groups
}

fn name_of(e: &walkdir::DirEntry) -> &str {
    e.file_name().to_str().unwrap_or("")
}

/// True when any directory above `path` carries one of `names` — the old
/// providers' "don't report a nested match" test, kept component-exact.
fn parent_has_dir_name(path: &Path, names: &[&str]) -> bool {
    path.parent()
        .map(|p| {
            p.components()
                .any(|c| names.contains(&c.as_os_str().to_str().unwrap_or("")))
        })
        .unwrap_or(false)
}

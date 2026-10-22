//! Python cache provider. Finds regenerable bytecode/test caches
//! (`__pycache__/`, `.pytest_cache/`, `.mypy_cache/`, `.ruff_cache/`,
//! `.tox/`, `__pypackages__/`) under scan roots and deletes the directory
//! only.
//!
//! Virtualenvs (`.venv/`, `venv/`, …) are NEVER reported: they may hold
//! non-reinstallable state and count as significant destructive scope
//! (G-SAFE-03). `.tox` is not a user venv — it is tox's own environment
//! cache, recreated by the next `tox` run (Wave D2).

use super::provider::{
    age_days, impact, measure_dir, project_detail, CleanupAction, DirSize, Provider,
};
use crate::tree::{Match, TreeRule, TreeRuleSpec};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};

pub struct PythonCacheProvider;

/// Cache dir names we own. Anything else (notably user venvs) is ignored.
const CACHE_DIRS: &[&str] = &[
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".tox",
    "__pypackages__",
];

impl Provider for PythonCacheProvider {
    fn id(&self) -> &'static str {
        "python_cache"
    }
    fn name(&self) -> &'static str {
        "Python caches"
    }
    fn default_safety(&self) -> SafetyLevel {
        SafetyLevel::Safe
    }

    fn measure(&self, path: &Path) -> Option<u64> {
        measure_dir(path)
    }

    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        CleanupAction::RemoveDir(path.to_path_buf())
    }

    /// A Python cache dir sits one level under the project that owns it.
    fn project_root(&self, artifact: &Path) -> Option<PathBuf> {
        artifact.parent().map(Path::to_path_buf)
    }
}
impl TreeRule for PythonCacheProvider {
    fn spec(&self) -> TreeRuleSpec {
        TreeRuleSpec {
            dir_names: CACHE_DIRS,
            sibling_files: &[],
            max_depth: 6,
            // The old walk reported __pycache__ wherever it sat, including
            // one inside another — characterized by the equivalence suite,
            // so the shared walk keeps it.
            skip_nested: false,
            extra_prune_dirs: &[],
        }
    }

    fn materialize(&self, m: &Match, size: &DirSize) -> Option<Resource> {
        if size.allocated == 0 {
            return None;
        }
        let dir = &m.dir;
        let name = dir.file_name()?.to_string_lossy().into_owned();
        let project = dir.parent().unwrap_or(dir);
        let project_name = project
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "?".to_string());
        Some(Resource {
            id: String::new(),
            provider: self.id().to_string(),
            name: format!("{project_name}/{name}"),
            path: dir.to_path_buf(),
            size_bytes: size.allocated,
            reclaimable_bytes: size.allocated,
            apparent_bytes: size.apparent_if_material(),
            safety: SafetyLevel::Safe,
            detail: Some(project_detail(&project_name, project, size.allocated)),
            age_days: age_days(project),
            activity: None,
            last_commit_unix: None,
            blocked_reason: None,
            blocked_detail: None,
            impact: impact(
                "Bytecode and test caches only; regenerated automatically.",
                Some("First run recompiles"),
                None,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::walk_tree_rules;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    fn fixture_root(tag: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("devclean-test-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&root);
        let proj = root.join("proj");
        std::fs::create_dir_all(proj.join("__pycache__")).unwrap();
        std::fs::write(proj.join("__pycache__/mod.pyc"), "x".repeat(1000)).unwrap();
        // A venv must NEVER be reported, even though it is a directory.
        std::fs::create_dir_all(proj.join(".venv/lib")).unwrap();
        std::fs::write(proj.join(".venv/pyvenv.cfg"), "x".repeat(1000)).unwrap();
        root
    }

    #[test]
    fn finds_pycache_but_never_venvs() {
        let root = fixture_root("py");
        let cancel = AtomicBool::new(false);
        let found = walk_tree_rules(
            std::slice::from_ref(&root),
            &[Arc::new(PythonCacheProvider)],
            &cancel,
        );
        assert_eq!(found.len(), 1, "one group per rule");
        let found: Vec<Resource> = found.into_iter().flatten().collect();
        assert_eq!(found.len(), 1, "expected only __pycache__: {found:?}");
        assert!(found[0].name.ends_with("__pycache__"));
        assert!(found[0].size_bytes >= 1000);
        let impact = found[0].impact.as_ref().expect("impact must be set");
        assert!(!impact.consequence.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}

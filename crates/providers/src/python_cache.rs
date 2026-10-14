//! Python cache provider. Finds regenerable bytecode/test caches
//! (`__pycache__/`, `.pytest_cache/`, `.mypy_cache/`) under scan roots and
//! deletes the directory only.
//!
//! Virtualenvs (`.venv/`, `venv/`, …) are NEVER reported: they may hold
//! non-reinstallable state and count as significant destructive scope
//! (G-SAFE-03).

use super::provider::{
    age_days, dir_size_checked, impact, measure_dir, project_detail, walk_cancelled, CleanupAction,
    Provider,
};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct PythonCacheProvider;

/// Cache dir names we own. Anything else (notably venvs) is ignored.
const CACHE_DIRS: &[&str] = &["__pycache__", ".pytest_cache", ".mypy_cache"];

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

    fn scan(&self, roots: &[PathBuf], cancel: &AtomicBool) -> Vec<Resource> {
        let mut out = Vec::new();
        for root in roots {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            walk_cancelled(root, 6, cancel, |entry| {
                if !entry.file_type().is_dir() {
                    return;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                if !CACHE_DIRS.contains(&name.as_str()) {
                    return;
                }
                let dir = entry.path();
                let Some(size) = dir_size_checked(dir, cancel) else {
                    return;
                };
                if size == 0 {
                    return;
                }
                let project = dir.parent().unwrap_or(dir);
                let project_name = project
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "?".to_string());
                out.push(Resource {
                    id: String::new(),
                    provider: self.id().to_string(),
                    name: format!("{project_name}/{name}"),
                    path: dir.to_path_buf(),
                    size_bytes: size,
                    reclaimable_bytes: size,
                    safety: SafetyLevel::Safe,
                    detail: Some(project_detail(&project_name, project, size)),
                    age_days: age_days(project),
                    retention_eligible: true,
                    impact: impact(
                        "Bytecode and test caches only; regenerated automatically.",
                        Some("First run recompiles"),
                        None,
                    ),
                });
            });
        }
        out
    }

    fn measure(&self, path: &Path) -> Option<u64> {
        measure_dir(path)
    }

    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        CleanupAction::RemoveDir(path.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let found = PythonCacheProvider.scan(std::slice::from_ref(&root), &cancel);
        assert_eq!(found.len(), 1, "expected only __pycache__: {found:?}");
        assert!(found[0].name.ends_with("__pycache__"));
        assert!(found[0].size_bytes >= 1000);
        let impact = found[0].impact.as_ref().expect("impact must be set");
        assert!(!impact.consequence.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}

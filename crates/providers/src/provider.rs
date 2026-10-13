//! The `Provider` trait: the single extension point for new resource kinds.
//!
//! To add a provider (e.g. Python cache): implement this trait, add it to
//! [`crate::all_providers`], done. No `if/else` chains elsewhere.

use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};

/// How a resource must be removed. Providers declare *what* removal means;
/// the cleanup engine + safety validator decide *whether* it may run
/// (guardrails G-SAFE-01..04).
#[derive(Debug, Clone)]
pub enum CleanupAction {
    /// Delete the directory tree (e.g. `node_modules`, `target/`).
    RemoveDir(PathBuf),
    /// Run an official tool with fixed args (e.g. `go clean -cache`).
    /// Never constructed from user input (G-SAFE-02).
    RunCommand {
        program: &'static str,
        args: &'static [&'static str],
    },
}

pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn default_safety(&self) -> SafetyLevel;

    /// Find resources under `roots`. Returned [`Resource`]s carry `id == ""`;
    /// the [`crate::ProviderRegistry`] assigns opaque ids after scanning so
    /// providers never invent the security-sensitive identifier.
    fn scan(&self, roots: &[PathBuf]) -> Vec<Resource>;

    /// How `path` (previously returned by [`Provider::scan`]) must be
    /// removed. Called only after the safety validator approves the path.
    fn cleanup_action(&self, path: &Path) -> CleanupAction;
}

/// Sum of file sizes under `path`. Symlinks are not followed (a symlink farm
/// must never explode a scan, nor cause deletion outside the tree).
pub fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

/// Age of `path` in whole days, from filesystem mtime. This is a display
/// signal only — never a safety verdict (a stale project is not automatically
/// safe to delete; the validator still decides).
pub fn age_days(path: &Path) -> Option<u64> {
    let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
    let age = std::time::SystemTime::now()
        .duration_since(mtime)
        .unwrap_or_default();
    Some(age.as_secs() / 86_400)
}

/// Human age suffix for the `detail` string, e.g. `" · 143d old"`.
/// Empty when the mtime is unreadable.
pub fn age_suffix(path: &Path) -> String {
    match age_days(path) {
        Some(d) if d < 30 => format!(" · {d}d old"),
        Some(d) if d < 365 => format!(" · {}mo old", d / 30),
        Some(d) => format!(" · {}y old", d / 365),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn age_of_fresh_dir_is_zero_days() {
        let dir = std::env::temp_dir().join(format!("devclean-age-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(age_days(&dir), Some(0));
        assert_eq!(age_suffix(&dir), " · 0d old");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn age_of_missing_path_is_none() {
        let missing = std::path::PathBuf::from("/definitely/not/a/devclean/path");
        assert_eq!(age_days(&missing), None);
        assert_eq!(age_suffix(&missing), "");
    }
}

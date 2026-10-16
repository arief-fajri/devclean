//! Rust `target/` provider. Finds `Cargo.toml` projects, then their
//! `target/` dir. Regenerable via `cargo build`, hence `Safe`.

use super::provider::{
    age_days, dir_size_detail_checked, impact, measure_dir, project_detail, walk_cancelled,
    CleanupAction, Provider,
};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct RustTargetProvider;

impl Provider for RustTargetProvider {
    fn id(&self) -> &'static str {
        "rust_target"
    }
    fn name(&self) -> &'static str {
        "Rust target"
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
            walk_cancelled(root, 5, cancel, |entry| {
                if !(entry.file_type().is_file() && entry.file_name() == "Cargo.toml") {
                    return;
                }
                let project = entry.path().parent().unwrap_or(Path::new("/"));
                let target = project.join("target");
                if !target.is_dir() {
                    return;
                }
                let Some(size) = dir_size_detail_checked(&target, cancel) else {
                    return;
                };
                if size.allocated == 0 {
                    return;
                }
                let project_name = project
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "?".to_string());
                out.push(Resource {
                    id: String::new(),
                    provider: self.id().to_string(),
                    name: format!("{project_name}/target"),
                    path: target,
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
                        "Build artifacts only; your source code is untouched.",
                        Some("First build after cleanup recompiles from scratch"),
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

    /// `target/` sits one level under the Cargo project that owns it.
    fn project_root(&self, artifact: &Path) -> Option<PathBuf> {
        artifact.parent().map(Path::to_path_buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_target_and_describes_impact() {
        let root = std::env::temp_dir().join(format!("devclean-test-{}-rs", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let proj = root.join("proj");
        std::fs::create_dir_all(proj.join("target/debug")).unwrap();
        std::fs::write(proj.join("Cargo.toml"), "[package]\nname = \"proj\"").unwrap();
        std::fs::write(proj.join("target/debug/app"), "x".repeat(1000)).unwrap();
        let cancel = AtomicBool::new(false);
        let found = RustTargetProvider.scan(std::slice::from_ref(&root), &cancel);
        assert_eq!(found.len(), 1, "expected exactly one hit: {found:?}");
        assert!(found[0].name.ends_with("/target"));
        let impact = found[0].impact.as_ref().expect("impact must be set");
        assert!(!impact.consequence.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}

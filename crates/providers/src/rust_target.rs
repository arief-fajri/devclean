//! Rust `target/` provider. Finds `Cargo.toml` projects, then their
//! `target/` dir. Regenerable via `cargo build`, hence `Safe`.

use super::provider::{dir_size_checked, project_detail, walk_cancelled, CleanupAction, Provider};
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
                let Some(size) = dir_size_checked(&target, cancel) else {
                    return;
                };
                if size == 0 {
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
                    size_bytes: size,
                    reclaimable_bytes: size,
                    safety: SafetyLevel::Safe,
                    detail: Some(project_detail(&project_name, project, size)),
                });
            });
        }
        out
    }

    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        CleanupAction::RemoveDir(path.to_path_buf())
    }
}

//! Rust `target/` provider. Finds `Cargo.toml` projects, then their
//! `target/` dir. Regenerable via `cargo build`, hence `Safe`.

use super::provider::{age_suffix, dir_size, CleanupAction, Provider};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};

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

    fn scan(&self, roots: &[PathBuf]) -> Vec<Resource> {
        let mut out = Vec::new();
        for root in roots {
            for entry in walkdir::WalkDir::new(root)
                .follow_links(false)
                .max_depth(5)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file() && e.file_name() == "Cargo.toml")
            {
                let project = entry.path().parent().unwrap_or(Path::new("/"));
                let target = project.join("target");
                if !target.is_dir() {
                    continue;
                }
                let size = dir_size(&target);
                if size == 0 {
                    continue;
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
                    detail: Some(format!("project: {project_name}{}", age_suffix(project))),
                });
            }
        }
        out
    }

    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        CleanupAction::RemoveDir(path.to_path_buf())
    }
}

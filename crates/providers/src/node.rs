//! `node_modules` provider (MVP flagship).
//!
//! Finds `node_modules/` directories whose parent holds a `package.json`.
//! Cleans by deleting the directory only — lockfiles are never touched.

use super::provider::{age_suffix, dir_size, CleanupAction, Provider};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};

pub struct NodeModulesProvider;

impl Provider for NodeModulesProvider {
    fn id(&self) -> &'static str {
        "node_modules"
    }
    fn name(&self) -> &'static str {
        "node_modules"
    }
    fn default_safety(&self) -> SafetyLevel {
        SafetyLevel::Safe
    }

    fn scan(&self, roots: &[PathBuf]) -> Vec<Resource> {
        let mut out = Vec::new();
        for root in roots {
            for entry in walkdir::WalkDir::new(root)
                .follow_links(false)
                .max_depth(6)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_dir() && e.file_name() == "node_modules")
            {
                let dir = entry.path();
                // Don't descend into nested node_modules twice.
                if dir
                    .parent()
                    .map(|p| p.components().any(|c| c.as_os_str() == "node_modules"))
                    .unwrap_or(false)
                {
                    continue;
                }
                let project = dir.parent().unwrap_or(dir);
                if !project.join("package.json").is_file() {
                    continue;
                }
                let size = dir_size(dir);
                let project_name = project
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "?".to_string());
                out.push(Resource {
                    id: String::new(),
                    provider: self.id().to_string(),
                    name: format!("{project_name}/node_modules"),
                    path: dir.to_path_buf(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_root(tag: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("devclean-test-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&root);
        let proj = root.join("proj");
        std::fs::create_dir_all(proj.join("node_modules/some-pkg")).unwrap();
        std::fs::write(proj.join("package.json"), r#"{"name":"proj"}"#).unwrap();
        std::fs::write(
            proj.join("node_modules/some-pkg/index.js"),
            "x".repeat(1000),
        )
        .unwrap();
        // A node_modules WITHOUT package.json sibling must be ignored.
        let stray = root.join("stray/node_modules");
        std::fs::create_dir_all(&stray).unwrap();
        root
    }

    #[test]
    fn finds_node_modules_with_package_json() {
        let root = fixture_root("node");
        let found = NodeModulesProvider.scan(std::slice::from_ref(&root));
        assert_eq!(found.len(), 1, "expected exactly one hit: {found:?}");
        assert_eq!(found[0].provider, "node_modules");
        assert!(found[0].size_bytes >= 1000);
        assert_eq!(found[0].reclaimable_bytes, found[0].size_bytes);
        let _ = std::fs::remove_dir_all(&root);
    }
}

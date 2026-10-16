//! `node_modules` provider (MVP flagship).
//!
//! Finds `node_modules/` directories whose parent holds a `package.json`.
//! Cleans by deleting the directory only — lockfiles are never touched.
//! Matching happens in the shared project-tree walk ([`crate::tree`]).

use super::provider::{
    age_days, impact, measure_dir, project_detail, CleanupAction, DirSize, Provider,
};
use crate::tree::{Match, TreeRule, TreeRuleSpec};
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

    fn measure(&self, path: &Path) -> Option<u64> {
        measure_dir(path)
    }

    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        CleanupAction::RemoveDir(path.to_path_buf())
    }

    /// `node_modules` sits one level under the project that owns it, so the
    /// parent is the project whose age (and repository) classify it.
    fn project_root(&self, artifact: &Path) -> Option<PathBuf> {
        artifact.parent().map(Path::to_path_buf)
    }
}
impl TreeRule for NodeModulesProvider {
    fn spec(&self) -> TreeRuleSpec {
        TreeRuleSpec {
            dir_names: &["node_modules"],
            sibling_file: Some("package.json"),
            max_depth: 6,
            // A node_modules inside a node_modules: not a second artifact —
            // its bytes already belong to the enclosing one.
            skip_nested: true,
            extra_prune_dirs: &[],
        }
    }

    fn materialize(&self, m: &Match, size: &DirSize) -> Option<Resource> {
        let dir = &m.dir;
        let project = dir.parent().unwrap_or(dir);
        let project_name = project
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "?".to_string());
        Some(Resource {
            id: String::new(),
            provider: self.id().to_string(),
            name: format!("{project_name}/node_modules"),
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
                "Dependencies are reinstalled from the lockfile; source code is untouched.",
                Some("Next install re-downloads packages"),
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
        let cancel = AtomicBool::new(false);
        let found = walk_tree_rules(
            std::slice::from_ref(&root),
            &[Arc::new(NodeModulesProvider)],
            &cancel,
        );
        assert_eq!(found.len(), 1, "one group per rule");
        let found: Vec<Resource> = found.into_iter().flatten().collect();
        assert_eq!(found.len(), 1, "expected exactly one hit: {found:?}");
        assert_eq!(found[0].provider, "node_modules");
        assert!(found[0].size_bytes >= 1000);
        assert_eq!(found[0].reclaimable_bytes, found[0].size_bytes);
        let impact = found[0].impact.as_ref().expect("impact must be set");
        assert!(!impact.consequence.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}

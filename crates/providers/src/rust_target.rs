//! Rust `target/` provider. Finds `target/` dirs whose parent holds a
//! `Cargo.toml` — re-expressed from the old `Cargo.toml`-file walk so the
//! shared project-tree walk ([`crate::tree`]) can match on directories;
//! both sit at the same depth, so the depth boundary is unchanged.
//! Regenerable via `cargo build`, hence `Safe`.

use super::provider::{
    age_days, impact, measure_dir, project_detail, CleanupAction, DirSize, Provider,
};
use crate::tree::{Match, TreeRule, TreeRuleSpec};
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
impl TreeRule for RustTargetProvider {
    fn spec(&self) -> TreeRuleSpec {
        TreeRuleSpec {
            dir_names: &["target"],
            sibling_files: &["Cargo.toml"],
            max_depth: 5,
            // The old walk matched Cargo.toml files and checked for a sibling
            // `target/`; it never special-cased nesting, so neither do we.
            skip_nested: false,
        }
    }

    fn materialize(&self, m: &Match, size: &DirSize) -> Option<Resource> {
        if size.allocated == 0 {
            return None;
        }
        let project = m.dir.parent().unwrap_or(Path::new("/"));
        let project_name = project
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "?".to_string());
        Some(Resource {
            id: String::new(),
            provider: self.id().to_string(),
            name: format!("{project_name}/target"),
            path: m.dir.clone(),
            size_bytes: size.allocated,
            reclaimable_bytes: size.allocated,
            apparent_bytes: size.apparent_if_material(),
            inode_bytes: size.inode_bytes.clone(),
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
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::walk_tree_rules;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    #[test]
    fn finds_target_and_describes_impact() {
        let root = std::env::temp_dir().join(format!("devclean-test-{}-rs", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let proj = root.join("proj");
        std::fs::create_dir_all(proj.join("target/debug")).unwrap();
        std::fs::write(proj.join("Cargo.toml"), "[package]\nname = \"proj\"").unwrap();
        std::fs::write(proj.join("target/debug/app"), "x".repeat(1000)).unwrap();
        let cancel = AtomicBool::new(false);
        let found = walk_tree_rules(
            std::slice::from_ref(&root),
            &[Arc::new(RustTargetProvider)],
            &cancel,
        );
        assert_eq!(found.len(), 1, "one group per rule");
        let found: Vec<Resource> = found.into_iter().flatten().collect();
        assert_eq!(found.len(), 1, "expected exactly one hit: {found:?}");
        assert!(found[0].name.ends_with("/target"));
        let impact = found[0].impact.as_ref().expect("impact must be set");
        assert!(!impact.consequence.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}

//! Go `vendor/` provider. Finds `vendor/` dirs whose parent holds a
//! `go.mod` — the vendored-dependency tree Go modules can pin. Regenerable
//! via `go mod vendor`, but we do not own the directory's internals, so
//! `Caution`. Rides the shared project-tree walk ([`crate::tree`]).

use super::provider::{
    age_days, impact, measure_dir, project_detail, CleanupAction, DirSize, Provider,
};
use crate::tree::{Match, TreeRule, TreeRuleSpec};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};

pub struct GoVendorProvider;

impl Provider for GoVendorProvider {
    fn id(&self) -> &'static str {
        "go_vendor"
    }
    fn name(&self) -> &'static str {
        "Go vendor"
    }
    fn default_safety(&self) -> SafetyLevel {
        SafetyLevel::Caution
    }

    fn measure(&self, path: &Path) -> Option<u64> {
        measure_dir(path)
    }

    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        CleanupAction::RemoveDir(path.to_path_buf())
    }

    /// `vendor/` sits one level under the module that owns it.
    fn project_root(&self, artifact: &Path) -> Option<PathBuf> {
        artifact.parent().map(Path::to_path_buf)
    }
}
impl TreeRule for GoVendorProvider {
    fn spec(&self) -> TreeRuleSpec {
        TreeRuleSpec {
            dir_names: &["vendor"],
            sibling_files: &["go.mod"],
            max_depth: 6,
            // A vendor/ inside a vendor/ is not a second module's vendor
            // tree in practice; nothing characterizes nesting, so keep the
            // match wherever it sits and size it in full.
            skip_nested: false,
            extra_prune_dirs: &[],
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
            name: format!("{project_name}/vendor"),
            path: m.dir.clone(),
            size_bytes: size.allocated,
            reclaimable_bytes: size.allocated,
            apparent_bytes: size.apparent_if_material(),
            inode_bytes: size.inode_bytes.clone(),
            safety: SafetyLevel::Caution,
            detail: Some(project_detail(&project_name, project, size.allocated)),
            age_days: age_days(project),
            activity: None,
            last_commit_unix: None,
            blocked_reason: None,
            blocked_detail: None,
            impact: impact(
                "Vendored dependencies; `go mod vendor` recreates them from the module cache.",
                Some("The next build re-resolves dependencies"),
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
    fn finds_vendor_beside_go_mod_and_ignores_a_stray() {
        let root = std::env::temp_dir().join(format!("devclean-test-{}-gv", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let proj = root.join("proj");
        std::fs::create_dir_all(proj.join("vendor/example.com/lib")).unwrap();
        std::fs::write(proj.join("go.mod"), "module example.com/proj\n").unwrap();
        std::fs::write(proj.join("vendor/example.com/lib/lib.go"), "x".repeat(1000)).unwrap();
        // vendor/ WITHOUT go.mod: not a module, never reported.
        std::fs::create_dir_all(root.join("stray/vendor")).unwrap();
        std::fs::write(root.join("stray/vendor/x.go"), "x").unwrap();

        let cancel = AtomicBool::new(false);
        let found = walk_tree_rules(
            std::slice::from_ref(&root),
            &[Arc::new(GoVendorProvider)],
            &cancel,
        );
        let found: Vec<Resource> = found.into_iter().flatten().collect();
        assert_eq!(found.len(), 1, "expected exactly one hit: {found:?}");
        assert!(found[0].name.ends_with("/vendor"));
        assert_eq!(found[0].safety, SafetyLevel::Caution);
        let _ = std::fs::remove_dir_all(&root);
    }
}

//! CocoaPods provider. Finds `Pods/` dirs whose parent holds a `Podfile`
//! — the iOS half of "React Native" and every other CocoaPods project.
//! Regenerable via `pod install`, but we do not own the directory's
//! internals, so `Caution`. Rides the shared project-tree walk
//! ([`crate::tree`]).
//!
//! The shared CocoaPods download cache lives in `global_cache`; this
//! provider is the per-project installed-pods tree only.

use super::provider::{
    age_days, impact, measure_dir, project_detail, CleanupAction, DirSize, Provider,
};
use crate::tree::{Match, TreeRule, TreeRuleSpec};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};

pub struct PodsProvider;

impl Provider for PodsProvider {
    fn id(&self) -> &'static str {
        "pods"
    }
    fn name(&self) -> &'static str {
        "CocoaPods"
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

    /// `Pods/` sits one level under the project (usually `ios/`) that owns
    /// the Podfile.
    fn project_root(&self, artifact: &Path) -> Option<PathBuf> {
        artifact.parent().map(Path::to_path_buf)
    }
}
impl TreeRule for PodsProvider {
    fn spec(&self) -> TreeRuleSpec {
        TreeRuleSpec {
            dir_names: &["Pods"],
            sibling_files: &["Podfile"],
            max_depth: 6,
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
            name: format!("{project_name}/Pods"),
            path: m.dir.clone(),
            size_bytes: size.allocated,
            reclaimable_bytes: size.allocated,
            apparent_bytes: size.apparent_if_material(),
            safety: SafetyLevel::Caution,
            detail: Some(project_detail(&project_name, project, size.allocated)),
            age_days: age_days(project),
            activity: None,
            last_commit_unix: None,
            blocked_reason: None,
            blocked_detail: None,
            impact: impact(
                "Installed pods; your Podfile and source are untouched.",
                Some("The next `pod install` re-downloads every pod"),
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
    fn finds_pods_beside_podfile_and_ignores_a_stray() {
        let root = std::env::temp_dir().join(format!("devclean-test-{}-pd", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let ios = root.join("ios");
        std::fs::create_dir_all(ios.join("Pods/Alamofire")).unwrap();
        std::fs::write(ios.join("Podfile"), "platform :ios\n").unwrap();
        std::fs::write(ios.join("Pods/Alamofire/Source.swift"), "x".repeat(1000)).unwrap();
        // Pods/ WITHOUT a Podfile: not a CocoaPods project.
        std::fs::create_dir_all(root.join("stray/Pods")).unwrap();
        std::fs::write(root.join("stray/Pods/x"), "x").unwrap();

        let cancel = AtomicBool::new(false);
        let found = walk_tree_rules(
            std::slice::from_ref(&root),
            &[Arc::new(PodsProvider)],
            &cancel,
        );
        let found: Vec<Resource> = found.into_iter().flatten().collect();
        assert_eq!(found.len(), 1, "expected exactly one hit: {found:?}");
        assert!(found[0].name.ends_with("/Pods"));
        assert_eq!(found[0].safety, SafetyLevel::Caution);
        let _ = std::fs::remove_dir_all(&root);
    }
}

//! Android/Gradle project build state. Finds `build/`, `.gradle/` and
//! `.cxx/` next to any gradle marker (`build.gradle`, `build.gradle.kts`,
//! `settings.gradle`, `settings.gradle.kts`). Covers plain Android
//! projects and the Android half of React Native. Rides the shared
//! project-tree walk ([`crate::tree`]).
//!
//! The marker list is one-of-many — the reason [`crate::tree::TreeRuleSpec`]
//! grew `sibling_files` (Wave D1). The GLOBAL Gradle download cache
//! (`~/.gradle/caches`) is a different thing and stays with the
//! `gradle_cache` provider; this one is per-project build output.

use super::provider::{
    age_days, impact, measure_dir, project_detail, CleanupAction, DirSize, Provider,
};
use crate::tree::{Match, TreeRule, TreeRuleSpec};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};

pub struct AndroidProvider;

impl Provider for AndroidProvider {
    fn id(&self) -> &'static str {
        "android"
    }
    fn name(&self) -> &'static str {
        "Android build"
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

    /// The matched directory sits one level under the gradle module that
    /// owns it.
    fn project_root(&self, artifact: &Path) -> Option<PathBuf> {
        artifact.parent().map(Path::to_path_buf)
    }
}
impl TreeRule for AndroidProvider {
    fn spec(&self) -> TreeRuleSpec {
        TreeRuleSpec {
            dir_names: &["build", ".gradle", ".cxx"],
            sibling_files: &[
                "build.gradle",
                "build.gradle.kts",
                "settings.gradle",
                "settings.gradle.kts",
            ],
            max_depth: 6,
            // A gradle module inside another (android/app/) is its own
            // artifact with its own marker — both are reported.
            skip_nested: false,
            extra_prune_dirs: &[],
        }
    }

    fn materialize(&self, m: &Match, size: &DirSize) -> Option<Resource> {
        if size.allocated == 0 {
            return None;
        }
        let dir = &m.dir;
        let project = dir.parent().unwrap_or(Path::new("/"));
        let project_name = project
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "?".to_string());
        let dir_name = dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "?".to_string());
        Some(Resource {
            id: String::new(),
            provider: self.id().to_string(),
            name: format!("{project_name}/{dir_name}"),
            path: dir.to_path_buf(),
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
                "Gradle/CMake build output; your source and gradle files are untouched.",
                Some("The next build starts from scratch"),
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
    fn any_gradle_marker_is_enough_and_a_stray_build_is_ignored() {
        let root = std::env::temp_dir().join(format!("devclean-test-{}-ad", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let proj = root.join("android-proj");
        std::fs::create_dir_all(proj.join("build")).unwrap();
        std::fs::write(proj.join("build.gradle.kts"), "// module\n").unwrap();
        std::fs::write(proj.join("build/outputs.apk"), "x".repeat(1000)).unwrap();
        std::fs::create_dir_all(proj.join(".gradle")).unwrap();
        std::fs::write(proj.join(".gradle/file-hash.bin"), "x").unwrap();
        // build/ WITHOUT any gradle marker: never reported.
        std::fs::create_dir_all(root.join("stray/build")).unwrap();
        std::fs::write(root.join("stray/build/x"), "x").unwrap();

        let cancel = AtomicBool::new(false);
        let found = walk_tree_rules(
            std::slice::from_ref(&root),
            &[Arc::new(AndroidProvider)],
            &cancel,
        );
        let found: Vec<Resource> = found.into_iter().flatten().collect();
        assert_eq!(found.len(), 2, "expected build/ and .gradle/: {found:?}");
        assert!(found.iter().all(|r| r.safety == SafetyLevel::Caution));
        let _ = std::fs::remove_dir_all(&root);
    }
}

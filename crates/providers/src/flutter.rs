//! Flutter project caches. Finds `.dart_tool/` (pub resolution state) and
//! `build/` (build output) next to a `pubspec.yaml`. Rides the shared
//! project-tree walk ([`crate::tree`]).
//!
//! Safety is per-directory: `.dart_tool` is a pure cache the next
//! `pub get` rebuilds (`Safe`); `build/` holds build output whose
//! regeneration is slow but complete (`Caution`). Shared caches that are
//! NOT project artifacts — the Dart pub cache among them — ride
//! `global_cache`, not this provider.

use super::provider::{
    age_days, impact, measure_dir, project_detail, CleanupAction, DirSize, Provider,
};
use crate::tree::{Match, TreeRule, TreeRuleSpec};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};

pub struct FlutterProvider;

impl Provider for FlutterProvider {
    fn id(&self) -> &'static str {
        "flutter"
    }
    fn name(&self) -> &'static str {
        "Flutter"
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

    /// Either directory sits one level under the project that owns it.
    fn project_root(&self, artifact: &Path) -> Option<PathBuf> {
        artifact.parent().map(Path::to_path_buf)
    }
}
impl TreeRule for FlutterProvider {
    fn spec(&self) -> TreeRuleSpec {
        TreeRuleSpec {
            dir_names: &[".dart_tool", "build"],
            sibling_files: &["pubspec.yaml"],
            max_depth: 6,
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
        let safety = if dir_name == ".dart_tool" {
            SafetyLevel::Safe
        } else {
            SafetyLevel::Caution
        };
        let impact = if dir_name == ".dart_tool" {
            impact(
                "Pub resolution state only; the next `pub get` recreates it.",
                Some("The next `pub get` re-resolves packages"),
                None,
            )
        } else {
            impact(
                "Build output only; your source and pubspec are untouched.",
                Some("The next `flutter build` starts from scratch"),
                None,
            )
        };
        Some(Resource {
            id: String::new(),
            provider: self.id().to_string(),
            name: format!("{project_name}/{dir_name}"),
            path: dir.to_path_buf(),
            size_bytes: size.allocated,
            reclaimable_bytes: size.allocated,
            apparent_bytes: size.apparent_if_material(),
            safety,
            detail: Some(project_detail(&project_name, project, size.allocated)),
            age_days: age_days(project),
            activity: None,
            last_commit_unix: None,
            blocked_reason: None,
            blocked_detail: None,
            impact,
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
    fn dart_tool_is_safe_and_build_is_caution() {
        let root = std::env::temp_dir().join(format!("devclean-test-{}-fl", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let proj = root.join("app");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("pubspec.yaml"), "name: app\n").unwrap();
        std::fs::create_dir_all(proj.join(".dart_tool/package_config")).unwrap();
        std::fs::write(proj.join(".dart_tool/package_config.json"), "{}").unwrap();
        std::fs::create_dir_all(proj.join("build")).unwrap();
        std::fs::write(proj.join("build/app.dill"), "x".repeat(1000)).unwrap();
        // A build/ WITHOUT pubspec.yaml: not a Flutter project.
        std::fs::create_dir_all(root.join("stray/build")).unwrap();
        std::fs::write(root.join("stray/build/x"), "x").unwrap();

        let cancel = AtomicBool::new(false);
        let found = walk_tree_rules(
            std::slice::from_ref(&root),
            &[Arc::new(FlutterProvider)],
            &cancel,
        );
        let found: Vec<Resource> = found.into_iter().flatten().collect();
        assert_eq!(found.len(), 2, "expected both artifacts: {found:?}");
        let dart_tool = found
            .iter()
            .find(|r| r.name.ends_with("/.dart_tool"))
            .unwrap();
        assert_eq!(dart_tool.safety, SafetyLevel::Safe);
        let build = found.iter().find(|r| r.name.ends_with("/build")).unwrap();
        assert_eq!(build.safety, SafetyLevel::Caution);
        let _ = std::fs::remove_dir_all(&root);
    }
}

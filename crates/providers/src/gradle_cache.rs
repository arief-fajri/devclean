//! Gradle cache provider (`~/.gradle/caches`). Holds downloaded
//! distributions, module metadata, and build caches; re-downloaded on the
//! next build.
//!
//! `Caution`: Gradle has no official global cache-clean command, so this
//! deletes the caches directory (regenerable, but a cold sync is slow).
//! Lives outside scan roots → declares
//! [`Provider::extra_allowed_roots`]; protected-path rules still apply.

use super::provider::{dir_size_detail, home_dir, impact, measure_dir, CleanupAction, Provider};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub struct GradleCacheProvider;

pub fn gradle_caches_dir() -> Option<PathBuf> {
    let dir = home_dir()?.join(".gradle/caches");
    dir.is_dir().then_some(dir)
}

impl Provider for GradleCacheProvider {
    fn id(&self) -> &'static str {
        "gradle_cache"
    }
    fn name(&self) -> &'static str {
        "Gradle caches"
    }
    fn default_safety(&self) -> SafetyLevel {
        SafetyLevel::Caution
    }

    fn scan(&self, _roots: &[PathBuf], _cancel: &AtomicBool) -> Vec<Resource> {
        let Some(dir) = gradle_caches_dir() else {
            return Vec::new();
        };
        let size = dir_size_detail(&dir);
        if size.allocated == 0 {
            return Vec::new();
        }
        vec![Resource {
            id: String::new(),
            provider: self.id().to_string(),
            name: "Gradle caches".to_string(),
            path: dir,
            size_bytes: size.allocated,
            reclaimable_bytes: size.allocated,
            apparent_bytes: size.apparent_if_material(),
            safety: SafetyLevel::Caution,
            detail: None,
            age_days: None,
            activity: None,
            last_commit_unix: None,
            blocked_reason: None,
            blocked_detail: None,
            impact: impact(
                "Gradle re-downloads dependencies on the next sync.",
                Some("Requires network · cold syncs are slow"),
                None,
            ),
        }]
    }

    fn measure(&self, path: &Path) -> Option<u64> {
        measure_dir(path)
    }

    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        CleanupAction::RemoveDir(path.to_path_buf())
    }

    fn extra_allowed_roots(&self) -> Vec<PathBuf> {
        gradle_caches_dir().into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_dir_is_home_relative_when_present() {
        // Absence (no Gradle installed) is normal — only assert shape.
        if let Some(dir) = gradle_caches_dir() {
            assert!(dir.ends_with(".gradle/caches"));
            assert_eq!(GradleCacheProvider.extra_allowed_roots(), vec![dir]);
        }
    }
}

//! Xcode data provider (`~/Library/Developer/Xcode/DerivedData`).
//! Derived data is pure build output — regenerable via Xcode, hence safe
//! to delete, but a full rebuild is slow, so `Caution`.
//!
//! macOS-only in practice (the path never exists elsewhere → empty).
//! Lives outside scan roots → declares
//! [`Provider::extra_allowed_roots`]; protected-path rules still apply.

use super::provider::{
    dir_size_detail_checked, home_dir, impact, measure_dir, CleanupAction, OwnScanner, Provider,
};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub struct XcodeDataProvider;

pub fn derived_data_dir() -> Option<PathBuf> {
    let dir = home_dir()?.join("Library/Developer/Xcode/DerivedData");
    dir.is_dir().then_some(dir)
}

impl Provider for XcodeDataProvider {
    fn id(&self) -> &'static str {
        "xcode_data"
    }
    fn name(&self) -> &'static str {
        "Xcode DerivedData"
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

    fn extra_allowed_roots(&self) -> Vec<PathBuf> {
        derived_data_dir().into_iter().collect()
    }
}
impl OwnScanner for XcodeDataProvider {
    fn scan(&self, _roots: &[PathBuf], cancel: &AtomicBool) -> Vec<Resource> {
        let Some(dir) = derived_data_dir() else {
            return Vec::new();
        };
        // Cancellable (G-SCAN-02): a cancel during the measurement drops
        // this resource rather than settling only when the walk finishes.
        let Some(size) = dir_size_detail_checked(&dir, cancel) else {
            return Vec::new();
        };
        if size.allocated == 0 {
            return Vec::new();
        }
        vec![Resource {
            id: String::new(),
            provider: self.id().to_string(),
            name: "Xcode DerivedData".to_string(),
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
                "Build output only; your source code is untouched.",
                Some("Full rebuild from scratch is slow"),
                None,
            ),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_data_dir_is_home_relative_when_present() {
        if let Some(dir) = derived_data_dir() {
            assert!(dir.ends_with("Library/Developer/Xcode/DerivedData"));
            assert_eq!(XcodeDataProvider.extra_allowed_roots(), vec![dir]);
        }
    }
}

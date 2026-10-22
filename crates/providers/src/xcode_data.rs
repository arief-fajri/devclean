//! Xcode data provider (`~/Library/Developer/…`).
//!
//! Once a single DerivedData row; now a fixed target list (Wave D2):
//! DerivedData, Archives, iOS DeviceSupport and the CoreSimulator caches.
//! Each is regenerable in some sense, but the costs differ sharply — which
//! is why the safeties do too. Archives is the first `Dangerous` resource
//! in the product: the dSYMs of shipped builds die with it, and no gate
//! can bring them back.
//!
//! `CoreSimulator/Devices` is DELIBERATELY NOT a target: simulator data is
//! user data, not a cache, and no amount of gating turns it into one.
//!
//! macOS-only in practice (the paths never exist elsewhere → empty).
//! Lives outside scan roots → declares
//! [`Provider::extra_allowed_roots`]; protected-path rules still apply.

use super::provider::{
    dir_size_detail_checked, home_dir, impact, measure_dir, CleanupAction, OwnScanner, Provider,
};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct XcodeDataProvider;

/// One Xcode directory we own, under `~/Library/Developer`.
struct Target {
    rel: &'static str,
    label: &'static str,
    safety: SafetyLevel,
    consequence: &'static str,
    cost: &'static str,
}

/// Fixed list — no user input, ever (G-SAFE-01/02).
const TARGETS: &[Target] = &[
    Target {
        rel: "Xcode/DerivedData",
        label: "Xcode DerivedData",
        safety: SafetyLevel::Caution,
        consequence: "Build output only; your source code is untouched.",
        cost: "Full rebuild from scratch is slow",
    },
    Target {
        rel: "Xcode/Archives",
        label: "Xcode Archives",
        safety: SafetyLevel::Dangerous,
        consequence: "The dSYMs of shipped builds live here; deleting them ends crash symbolication for those builds.",
        cost: "Crash reports for archived builds can never be symbolicated again",
    },
    Target {
        rel: "Xcode/iOS DeviceSupport",
        label: "iOS DeviceSupport",
        safety: SafetyLevel::Caution,
        consequence: "Symbol files Xcode fetched for a connected device; Xcode re-fetches them on the next connection.",
        cost: "Device debugging pauses until the symbols are re-fetched",
    },
    Target {
        rel: "CoreSimulator/Caches",
        label: "Simulator caches",
        safety: SafetyLevel::Caution,
        consequence: "Simulator caches only; the Simulator rebuilds them.",
        cost: "The first launch after clearing is slow",
    },
];

pub fn derived_data_dir() -> Option<PathBuf> {
    let dir = home_dir()?.join("Library/Developer/Xcode/DerivedData");
    dir.is_dir().then_some(dir)
}

fn path_for(t: &Target) -> Option<PathBuf> {
    let p = home_dir()?.join("Library/Developer").join(t.rel);
    p.is_dir().then_some(p)
}

impl Provider for XcodeDataProvider {
    fn id(&self) -> &'static str {
        "xcode_data"
    }
    fn name(&self) -> &'static str {
        "Xcode data"
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
        TARGETS.iter().filter_map(path_for).collect()
    }
}
impl OwnScanner for XcodeDataProvider {
    fn scan(&self, _roots: &[PathBuf], cancel: &AtomicBool) -> Vec<Resource> {
        TARGETS
            .iter()
            .filter_map(|t| {
                // Cancellable (G-SCAN-02): stop between targets, and drop
                // the one being measured, the moment cancel is set.
                if cancel.load(Ordering::Relaxed) {
                    return None;
                }
                let dir = path_for(t)?;
                let size = dir_size_detail_checked(&dir, cancel)?;
                if size.allocated == 0 {
                    return None;
                }
                Some(Resource {
                    id: String::new(),
                    provider: self.id().to_string(),
                    name: t.label.to_string(),
                    path: dir,
                    size_bytes: size.allocated,
                    reclaimable_bytes: size.allocated,
                    apparent_bytes: size.apparent_if_material(),
                    safety: t.safety,
                    detail: None,
                    age_days: None,
                    activity: None,
                    last_commit_unix: None,
                    blocked_reason: None,
                    blocked_detail: None,
                    impact: impact(t.consequence, Some(t.cost), None),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_data_dir_is_home_relative_when_present() {
        if let Some(dir) = derived_data_dir() {
            assert!(dir.ends_with("Library/Developer/Xcode/DerivedData"));
        }
    }

    /// Declared roots are exactly the target dirs — never the Devices tree.
    #[test]
    fn declared_roots_are_targets_and_never_simulator_devices() {
        for r in XcodeDataProvider.extra_allowed_roots() {
            let s = r.to_string_lossy();
            assert!(
                TARGETS.iter().any(|t| s.ends_with(t.rel)),
                "declared root {s} is not a declared target"
            );
            assert!(
                !s.contains("CoreSimulator/Devices"),
                "simulator data is not a cache and must never be declared"
            );
        }
    }

    /// Archives is the only `Dangerous` target, and every row the scan
    /// produces carries its target's declared safety.
    #[test]
    fn only_archives_is_dangerous_and_rows_carry_the_declared_safety() {
        let dangerous: Vec<&str> = TARGETS
            .iter()
            .filter(|t| t.safety == SafetyLevel::Dangerous)
            .map(|t| t.rel)
            .collect();
        assert_eq!(dangerous, vec!["Xcode/Archives"]);
        let cancel = AtomicBool::new(false);
        for r in XcodeDataProvider.scan(&[], &cancel) {
            let t = TARGETS
                .iter()
                .find(|t| r.path.ends_with(t.rel))
                .expect("row must come from a declared target");
            assert_eq!(r.safety, t.safety, "{} carries the wrong safety", r.name);
        }
    }
}

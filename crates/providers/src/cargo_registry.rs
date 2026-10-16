//! Cargo registry provider (`~/.cargo/registry`). Holds downloaded crate
//! sources + extracted sources + the index cache; everything is
//! re-downloaded on the next build, so the content is regenerable.
//!
//! `Caution` (not `Safe`): a cold rebuild re-downloads the world, which can
//! be slow on metered connections. The registry lives outside scan roots,
//! so this provider declares [`Provider::extra_allowed_roots`] — the
//! validator still enforces protected paths and the exact dir (I2, I7).

use super::provider::{
    dir_size_detail_checked, home_dir, impact, measure_dir, CleanupAction, OwnScanner, Provider,
};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub struct CargoRegistryProvider;

pub fn registry_dir() -> Option<PathBuf> {
    let dir = home_dir()?.join(".cargo/registry");
    dir.is_dir().then_some(dir)
}

impl Provider for CargoRegistryProvider {
    fn id(&self) -> &'static str {
        "cargo_registry"
    }
    fn name(&self) -> &'static str {
        "Cargo registry"
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
        registry_dir().into_iter().collect()
    }
}
impl OwnScanner for CargoRegistryProvider {
    fn scan(&self, _roots: &[PathBuf], cancel: &AtomicBool) -> Vec<Resource> {
        let Some(dir) = registry_dir() else {
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
            name: "Cargo registry".to_string(),
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
                "Crates are re-downloaded from crates.io on the next build.",
                Some("Requires network · slow on metered connections"),
                None,
            ),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_dir_is_home_relative() {
        // Pure path math: must stay under ~/.cargo, never bare or absolute
        // elsewhere. Skipped when HOME is unset.
        if let Some(dir) = registry_dir() {
            assert!(dir.ends_with(".cargo/registry"));
            assert_eq!(
                CargoRegistryProvider.extra_allowed_roots(),
                vec![dir.clone()]
            );
            assert!(matches!(
                CargoRegistryProvider.cleanup_action(&dir),
                CleanupAction::RemoveDir(p) if p == dir
            ));
        }
    }
}

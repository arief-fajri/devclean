//! Ruby caches. Fixed target list (Wave D5): the gem cache and bundler's
//! download cache — the two directories Ruby tooling regenerates from the
//! network.
//!
//! `~/.bundle` itself is NEVER a target: its root holds the user's bundler
//! config, which can carry registry credentials. Only `~/.bundle/cache`
//! is reported. `vendor/bundle` (a Rails project's installed gems) is
//! likewise NOT matched: its marker (`Gemfile`) sits two levels up, and a
//! rule that matched any `vendor/` would offer committed code in every
//! non-Go project. Both exclusions are recorded in
//! `docs/decisions/wave-d-coverage.md`.
//!
//! Every target is `SafetyLevel::Caution` — we do not own their internals.

use super::provider::{
    dir_size_detail_checked, home_dir, impact, measure_dir, CleanupAction, OwnScanner, Provider,
};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

struct Target {
    rel: &'static str,
    label: &'static str,
    regenerates: &'static str,
}

/// Fixed list — no user input, ever (G-SAFE-01/02).
const TARGETS: &[Target] = &[
    Target {
        rel: ".gem",
        label: "Gem cache",
        regenerates: "The next `gem install` re-downloads the gem.",
    },
    Target {
        rel: ".bundle/cache",
        label: "Bundler download cache",
        regenerates: "The next `bundle install` re-downloads the gems.",
    },
];

pub struct RubyProvider;

fn path_for(t: &Target) -> Option<PathBuf> {
    let p = home_dir()?.join(t.rel);
    p.is_dir().then_some(p)
}

impl Provider for RubyProvider {
    fn id(&self) -> &'static str {
        "ruby"
    }
    fn name(&self) -> &'static str {
        "Ruby caches"
    }
    fn default_safety(&self) -> SafetyLevel {
        SafetyLevel::Caution
    }

    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        CleanupAction::RemoveDir(path.to_path_buf())
    }

    fn measure(&self, path: &Path) -> Option<u64> {
        measure_dir(path)
    }

    /// No per-item age: whole caches, never held back by age (I9).
    fn extra_allowed_roots(&self) -> Vec<PathBuf> {
        TARGETS.iter().filter_map(path_for).collect()
    }
}

impl OwnScanner for RubyProvider {
    fn scan(&self, _roots: &[PathBuf], cancel: &AtomicBool) -> Vec<Resource> {
        TARGETS
            .iter()
            .filter_map(|t| {
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
                    safety: SafetyLevel::Caution,
                    detail: None,
                    age_days: None,
                    activity: None,
                    last_commit_unix: None,
                    blocked_reason: None,
                    blocked_detail: None,
                    impact: impact(
                        t.regenerates,
                        Some("Re-downloading gems costs network and time"),
                        None,
                    ),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_target_is_home_relative_and_the_bundle_root_is_not_one() {
        for t in TARGETS {
            assert!(!t.rel.starts_with('/'), "{} must be home-relative", t.rel);
            assert!(!t.rel.contains(".."), "{} must not escape HOME", t.rel);
            // The bundler ROOT holds config/credentials: never a target.
            assert_ne!(t.rel, ".bundle");
            assert!(!t.regenerates.is_empty(), "{} needs a consequence", t.label);
        }
    }

    /// G-SAFE-03: caches whose internals we do not own are never `Safe`.
    #[test]
    fn nothing_in_this_provider_is_marked_safe() {
        assert_eq!(RubyProvider.default_safety(), SafetyLevel::Caution);
        let cancel = AtomicBool::new(false);
        for r in RubyProvider.scan(&[], &cancel) {
            assert_eq!(r.safety, SafetyLevel::Caution);
        }
    }

    #[test]
    fn declared_roots_are_the_caches_never_the_bundle_root() {
        for r in RubyProvider.extra_allowed_roots() {
            let s = r.to_string_lossy();
            assert!(
                !s.ends_with("/.bundle"),
                "declaring the bundler root would expose its config: {s}"
            );
        }
    }

    #[test]
    fn duplicate_targets_are_rejected() {
        let mut seen = std::collections::HashSet::new();
        for t in TARGETS {
            assert!(seen.insert(t.rel), "duplicate target {}", t.rel);
        }
    }
}

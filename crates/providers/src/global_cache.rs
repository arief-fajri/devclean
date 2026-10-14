//! Global caches with no owning project and no official clean command.
//!
//! The legacy `cleanup.sh` clears eight of these with bare `rm -rf`. They
//! are genuinely regenerable — a browser re-fetches, a shell recompiles,
//! Homebrew re-downloads — but there is no `chrome clean` or `brew clean`
//! scoped to just the cache, so P5's "official tools first" has no
//! equivalent here.
//!
//! Every target is therefore `SafetyLevel::Caution`, never `Safe`: we are
//! deleting a directory whose internals we do not own. That is a deliberate
//! difference from the tool-backed caches, and the UI says so.
//!
//! Shell completion dumps (`.zcompdump*`) are deliberately NOT handled: they
//! are individual files matched by a glob, and `read_dir` cannot glob. At
//! ~260 KB total they are not worth a bespoke code path that could get the
//! glob wrong and delete something else.
//!
//! Each target declares its own `extra_allowed_roots`, so the safety
//! validator keeps applying the protected-path rules (I2, I7) — nothing
//! under `~/Documents` or `~/.ssh` can be reached from here.

use super::provider::{dir_size_detail, home_dir, impact, measure_dir, CleanupAction, Provider};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// One cache we own.
struct Target {
    /// Relative to `$HOME`, or an absolute literal for system paths.
    rel: &'static str,
    label: &'static str,
    /// How to express it in the `impact.consequence` sentence.
    regenerates: &'static str,
}

/// Fixed list — no user input, ever (G-SAFE-01/02).
const TARGETS: &[Target] = &[
    Target {
        rel: "Library/Caches/Google",
        label: "Chrome cache",
        regenerates: "Chrome re-fetches pages as you browse.",
    },
    Target {
        rel: "Library/Caches/com.microsoft.VSCode.ShipIt",
        label: "VS Code updater",
        regenerates: "The updater re-downloads when VS Code next checks.",
    },
    Target {
        rel: "Library/Caches/pnpm",
        label: "pnpm store cache",
        regenerates: "pnpm re-fetches packages on the next install.",
    },
    Target {
        rel: "Library/Caches/bun",
        label: "bun cache",
        regenerates: "bun re-fetches packages on the next install.",
    },
    Target {
        rel: "Library/Caches/Homebrew",
        label: "Homebrew cache",
        regenerates: "Homebrew re-downloads bottles when you upgrade.",
    },
    Target {
        rel: "Library/Caches/gopls",
        label: "gopls cache",
        regenerates: "The Go language server rebuilds it on next start.",
    },
    Target {
        rel: "Library/Caches/node-gyp",
        label: "node-gyp cache",
        regenerates: "node-gyp re-downloads headers when you build native modules.",
    },
    Target {
        rel: ".bun/install/cache",
        label: "bun package cache",
        regenerates: "bun re-fetches packages on the next install.",
    },
    Target {
        rel: ".cache/uv",
        label: "uv cache",
        regenerates: "uv re-downloads wheels on the next sync.",
    },
    Target {
        rel: ".cache/act",
        label: "act cache",
        regenerates: "act rebuilds its images on the next run.",
    },
    Target {
        rel: ".cache/opencode",
        label: "opencode cache",
        regenerates: "opencode rebuilds it on the next start.",
    },
];

pub struct GlobalCacheProvider;

fn path_for(t: &Target) -> Option<PathBuf> {
    let p = home_dir()?.join(t.rel);
    p.is_dir().then_some(p)
}

impl Provider for GlobalCacheProvider {
    fn id(&self) -> &'static str {
        "global_cache"
    }
    fn name(&self) -> &'static str {
        "System & tool caches"
    }
    fn default_safety(&self) -> SafetyLevel {
        // Deleting a directory we do not own the internals of. Never `Safe`.
        SafetyLevel::Caution
    }

    fn scan(&self, _roots: &[PathBuf], _cancel: &AtomicBool) -> Vec<Resource> {
        TARGETS
            .iter()
            .filter_map(|t| {
                let dir = path_for(t)?;
                let size = dir_size_detail(&dir);
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
                    blocked_reason: None,
                    blocked_detail: None,
                    impact: impact(
                        t.regenerates,
                        Some("The tool rebuilds this; nothing is re-downloaded for nothing."),
                        None,
                    ),
                })
            })
            .collect()
    }

    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        CleanupAction::RemoveDir(path.to_path_buf())
    }

    fn measure(&self, path: &Path) -> Option<u64> {
        measure_dir(path)
    }

    /// No per-item age: these are single directories, and holding them
    /// back by age would only fill the disk (I9).
    fn extra_allowed_roots(&self) -> Vec<PathBuf> {
        let Some(home) = home_dir() else {
            return Vec::new();
        };
        // Declare exactly the directories we own — NOT `~/.cache` or
        // `~/Library`, which hold far more than we are allowed to touch.
        TARGETS
            .iter()
            .filter_map(|t| {
                let p = home.join(t.rel);
                p.is_dir().then_some(p)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_target_is_home_relative_and_non_empty() {
        let home = home_dir().unwrap();
        for t in TARGETS {
            assert!(!t.rel.starts_with('/'), "{} must be home-relative", t.rel);
            assert!(!t.rel.contains(".."), "{} must not escape HOME", t.rel);
            assert!(!t.rel.is_empty());
            assert!(!t.regenerates.is_empty(), "{} needs a consequence", t.label);
            let _ = home.join(t.rel);
        }
    }

    /// G-SAFE-03: this provider deletes directories whose internals it does
    /// not own, so `Caution` is the floor — never `Safe`.
    #[test]
    fn nothing_in_this_provider_is_marked_safe() {
        assert_eq!(GlobalCacheProvider.default_safety(), SafetyLevel::Caution);
        let cancel = AtomicBool::new(false);
        for r in GlobalCacheProvider.scan(&[], &cancel) {
            assert_eq!(
                r.safety,
                SafetyLevel::Caution,
                "{} must not be advertised as Safe",
                r.name
            );
        }
    }

    /// The roots we declare must be the narrow directories, never a parent
    /// that holds unrelated data.
    #[test]
    fn declared_roots_never_include_a_broad_parent() {
        let roots = GlobalCacheProvider.extra_allowed_roots();
        for r in &roots {
            let s = r.to_string_lossy();
            assert!(
                !s.ends_with("/.cache") && !s.ends_with("/Library/Caches"),
                "declaring {} would widen the validator too far",
                s
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

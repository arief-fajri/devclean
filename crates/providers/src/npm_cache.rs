//! npm/pnpm/yarn cache provider. One resource per package manager found,
//! each cleaned via its official command — cache internals are never
//! deleted by hand (P5). All caches are regenerable (`npm install`, …),
//! hence `Safe`.

use super::provider::{
    dir_size_checked, home_dir, impact, measure_dir, tool_output_dir, CleanupAction, Provider,
};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct NpmCacheProvider;

/// (id suffix, label, program, args to print the cache dir, clean args,
/// cleanup consequence, reclaim cost). Fixed strings only (G-SAFE-02).
struct Manager {
    label: &'static str,
    program: &'static str,
    dir_args: &'static [&'static str],
    clean_args: &'static [&'static str],
    consequence: &'static str,
    cost: &'static str,
}

const MANAGERS: &[Manager] = &[
    Manager {
        label: "npm cache",
        program: "npm",
        dir_args: &["config", "get", "cache"],
        clean_args: &["cache", "clean", "--force"],
        consequence: "Package cache only; your projects are untouched.",
        cost: "Next install re-downloads",
    },
    Manager {
        label: "pnpm store",
        program: "pnpm",
        dir_args: &["store", "path"],
        clean_args: &["store", "prune"],
        consequence: "Package store only; your projects are untouched.",
        cost: "Next install re-fetches",
    },
    Manager {
        label: "yarn cache",
        program: "yarn",
        dir_args: &["cache", "dir"],
        clean_args: &["cache", "clean"],
        consequence: "Download cache only; your projects are untouched.",
        cost: "Next install re-downloads",
    },
];

/// The exact command shown as secondary detail, e.g. `"pnpm store prune"`.
fn clean_command(m: &Manager) -> String {
    format!("{} {}", m.program, m.clean_args.join(" "))
}

/// Resolve a manager's cache dir: prefer the tool's own report, fall back
/// to the conventional home-relative location for npm.
fn cache_dir(m: &Manager) -> Option<PathBuf> {
    if let Some(dir) = tool_output_dir(m.program, m.dir_args) {
        if dir.is_dir() {
            return Some(dir);
        }
    }
    if m.program == "npm" {
        let fallback = home_dir()?.join(".npm");
        if fallback.is_dir() {
            return Some(fallback);
        }
    }
    None
}

impl Provider for NpmCacheProvider {
    fn id(&self) -> &'static str {
        "npm_cache"
    }
    fn name(&self) -> &'static str {
        "npm/pnpm/yarn caches"
    }
    fn default_safety(&self) -> SafetyLevel {
        SafetyLevel::Safe
    }

    fn scan(&self, _roots: &[PathBuf], cancel: &AtomicBool) -> Vec<Resource> {
        let mut out = Vec::new();
        for m in MANAGERS {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let Some(dir) = cache_dir(m) else { continue };
            // Skip (don't abort) caches whose size can't be measured;
            // empty caches report nothing reclaimable.
            let Some(size) = dir_size_checked(&dir, cancel) else {
                continue;
            };
            if size == 0 {
                continue;
            };
            out.push(Resource {
                id: String::new(),
                provider: self.id().to_string(),
                name: m.label.to_string(),
                path: dir,
                size_bytes: size,
                reclaimable_bytes: size,
                safety: SafetyLevel::Safe,
                detail: None,
                age_days: None,
                retention_eligible: true,
                impact: impact(m.consequence, Some(m.cost), Some(&clean_command(m))),
            });
        }
        out
    }

    fn measure(&self, path: &Path) -> Option<u64> {
        measure_dir(path)
    }

    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        // Match the scanned dir back to its manager by re-resolving the
        // tool-reported dirs (server-side, no user input — G-SAFE-02).
        for m in MANAGERS {
            if cache_dir(m).as_deref() == Some(path) {
                return CleanupAction::RunCommand {
                    program: m.program,
                    args: m.clean_args,
                };
            }
        }
        // Unknown dir (e.g. cache vanished after scan): npm clean is the
        // conservative default — still an official fixed command.
        CleanupAction::RunCommand {
            program: "npm",
            args: &["cache", "clean", "--force"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_dir_needs_an_existing_dir() {
        // A manager pointing at a missing tool yields no dir (absence is
        // normal, not an error).
        let missing = Manager {
            label: "nope",
            program: "devclean-definitely-missing-bin",
            dir_args: &[],
            clean_args: &[],
            consequence: "",
            cost: "",
        };
        assert!(cache_dir(&missing).is_none());
    }

    #[test]
    fn managers_describe_impact_and_commands() {
        for m in MANAGERS {
            assert!(!m.consequence.is_empty(), "{} needs a consequence", m.label);
            assert!(!m.cost.is_empty(), "{} needs a cost", m.label);
            let cmd = clean_command(m);
            assert!(cmd.starts_with(m.program), "{cmd} must name the tool");
            assert!(cmd.len() > m.program.len(), "{cmd} must include args");
        }
    }

    #[test]
    fn scan_returns_only_existing_caches() {
        let cancel = AtomicBool::new(false);
        for r in NpmCacheProvider.scan(&[], &cancel) {
            assert!(r.path.is_dir(), "{} must exist", r.path.display());
            assert_eq!(r.provider, "npm_cache");
            assert_eq!(r.safety, SafetyLevel::Safe);
            let impact = r.impact.as_ref().expect("impact must be set");
            assert!(!impact.consequence.is_empty());
            assert!(impact.command.as_deref().is_some_and(|c| !c.is_empty()));
        }
    }
}

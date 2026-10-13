//! Go cache provider. Split into build cache + module cache, cleaned via
//! the official `go` tool — never by deleting cache internals by hand.

use super::provider::{dir_size_checked, CleanupAction, Provider};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct GoProvider;

fn go_env(key: &str) -> Option<PathBuf> {
    std::process::Command::new("go")
        .args(["env", key])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| PathBuf::from(s.trim()))
        .filter(|p| !p.as_os_str().is_empty())
}

impl Provider for GoProvider {
    fn id(&self) -> &'static str {
        "go_cache"
    }
    fn name(&self) -> &'static str {
        "Go cache"
    }
    fn default_safety(&self) -> SafetyLevel {
        SafetyLevel::Safe
    }

    fn scan(&self, _roots: &[PathBuf], cancel: &AtomicBool) -> Vec<Resource> {
        let mut out = Vec::new();
        // (label, env key, clean args) — fixed strings only (G-SAFE-02).
        for (label, key, clean_args) in [
            ("Go build cache", "GOCACHE", "go clean -cache"),
            ("Go module cache", "GOMODCACHE", "go clean -modcache"),
        ] {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            if let Some(dir) = go_env(key) {
                if !dir.is_dir() {
                    continue;
                }
                // Skip (don't abort) caches whose size can't be measured.
                let Some(size) = dir_size_checked(&dir, cancel) else {
                    continue;
                };
                out.push(Resource {
                    id: String::new(),
                    provider: self.id().to_string(),
                    name: label.to_string(),
                    path: dir,
                    size_bytes: size,
                    reclaimable_bytes: size,
                    safety: SafetyLevel::Safe,
                    detail: Some(format!("cleaned via `{clean_args}`")),
                });
            }
        }
        out
    }

    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        // Module cache needs `-modcache`; build cache uses `-cache`.
        // The path was resolved server-side from our own scan, so comparing
        // it against `go env` output is safe (no user input involved).
        if go_env("GOMODCACHE").as_deref() == Some(path) {
            CleanupAction::RunCommand {
                program: "go",
                args: &["clean", "-modcache"],
            }
        } else {
            CleanupAction::RunCommand {
                program: "go",
                args: &["clean", "-cache"],
            }
        }
    }
}

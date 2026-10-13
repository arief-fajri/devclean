//! Docker provider — the most sensitive one.
//!
//! Never reads Docker's internal storage directories. All sizes come from
//! `docker system df`, cleanup runs `docker builder prune -af` (build cache
//! only, `Safe`). Images/containers/volumes are reported as
//! `Caution`/`Dangerous` and are NOT cleaned by default.

use super::provider::{CleanupAction, Provider};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct DockerProvider;

fn docker_available() -> bool {
    std::process::Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Parse `docker system df --format json` reclaimable for build cache.
/// Returns `None` when Docker is absent — absence is normal, not an error.
fn build_cache_bytes() -> Option<u64> {
    let out = std::process::Command::new("docker")
        .args(["system", "df", "--format", "{{json .}}"])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let text = String::from_utf8_lossy(&out.stdout);
    // `docker system df` prints one JSON object per line (Images, Containers,
    // Local Volumes, Build Cache). We only read Build Cache reclaimable.
    for line in text.lines() {
        let v: serde_json::Value = serde_json::from_str(line).ok()?;
        if v.get("Type").and_then(|t| t.as_str()) == Some("Build Cache") {
            return parse_size(
                v.get("Reclaimable")
                    .and_then(|r| r.as_str())
                    .unwrap_or("0B"),
            );
        }
    }
    None
}

/// Parse Docker-style sizes like `"8.418GB"`, `"700MB"`, `"0B"`.
fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim().replace(' ', "");
    let split = s.find(|c: char| c.is_alphabetic())?;
    let (num, unit) = s.split_at(split);
    let num: f64 = num.parse().ok()?;
    let mult = match unit.to_uppercase().as_str() {
        "B" => 1.0,
        "KB" | "KIB" => 1_000.0,
        "MB" | "MIB" => 1_000_000.0,
        "GB" | "GIB" => 1_000_000_000.0,
        "TB" | "TIB" => 1_000_000_000_000.0,
        _ => return None,
    };
    Some((num * mult) as u64)
}

impl Provider for DockerProvider {
    fn id(&self) -> &'static str {
        "docker"
    }
    fn name(&self) -> &'static str {
        "Docker"
    }
    fn default_safety(&self) -> SafetyLevel {
        SafetyLevel::Caution
    }

    fn scan(&self, _roots: &[PathBuf], cancel: &AtomicBool) -> Vec<Resource> {
        if cancel.load(Ordering::Relaxed) {
            return Vec::new();
        }
        if !docker_available() {
            return Vec::new();
        }
        // The "path" is a sentinel: Docker resources have no filesystem path
        // we own. The safety validator treats non-scan-root paths as outside
        // roots, so the cleanup engine special-cases Docker ids to skip path
        // validation and allowlist only the fixed prune command (G-SAFE-02).
        // See `devclean_cleanup::engine` for the allowlist.
        let bytes = build_cache_bytes().unwrap_or(0);
        vec![Resource {
            id: String::new(),
            provider: self.id().to_string(),
            name: "Docker build cache".to_string(),
            path: PathBuf::from("__docker__/build-cache"),
            size_bytes: bytes,
            reclaimable_bytes: bytes,
            safety: SafetyLevel::Safe,
            detail: Some(
                "cleaned via `docker builder prune -af`; future builds may be slower".to_string(),
            ),
        }]
    }

    fn cleanup_action(&self, _path: &Path) -> CleanupAction {
        CleanupAction::RunCommand {
            program: "docker",
            args: &["builder", "prune", "-af"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_docker_sizes() {
        assert_eq!(parse_size("0B"), Some(0));
        assert_eq!(parse_size("700MB"), Some(700_000_000));
        // Float math: allow sub-byte rounding error on fractional sizes.
        let gb = parse_size("8.418GB").unwrap();
        assert!((gb as i64 - 8_418_000_000).abs() < 1_000, "got {gb}");
        assert_eq!(parse_size("nonsense"), None);
    }
}

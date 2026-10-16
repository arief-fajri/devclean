//! Docker provider — the most sensitive one.
//!
//! Never reads Docker's internal storage directories. All sizes come from
//! `docker system df`, cleanup runs `docker builder prune -af` (build cache
//! only, `Safe`). Images/containers/volumes are reported as
//! `Caution`/`Dangerous` and are NOT cleaned by default.

use super::provider::{impact, CleanupAction, Provider};
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
    parse_df_output(&text)
}

/// Pure parser for `docker system df --format "{{json .}}"` output:
/// returns the reclaimable bytes of the `Build Cache` row. Split out from
/// [`build_cache_bytes`] so it is unit-testable without a Docker daemon.
///
/// Unparseable lines are SKIPPED, never fatal — a stray warning line or a
/// blank trailing line must not hide a later Build Cache row.
fn parse_df_output(text: &str) -> Option<u64> {
    // Rows: Images, Containers, Local Volumes, Build Cache. We only read
    // Build Cache reclaimable.
    for line in text.lines() {
        let Some(v) = serde_json::from_str::<serde_json::Value>(line).ok() else {
            continue;
        };
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

/// Reclaimable bytes per row of `docker system df`, in the order the CLI
/// prints them. Used to report what the WIDER scopes would give back —
/// without it, images and volumes are invisible even when the user has
/// enabled those scopes.
fn parse_df_rows(text: &str) -> Vec<(String, u64)> {
    text.lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| {
            let ty = v.get("Type")?.as_str()?.to_string();
            let rec = v
                .get("Reclaimable")
                .and_then(|r| r.as_str())
                .and_then(parse_size)?;
            Some((ty, rec))
        })
        .collect()
}

/// Reclaimable bytes for one row type.
pub fn reclaimable_for(text: &str, row: &str) -> Option<u64> {
    parse_df_rows(text)
        .into_iter()
        .find(|(t, _)| t == row)
        .map(|(_, b)| b)
}

/// Run `docker system df` and return the raw text.
/// `docker system df` text, for sibling providers that read other rows.
pub fn df_text() -> Option<String> {
    df_text_impl()
}
fn df_text_impl() -> Option<String> {
    let out = std::process::Command::new("docker")
        .args(["system", "df", "--format", "{{json .}}"])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Parse Docker-style sizes like `"8.418GB"`, `"700MB"`, `"0B"`.
///
/// Also accepts the real-world form `"11.95GB (83%)"`, where Docker appends
/// a reclaimable percentage in parentheses. That suffix used to make the
/// parse fail, and a failing parse became a silent `None` → reported as
/// `0 B` — so a machine with a genuinely large build cache was reported as
/// having none at all.
fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    // Drop a trailing "(83%)" — and any other parenthesised suffix.
    let s = match s.split_once('(') {
        Some((head, _)) => head.trim(),
        None => s,
    };
    let s = s.replace(' ', "");
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
        // we own. `CleanupAction::RunCommand` never touches a path, so the
        // validator is not consulted for this branch (G-SAFE-02: the only
        // command possible is the fixed pair below — there is no allowlist
        // to consult, because there is nothing path-shaped to allow).
        let bytes = build_cache_bytes().unwrap_or(0);
        // Report nothing when there is nothing to reclaim. Every other
        // provider skips empty results; without this the dashboard showed
        // "Docker build cache · 0 B · Safe" as a cleanable item that does
        // nothing.
        if bytes == 0 {
            return Vec::new();
        }
        vec![Resource {
            id: String::new(),
            provider: self.id().to_string(),
            name: "Docker build cache".to_string(),
            path: PathBuf::from("__docker__/build-cache"),
            size_bytes: bytes,
            reclaimable_bytes: bytes,
            // External figure (`docker system df`), not a tree walk: there is
            // no separate logical size to report.
            apparent_bytes: None,
            safety: SafetyLevel::Safe,
            detail: None,
            age_days: None,
            activity: None,
            last_commit_unix: None,
            blocked_reason: None,
            blocked_detail: None,
            impact: impact(
                "Docker rebuilds image layers from your Dockerfile.",
                Some("Next image build starts from scratch"),
                Some("docker builder prune -af"),
            ),
        }]
    }

    fn cleanup_action(&self, _path: &Path) -> CleanupAction {
        CleanupAction::RunCommand {
            program: "docker",
            args: &["builder", "prune", "-af"],
        }
    }

    /// Docker build cache has no directory of ours — the sentinel path is
    /// never a real dir — so the only honest measurement is re-querying
    /// `docker system df`. `None` (daemon down, output unparseable) means
    /// "unmeasurable", and the engine reports 0 freed with a reason rather
    /// than assuming the requested figure was reclaimed.
    fn measure(&self, _path: &Path) -> Option<u64> {
        docker_available().then(build_cache_bytes).flatten()
    }

    /// Docker build cache has no per-item age — a prune either reclaims the
    /// whole thing or nothing — so retention never holds it back (I9).
    fn required_docker_scope(&self) -> Option<devclean_core::DockerPruneScope> {
        Some(devclean_core::DockerPruneScope::BuildCache)
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

    /// Regression, and the third bug in this family: Docker's REAL output
    /// appends a reclaimable percentage. Parsing failed, the failure became
    /// a silent `None`, and `None` became "0 B" — so a machine with a large
    /// build cache was reported as having none.
    #[test]
    fn parses_the_percentage_suffix_docker_actually_emits() {
        let gb = parse_size("11.95GB (83%)").unwrap();
        assert!((gb as i64 - 11_950_000_000).abs() < 1_000, "got {gb}");
        assert_eq!(parse_size("4.768GB (83%)").unwrap(), 4_768_000_000);
        assert_eq!(parse_size("21.18MB (100%)").unwrap(), 21_180_000);
        assert_eq!(parse_size("1.5GB(0%)").unwrap(), 1_500_000_000);
        assert_eq!(parse_size("  700MB  (50%) ").unwrap(), 700_000_000);
    }

    /// Real `docker system df --format "{{json .}}"` output shape, with a
    /// warning line and a blank line injected before the Build Cache row.
    /// The old `?` operator aborted the whole function on the first bad
    /// line, silently reporting 0 B and hiding a real 9 GB cache.
    #[test]
    fn unparseable_lines_are_skipped_not_fatal() {
        let out = concat!(
            "WARNING: No swap limit support\n",
            r#"{"Active":"3","Reclaimable":"11.95GB (83%)","Size":"14.38GB","TotalCount":"17","Type":"Images"}"#,
            "\n",
            "not json at all\n",
            r#"{"Active":"0","Reclaimable":"0B","Size":"0B","TotalCount":"0","Type":"Containers"}"#,
            "\n",
            r#"{"Active":"0","Reclaimable":"9.5GB","Size":"9.5GB","TotalCount":"42","Type":"Build Cache"}"#,
            "\n",
        );
        assert_eq!(parse_df_output(out), Some(9_500_000_000));
    }

    #[test]
    fn missing_build_cache_row_yields_none() {
        let out = r#"{"Active":"3","Reclaimable":"11.95GB (83%)","Type":"Images"}"#;
        assert_eq!(parse_df_output(out), None);
        assert_eq!(parse_df_output(""), None);
        assert_eq!(parse_df_output("garbage"), None);
    }

    #[test]
    fn zero_build_cache_is_reported_as_zero_not_none() {
        // Real output from a machine with no build cache: the row EXISTS
        // with 0 B. Distinguishing this from "Docker absent" is what lets
        // the provider decide to emit no resource at all.
        let out = r#"{"Active":"0","Reclaimable":"0B","Size":"0B","TotalCount":"0","Type":"Build Cache"}"#;
        assert_eq!(parse_df_output(out), Some(0));
    }
}

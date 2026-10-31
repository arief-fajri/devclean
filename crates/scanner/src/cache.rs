//! Cached scan results: show the previous scan instantly at startup,
//! then rescan in the background.
//!
//! The cache is a versioned envelope bound to the exact scan roots: if the
//! roots changed (or the schema bumped), the cache is ignored rather than
//! shown stale.

use devclean_core::{CachedScan, ScanSummary};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Bumped when the meaning of a cached field changes, not just its shape.
///
/// `3` — `Resource.age_days` moved from "project directory age" to the
/// composite of artifact mtime, last commit and project mtime, and
/// `activity`/`last_commit_unix` were added. A v2 cache holds the old,
/// mtime-only age, so a project committed to daily would still look dormant
/// until a rescan; rejecting it forces the honest numbers.
///
/// `2` — `Resource.size_bytes` moved from logical (sum of file lengths) to
/// allocated blocks. A v1 cache holds numbers that are up to 19% low, so it
/// must be rejected rather than shown: a stale dashboard that quietly
/// disagrees with `du` is worse than a rescan.
const CACHE_VERSION: u32 = 3;

#[derive(Debug, Serialize, Deserialize)]
struct CacheEnvelope {
    version: u32,
    roots: Vec<PathBuf>,
    saved_at_unix: u64,
    summary: ScanSummary,
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn same_roots(a: &[PathBuf], b: &[PathBuf]) -> bool {
    let a: HashSet<&Path> = a.iter().map(|p| p.as_path()).collect();
    let b: HashSet<&Path> = b.iter().map(|p| p.as_path()).collect();
    a == b
}

pub fn save_cached_summary(
    path: &Path,
    roots: &[PathBuf],
    summary: &ScanSummary,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let envelope = CacheEnvelope {
        version: CACHE_VERSION,
        roots: roots.to_vec(),
        saved_at_unix: now_unix(),
        summary: summary.clone(),
    };
    std::fs::write(path, serde_json::to_string_pretty(&envelope).unwrap())
}

/// Load the cache only when it matches `current_roots` and the schema
/// version. Anything else → `None` (caller rescans). Carries `saved_at_unix`
/// so the UI can show how old the last scan is.
pub fn load_cached_scan(path: &Path, current_roots: &[PathBuf]) -> Option<CachedScan> {
    let text = std::fs::read_to_string(path).ok()?;
    let envelope: CacheEnvelope = serde_json::from_str(&text).ok()?;
    if envelope.version != CACHE_VERSION {
        return None;
    }
    if !same_roots(&envelope.roots, current_roots) {
        return None;
    }
    Some(CachedScan {
        summary: envelope.summary,
        saved_at_unix: envelope.saved_at_unix,
    })
}

/// Summary-only view of the cache, for callers that do not need the age
/// (the menu bar reads the file's mtime for that instead).
pub fn load_cached_summary(path: &Path, current_roots: &[PathBuf]) -> Option<ScanSummary> {
    load_cached_scan(path, current_roots).map(|c| c.summary)
}

/// Merge one provider's fresh scan into the cached summary, replacing that
/// provider's resources and its per-provider totals.
///
/// A targeted re-scan (`scan_provider`) must not leave the cache holding
/// resources the provider no longer has, or a restart would resurrect cleaned
/// items. The cache's `saved_at_unix` is **preserved**: it still means "the
/// last full scan", so a partially-refreshed cache is never presented as
/// fresher than it is. A missing cache, a schema mismatch or different roots
/// is a no-op — there is nothing valid to update.
pub fn merge_cached_provider(
    path: &Path,
    current_roots: &[PathBuf],
    provider_id: &str,
    partial: &ScanSummary,
) -> std::io::Result<()> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    let mut envelope: CacheEnvelope = match serde_json::from_str(&text) {
        Ok(envelope) => envelope,
        Err(_) => return Ok(()),
    };
    if envelope.version != CACHE_VERSION || !same_roots(&envelope.roots, current_roots) {
        return Ok(());
    }

    envelope
        .summary
        .resources
        .retain(|r| r.provider != provider_id);
    envelope
        .summary
        .resources
        .extend(partial.resources.iter().cloned());
    envelope
        .summary
        .providers
        .retain(|p| p.provider != provider_id);
    envelope
        .summary
        .providers
        .extend(partial.providers.iter().cloned());
    envelope
        .summary
        .providers
        .sort_by_key(|p| std::cmp::Reverse(p.reclaimable_bytes));
    envelope
        .summary
        .provider_status
        .retain(|s| s.provider != provider_id);
    envelope
        .summary
        .provider_status
        .extend(partial.provider_status.iter().cloned());
    envelope
        .summary
        .provider_status
        .sort_by(|a, b| a.provider.cmp(&b.provider));
    envelope.summary.total_bytes = envelope
        .summary
        .resources
        .iter()
        .map(|r| r.size_bytes)
        .sum();
    envelope.summary.reclaimable_bytes = envelope
        .summary
        .resources
        .iter()
        .map(|r| r.reclaimable_bytes)
        .sum();
    envelope.summary.blocked_bytes = envelope
        .summary
        .resources
        .iter()
        .filter(|r| r.blocked_reason.is_some())
        .map(|r| r.reclaimable_bytes)
        .sum();
    // Wave D6: a targeted merge replaces ONE provider's rows — the
    // whole-scan dedup figure is no longer measurable, so it becomes
    // `None`. Never a stale `Some` from the pre-merge scan.
    envelope.summary.unique_bytes = None;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&envelope).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use devclean_core::SafetyLevel;

    fn summary() -> ScanSummary {
        ScanSummary {
            blocked_bytes: 0,
            unique_bytes: None,
            total_bytes: 10,
            reclaimable_bytes: 10,
            providers: vec![],
            provider_status: vec![],
            resources: vec![devclean_core::Resource {
                id: "nm:0000".to_string(),
                provider: "node_modules".to_string(),
                name: "x".to_string(),
                path: PathBuf::from("/tmp/x"),
                size_bytes: 10,
                reclaimable_bytes: 10,
                apparent_bytes: None,
                safety: SafetyLevel::Safe,
                detail: None,
                impact: None,
                age_days: None,
                activity: None,
                last_commit_unix: None,
                blocked_reason: None,
                blocked_detail: None,
                inode_bytes: Default::default(),
            }],
        }
    }

    fn cache_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("devclean-cache-{tag}"))
    }

    #[test]
    fn roundtrip_with_same_roots() {
        let path = cache_path("ok");
        let _ = std::fs::remove_file(&path);
        let roots = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        save_cached_summary(&path, &roots, &summary()).unwrap();
        let loaded = load_cached_summary(&path, &[PathBuf::from("/b"), PathBuf::from("/a")]);
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().resources.len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_cache_carries_the_time_it_was_saved() {
        // The UI rests on the cache now instead of always rescanning, so it
        // must be able to show how old the numbers are — not the launch time.
        let path = cache_path("saved-at");
        let _ = std::fs::remove_file(&path);
        save_cached_summary(&path, &[], &summary()).unwrap();
        let loaded = load_cached_scan(&path, &[]).expect("cache loads");
        assert!(
            loaded.saved_at_unix > 0,
            "a saved scan must carry its timestamp"
        );
        assert_eq!(loaded.summary.resources.len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn changed_roots_invalidate_cache() {
        let path = cache_path("roots");
        let _ = std::fs::remove_file(&path);
        save_cached_summary(&path, &[PathBuf::from("/a")], &summary()).unwrap();
        assert!(load_cached_summary(&path, &[PathBuf::from("/a"), PathBuf::from("/c")]).is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupt_or_versioned_cache_is_ignored() {
        let path = cache_path("bad");
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, "{not json").unwrap();
        assert!(load_cached_summary(&path, &[]).is_none());
        std::fs::write(&path, r#"{"version":999,"roots":[],"saved_at_unix":0,"summary":{"total_bytes":0,"reclaimable_bytes":0,"providers":[],"resources":[]}}"#).unwrap();
        assert!(load_cached_summary(&path, &[]).is_none());
        let _ = std::fs::remove_file(&path);
    }

    /// A v1 cache holds `size_bytes` computed as a sum of file LENGTHS, which
    /// reads up to 19% below what deleting the tree actually frees. Showing
    /// it after the upgrade would put a number on the dashboard that quietly
    /// disagrees with `du` — worse than a rescan.
    #[test]
    fn a_v1_cache_with_logical_sizes_is_rejected() {
        let path = cache_path("v1");
        let _ = std::fs::remove_file(&path);
        let v1 = r#"{"version":1,"roots":[],"saved_at_unix":0,"summary":{
            "total_bytes":10,"reclaimable_bytes":10,"blocked_bytes":0,
            "providers":[],"resources":[]}}"#;
        std::fs::write(&path, v1).unwrap();
        assert!(
            load_cached_summary(&path, &[]).is_none(),
            "a pre-allocated-sizing cache must never be shown"
        );

        // …and the current version still round-trips.
        save_cached_summary(&path, &[], &summary()).unwrap();
        assert!(load_cached_summary(&path, &[]).is_some());
        let _ = std::fs::remove_file(&path);
    }

    /// A v2 cache holds the old mtime-only `age_days`, so a project committed
    /// to daily would still look dormant until a rescan. It is rejected rather
    /// than shown.
    #[test]
    fn a_v2_cache_with_mtime_only_age_is_rejected() {
        let path = cache_path("v2");
        let _ = std::fs::remove_file(&path);
        let v2 = r#"{"version":2,"roots":[],"saved_at_unix":0,"summary":{
            "total_bytes":10,"reclaimable_bytes":10,"blocked_bytes":0,
            "providers":[],"resources":[]}}"#;
        std::fs::write(&path, v2).unwrap();
        assert!(
            load_cached_summary(&path, &[]).is_none(),
            "a pre-composite-age cache must never be shown"
        );
        let _ = std::fs::remove_file(&path);
    }

    fn res(id: &str, provider: &str, bytes: u64) -> devclean_core::Resource {
        devclean_core::Resource {
            id: id.to_string(),
            provider: provider.to_string(),
            name: id.to_string(),
            path: PathBuf::from(format!("/tmp/{id}")),
            size_bytes: bytes,
            reclaimable_bytes: bytes,
            apparent_bytes: None,
            safety: SafetyLevel::Safe,
            detail: None,
            impact: None,
            age_days: None,
            activity: None,
            last_commit_unix: None,
            blocked_reason: None,
            blocked_detail: None,
            inode_bytes: Default::default(),
        }
    }

    fn two_provider_summary() -> ScanSummary {
        ScanSummary {
            total_bytes: 700,
            reclaimable_bytes: 700,
            blocked_bytes: 0,
            // A full scan measured it — which is exactly why the targeted
            // merge below must clear it.
            unique_bytes: Some(700),
            provider_status: vec![
                devclean_core::ProviderStatus {
                    provider: "node_modules".into(),
                    status: devclean_core::ProviderStatusKind::Ok,
                    detail: None,
                },
                devclean_core::ProviderStatus {
                    provider: "go_cache".into(),
                    status: devclean_core::ProviderStatusKind::Ok,
                    detail: None,
                },
            ],
            providers: vec![
                devclean_core::ProviderSummary {
                    provider: "node_modules".into(),
                    name: "node_modules".into(),
                    total_bytes: 400,
                    reclaimable_bytes: 400,
                    resource_count: 1,
                },
                devclean_core::ProviderSummary {
                    provider: "go_cache".into(),
                    name: "Go cache".into(),
                    total_bytes: 300,
                    reclaimable_bytes: 300,
                    resource_count: 1,
                },
            ],
            resources: vec![
                res("nm:0", "node_modules", 400),
                res("go:0", "go_cache", 300),
            ],
        }
    }

    /// A targeted rescan updates only its provider in the cache, preserving the
    /// other providers and the cache's timestamp (still "the last full scan").
    #[test]
    fn a_targeted_rescan_replaces_one_provider_and_keeps_the_rest() {
        let path = cache_path("merge");
        let _ = std::fs::remove_file(&path);
        save_cached_summary(&path, &[], &two_provider_summary()).unwrap();
        let before = load_cached_scan(&path, &[]).unwrap();

        // node_modules shrank to one 50-byte cache; go_cache is untouched.
        let partial = ScanSummary {
            total_bytes: 50,
            reclaimable_bytes: 50,
            blocked_bytes: 0,
            unique_bytes: None,
            provider_status: vec![devclean_core::ProviderStatus {
                provider: "node_modules".into(),
                status: devclean_core::ProviderStatusKind::Unavailable,
                detail: Some("Docker daemon is not running".into()),
            }],
            providers: vec![devclean_core::ProviderSummary {
                provider: "node_modules".into(),
                name: "node_modules".into(),
                total_bytes: 50,
                reclaimable_bytes: 50,
                resource_count: 1,
            }],
            resources: vec![res("nm:new", "node_modules", 50)],
        };
        merge_cached_provider(&path, &[], "node_modules", &partial).unwrap();

        let after = load_cached_scan(&path, &[]).unwrap();
        assert_eq!(
            after.saved_at_unix, before.saved_at_unix,
            "a targeted rescan must not claim a fresh full scan"
        );
        assert_eq!(
            after
                .summary
                .resources
                .iter()
                .map(|r| r.id.clone())
                .collect::<std::collections::BTreeSet<_>>(),
            ["go:0".to_string(), "nm:new".to_string()]
                .into_iter()
                .collect()
        );
        assert_eq!(after.summary.total_bytes, 350);
        assert_eq!(after.summary.reclaimable_bytes, 350);
        // Wave D6: the merge replaced ONE provider's rows, so the whole-scan
        // dedup figure is no longer measurable — cleared, never stale.
        assert_eq!(
            after.summary.unique_bytes, None,
            "a targeted merge must clear unique_bytes"
        );
        assert_eq!(
            after
                .summary
                .providers
                .iter()
                .find(|p| p.provider == "go_cache")
                .map(|p| p.reclaimable_bytes),
            Some(300),
            "the untouched provider must survive"
        );
        let status = |provider: &str| {
            after
                .summary
                .provider_status
                .iter()
                .find(|s| s.provider == provider)
                .unwrap_or_else(|| panic!("missing status for {provider}"))
        };
        assert_eq!(
            status("node_modules").status,
            devclean_core::ProviderStatusKind::Unavailable
        );
        assert_eq!(
            status("node_modules").detail.as_deref(),
            Some("Docker daemon is not running")
        );
        assert_eq!(
            status("go_cache").status,
            devclean_core::ProviderStatusKind::Ok,
            "the untouched provider's status must survive"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Wave D6: `unique_bytes` is `#[serde(default)]` with NO cache version
    /// bump — a cache written before the field exists loads `None`, which is
    /// the honest answer ("cannot measure"), not an error and not a zero.
    #[test]
    fn a_cache_written_before_unique_bytes_loads_none() {
        let path = cache_path("pre-unique");
        let _ = std::fs::remove_file(&path);
        let pre = format!(
            r#"{{"version":{CACHE_VERSION},"roots":[],"saved_at_unix":1,"summary":{{
            "total_bytes":10,"reclaimable_bytes":10,"blocked_bytes":0,
            "providers":[],"resources":[]}}}}"#
        );
        std::fs::write(&path, pre).unwrap();
        let loaded = load_cached_scan(&path, &[]).expect("the cache must still load");
        assert_eq!(
            loaded.summary.unique_bytes, None,
            "absence is the honest default for an older cache"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Different roots (or a missing file) mean there is nothing valid to
    /// update; the merge is a silent no-op, never a corruption.
    #[test]
    fn a_targeted_rescan_is_ignored_when_roots_or_file_do_not_match() {
        let path = cache_path("merge-mismatch");
        let _ = std::fs::remove_file(&path);
        let partial = ScanSummary {
            total_bytes: 0,
            reclaimable_bytes: 0,
            blocked_bytes: 0,
            unique_bytes: None,
            providers: vec![],
            provider_status: vec![],
            resources: vec![],
        };
        // No file at all.
        assert!(merge_cached_provider(&path, &[], "node_modules", &partial).is_ok());

        // File saved for roots ["/a"], merged against ["/b"].
        save_cached_summary(&path, &[PathBuf::from("/a")], &two_provider_summary()).unwrap();
        merge_cached_provider(&path, &[PathBuf::from("/b")], "node_modules", &partial).unwrap();
        let after = load_cached_summary(&path, &[PathBuf::from("/a")]).unwrap();
        assert_eq!(
            after.resources.len(),
            2,
            "a mismatched roots cache must be left untouched"
        );
        let _ = std::fs::remove_file(&path);
    }
}

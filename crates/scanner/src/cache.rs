//! Cached scan results: show the previous scan instantly at startup,
//! then rescan in the background.
//!
//! The cache is a versioned envelope bound to the exact scan roots: if the
//! roots changed (or the schema bumped), the cache is ignored rather than
//! shown stale.

use devclean_core::ScanSummary;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const CACHE_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct CachedScan {
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
    let envelope = CachedScan {
        version: CACHE_VERSION,
        roots: roots.to_vec(),
        saved_at_unix: now_unix(),
        summary: summary.clone(),
    };
    std::fs::write(path, serde_json::to_string_pretty(&envelope).unwrap())
}

/// Load the cache only when it matches `current_roots` and the schema
/// version. Anything else → `None` (caller rescans).
pub fn load_cached_summary(path: &Path, current_roots: &[PathBuf]) -> Option<ScanSummary> {
    let text = std::fs::read_to_string(path).ok()?;
    let envelope: CachedScan = serde_json::from_str(&text).ok()?;
    if envelope.version != CACHE_VERSION {
        return None;
    }
    if !same_roots(&envelope.roots, current_roots) {
        return None;
    }
    Some(envelope.summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use devclean_core::SafetyLevel;

    fn summary() -> ScanSummary {
        ScanSummary {
            total_bytes: 10,
            reclaimable_bytes: 10,
            providers: vec![],
            resources: vec![devclean_core::Resource {
                id: "nm:0000".to_string(),
                provider: "node_modules".to_string(),
                name: "x".to_string(),
                path: PathBuf::from("/tmp/x"),
                size_bytes: 10,
                reclaimable_bytes: 10,
                safety: SafetyLevel::Safe,
                detail: None,
                impact: None,
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
}

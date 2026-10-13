//! Shared domain types. These are serialized to the Svelte frontend,
//! so every field name is part of the Tauri contract
//! (`docs/reference/tauri-contract.md`). Renaming requires a contract update.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// How safe a resource is to clean. Decided by the provider + safety layer,
/// never by the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SafetyLevel {
    Safe,
    Caution,
    Dangerous,
}

/// A single reclaimable resource found by a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    /// Opaque id issued by the backend registry (e.g. `"nm:3f9a"`).
    /// The frontend must send this back for preview/cleanup — never a path.
    pub id: String,
    /// Provider id, e.g. `"node_modules"`, `"rust_target"`.
    pub provider: String,
    /// Human-readable name, e.g. `"my-app/node_modules"`.
    pub name: String,
    /// Absolute path resolved by the backend. Informational for display only.
    pub path: PathBuf,
    pub size_bytes: u64,
    pub reclaimable_bytes: u64,
    pub safety: SafetyLevel,
    /// Extra context, e.g. `"143 days stale · pnpm"`.
    pub detail: Option<String>,
    /// Human-readable cleanup consequence, set by the provider.
    /// `#[serde(default)]` keeps v0.2.x on-disk scan caches readable.
    #[serde(default)]
    pub impact: Option<ResourceImpact>,
}

/// What happens when a resource is cleaned — one honest sentence plus
/// the price the user pays (time / network) and the exact official
/// command. Decided by the provider, never by the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceImpact {
    /// What happens after cleanup, e.g. `"Dependencies are reinstalled
    /// from the lockfile; source code is untouched."`
    pub consequence: String,
    /// The price, e.g. `"Requires network · slow on metered connections"`.
    pub cost: Option<String>,
    /// The exact official command, shown as secondary detail.
    pub command: Option<String>,
}

/// Per-provider totals for the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSummary {
    pub provider: String,
    pub name: String,
    pub total_bytes: u64,
    pub reclaimable_bytes: u64,
    pub resource_count: u32,
}

/// Aggregate result of a full scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSummary {
    pub total_bytes: u64,
    pub reclaimable_bytes: u64,
    pub providers: Vec<ProviderSummary>,
    pub resources: Vec<Resource>,
}

/// One line of a cleanup preview (shown in the confirmation dialog).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPreviewItem {
    pub resource_id: String,
    pub name: String,
    pub bytes: u64,
    pub safety: SafetyLevel,
    pub warning: Option<String>,
}

/// What the user sees before confirming.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPreview {
    pub items: Vec<CleanupPreviewItem>,
    pub total_bytes: u64,
}

/// Per-resource outcome. Partial failure is reported here, never as a
/// generic "Cleanup failed" string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupResult {
    pub resource_id: String,
    pub requested_bytes: u64,
    pub freed_bytes: u64,
    pub duration_ms: u64,
    pub status: CleanupStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CleanupStatus {
    Success,
    Partial,
    Failed,
    Cancelled,
}

/// One row of the cleanup history log (JSONL on disk, `get_history`
/// over the bridge). Written after every `cleanup` call, one entry per
/// resource — a durable mirror of [`CleanupResult`].
#[cfg(test)]
mod tests {
    use super::*;

    fn impact() -> ResourceImpact {
        ResourceImpact {
            consequence: "Dependencies are reinstalled from the lockfile.".to_string(),
            cost: Some("Next install re-downloads packages".to_string()),
            command: None,
        }
    }

    #[test]
    fn old_cached_scans_without_impact_still_deserialize() {
        // v0.2.x on-disk cache payload: no `impact` field at all.
        let json = r#"{"id":"nm:0000","provider":"node_modules","name":"x","path":"/tmp/x","size_bytes":10,"reclaimable_bytes":10,"safety":"safe","detail":null}"#;
        let r: Resource = serde_json::from_str(json).expect("old cache must load");
        assert_eq!(r.impact, None);
    }

    #[test]
    fn impact_round_trips_through_json() {
        let mut r = Resource {
            id: "nm:0000".to_string(),
            provider: "node_modules".to_string(),
            name: "x".to_string(),
            path: std::path::PathBuf::from("/tmp/x"),
            size_bytes: 10,
            reclaimable_bytes: 10,
            safety: SafetyLevel::Safe,
            detail: None,
            impact: Some(impact()),
        };
        let back: Resource = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(back.impact, r.impact);
        r.impact = None;
        let back: Resource = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(back.impact, None);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Unix timestamp (seconds) when the cleanup finished.
    pub finished_at_unix: u64,
    pub resource_id: String,
    pub provider: String,
    pub name: String,
    pub requested_bytes: u64,
    pub freed_bytes: u64,
    pub status: CleanupStatus,
    pub error: Option<String>,
}

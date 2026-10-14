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
    /// Age of the underlying item in whole days, when the provider can tell.
    /// `None` for global caches whose age is meaningless. This is the input
    /// to the retention policy (I9) and a display signal only — never a
    /// safety verdict.
    #[serde(default)]
    pub age_days: Option<u64>,
    /// False when the active retention policy keeps this resource
    /// (`Settings.retention_days`). Such a resource is reported but must be
    /// refused by the cleanup engine even if its id is sent directly.
    /// `#[serde(default = "default_true")]` keeps old caches eligible.
    #[serde(default = "default_true")]
    pub retention_eligible: bool,
}

fn default_true() -> bool {
    true
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

/// Static provider metadata for the Settings UI (contract v0.4.0).
/// Cheap by design: id + display name + enabled flag, never a scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    /// Whether the provider runs during scans
    /// (`Settings.disabled_providers`).
    pub enabled: bool,
}

/// Free space on the volume holding the app's data (contract v0.5.0).
/// `known: false` when the platform query failed — the UI must then say
/// "unknown", never "0 bytes free".
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiskFree {
    pub available_bytes: u64,
    pub total_bytes: u64,
    pub known: bool,
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
    /// Bytes held back by the active retention policy. Reported separately
    /// so the dashboard can say "12.4 GB reclaimable · 3.1 GB kept" instead
    /// of silently shrinking the headline (I9).
    #[serde(default)]
    pub retention_blocked_bytes: u64,
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
    /// Opaque proof that this exact set of ids was measured and shown to the
    /// user. `cleanup` requires it while `require_confirmation` is on, so
    /// the confirmation gate is enforced by the backend and not merely by
    /// the UI choosing to call `preview_cleanup` first (G-UI-01).
    pub preview_token: String,
    /// Unix seconds after which `preview_token` stops being accepted.
    pub expires_at_unix: u64,
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
            age_days: None,
            retention_eligible: true,
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
    /// Id of the cleanup run this entry belongs to. `#[serde(default)]`
    /// keeps every pre-v0.6 history line readable: legacy entries get an
    /// empty id and are grouped by `finished_at_unix` instead.
    #[serde(default)]
    pub run_id: String,
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

/// One cleanup run, aggregated over its per-resource entries. The primary
/// unit of the history UI: a single run can be hundreds of resources, so
/// an entry-count limit (without run grouping) would let one large cleanup
/// hide every run before it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupRun {
    pub run_id: String,
    pub finished_at_unix: u64,
    pub entry_count: u32,
    pub requested_bytes: u64,
    pub freed_bytes: u64,
    pub success: u32,
    pub partial: u32,
    pub failed: u32,
    pub cancelled: u32,
    pub providers: Vec<String>,
    pub entries: Vec<HistoryEntry>,
}

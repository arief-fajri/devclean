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
    /// Bytes this resource actually occupies — what deleting it frees.
    /// Block-accurate (`st_blocks * 512`), not a sum of payload lengths.
    pub size_bytes: u64,
    pub reclaimable_bytes: u64,
    /// Logical size, present only when it is materially LARGER than
    /// `size_bytes` — i.e. the resource holds sparse files (a 460 GB-declared
    /// disk image occupying 8 GB). `None` for ordinary trees, where block
    /// rounding makes the logical figure smaller, not larger.
    #[serde(default)]
    pub apparent_bytes: Option<u64>,
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
    /// Why this resource cannot be selected for cleanup, or `None` when it
    /// can. Reported resources with a reason are still listed — the user
    /// should see what exists and why it is held back — but the cleanup
    /// engine refuses them even if their id is sent directly (I11).
    ///
    /// Replaces the old `retention_eligible: bool`, which could only say
    /// "no" without saying why.
    #[serde(default)]
    pub blocked_reason: Option<BlockReason>,
    /// One human sentence explaining `blocked_reason`, e.g. "kept by your
    /// 30-day retention policy (this is 12d old)".
    #[serde(default)]
    pub blocked_detail: Option<String>,
}

/// Why a reported resource cannot be cleaned (contract v0.8.0).
///
/// Deliberately a closed set: every reason has exactly one enforcement point
/// in `devclean_cleanup::engine::refusal_for`, so the UI can explain a
/// resource and the engine can refuse it from the same value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockReason {
    /// `Settings.retention_days` keeps it — it is younger than the policy.
    Retention,
    /// AI conversation history, and `Settings.ai_cleanup_enabled` is off.
    AiReadOnly,
    /// The action needs `Settings.allow_privileged_operations`.
    PrivilegedRequired,
    /// `Settings.docker_prune_scope` does not cover this action.
    DockerScope,
    /// Git-aware protection: the artifact is tracked and the repo is dirty.
    Protected,
}

impl BlockReason {
    /// Short label for a badge.
    pub fn label(self) -> &'static str {
        match self {
            Self::Retention => "kept",
            Self::AiReadOnly => "report only",
            Self::PrivilegedRequired => "needs opt-in",
            Self::DockerScope => "scope off",
            Self::Protected => "protected",
        }
    }
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

/// Free space on the volume holding `$HOME` — the volume the dashboard's
/// "free space" card describes (contract v0.7.3).
///
/// This is the *home* volume, not necessarily the volume holding the scan
/// roots; a root on an external drive sits on a different volume. `mount`
/// names the volume so the card can say which one it means instead of leaving
/// "this volume" to guesswork (macOS only; `None` elsewhere).
///
/// `known: false` when the platform query failed — the UI must then say
/// "unknown", never "0 bytes free".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiskFree {
    pub available_bytes: u64,
    pub total_bytes: u64,
    pub known: bool,
    /// Mount point of this volume (e.g. `/Volumes/Work`), when the platform
    /// can name it.
    #[serde(default)]
    pub mount: Option<String>,
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
    /// Bytes returned to the filesystem NOW. Under `DeleteMode::Trash` this
    /// is 0 — the artifact is recoverable and the space returns only when the
    /// user empties the Trash. Claiming it earlier would be a lie.
    pub freed_bytes: u64,
    /// True when the artifact was moved to the Trash rather than removed.
    #[serde(default)]
    pub moved_to_trash: bool,
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
            apparent_bytes: None,
            safety: SafetyLevel::Safe,
            detail: None,
            impact: Some(impact()),
            age_days: None,
            blocked_reason: None,
            blocked_detail: None,
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
    /// Mirrors [`CleanupResult::moved_to_trash`] for the durable log.
    #[serde(default)]
    pub moved_to_trash: bool,
    /// Which sizing convention produced these byte figures.
    ///
    /// `#[serde(default)]` → [`Sizing::Apparent`], which is what every line
    /// written before v0.8.0 actually used (a sum of file lengths). Those
    /// rows stay readable and the UI marks them "before 0.1.0" rather than
    /// silently comparing them against allocated figures.
    #[serde(default)]
    pub sizing: Sizing,
    pub status: CleanupStatus,
    pub error: Option<String>,
}

/// Which sizing convention a byte figure was produced with (v0.8.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sizing {
    /// Sum of file lengths. Every entry written before v0.8.0.
    #[default]
    Apparent,
    /// Allocated blocks — what deleting frees. v0.8.0 onwards.
    Allocated,
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
    /// True when ANY entry in the run used the pre-v0.8.0 sizing convention
    /// (a sum of file lengths). The UI flags such a run so its totals are not
    /// silently compared against allocated figures.
    #[serde(default)]
    pub pre_allocated_sizing: bool,
    pub entries: Vec<HistoryEntry>,
}

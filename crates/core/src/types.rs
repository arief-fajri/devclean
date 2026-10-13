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

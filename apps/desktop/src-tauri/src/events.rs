//! Tauri event names + payloads. Part of the contract
//! (`docs/reference/tauri-contract.md`) — renaming is a contract change.

pub const SCAN_STARTED: &str = "scan_started";
pub const SCAN_PROGRESS: &str = "scan_progress";
pub const SCAN_COMPLETED: &str = "scan_completed";

pub const CLEANUP_STARTED: &str = "cleanup_started";
pub const CLEANUP_PROGRESS: &str = "cleanup_progress";
pub const CLEANUP_COMPLETED: &str = "cleanup_completed";

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanProgressPayload {
    pub provider: String,
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CleanupProgressPayload {
    pub done: usize,
    pub total: usize,
}

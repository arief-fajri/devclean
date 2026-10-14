//! Tauri event names + payloads. Part of the contract
//! (`docs/reference/tauri-contract.md`) — renaming is a contract change.

pub const SCAN_STARTED: &str = "scan_started";
pub const SCAN_PROGRESS: &str = "scan_progress";
pub const SCAN_COMPLETED: &str = "scan_completed";
/// v0.5.0: emitted when a provider BEGINS. `scan_progress` only reports
/// providers that already finished, so without this the UI cannot say which
/// one is running *right now*.
pub const SCAN_PROVIDER_STARTED: &str = "scan_provider_started";

/// v0.7.0: the menu bar asking the window to show a page. Emitted only after
/// the window is brought forward, so the user always sees what happened.
pub const TRAY_ACTION: &str = "tray_action";

pub const CLEANUP_STARTED: &str = "cleanup_started";
pub const CLEANUP_PROGRESS: &str = "cleanup_progress";
pub const CLEANUP_COMPLETED: &str = "cleanup_completed";

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanProgressPayload {
    pub provider: String,
    pub done: usize,
    pub total: usize,
}

/// Payload of [`TRAY_ACTION`]. `filter` is a hint for the list view
/// (`"safe"` pre-selects the safety filter) — the frontend validates both
/// fields and ignores anything it does not know.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TrayAction {
    pub route: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CleanupProgressPayload {
    pub done: usize,
    pub total: usize,
}

//! DevClean core: shared types, safety gate, and system helpers.
//!
//! This crate must stay dependency-light and must NOT depend on any other
//! DevClean crate. The dependency direction is:
//!
//! ```text
//! core <- providers <- scanner / cleanup <- src-tauri <- UI
//! ```

pub mod format;
pub mod limits;
pub mod policy;
pub mod preview_token;
pub mod safety;
pub mod settings;
pub mod system;
pub mod types;

pub use format::{format_age, format_bytes};
pub use limits::{CLEANUP_COMMAND_TIMEOUT, PREVIEW_TIMEOUT, PROVIDER_SCAN_TIMEOUT};
pub use policy::{block_reason_for, stamp_blocked_reason, PolicyInputs};
pub use preview_token::{authorize, GateError, PreviewToken, PREVIEW_TTL_SECS};

pub use safety::{SafetyError, SafetyValidator};
pub use settings::{DeleteMode, DockerPruneScope, Settings, ThemeMode};
pub use system::{
    app_data_dir, default_scan_roots, detect_scan_roots, home_dir, platform_name, ScanRootCandidate,
};
pub use types::{
    BlockReason, CachedScan, CleanupPreview, CleanupPreviewItem, CleanupResult, CleanupRun,
    CleanupStatus, DiskFree, HistoryEntry, ProviderInfo, ProviderSummary, Resource, ResourceImpact,
    SafetyLevel, ScanSummary, Sizing,
};

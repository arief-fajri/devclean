//! DevClean core: shared types, safety gate, and system helpers.
//!
//! This crate must stay dependency-light and must NOT depend on any other
//! DevClean crate. The dependency direction is:
//!
//! ```text
//! core <- providers <- scanner / cleanup <- src-tauri <- UI
//! ```

pub mod safety;
pub mod settings;
pub mod system;
pub mod types;

pub use safety::{SafetyError, SafetyValidator};
pub use settings::Settings;
pub use system::{default_scan_roots, platform_name};
pub use types::{
    CleanupPreview, CleanupPreviewItem, CleanupResult, CleanupStatus, ProviderSummary, Resource,
    SafetyLevel, ScanSummary,
};

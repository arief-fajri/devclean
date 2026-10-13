//! Cached scan results: show the previous scan instantly at startup,
//! then rescan in the background.

use devclean_core::ScanSummary;
use std::path::Path;

pub fn save_cached_summary(path: &Path, summary: &ScanSummary) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(summary).unwrap())
}

pub fn load_cached_summary(path: &Path) -> Option<ScanSummary> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

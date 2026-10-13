//! User settings. Privileged operations default to OFF (guardrail G-SAFE-03).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub scan_roots: Vec<PathBuf>,
    pub excluded_paths: Vec<PathBuf>,
    pub require_confirmation: bool,
    pub show_preview: bool,
    pub allow_privileged_operations: bool,
    pub scan_automatically: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scan_roots: crate::system::default_scan_roots(),
            excluded_paths: Vec::new(),
            require_confirmation: true,
            show_preview: true,
            allow_privileged_operations: false,
            scan_automatically: false,
        }
    }
}

impl Settings {
    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap())
    }
}

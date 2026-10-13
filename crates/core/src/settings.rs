//! User settings. Privileged operations default to OFF (guardrail G-SAFE-03).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub scan_roots: Vec<PathBuf>,
    pub excluded_paths: Vec<PathBuf>,
    pub require_confirmation: bool,
    pub show_preview: bool,
    pub allow_privileged_operations: bool,
    pub scan_automatically: bool,
    /// M3: persisted theme. `#[serde(default)]` keeps pre-M3 settings.json
    /// files readable (they fall back to System).
    #[serde(default)]
    pub theme: ThemeMode,
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
            theme: ThemeMode::System,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_m3_settings_file_without_theme_still_loads() {
        let dir = std::env::temp_dir().join(format!("devclean-set-pre-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(
            &path,
            r#"{"scan_roots":[],"excluded_paths":[],"require_confirmation":true,"show_preview":true,"allow_privileged_operations":false,"scan_automatically":false}"#,
        )
        .unwrap();
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.theme, ThemeMode::System);
        assert!(loaded.require_confirmation);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_roundtrip_with_theme() {
        let dir = std::env::temp_dir().join(format!("devclean-set-rt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("settings.json");
        let s = Settings {
            theme: ThemeMode::Dark,
            ..Settings::default()
        };
        s.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path).theme, ThemeMode::Dark);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

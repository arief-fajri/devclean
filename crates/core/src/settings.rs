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

/// How far Docker cleanup is allowed to go (contract v0.5.0).
///
/// Default is [`DockerPruneScope::BuildCache`] — the behaviour DevClean has
/// always had. Everything beyond that removes data users commonly need:
/// images they rebuild by hand, volumes holding database state. Those scopes
/// additionally require `allow_privileged_operations` (G-SAFE-03), which is
/// now actually READ by the cleanup engine instead of being a stored flag
/// nobody consults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DockerPruneScope {
    /// Never touch Docker.
    Off,
    /// `docker builder prune -af` — build cache only. Default.
    #[default]
    BuildCache,
    /// + `docker image prune -af` — unused images.
    ///
    /// Requires privileged opt-in.
    BuildCacheImages,
    /// + `docker volume prune -af` — unused volumes, which may hold state.
    ///
    /// Requires privileged opt-in.
    All,
}

impl DockerPruneScope {
    /// Scopes beyond build cache must be unlocked by the user.
    pub fn requires_privileged(self) -> bool {
        !matches!(self, Self::Off | Self::BuildCache)
    }

    /// One honest sentence describing what this scope removes. Shown next
    /// to the setting and reused in the refusal message, so the user always
    /// sees the exact consequence of turning it up.
    pub fn consequence(self) -> &'static str {
        match self {
            Self::Off => "no Docker cleanup at all",
            Self::BuildCache => "only the build cache — images rebuild from your Dockerfile",
            Self::BuildCacheImages => "also unused images, which you rebuild by hand",
            Self::All => "also unused images AND volumes — volumes can hold database state",
        }
    }
}

/// How a cleanup removes something (contract v0.8.0).
///
/// Default is [`DeleteMode::Trash`]: a mistake must be recoverable. This tool
/// runs against directories the user did not hand-pick one by one, so "one
/// wrong click" has to be undoable — and emptying the Trash is a decision the
/// user makes later, deliberately.
///
/// `Permanent` exists because some users genuinely want the space back now (a
/// 13 GB database does not fit in a Trash nobody empties), but it is never
/// the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeleteMode {
    /// Move to the OS Trash / Recycle Bin. Recoverable. Default.
    #[default]
    Trash,
    /// Remove outright. Frees space immediately, with no undo.
    Permanent,
}

impl DeleteMode {
    /// True when a cleanup under this mode returns space to the filesystem
    /// right away. Under `Trash` the bytes are *pending* until the user
    /// empties it, so `freed_bytes` must not claim them.
    pub fn frees_space_immediately(self) -> bool {
        matches!(self, Self::Permanent)
    }
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
    /// Provider ids excluded from scans (empty = everything active).
    /// `#[serde(default)]` keeps older settings.json files readable — they
    /// simply enable all providers. Disabled on purpose, never by default.
    #[serde(default)]
    pub disabled_providers: Vec<String>,
    /// v0.5.0: keep any resource younger than this many days out of
    /// cleanup. `None` (default) = no age policy. Affected resources are
    /// still REPORTED, with `retention_eligible: false`, so the user can
    /// see exactly what the policy holds back (I9).
    #[serde(default)]
    pub retention_days: Option<u32>,
    /// v0.5.0: how far Docker cleanup may go.
    #[serde(default)]
    pub docker_prune_scope: DockerPruneScope,
    /// v0.5.0: copy the AI database aside before a destructive SQL cleanup.
    /// Defaults to FALSE — a 13 GB backup needs 13 GB free, and silently
    /// requiring that would make the feature unusable on smaller disks.
    /// When false the confirmation dialog states the size that would be
    /// needed and shows the risk, instead of failing at the last moment.
    #[serde(default)]
    pub sqlite_backup: bool,
    /// v0.8.0: how a cleanup removes a resource. Default `trash`, so a
    /// mistake is recoverable. `#[serde(default)]` keeps older settings.json
    /// files readable — they get the safer default, not the old behaviour.
    #[serde(default)]
    pub delete_mode: DeleteMode,
    /// v0.8.0: allow cleanup of AI conversation history.
    ///
    /// Defaults to FALSE. Session transcripts, project memory and agent
    /// state are not regenerable, so the shipped default is to REPORT them
    /// and refuse to delete until the user opts in explicitly (I11).
    #[serde(default)]
    pub ai_cleanup_enabled: bool,
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
            disabled_providers: Vec::new(),
            retention_days: None,
            docker_prune_scope: DockerPruneScope::default(),
            sqlite_backup: false,
            delete_mode: DeleteMode::default(),
            ai_cleanup_enabled: false,
        }
    }
}

impl Settings {
    pub fn load_from(path: &Path) -> Self {
        let loaded: Self = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        // Migration: v0.4.x persisted `scan_roots: []` whenever the old
        // default list (`~/Projects`, `~/Code`, `~/Workspace`) matched no
        // existing folder. That empty list is indistinguishable from "the
        // user removed every root", but leaving it in place means every
        // root-driven provider (node_modules, rust_target, python_cache)
        // silently scans nothing forever. Re-seed from the current defaults;
        // a user who genuinely wants no roots keeps none because the
        // Settings dialog refuses to save an empty list.
        if loaded.scan_roots.is_empty() {
            Self {
                scan_roots: crate::system::default_scan_roots(),
                ..loaded
            }
        } else {
            loaded
        }
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
    use crate::system::home_dir;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("devclean-set-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.join("settings.json")
    }

    /// Regression: a v0.4.x settings file with `scan_roots: []` kept the
    /// app permanently blind to the user's dev folders, because the file
    /// shadowed the default list forever.
    #[test]
    fn empty_scan_roots_are_reseeded_from_defaults() {
        let path = tmp("reseed");
        std::fs::write(
            &path,
            r#"{"scan_roots":[],"excluded_paths":[],"require_confirmation":true,"show_preview":true,"allow_privileged_operations":false,"scan_automatically":false}"#,
        )
        .unwrap();
        let loaded = Settings::load_from(&path);
        let defaults = crate::system::default_scan_roots();
        if defaults.is_empty() {
            // No dev folder on this machine: nothing to re-seed, but the
            // load must still succeed and keep the other fields.
            assert!(loaded.scan_roots.is_empty());
        } else {
            assert_eq!(
                loaded.scan_roots, defaults,
                "an empty persisted root list must fall back to the defaults"
            );
        }
        assert!(loaded.require_confirmation);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_deliberately_configured_root_list_is_never_overwritten() {
        let path = tmp("keep");
        let mine = home_dir().unwrap().join("my-work");
        std::fs::write(
            &path,
            format!(
                r#"{{"scan_roots":["{}"],"excluded_paths":[],"require_confirmation":true,"show_preview":true,"allow_privileged_operations":false,"scan_automatically":false}}"#,
                mine.display()
            ),
        )
        .unwrap();
        assert_eq!(Settings::load_from(&path).scan_roots, vec![mine]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

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
    fn pre_v04_settings_file_without_disabled_providers_enables_everything() {
        let dir = std::env::temp_dir().join(format!("devclean-set-pre4-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(
            &path,
            r#"{"scan_roots":[],"excluded_paths":[],"require_confirmation":true,"show_preview":true,"allow_privileged_operations":false,"scan_automatically":false,"theme":"system"}"#,
        )
        .unwrap();
        let loaded = Settings::load_from(&path);
        assert!(loaded.disabled_providers.is_empty());
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

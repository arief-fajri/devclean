//! Shared Tauri state: the single owner of scan results, the id→path
//! registry, and the scan/cleanup managers.
//!
//! The Svelte frontend never sees raw paths as capabilities — it sends
//! opaque resource ids, and this state resolves them server-side (I1).
//!
//! M3: settings, scan cache, and cleanup history persist under the
//! per-platform app data dir (never inside a scan root).

use devclean_cleanup::CleanupEngine;
use devclean_core::{app_data_dir, DiskFree, Resource, Settings, ThemeMode};
use devclean_scanner::ScanManager;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

pub struct AppState {
    pub scan: Arc<ScanManager>,
    pub cleanup: CleanupEngine,
    /// Last scan results by resource id.
    pub resources: Mutex<HashMap<String, Resource>>,
    pub settings: Mutex<Settings>,
    /// On-disk locations (all under [`app_data_dir`]).
    pub settings_path: PathBuf,
    pub scan_cache_path: PathBuf,
    pub history_path: PathBuf,
    /// Free space on the data volume, measured once per scan.
    ///
    /// The menu bar reads this rather than calling `get_disk_free`: a probe
    /// that spawns `df` must never run on "the user opened a menu" (G-REL-01 —
    /// no unbounded work on a UI path).
    pub disk_free: Mutex<DiskFree>,
}

impl Default for AppState {
    fn default() -> Self {
        let data_dir = app_data_dir();
        let settings_path = data_dir.join("settings.json");
        let settings = Settings::load_from(&settings_path);
        Self {
            scan: Arc::new(ScanManager::default()),
            cleanup: CleanupEngine::default(),
            resources: Mutex::new(HashMap::new()),
            settings: Mutex::new(settings),
            settings_path,
            scan_cache_path: data_dir.join("scan-cache.json"),
            history_path: data_dir.join("history.jsonl"),
            disk_free: Mutex::new(DiskFree {
                available_bytes: 0,
                total_bytes: 0,
                known: false,
            }),
        }
    }
}

impl AppState {
    pub fn snapshot_resources(&self) -> HashMap<String, Resource> {
        self.resources.lock().unwrap().clone()
    }

    pub fn store_resources(&self, resources: Vec<Resource>) {
        let mut map = self.resources.lock().unwrap();
        map.clear();
        for r in resources {
            map.insert(r.id.clone(), r);
        }
    }

    /// Record the free space measured by the latest scan, for the menu bar.
    pub fn store_disk_free(&self, free: DiskFree) {
        *self.disk_free.lock().unwrap() = free;
    }

    pub fn disk_free(&self) -> DiskFree {
        *self.disk_free.lock().unwrap()
    }

    /// Persist settings best-effort: a failed write is logged, never fatal
    /// to the user's save (the in-memory value is still applied).
    pub fn persist_settings(&self, settings: &Settings) {
        if let Err(e) = settings.save_to(&self.settings_path) {
            tracing::warn!("could not persist settings: {e}");
        }
    }
}

/// Window background colors, mirroring the app surfaces (zinc-50 / zinc-950)
/// so the native title bar blends into the webview instead of showing
/// whatever is behind the window.
const BG_LIGHT: tauri::webview::Color = tauri::webview::Color(250, 250, 250, 255);
const BG_DARK: tauri::webview::Color = tauri::webview::Color(9, 9, 11, 255);

/// Best-effort system dark-mode probe. Dependency-free: macOS reports it
/// through the `AppleInterfaceStyle` default; elsewhere we assume light.
pub fn system_prefers_dark() -> bool {
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleInterfaceStyle"])
            .stdin(std::process::Stdio::null())
            .output()
        {
            if out.status.success() {
                return String::from_utf8_lossy(&out.stdout).trim() == "Dark";
            }
        }
    }
    false
}

/// Paint the native window background so the title bar is opaque and
/// follows the active theme. Called at startup and whenever the theme
/// changes (theme changes always flow through `update_settings`).
pub fn apply_window_background(app: &AppHandle, theme: ThemeMode) {
    let dark = match theme {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::System => system_prefers_dark(),
    };
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if let Err(e) = window.set_background_color(Some(if dark { BG_DARK } else { BG_LIGHT })) {
        tracing::warn!("could not set window background: {e}");
    }
}

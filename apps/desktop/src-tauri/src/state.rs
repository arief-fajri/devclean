//! Shared Tauri state: the single owner of scan results, the id→path
//! registry, and the scan/cleanup managers.
//!
//! The Svelte frontend never sees raw paths as capabilities — it sends
//! opaque resource ids, and this state resolves them server-side (I1).
//!
//! M3: settings, scan cache, and cleanup history persist under the
//! per-platform app data dir (never inside a scan root).

use devclean_cleanup::CleanupEngine;
use devclean_core::{app_data_dir, DiskFree, Resource, Settings, ThemeMode, UpdateStatus};
use devclean_scanner::ScanManager;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
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
    /// v0.12.0: the pending update, held between `check_for_update` and
    /// `install_update`. The updater handle owns the download URL and the
    /// signature that must be verified, so it cannot be re-derived from a
    /// version string alone.
    pub update: Mutex<Option<tauri_plugin_updater::Update>>,
    /// v0.12.0: the last updater state, so `get_update_status` answers
    /// without a network call and the Settings page renders on mount.
    pub update_status: Mutex<UpdateStatus>,
    /// v0.12.0: true while a scan or cleanup owns the filesystem. Installing
    /// an update restarts the app, and a restart mid-cleanup would interrupt
    /// a deletion, so `install_update` refuses while this is set (G-UPD-03).
    pub op_running: AtomicBool,
}

/// RAII marker for "a scan or cleanup is running" (G-UPD-03).
///
/// Held for the whole command, so every exit path — success, error, panic —
/// clears the flag. A plain `store(false)` at the end would leak the flag on
/// the error paths and permanently block updates.
pub struct BusyGuard<'a>(&'a AtomicBool);

impl<'a> BusyGuard<'a> {
    pub fn new(flag: &'a AtomicBool) -> Self {
        flag.store(true, Ordering::SeqCst);
        Self(flag)
    }
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
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
                mount: None,
            }),
            update: Mutex::new(None),
            // Stamped with the app's version by the commands; the field only
            // needs to exist before the first check.
            update_status: Mutex::new(UpdateStatus::idle(env!("CARGO_PKG_VERSION"))),
            op_running: AtomicBool::new(false),
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

    /// A copy of the last measured free space, for the menu bar. Cloned: the
    /// value is three numbers and one optional mount name, and callers hold it
    /// across a paint.
    pub fn disk_free(&self) -> DiskFree {
        self.disk_free.lock().unwrap().clone()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// G-UPD-03: the busy flag must clear on the normal path, so a finished
    /// scan does not permanently block updates.
    #[test]
    fn busy_guard_clears_the_flag_on_drop() {
        let flag = AtomicBool::new(false);
        {
            let _guard = BusyGuard::new(&flag);
            assert!(flag.load(Ordering::SeqCst), "the guard must raise the flag");
        }
        assert!(!flag.load(Ordering::SeqCst), "and lower it on drop");
    }

    /// ...and on the panic path too: an unwinding scan must not leave the app
    /// in a state where updates can never be installed.
    #[test]
    fn busy_guard_clears_the_flag_when_unwound() {
        let flag = AtomicBool::new(false);
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = BusyGuard::new(&flag);
            panic!("scan blew up");
        }));
        assert!(caught.is_err(), "the panic must propagate");
        assert!(
            !flag.load(Ordering::SeqCst),
            "a panic must not leave the filesystem marked busy forever"
        );
    }

    /// The updater rests in `Idle` with the running version, so
    /// `get_update_status` can answer before any check happens.
    #[test]
    fn default_state_rests_in_idle() {
        let state = AppState::default();
        let status = state.update_status.lock().unwrap();
        assert_eq!(status.phase, devclean_core::UpdatePhase::Idle);
        assert!(!status.current_version.is_empty());
        assert!(state.update.lock().unwrap().is_none());
    }
}

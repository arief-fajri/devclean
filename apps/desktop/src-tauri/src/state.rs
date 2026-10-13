//! Shared Tauri state: the single owner of scan results, the id→path
//! registry, and the scan/cleanup managers.
//!
//! The Svelte frontend never sees raw paths as capabilities — it sends
//! opaque resource ids, and this state resolves them server-side (I1).
//!
//! M3: settings, scan cache, and cleanup history persist under the
//! per-platform app data dir (never inside a scan root).

use devclean_cleanup::CleanupEngine;
use devclean_core::{app_data_dir, Resource, Settings};
use devclean_scanner::ScanManager;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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

    /// Persist settings best-effort: a failed write is logged, never fatal
    /// to the user's save (the in-memory value is still applied).
    pub fn persist_settings(&self, settings: &Settings) {
        if let Err(e) = settings.save_to(&self.settings_path) {
            tracing::warn!("could not persist settings: {e}");
        }
    }
}

//! Shared Tauri state: the single owner of scan results, the id→path
//! registry, and the scan/cleanup managers.
//!
//! The Svelte frontend never sees raw paths as capabilities — it sends
//! opaque resource ids, and this state resolves them server-side (I1).

use devclean_cleanup::CleanupEngine;
use devclean_core::{Resource, Settings};
use devclean_scanner::ScanManager;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub scan: Arc<ScanManager>,
    pub cleanup: CleanupEngine,
    /// Last scan results by resource id.
    pub resources: Mutex<HashMap<String, Resource>>,
    pub settings: Mutex<Settings>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            scan: Arc::new(ScanManager::default()),
            cleanup: CleanupEngine::default(),
            resources: Mutex::new(HashMap::new()),
            settings: Mutex::new(Settings::default()),
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
}

//! Opaque id registry. The frontend only ever sees ids like `"nm:0"`;
//! the real path lives here, server-side (guardrail G-SAFE-01).

use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct ProviderRegistry {
    paths: HashMap<String, PathBuf>,
    counters: HashMap<String, usize>,
}

impl ProviderRegistry {
    /// Assign ids and remember paths. Returns the resources with ids filled.
    pub fn register(
        &mut self,
        provider_id: &str,
        prefix: &str,
        mut resources: Vec<devclean_core::Resource>,
    ) -> Vec<devclean_core::Resource> {
        let counter = self.counters.entry(provider_id.to_string()).or_insert(0);
        for r in &mut resources {
            let id = format!("{prefix}:{counter:04}");
            *counter += 1;
            self.paths.insert(id.clone(), r.path.clone());
            r.id = id;
        }
        resources
    }

    pub fn lookup(&self, resource_id: &str) -> Option<PathBuf> {
        self.paths.get(resource_id).cloned()
    }
}

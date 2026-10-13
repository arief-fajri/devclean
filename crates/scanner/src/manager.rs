//! Scan orchestration: fan out to providers, aggregate, register ids.

use devclean_core::{ProviderSummary, Resource, ScanSummary};
use devclean_providers::{all_providers, Provider, ProviderRegistry};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::Semaphore;

/// Progress callback invoked per completed provider (throttle at the caller
/// to ~100-500ms for UI updates).
pub type ProgressFn = Box<dyn Fn(&str, usize, usize) + Send + Sync>;

pub struct ScanManager {
    providers: Vec<Arc<dyn Provider>>,
    semaphore: Arc<Semaphore>,
    cancelled: Arc<AtomicBool>,
}

impl Default for ScanManager {
    fn default() -> Self {
        Self {
            providers: all_providers(),
            // Bounded concurrency: at most 4 providers scan at once (G-SCAN-01).
            semaphore: Arc::new(Semaphore::new(4)),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl ScanManager {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Run all providers concurrently and return the aggregated summary.
    /// Blocking filesystem walks run on `spawn_blocking` so the async
    /// runtime stays responsive.
    pub async fn scan_all(
        &self,
        roots: Vec<PathBuf>,
        on_progress: Option<ProgressFn>,
    ) -> ScanSummary {
        let on_progress: Option<Arc<ProgressFn>> = on_progress.map(Arc::new);
        let mut handles = Vec::new();

        for (idx, provider) in self.providers.iter().enumerate() {
            let total = self.providers.len();
            let p = Arc::clone(provider);
            let roots = roots.clone();
            let sem = Arc::clone(&self.semaphore);
            let cancelled = Arc::clone(&self.cancelled);
            let cb = on_progress.clone();
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire_owned().await.unwrap();
                if cancelled.load(Ordering::SeqCst) {
                    return (p.id().to_string(), Vec::new());
                }
                let roots = roots.clone();
                let pid = p.id().to_string();
                let found = tokio::task::spawn_blocking(move || p.scan(&roots))
                    .await
                    .unwrap_or_default();
                if let Some(cb) = cb {
                    cb(&pid, idx + 1, total);
                }
                (pid, found)
            }));
        }

        let mut registry = ProviderRegistry::default();
        let mut resources: Vec<Resource> = Vec::new();
        for h in handles {
            let (provider_id, found) = h.await.unwrap_or_default();
            let prefix = short_prefix(&provider_id);
            resources.extend(registry.register(&provider_id, prefix, found));
        }

        aggregate(resources)
    }
}

fn short_prefix(provider_id: &str) -> &str {
    match provider_id {
        "node_modules" => "nm",
        "rust_target" => "rs",
        "go_cache" => "go",
        "docker" => "dk",
        _ => "xx",
    }
}

fn aggregate(resources: Vec<Resource>) -> ScanSummary {
    let mut by_provider: HashMap<String, Vec<&Resource>> = HashMap::new();
    for r in &resources {
        by_provider.entry(r.provider.clone()).or_default().push(r);
    }
    let providers = all_providers();
    let mut summaries: Vec<ProviderSummary> = by_provider
        .into_iter()
        .map(|(provider, rs)| {
            let name = providers
                .iter()
                .find(|p| p.id() == provider)
                .map(|p| p.name().to_string())
                .unwrap_or_else(|| provider.clone());
            ProviderSummary {
                provider,
                name,
                total_bytes: rs.iter().map(|r| r.size_bytes).sum(),
                reclaimable_bytes: rs.iter().map(|r| r.reclaimable_bytes).sum(),
                resource_count: rs.len() as u32,
            }
        })
        .collect();
    summaries.sort_by_key(|a| std::cmp::Reverse(a.reclaimable_bytes));

    ScanSummary {
        total_bytes: resources.iter().map(|r| r.size_bytes).sum(),
        reclaimable_bytes: resources.iter().map(|r| r.reclaimable_bytes).sum(),
        providers: summaries,
        resources,
    }
}

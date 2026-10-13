//! Scan orchestration: fan out to providers, aggregate, register ids.

use devclean_core::{ProviderSummary, Resource, ScanSummary, PROVIDER_SCAN_TIMEOUT};
use devclean_providers::{all_providers, Provider, ProviderRegistry};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::sync::Semaphore;

/// Progress callback invoked per completed provider (throttle at the caller
/// to ~100-500ms for UI updates).
pub type ProgressFn = Box<dyn Fn(&str, usize, usize) + Send + Sync>;

pub struct ScanManager {
    providers: Vec<Arc<dyn Provider>>,
    semaphore: Arc<Semaphore>,
    cancelled: Arc<AtomicBool>,
    /// Per-provider wall clock limit (G-REL-01). Exceeded → that provider
    /// contributes nothing this round (logged, never fatal).
    timeout: Duration,
}

impl Default for ScanManager {
    fn default() -> Self {
        Self::with_config(all_providers(), PROVIDER_SCAN_TIMEOUT)
    }
}

impl ScanManager {
    pub fn with_config(providers: Vec<Arc<dyn Provider>>, timeout: Duration) -> Self {
        Self {
            providers,
            // Bounded concurrency: at most 4 providers scan at once (G-SCAN-01).
            semaphore: Arc::new(Semaphore::new(4)),
            cancelled: Arc::new(AtomicBool::new(false)),
            timeout,
        }
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Run all providers concurrently and return the aggregated summary.
    /// Blocking filesystem walks run on `spawn_blocking` so the async
    /// runtime stays responsive. Each provider is bounded by
    /// [`PROVIDER_SCAN_TIMEOUT`] (G-REL-01); starting a scan clears any
    /// previous cancellation so a stale flag can never poison a new run.
    ///
    /// Resources at or under any `excluded` path are dropped (project
    /// ignore list). Exclusion only skips reporting — the safety validator
    /// still governs every deletion.
    pub async fn scan_all(
        &self,
        roots: Vec<PathBuf>,
        excluded: &[PathBuf],
        on_progress: Option<ProgressFn>,
    ) -> ScanSummary {
        self.cancelled.store(false, Ordering::SeqCst);
        let on_progress: Option<Arc<ProgressFn>> = on_progress.map(Arc::new);
        let mut handles = Vec::new();

        for (idx, provider) in self.providers.iter().enumerate() {
            let total = self.providers.len();
            let p = Arc::clone(provider);
            let roots = roots.clone();
            let sem = Arc::clone(&self.semaphore);
            let cancelled = Arc::clone(&self.cancelled);
            let cb = on_progress.clone();
            let timeout = self.timeout;
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire_owned().await.unwrap();
                if cancelled.load(Ordering::SeqCst) {
                    return (p.id().to_string(), Vec::new());
                }
                let roots = roots.clone();
                let pid = p.id().to_string();
                let scan = tokio::task::spawn_blocking(move || p.scan(&roots, &cancelled));
                let found = match tokio::time::timeout(timeout, scan).await {
                    Ok(Ok(found)) => found,
                    Ok(Err(e)) => {
                        tracing::warn!(provider = %pid, "scan task panicked: {e}; skipping");
                        Vec::new()
                    }
                    Err(_) => {
                        tracing::warn!(provider = %pid, "scan timed out after {timeout:?}; skipping");
                        Vec::new()
                    }
                };
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
        resources.retain(|r| !is_excluded(&r.path, excluded));

        aggregate(resources)
    }
}

/// True when `path` is the excluded dir itself or lives under it.
fn is_excluded(path: &std::path::Path, excluded: &[PathBuf]) -> bool {
    excluded.iter().any(|x| path == *x || path.starts_with(x))
}

fn short_prefix(provider_id: &str) -> &str {
    match provider_id {
        "node_modules" => "nm",
        "rust_target" => "rs",
        "go_cache" => "go",
        "docker" => "dk",
        "npm_cache" => "np",
        "python_cache" => "py",
        "cargo_registry" => "cg",
        "gradle_cache" => "gr",
        "xcode_data" => "xc",
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

#[cfg(test)]
mod tests {
    use super::*;
    use devclean_core::SafetyLevel;
    use std::path::Path;

    /// Stub provider that works in small cooperative slices so cancellation
    /// is observable, like the real filesystem providers.
    struct SlowProvider {
        slices: usize,
    }

    impl Provider for SlowProvider {
        fn id(&self) -> &'static str {
            "slow"
        }
        fn name(&self) -> &'static str {
            "Slow stub"
        }
        fn default_safety(&self) -> SafetyLevel {
            SafetyLevel::Safe
        }
        fn scan(&self, _roots: &[PathBuf], cancel: &AtomicBool) -> Vec<Resource> {
            let mut out = Vec::new();
            for i in 0..self.slices {
                if cancel.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
                out.push(Resource {
                    id: String::new(),
                    provider: self.id().to_string(),
                    name: format!("slow-{i}"),
                    path: Path::new("/tmp").to_path_buf(),
                    size_bytes: 1,
                    reclaimable_bytes: 1,
                    safety: SafetyLevel::Safe,
                    detail: None,
                });
            }
            out
        }
        fn cleanup_action(&self, path: &Path) -> devclean_providers::CleanupAction {
            devclean_providers::CleanupAction::RemoveDir(path.to_path_buf())
        }
    }

    fn manager_with(provider: Arc<dyn Provider>, timeout: Duration) -> ScanManager {
        ScanManager::with_config(vec![provider], timeout)
    }

    #[tokio::test]
    async fn cancel_settles_in_under_one_second() {
        let slow = Arc::new(SlowProvider { slices: 500 });
        let mgr = manager_with(slow, Duration::from_secs(60));
        let mgr = Arc::new(mgr);
        let mgr2 = Arc::clone(&mgr);
        let handle =
            tokio::spawn(
                async move { mgr2.scan_all(vec![PathBuf::from("/tmp")], &[], None).await },
            );
        // Let the scan start, then cancel and measure the settle time.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let t0 = std::time::Instant::now();
        mgr.cancel();
        let summary = handle.await.unwrap();
        assert!(
            t0.elapsed() < Duration::from_secs(1),
            "cancel must settle in <1s (G-SCAN-02), took {:?}",
            t0.elapsed()
        );
        // Partial results are fine; the point is it stopped early.
        assert!(summary.resources.len() < 500);
    }

    #[tokio::test]
    async fn provider_timeout_yields_empty_without_hanging() {
        // 500 slices × 20ms = ~10s of work, timeout 100ms.
        let slow = Arc::new(SlowProvider { slices: 500 });
        let mgr = manager_with(slow, Duration::from_millis(100));
        let t0 = std::time::Instant::now();
        let summary = mgr.scan_all(vec![PathBuf::from("/tmp")], &[], None).await;
        assert!(t0.elapsed() < Duration::from_secs(5));
        assert!(summary.resources.is_empty());
    }

    #[tokio::test]
    async fn new_scan_clears_previous_cancel() {
        let slow = Arc::new(SlowProvider { slices: 5 });
        let mgr = manager_with(slow, Duration::from_secs(60));
        mgr.cancel();
        // A fresh scan_all must not inherit the stale cancel flag.
        let summary = mgr.scan_all(vec![PathBuf::from("/tmp")], &[], None).await;
        assert_eq!(summary.resources.len(), 5);
    }

    #[tokio::test]
    async fn excluded_paths_are_dropped() {
        let slow = Arc::new(SlowProvider { slices: 3 });
        let mgr = manager_with(slow, Duration::from_secs(60));
        // The stub reports paths under /tmp: excluding /tmp drops all.
        let summary = mgr
            .scan_all(vec![PathBuf::from("/tmp")], &[PathBuf::from("/tmp")], None)
            .await;
        assert!(summary.resources.is_empty());
        let summary = mgr.scan_all(vec![PathBuf::from("/tmp")], &[], None).await;
        assert_eq!(summary.resources.len(), 3);
    }
}

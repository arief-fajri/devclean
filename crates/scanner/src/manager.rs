//! Scan orchestration: fan out to providers, aggregate, register ids.

use devclean_core::{
    stamp_blocked_reason, PolicyInputs, ProviderSummary, Resource, ScanSummary,
    PROVIDER_SCAN_TIMEOUT,
};
use devclean_providers::{all_providers, Provider, ProviderRegistry};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::sync::Semaphore;

/// Progress callback invoked per completed provider (throttle at the caller
/// to ~100-500ms for UI updates).
pub type ProgressFn = Box<dyn Fn(&str, usize, usize) + Send + Sync>;

/// Fired the moment a provider begins scanning, so the UI can show which
/// one is running rather than only which ones are already done (contract
/// v0.5.0). A no-op when not supplied.
pub type StartedFn = Box<dyn Fn(&str) + Send + Sync>;

/// Everything one fan-out needs. Bundled so the internal `run` keeps a
/// readable signature as scan options accumulate.
struct ScanRequest<'a> {
    /// Restrict to a single provider id; `None` runs every enabled one.
    only: Option<&'a str>,
    roots: Vec<PathBuf>,
    excluded: &'a [PathBuf],
    disabled: &'a [String],
    policy: PolicyInputs,
    on_progress: Option<ProgressFn>,
    on_started: Option<StartedFn>,
}

/// How many providers walk the filesystem at the same time by default
/// (G-SCAN-01). A scan is I/O bound, so raising this is not free: it costs
/// page cache and, on spinning disks, head thrash. `bench_scan` measures it.
pub const DEFAULT_SCAN_CONCURRENCY: usize = 4;

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
        Self::with_concurrency(providers, timeout, DEFAULT_SCAN_CONCURRENCY)
    }

    /// Same, with the fan-out width chosen by the caller. Used by the
    /// concurrency benchmark; the app itself always takes the default.
    pub fn with_concurrency(
        providers: Vec<Arc<dyn Provider>>,
        timeout: Duration,
        max_concurrent: usize,
    ) -> Self {
        Self {
            providers,
            // Bounded concurrency: at most `max_concurrent` providers scan at
            // once (G-SCAN-01). Zero would deadlock, so it is never allowed.
            semaphore: Arc::new(Semaphore::new(max_concurrent.max(1))),
            cancelled: Arc::new(AtomicBool::new(false)),
            timeout,
        }
    }

    /// The fan-out width this manager was built with.
    pub fn max_concurrent(&self) -> usize {
        self.semaphore.available_permits().max(1)
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Run all enabled providers concurrently and return the aggregated
    /// summary. Blocking filesystem walks run on `spawn_blocking` so the
    /// async runtime stays responsive. Each provider is bounded by
    /// [`PROVIDER_SCAN_TIMEOUT`] (G-REL-01); starting a scan clears any
    /// previous cancellation so a stale flag can never poison a new run.
    ///
    /// Providers listed in `disabled` are skipped before fan-out (they
    /// never occupy a semaphore slot and never appear in progress totals).
    /// Resources at or under any `excluded` path are dropped (project
    /// ignore list). Exclusion only skips reporting — the safety validator
    /// still governs every deletion.
    pub async fn scan_all(
        &self,
        roots: Vec<PathBuf>,
        excluded: &[PathBuf],
        disabled: &[String],
        policy: PolicyInputs,
        on_progress: Option<ProgressFn>,
        on_started: Option<StartedFn>,
    ) -> ScanSummary {
        self.run(ScanRequest {
            only: None,
            roots,
            excluded,
            disabled,
            policy,
            on_progress,
            on_started,
        })
        .await
    }

    /// Scan exactly one provider. The others are never walked, never occupy
    /// a semaphore slot, and never appear in progress totals — so refreshing
    /// a single card after a cleanup costs that provider's scan, not a full
    /// sweep. `progress` totals are `1/1`.
    pub async fn scan_one(
        &self,
        provider: &str,
        roots: Vec<PathBuf>,
        excluded: &[PathBuf],
        policy: PolicyInputs,
        on_progress: Option<ProgressFn>,
        on_started: Option<StartedFn>,
    ) -> ScanSummary {
        self.run(ScanRequest {
            only: Some(provider),
            roots,
            excluded,
            disabled: &[],
            policy,
            on_progress,
            on_started,
        })
        .await
    }

    /// Shared fan-out. `only` restricts the run to a single provider id;
    /// `None` runs every provider that is not in `disabled`.
    async fn run(&self, req: ScanRequest<'_>) -> ScanSummary {
        let ScanRequest {
            only,
            roots,
            excluded,
            disabled,
            policy,
            on_progress,
            on_started,
        } = req;
        self.cancelled.store(false, Ordering::SeqCst);
        let on_progress: Option<Arc<ProgressFn>> = on_progress.map(Arc::new);
        let on_started: Option<Arc<StartedFn>> = on_started.map(Arc::new);
        let mut handles = Vec::new();

        let active: Vec<&Arc<dyn Provider>> = self
            .providers
            .iter()
            .filter(|p| !disabled.contains(&p.id().to_string()))
            .filter(|p| only.is_none_or(|id| p.id() == id))
            .collect();
        let total = active.len();
        // Monotonic completion counter for THIS run. `scan_progress.done`
        // must count providers that FINISHED, never their static position
        // in the list — otherwise a fast provider at index 5 reports
        // `done=6` before provider 0 reports `done=1`, the bar runs
        // backwards and can read 9/9 with 4 still pending. Local (not a
        // struct field) so two concurrent runs never share it.
        let completed = Arc::new(AtomicUsize::new(0));
        for provider in &active {
            let p = Arc::clone(provider);
            let roots = roots.clone();
            let sem = Arc::clone(&self.semaphore);
            let cancelled = Arc::clone(&self.cancelled);
            let cb = on_progress.clone();
            let started = on_started.clone();
            let timeout = self.timeout;
            let completed = Arc::clone(&completed);
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire_owned().await.unwrap();
                if let Some(started) = &started {
                    started(p.id());
                }
                let report = |cb: &Option<Arc<ProgressFn>>, pid: &str| {
                    // Fires exactly once per provider, on every exit path
                    // (cancel, panic, timeout, success) so the UI bar can
                    // never stall below its true total.
                    let done = completed.fetch_add(1, Ordering::SeqCst) + 1;
                    if let Some(cb) = cb {
                        cb(pid, done, total);
                    }
                };
                if cancelled.load(Ordering::SeqCst) {
                    report(&cb, p.id());
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
                report(&cb, &pid);
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

        // Git-aware classification (Wave B). Subprocess work runs on the
        // blocking pool so the async runtime stays responsive (G-SCAN-04);
        // the resources are moved in and returned so the closure owns them.
        let git_providers = self.providers.clone();
        let git_cancel = Arc::clone(&self.cancelled);
        let (mut resources, protected) = tokio::task::spawn_blocking(move || {
            let by_id: HashMap<&str, &Arc<dyn Provider>> =
                git_providers.iter().map(|p| (p.id(), p)).collect();
            let protected =
                crate::git_classify::classify_repositories(&mut resources, &by_id, &git_cancel);
            (resources, protected)
        })
        .await
        .unwrap_or_default();

        // Stamp WHY anything is held back, here at scan time, so the UI can
        // explain a resource before the user tries to select it. The engine
        // re-derives the same verdict from the same function, so the
        // explanation and the enforcement cannot drift apart (I11).
        let by_id: HashMap<&str, &Arc<dyn Provider>> =
            self.providers.iter().map(|p| (p.id(), p)).collect();
        for r in &mut resources {
            let provider = by_id.get(r.provider.as_str());
            stamp_blocked_reason(
                r,
                PolicyInputs {
                    is_user_data: provider.is_some_and(|p| p.is_user_data()),
                    required_docker_scope: provider.and_then(|p| p.required_docker_scope()),
                    protected: protected.contains(&r.id),
                    ..policy
                },
            );
        }

        aggregate(resources)
    }
}

/// True when `path` is the excluded dir itself or lives under it.
fn is_excluded(path: &std::path::Path, excluded: &[PathBuf]) -> bool {
    excluded.iter().any(|x| path == *x || path.starts_with(x))
}

/// Two-letter prefix per provider for the opaque id (`nm:0000`). Every
/// registered provider needs its OWN prefix: counters are per-provider, so
/// a shared prefix silently hands two providers the same id, and the
/// id-keyed state map then resolves one of them to the other's path.
/// `every_provider_prefix_is_unique` fails until a new provider is added
/// here.
fn short_prefix(provider_id: &str) -> &str {
    match provider_id {
        "node_modules" => "nm",
        "rust_target" => "rs",
        "go_cache" => "go",
        "docker" => "dk",
        "docker_pruned" => "dp",
        "npm_cache" => "np",
        "python_cache" => "py",
        "cargo_registry" => "cg",
        "gradle_cache" => "gr",
        "xcode_data" => "xc",
        "global_cache" => "gl",
        "opencode_fs" => "of",
        "claude" => "cl",
        "opencode_db" => "od",
        // Stubs and future providers share one prefix on purpose: the
        // uniqueness test turns this into a failure instead of two
        // providers quietly sharing `xx:0000`.
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
        blocked_bytes: resources
            .iter()
            .filter(|r| r.blocked_reason.is_some())
            .map(|r| r.reclaimable_bytes)
            .sum(),
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
    /// is observable, like the real filesystem providers. `tag` makes each
    /// instance distinguishable so multi-provider tests can disable one.
    struct SlowProvider {
        tag: &'static str,
        slices: usize,
    }

    impl Provider for SlowProvider {
        fn id(&self) -> &'static str {
            self.tag
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
                    name: format!("{}-{i}", self.tag),
                    path: Path::new("/tmp").to_path_buf(),
                    size_bytes: 1,
                    reclaimable_bytes: 1,
                    apparent_bytes: None,
                    safety: SafetyLevel::Safe,
                    detail: None,
                    impact: None,
                    age_days: None,
                    activity: None,
                    last_commit_unix: None,
                    blocked_reason: None,
                    blocked_detail: None,
                });
            }
            out
        }
        fn measure(&self, _path: &Path) -> Option<u64> {
            None
        }
        fn cleanup_action(&self, path: &Path) -> devclean_providers::CleanupAction {
            devclean_providers::CleanupAction::RemoveDir(path.to_path_buf())
        }
    }

    /// Nothing blocked: tests exercise discovery, not policy.
    fn permissive() -> devclean_core::PolicyInputs {
        devclean_core::PolicyInputs {
            is_user_data: false,
            required_docker_scope: None,
            retention_days: None,
            docker_prune_scope: devclean_core::DockerPruneScope::BuildCache,
            allow_privileged_operations: true,
            ai_cleanup_enabled: true,
            protected: false,
        }
    }

    fn manager_with(provider: Arc<dyn Provider>, timeout: Duration) -> ScanManager {
        ScanManager::with_config(vec![provider], timeout)
    }

    #[tokio::test]
    async fn cancel_settles_in_under_one_second() {
        // 100 slices × 20ms = ~2s of work, which is still running when the
        // cancel arrives 100ms in. 500 slices made this test cost 10s of wall
        // clock for the same assertion.
        let slow = Arc::new(SlowProvider {
            tag: "slow",
            slices: 100,
        });
        let mgr = manager_with(slow, Duration::from_secs(60));
        let mgr = Arc::new(mgr);
        let mgr2 = Arc::clone(&mgr);
        let handle = tokio::spawn(async move {
            mgr2.scan_all(
                vec![PathBuf::from("/tmp")],
                &[],
                &[],
                permissive(),
                None,
                None,
            )
            .await
        });
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
        assert!(summary.resources.len() < 100);
    }

    #[tokio::test]
    async fn provider_timeout_yields_empty_without_hanging() {
        // 100 slices × 20ms = ~2s of work, timeout 100ms: the timeout must
        // fire long before the work would finish.
        let slow = Arc::new(SlowProvider {
            tag: "slow",
            slices: 100,
        });
        let mgr = manager_with(slow, Duration::from_millis(100));
        let t0 = std::time::Instant::now();
        let summary = mgr
            .scan_all(
                vec![PathBuf::from("/tmp")],
                &[],
                &[],
                permissive(),
                None,
                None,
            )
            .await;
        assert!(t0.elapsed() < Duration::from_secs(5));
        assert!(summary.resources.is_empty());
    }

    #[tokio::test]
    async fn new_scan_clears_previous_cancel() {
        let slow = Arc::new(SlowProvider {
            tag: "slow",
            slices: 5,
        });
        let mgr = manager_with(slow, Duration::from_secs(60));
        mgr.cancel();
        // A fresh scan_all must not inherit the stale cancel flag.
        let summary = mgr
            .scan_all(
                vec![PathBuf::from("/tmp")],
                &[],
                &[],
                permissive(),
                None,
                None,
            )
            .await;
        assert_eq!(summary.resources.len(), 5);
    }

    #[tokio::test]
    async fn excluded_paths_are_dropped() {
        let slow = Arc::new(SlowProvider {
            tag: "slow",
            slices: 3,
        });
        let mgr = manager_with(slow, Duration::from_secs(60));
        // The stub reports paths under /tmp: excluding /tmp drops all.
        let summary = mgr
            .scan_all(
                vec![PathBuf::from("/tmp")],
                &[PathBuf::from("/tmp")],
                &[],
                permissive(),
                None,
                None,
            )
            .await;
        assert!(summary.resources.is_empty());
        let summary = mgr
            .scan_all(
                vec![PathBuf::from("/tmp")],
                &[],
                &[],
                permissive(),
                None,
                None,
            )
            .await;
        assert_eq!(summary.resources.len(), 3);
    }

    #[tokio::test]
    async fn disabled_providers_are_skipped_entirely() {
        let fast = Arc::new(SlowProvider {
            tag: "fast",
            slices: 3,
        });
        // The expensive stub is disabled, so its slice count only matters if
        // the filter is wrong and it runs anyway — which the elapsed check
        // below catches. 100 is enough to be unmistakable without costing 10s.
        let slow = Arc::new(SlowProvider {
            tag: "slow",
            slices: 100,
        });
        let mgr = ScanManager::with_config(vec![fast, slow], Duration::from_secs(60));
        // Disabling the expensive stub must both drop its rows and make
        // the run finish immediately (it never occupies a semaphore slot).
        let t0 = std::time::Instant::now();
        let summary = mgr
            .scan_all(
                vec![PathBuf::from("/tmp")],
                &[],
                &["slow".to_string()],
                permissive(),
                None,
                None,
            )
            .await;
        assert!(
            t0.elapsed() < Duration::from_secs(5),
            "disabled scan must not block"
        );
        assert_eq!(
            summary.resources.len(),
            3,
            "only the enabled provider contributes"
        );
        assert!(summary.providers.iter().all(|p| p.provider != "slow"));
    }

    #[tokio::test]
    async fn disabling_every_provider_yields_an_empty_summary() {
        let slow = Arc::new(SlowProvider {
            tag: "slow",
            slices: 3,
        });
        let mgr = manager_with(slow, Duration::from_secs(60));
        let summary = mgr
            .scan_all(
                vec![PathBuf::from("/tmp")],
                &[],
                &["slow".to_string()],
                permissive(),
                None,
                None,
            )
            .await;
        assert!(summary.resources.is_empty());
        assert!(summary.providers.is_empty());
    }

    /// Stub whose scan time DECREASES with its index, so a fast provider
    /// finishes before a slow one listed earlier. This is the shape that
    /// made the old `idx + 1` counter report out-of-order progress.
    struct ReverseSpeedProvider {
        tag: &'static str,
        slices: usize,
        delay_ms: u64,
    }

    impl Provider for ReverseSpeedProvider {
        fn id(&self) -> &'static str {
            self.tag
        }
        fn name(&self) -> &'static str {
            "Reverse-speed stub"
        }
        fn default_safety(&self) -> SafetyLevel {
            SafetyLevel::Safe
        }
        fn scan(&self, _roots: &[PathBuf], _cancel: &AtomicBool) -> Vec<Resource> {
            std::thread::sleep(Duration::from_millis(self.delay_ms));
            (0..self.slices)
                .map(|i| Resource {
                    id: String::new(),
                    provider: self.id().to_string(),
                    name: format!("{}-{i}", self.tag),
                    path: Path::new("/tmp").to_path_buf(),
                    size_bytes: 1,
                    reclaimable_bytes: 1,
                    apparent_bytes: None,
                    safety: SafetyLevel::Safe,
                    detail: None,
                    impact: None,
                    age_days: None,
                    activity: None,
                    last_commit_unix: None,
                    blocked_reason: None,
                    blocked_detail: None,
                })
                .collect()
        }
        fn measure(&self, _path: &Path) -> Option<u64> {
            None
        }
        fn cleanup_action(&self, path: &Path) -> devclean_providers::CleanupAction {
            devclean_providers::CleanupAction::RemoveDir(path.to_path_buf())
        }
    }

    #[tokio::test]
    async fn progress_done_counts_completions_and_is_monotonic() {
        // Listed slowest-first; the last one finishes first. The old
        // `idx + 1` counter would have emitted done=5 before done=1.
        let providers: Vec<Arc<dyn Provider>> = vec![
            Arc::new(ReverseSpeedProvider {
                tag: "p0",
                slices: 1,
                delay_ms: 120,
            }),
            Arc::new(ReverseSpeedProvider {
                tag: "p1",
                slices: 1,
                delay_ms: 90,
            }),
            Arc::new(ReverseSpeedProvider {
                tag: "p2",
                slices: 1,
                delay_ms: 60,
            }),
            Arc::new(ReverseSpeedProvider {
                tag: "p3",
                slices: 1,
                delay_ms: 30,
            }),
            Arc::new(ReverseSpeedProvider {
                tag: "p4",
                slices: 1,
                delay_ms: 5,
            }),
        ];
        let mgr = ScanManager::with_config(providers, Duration::from_secs(60));

        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = std::sync::Arc::clone(&seen);
        let cb: ProgressFn = Box::new(move |_p, done, total| {
            sink.lock().unwrap().push((done, total));
        });
        mgr.scan_all(
            vec![PathBuf::from("/tmp")],
            &[],
            &[],
            permissive(),
            Some(cb),
            None,
        )
        .await;

        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 5, "one callback per provider, got {seen:?}");
        assert!(
            seen.iter().all(|(_, total)| *total == 5),
            "total must stay constant, got {seen:?}"
        );
        let dones: Vec<usize> = seen.iter().map(|(d, _)| *d).collect();
        assert_eq!(
            dones,
            vec![1, 2, 3, 4, 5],
            "done must count completions in order, got {dones:?}"
        );
    }

    /// v0.5.0: the UI needs to show WHICH provider is running, not only
    /// which ones finished. One `started` event per provider, and never
    /// for a provider that was disabled.
    #[tokio::test]
    async fn provider_started_fires_once_per_enabled_provider() {
        let providers: Vec<Arc<dyn Provider>> = vec![
            Arc::new(SlowProvider {
                tag: "s0",
                slices: 1,
            }),
            Arc::new(SlowProvider {
                tag: "s1",
                slices: 1,
            }),
            Arc::new(SlowProvider {
                tag: "s2",
                slices: 1,
            }),
        ];
        let mgr = ScanManager::with_config(providers, Duration::from_secs(60));

        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = std::sync::Arc::clone(&seen);
        let cb: StartedFn = Box::new(move |p| sink.lock().unwrap().push(p.to_string()));
        mgr.scan_all(
            vec![PathBuf::from("/tmp")],
            &[],
            &["s1".to_string()],
            permissive(),
            None,
            Some(cb),
        )
        .await;

        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 2, "only the enabled providers: {seen:?}");
        assert!(seen.contains(&"s0".to_string()));
        assert!(seen.contains(&"s2".to_string()));
        assert!(
            !seen.contains(&"s1".to_string()),
            "a disabled provider is silent"
        );
    }

    #[tokio::test]
    async fn cancelled_providers_still_emit_progress() {
        // A cancel before the fan-out runs: every provider takes the early
        // exit, and each must still tick the bar or it stalls forever.
        let providers: Vec<Arc<dyn Provider>> = (0..3)
            .map(|i| {
                Arc::new(SlowProvider {
                    tag: if i == 0 { "s0" } else { "s1" },
                    slices: 50,
                }) as Arc<dyn Provider>
            })
            .collect();
        let mgr = Arc::new(ScanManager::with_config(providers, Duration::from_secs(60)));
        mgr.cancel();
        let runner = Arc::clone(&mgr);

        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = std::sync::Arc::clone(&seen);
        let cb: ProgressFn = Box::new(move |_p, done, total| {
            sink.lock().unwrap().push((done, total));
        });
        runner
            .scan_all(
                vec![PathBuf::from("/tmp")],
                &[],
                &[],
                permissive(),
                Some(cb),
                None,
            )
            .await;

        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 3, "cancel must not swallow callbacks: {seen:?}");
        assert_eq!(
            seen.iter().map(|(d, _)| *d).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn every_provider_prefix_is_unique() {
        // Counters are per-provider, so two providers sharing a prefix
        // hand out the SAME id. The id-keyed state map then resolves one
        // provider's row to the other provider's path — a cleanup started
        // from what the user saw would delete something else. Every
        // registered provider must therefore own its prefix, and no
        // registered provider may sit on the stub fallback.
        let providers = all_providers();
        let mut seen: HashMap<&str, &str> = HashMap::new();
        for p in &providers {
            let prefix = short_prefix(p.id());
            assert_ne!(
                prefix,
                "xx",
                "provider {} has no prefix in short_prefix",
                p.id()
            );
            if let Some(previous) = seen.insert(prefix, p.id()) {
                panic!("prefix {prefix} is shared by {previous} and {}", p.id());
            }
        }
        assert_eq!(seen.len(), providers.len());
    }

    #[tokio::test]
    async fn providers_that_once_shared_a_prefix_never_share_an_id() {
        // Five real provider ids all used to map to `xx`, so each of their
        // first resources was `xx:0000`. Scanned together, the state map
        // kept only the last one and the UI's selection resolved ids across
        // providers. Every id must be unique within one scan.
        let ids = [
            "global_cache",
            "docker_pruned",
            "opencode_fs",
            "claude",
            "opencode_db",
        ];
        let providers: Vec<Arc<dyn Provider>> = ids
            .iter()
            .map(|id| Arc::new(SlowProvider { tag: id, slices: 2 }) as Arc<dyn Provider>)
            .collect();
        let mgr = ScanManager::with_config(providers, Duration::from_secs(60));
        let summary = mgr
            .scan_all(
                vec![PathBuf::from("/tmp")],
                &[],
                &[],
                permissive(),
                None,
                None,
            )
            .await;
        let unique: std::collections::HashSet<&str> =
            summary.resources.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            unique.len(),
            summary.resources.len(),
            "duplicate ids in one scan: {:?}",
            summary.resources.iter().map(|r| &r.id).collect::<Vec<_>>()
        );
        assert_eq!(summary.resources.len(), 10, "2 resources × 5 providers");
    }
}

#[cfg(test)]
mod concurrency_tests {
    use super::*;

    /// The fan-out width is a measured decision, not a preference: changing it
    /// silently would trade disk behaviour nobody asked for. This pins the
    /// default so a change has to be deliberate (and benchmarked).
    #[test]
    fn the_default_fan_out_is_four() {
        assert_eq!(DEFAULT_SCAN_CONCURRENCY, 4);
        assert_eq!(ScanManager::default().max_concurrent(), 4);
    }

    #[test]
    fn a_zero_width_would_deadlock_and_is_clamped_away() {
        let m = ScanManager::with_concurrency(all_providers(), Duration::from_secs(1), 0);
        assert!(
            m.max_concurrent() >= 1,
            "a semaphore of 0 would never let a provider scan"
        );
    }

    #[test]
    fn the_width_is_whatever_the_caller_asked_for() {
        let m = ScanManager::with_concurrency(all_providers(), Duration::from_secs(1), 8);
        assert_eq!(m.max_concurrent(), 8);
    }
}

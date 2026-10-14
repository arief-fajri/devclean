//! Tauri commands — the ONLY bridge between the Svelte UI and the Rust
//! core. This layer is intentionally thin: no filesystem logic lives here
//! beyond wiring crates together. See `docs/reference/tauri-contract.md`.

use crate::disk;
use crate::events::*;
use crate::metrics::MemSample;
use crate::state::AppState;
use devclean_core::{
    format_bytes, CleanupPreview, CleanupResult, CleanupRun, DiskFree, HistoryEntry, ProviderInfo,
    Resource, SafetyValidator, ScanRootCandidate, ScanSummary, Settings, PREVIEW_TIMEOUT,
};
use devclean_providers::{all_providers, Provider};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Unique id for one cleanup run. Groups every resource entry written by a
/// single `cleanup` call so the history UI can show runs, not a flat sea of
/// per-resource rows. Dependency-free: millis + monotonic counter.
fn new_run_id() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    format!(
        "{}-{}",
        millis,
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

fn providers_map() -> HashMap<String, Arc<dyn Provider>> {
    all_providers()
        .into_iter()
        .map(|p| (p.id().to_string(), p))
        .collect()
}

/// Scan every provider concurrently. Emits `scan_started` → N×
/// `scan_progress` → `scan_completed`. The returned summary is also cached
/// in state for `preview_cleanup` / `cleanup`.
#[tauri::command]
pub async fn scan_all(app: AppHandle, state: State<'_, AppState>) -> Result<ScanSummary, String> {
    run_scan(&app, &state).await
}

/// Shared scan implementation, callable from the `scan_all` command and
/// from the tray menu (M3) without going through the command layer.
pub async fn run_scan(app: &AppHandle, state: &AppState) -> Result<ScanSummary, String> {
    let mem_before = MemSample::capture();
    let (roots, excluded, disabled, policy) = {
        let s = state.settings.lock().unwrap();
        (
            s.scan_roots.clone(),
            s.excluded_paths.clone(),
            s.disabled_providers.clone(),
            devclean_core::PolicyInputs {
                // Per-provider flags are filled in by the scanner; these are
                // the user's settings.
                is_user_data: false,
                required_docker_scope: None,
                retention_days: s.retention_days,
                docker_prune_scope: s.docker_prune_scope,
                allow_privileged_operations: s.allow_privileged_operations,
                ai_cleanup_enabled: s.ai_cleanup_enabled,
            },
        )
    };
    app.emit(SCAN_STARTED, ()).map_err(|e| e.to_string())?;

    let app2 = app.clone();
    let progress: devclean_scanner::manager::ProgressFn = Box::new(move |provider, done, total| {
        let _ = app2.emit(
            SCAN_PROGRESS,
            ScanProgressPayload {
                provider: provider.to_string(),
                done,
                total,
            },
        );
    });

    let app3 = app.clone();
    let started: devclean_scanner::manager::StartedFn = Box::new(move |provider| {
        let _ = app3.emit(SCAN_PROVIDER_STARTED, provider);
    });

    let summary = state
        .scan
        .scan_all(
            roots,
            &excluded,
            &disabled,
            policy,
            Some(progress),
            Some(started),
        )
        .await;
    state.store_resources(summary.resources.clone());
    // One free-space probe per scan, never per menu open (G-REL-01): the menu
    // bar reads this instead of measuring while the user is looking at it.
    state.store_disk_free(disk_free());
    // Best-effort instant paint for the next startup (M3 scan cache).
    if let Err(e) = devclean_scanner::save_cached_summary(
        &state.scan_cache_path,
        &state.settings.lock().unwrap().scan_roots,
        &summary,
    ) {
        tracing::warn!("could not save scan cache: {e}");
    }
    app.emit(SCAN_COMPLETED, &summary)
        .map_err(|e| e.to_string())?;
    tracing::info!("{}", MemSample::capture().delta_line(&mem_before, "scan"));
    crate::tray::refresh(app, state);
    notify_if_background(
        app,
        "DevClean scan complete",
        &format!(
            "{} reclaimable across {} projects",
            format_bytes(summary.reclaimable_bytes),
            summary.resources.len()
        ),
    );
    Ok(summary)
}

/// Restrained notifications (never adware): only scan/cleanup completion,
/// and only when the main window is not focused.
fn notify_if_background(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    let focused = app
        .get_webview_window("main")
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(true);
    if focused {
        return;
    }
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::warn!("could not show notification: {e}");
    }
}

/// Last scan, instantly, if the cache matches the current scan roots.
/// The UI shows this first, then runs a background rescan.
#[tauri::command]
pub fn get_cached_scan(state: State<'_, AppState>) -> Result<Option<ScanSummary>, String> {
    cached_scan(&state)
}

/// Shared body of [`get_cached_scan`], separable so it can be tested without a
/// Tauri `State` (which only exists inside a running app).
fn cached_scan(state: &AppState) -> Result<Option<ScanSummary>, String> {
    let roots = state.settings.lock().unwrap().scan_roots.clone();
    let cached = devclean_scanner::load_cached_summary(&state.scan_cache_path, &roots);
    if let Some(summary) = &cached {
        // Seed the id registry from the cache too. Without this the window can
        // show resources (from the cache) while the backend still knows none of
        // them, and a cleanup started in that window re-measures to an empty
        // preview — "Clean 0 B from 0 resources?". A full scan replaces all of
        // this anyway; this only closes the window between launch and scan.
        state.store_resources(summary.resources.clone());
    }
    Ok(cached)
}

/// Re-scan a single provider and merge the result into the stored state.
///
/// Since v0.6.0 this walks **only** that provider (it used to run a full
/// scan and filter the result, which made a per-card refresh as expensive
/// as a whole sweep). The other providers' resources are kept in state so
/// `preview_cleanup` / `cleanup` keep resolving their ids.
#[tauri::command]
pub async fn scan_provider(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: String,
) -> Result<ScanSummary, String> {
    let (roots, excluded, policy) = {
        let s = state.settings.lock().unwrap();
        (
            s.scan_roots.clone(),
            s.excluded_paths.clone(),
            devclean_core::PolicyInputs {
                is_user_data: false,
                required_docker_scope: None,
                retention_days: s.retention_days,
                docker_prune_scope: s.docker_prune_scope,
                allow_privileged_operations: s.allow_privileged_operations,
                ai_cleanup_enabled: s.ai_cleanup_enabled,
            },
        )
    };
    app.emit(SCAN_STARTED, ()).map_err(|e| e.to_string())?;

    let progress_app = app.clone();
    let progress: devclean_scanner::manager::ProgressFn = Box::new(move |provider, done, total| {
        let _ = progress_app.emit(
            SCAN_PROGRESS,
            ScanProgressPayload {
                provider: provider.to_string(),
                done,
                total,
            },
        );
    });
    let started_app = app.clone();
    let started: devclean_scanner::manager::StartedFn = Box::new(move |provider| {
        let _ = started_app.emit(SCAN_PROVIDER_STARTED, provider);
    });

    let one = state
        .scan
        .scan_one(
            &provider,
            roots,
            &excluded,
            policy,
            Some(progress),
            Some(started),
        )
        .await;

    // Merge into the existing registry instead of replacing it: a targeted
    // scan must not drop the other providers' id→path entries, or a
    // preview/cleanup for them would fail to resolve.
    {
        let mut map = state.resources.lock().unwrap();
        map.retain(|_, r| r.provider != provider);
        for r in &one.resources {
            map.insert(r.id.clone(), r.clone());
        }
    }

    app.emit(SCAN_COMPLETED, &one).map_err(|e| e.to_string())?;
    Ok(one)
}

/// Details for one resource id (drill-down view).
#[tauri::command]
pub fn get_resource_details(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<Resource>, String> {
    Ok(state.resources.lock().unwrap().get(&id).cloned())
}

/// Re-measure the selected ids NOW and return what would be cleaned.
/// The UI must show this in the confirmation dialog (G-UI-01).
/// Async + `spawn_blocking`: re-measuring walks the filesystem and must
/// never freeze the UI before the dialog appears.
#[tauri::command]
pub async fn preview_cleanup(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<CleanupPreview, String> {
    let started = std::time::Instant::now();
    let before = MemSample::capture();
    let requested = ids.len();
    let engine = state.cleanup.clone();
    let resources = state.snapshot_resources();
    let result = bounded_preview(engine, ids, resources, PREVIEW_TIMEOUT).await;

    match &result {
        Ok(preview) => tracing::info!(
            "preview_cleanup: {requested} ids -> {} resolved in {:?} · {}",
            preview.items.len(),
            started.elapsed(),
            MemSample::capture().delta_line(&before, "preview")
        ),
        Err(e) => tracing::warn!(
            "preview_cleanup: {requested} ids failed after {:?}: {e}",
            started.elapsed()
        ),
    }
    result
}

/// The re-measure, under a deadline (G-REL-01).
///
/// `preview_cleanup` walks every selected path; without the bound a large
/// selection could leave the dialog waiting forever with nothing to show the
/// user.
async fn bounded_preview(
    engine: devclean_cleanup::CleanupEngine,
    ids: Vec<String>,
    resources: HashMap<String, Resource>,
    limit: std::time::Duration,
) -> Result<CleanupPreview, String> {
    let work =
        tokio::task::spawn_blocking(move || engine.preview(&ids, &resources, &providers_map()));
    bounded(work, limit).await
}

/// Run `work` under a deadline, turning an overrun into an explanation.
///
/// Split out from [`bounded_preview`] so the deadline itself can be tested
/// with a deliberately slow future — testing it through a real preview would
/// be a race against the machine's speed.
async fn bounded<F, T, E>(work: F, limit: std::time::Duration) -> Result<T, String>
where
    F: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    match tokio::time::timeout(limit, work).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(e)) => Err(format!("preview failed to complete: {e}")),
        Err(_) => Err(format!(
            "Re-measuring took longer than {}s — try selecting fewer items.",
            limit.as_secs()
        )),
    }
}

/// Execute cleanup behind the safety validator. Emits `cleanup_started` →
/// N×`cleanup_progress` (one per finished item) → `cleanup_completed`.
/// Each resource reports its own status so partial failure is transparent
/// (G-REL-02).
///
/// ## Gates enforced HERE, not in the UI (v0.5.0)
///
/// 1. `preview_token` — when `require_confirmation` is on, the caller must
///    present a token issued by `preview_cleanup` for exactly this id set,
///    still unexpired (G-UI-01). Previously the backend only logged
///    `tracing::debug!` and trusted the frontend to call preview first.
/// 2. `allow_privileged_operations` + `docker_prune_scope` — anything past
///    build-cache pruning is refused unless the user unlocked it (G-SAFE-03).
/// 3. Retention — resources the age policy keeps are refused even if their
///    ids are sent directly (I9).
///
/// Async + `spawn_blocking`: deletion + re-measurement walk the
/// filesystem and previously froze the UI for the whole run (sync
/// commands execute on the Tauri main thread).
#[tauri::command]
pub async fn cleanup(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
    preview_token: Option<String>,
) -> Result<Vec<CleanupResult>, String> {
    let mem_before = MemSample::capture();
    let settings = state.settings.lock().unwrap().clone();

    // Gate 1 — backend-enforced confirmation (G-UI-01).
    devclean_core::authorize(
        settings.require_confirmation,
        preview_token.as_deref(),
        &ids,
    )
    .map_err(|e| e.to_string())?;

    app.emit(CLEANUP_STARTED, &ids).map_err(|e| e.to_string())?;

    // Everything the blocking task needs is owned up front so the async
    // handler never holds a lock across the filesystem work.
    let engine = state.cleanup.clone();
    let resources = state.snapshot_resources();
    let roots = settings.scan_roots.clone();
    let privileged = settings.allow_privileged_operations;
    let scope = settings.docker_prune_scope;
    let delete_mode = settings.delete_mode;
    let ai_cleanup_enabled = settings.ai_cleanup_enabled;
    let retention_days = settings.retention_days;
    let progress_app = app.clone();
    let total_ids = ids.len();
    let results = tokio::task::spawn_blocking(move || {
        // Global cache dirs declared by providers (e.g. cargo registry)
        // widen the roots check only; protected-path rules still apply (I7).
        let providers = providers_map();
        let extra_roots: Vec<std::path::PathBuf> = providers
            .values()
            .flat_map(|p| p.extra_allowed_roots())
            .collect();
        let validator = SafetyValidator::new_with_extra_roots(roots, extra_roots);
        let lookup = |id: &str| resources.get(id).map(|r| r.path.clone());
        let on_progress: devclean_cleanup::CleanupProgressFn =
            Box::new(move |done, total, _result| {
                let _ = progress_app.emit(CLEANUP_PROGRESS, CleanupProgressPayload { done, total });
            });
        engine.execute_gated(
            &ids,
            &resources,
            &providers,
            &validator,
            lookup,
            Some(on_progress),
            &devclean_cleanup::CleanupPolicy {
                allow_privileged_operations: privileged,
                docker_prune_scope: scope,
                delete_mode,
                // The app always uses the platform Trash (~/.Trash).
                trash_dir: None,
                ai_cleanup_enabled,
                retention_days,
            },
        )
    })
    .await
    .map_err(|e| e.to_string())?;

    // Durable per-resource history (M3, JSONL). Best-effort: a failed
    // append is logged, never fatal to the cleanup itself.
    {
        let finished = now_unix();
        let run_id = new_run_id();
        for r in &results {
            let src = state.resources.lock().unwrap().get(&r.resource_id).cloned();
            let entry = HistoryEntry {
                run_id: run_id.clone(),
                finished_at_unix: finished,
                resource_id: r.resource_id.clone(),
                provider: src.as_ref().map(|s| s.provider.clone()).unwrap_or_default(),
                name: src.as_ref().map(|s| s.name.clone()).unwrap_or_default(),
                requested_bytes: r.requested_bytes,
                freed_bytes: r.freed_bytes,
                moved_to_trash: r.moved_to_trash,
                sizing: devclean_core::Sizing::Allocated,
                status: r.status,
                error: r.error.clone(),
            };
            if let Err(e) = devclean_cleanup::append_history_entry(&state.history_path, &entry) {
                tracing::warn!("could not append cleanup history: {e}");
                break;
            }
        }
    }

    // The child's peak is the interesting half here: `docker builder prune`
    // and friends are where the memory actually goes, not this process.
    tracing::info!(
        "{}",
        MemSample::capture().delta_line(&mem_before, "cleanup")
    );

    // Drop cleaned resources from state so the dashboard refreshes.
    {
        let mut map = state.resources.lock().unwrap();
        for r in &results {
            if matches!(
                r.status,
                devclean_core::CleanupStatus::Success | devclean_core::CleanupStatus::Partial
            ) {
                map.remove(&r.resource_id);
            }
        }
    }

    // Cleanup is the one event that moves free space. Re-measure here so the
    // dashboard card and the menu bar agree with the disk again, instead of
    // both quoting the number from before the deletion (contract v0.7.3).
    let freed_now = disk_free();
    state.store_disk_free(freed_now);
    crate::tray::refresh(&app, &state);

    // Progress already streamed per item; this final tick keeps the UI in
    // sync when ids were skipped (unknown ids report nothing).
    app.emit(
        CLEANUP_PROGRESS,
        CleanupProgressPayload {
            done: results.len(),
            total: total_ids,
        },
    )
    .map_err(|e| e.to_string())?;
    app.emit(CLEANUP_COMPLETED, &results)
        .map_err(|e| e.to_string())?;
    let freed: u64 = results.iter().map(|r| r.freed_bytes).sum();
    notify_if_background(
        &app,
        "DevClean cleanup complete",
        &format!("{} freed", format_bytes(freed)),
    );
    Ok(results)
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) -> Result<(), String> {
    state.scan.cancel();
    Ok(())
}

#[tauri::command]
pub fn cancel_cleanup(state: State<'_, AppState>) -> Result<(), String> {
    state.cleanup.cancel();
    Ok(())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.settings.lock().unwrap().clone())
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    // Guardrail G-SAFE-03: privileged operations stay off unless the user
    // explicitly opts in — which this assignment already represents.
    *state.settings.lock().unwrap() = settings.clone();
    state.persist_settings(&settings);
    // Theme changes flow through here, so re-paint the title bar to match.
    crate::state::apply_window_background(&app, settings.theme);
    Ok(settings)
}

/// Every registered provider with its current enabled state. The source of
/// truth for the Settings UI — the frontend never hardcodes the list, so a
/// provider with zero scan results is still discoverable.
#[tauri::command]
pub fn get_providers(state: State<'_, AppState>) -> Result<Vec<ProviderInfo>, String> {
    let disabled = state.settings.lock().unwrap().disabled_providers.clone();
    Ok(all_providers()
        .iter()
        .map(|p| ProviderInfo {
            id: p.id().to_string(),
            name: p.name().to_string(),
            enabled: !disabled.contains(&p.id().to_string()),
        })
        .collect())
}

/// Newest cleanup runs, newest first, with their per-resource entries
/// attached (contract v0.6.0). `limit` counts **runs**, not entries, so one
/// large cleanup cannot hide the runs before it. Defaults to 20, capped at
/// 200. Reads the tail of the log, so cost tracks `limit`, not log size.
#[tauri::command]
pub fn get_history_runs(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<CleanupRun>, String> {
    let limit = limit.unwrap_or(20).clamp(1, 200) as usize;
    Ok(devclean_cleanup::read_history_runs(
        &state.history_path,
        limit,
    ))
}

/// Newest cleanup history entries, oldest first (M3). `limit` defaults
/// to 100 and is capped at 1000.
///
/// Superseded by `get_history_runs` for any UI: this entry-level limit lets
/// a single large cleanup push every earlier run out of view. Kept for
/// contract stability.
#[tauri::command]
pub fn get_history(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<HistoryEntry>, String> {
    let limit = limit.unwrap_or(100).clamp(1, 1000) as usize;
    Ok(devclean_cleanup::read_history(&state.history_path, limit))
}

/// Candidate scan roots discovered under `$HOME`, largest cache first.
/// Backs the Settings "Detect folders" button: a machine whose work lives
/// in `~/VOLANTIS` must never have to have that path typed by hand.
/// Read-only and bounded — it never walks into a cache marker.
#[tauri::command]
pub async fn detect_scan_roots(
    state: State<'_, AppState>,
) -> Result<Vec<ScanRootCandidate>, String> {
    let configured = state.settings.lock().unwrap().scan_roots.clone();
    tokio::task::spawn_blocking(move || devclean_core::detect_scan_roots(&configured))
        .await
        .map_err(|e| e.to_string())
}

/// Free and total bytes of the volume holding `$HOME`, plus that volume's mount
/// name. Lets the UI say "not enough space to back up the 13 GB database"
/// BEFORE the user confirms a destructive SQLite cleanup.
///
/// Measured with `statvfs`, not a subprocess: this is awaited while the window
/// loads its settings (contract v0.7.3).
#[tauri::command]
pub fn get_disk_free() -> Result<DiskFree, String> {
    Ok(disk_free())
}

/// The reported volume, or `known: false` — never a silent zero.
fn disk_free() -> DiskFree {
    disk::report()
}

#[cfg(test)]
mod preview_deadline_tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn work_that_overruns_its_budget_fails_instead_of_hanging() {
        let slow = async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            Ok::<u8, String>(1)
        };
        let err = bounded(slow, Duration::from_millis(20))
            .await
            .expect_err("an overrun must not succeed");
        assert!(err.contains("longer than"), "unhelpful message: {err}");
        assert!(err.contains("fewer items"), "should say what to do: {err}");
    }

    #[tokio::test]
    async fn work_that_finishes_in_time_is_returned_untouched() {
        let quick = async { Ok::<u8, String>(7) };
        assert_eq!(bounded(quick, Duration::from_secs(30)).await.unwrap(), 7);
    }

    #[tokio::test]
    async fn a_failed_preview_reports_its_own_error() {
        let broken = async { Err::<u8, String>("disk went away".into()) };
        let err = bounded(broken, Duration::from_secs(30))
            .await
            .expect_err("the worker's error must surface");
        assert!(err.contains("disk went away"), "{err}");
    }

    #[tokio::test]
    async fn a_preview_within_budget_returns_a_preview() {
        // The real call path, with a generous budget so timing is irrelevant.
        let engine = devclean_cleanup::CleanupEngine::default();
        let ids = vec!["nm:does-not-matter".to_string()];
        let resources: HashMap<String, Resource> = HashMap::new();
        let preview = bounded_preview(engine, ids, resources, Duration::from_secs(30))
            .await
            .expect("an empty selection must preview fine");
        assert!(preview.items.is_empty(), "unknown ids resolve to nothing");
        assert!(
            !preview.preview_token.is_empty(),
            "a token is still issued for the requested id set"
        );
    }

    #[test]
    fn the_preview_budget_is_named_in_the_contract() {
        // v0.7.2: the deadline is caller-visible behaviour, so it is asserted
        // here rather than only documented.
        assert_eq!(PREVIEW_TIMEOUT, Duration::from_secs(120));
    }

    #[test]
    fn the_cached_summary_seeds_the_id_registry() {
        // Reproduces the launch window: the window paints from the cache
        // before the first scan fills `AppState::resources`. If the cache did
        // not seed it, a cleanup started in that window resolves nothing.
        let dir = std::env::temp_dir().join(format!("devclean-cache-seed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let cache_path = dir.join("scan-cache.json");
        let settings_path = dir.join("settings.json");
        let history_path = dir.join("history.jsonl");

        let summary = ScanSummary {
            total_bytes: 10,
            reclaimable_bytes: 10,
            retention_blocked_bytes: 0,
            providers: vec![devclean_core::ProviderSummary {
                provider: "node_modules".into(),
                name: "node_modules".into(),
                total_bytes: 10,
                reclaimable_bytes: 10,
                resource_count: 1,
            }],
            resources: vec![Resource {
                id: "nm:1".into(),
                provider: "node_modules".into(),
                name: "app/node_modules".into(),
                path: "/tmp/app/node_modules".into(),
                size_bytes: 10,
                reclaimable_bytes: 10,
                apparent_bytes: None,
                safety: devclean_core::SafetyLevel::Safe,
                detail: None,
                impact: None,
                age_days: None,
                blocked_reason: None,
                blocked_detail: None,
            }],
        };
        devclean_scanner::save_cached_summary(&cache_path, &["/tmp".into()], &summary)
            .expect("cache");

        let state = AppState {
            scan: std::sync::Arc::new(devclean_scanner::ScanManager::default()),
            cleanup: devclean_cleanup::CleanupEngine::default(),
            resources: std::sync::Mutex::new(HashMap::new()),
            settings: std::sync::Mutex::new(devclean_core::Settings {
                scan_roots: vec!["/tmp".into()],
                ..Default::default()
            }),
            settings_path,
            scan_cache_path: cache_path,
            history_path,
            disk_free: std::sync::Mutex::new(DiskFree {
                available_bytes: 0,
                total_bytes: 0,
                known: false,
                mount: None,
            }),
        };

        let cached = cached_scan(&state).expect("cached scan");
        assert!(cached.is_some(), "the cache should have loaded");
        assert_eq!(
            state.snapshot_resources().len(),
            1,
            "the cached ids must be resolvable before the first scan finishes"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}

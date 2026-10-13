//! Tauri commands — the ONLY bridge between the Svelte UI and the Rust
//! core. This layer is intentionally thin: no filesystem logic lives here
//! beyond wiring crates together. See `docs/reference/tauri-contract.md`.

use crate::events::*;
use crate::state::AppState;
use devclean_core::{
    CleanupPreview, CleanupResult, HistoryEntry, ProviderInfo, Resource, SafetyValidator,
    ScanSummary, Settings,
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
    let (roots, excluded, disabled) = {
        let s = state.settings.lock().unwrap();
        (
            s.scan_roots.clone(),
            s.excluded_paths.clone(),
            s.disabled_providers.clone(),
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

    let summary = state
        .scan
        .scan_all(roots, &excluded, &disabled, Some(progress))
        .await;
    state.store_resources(summary.resources.clone());
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

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let i = ((bytes as f64).log(1000.0) as usize).min(UNITS.len() - 1);
    let v = bytes as f64 / 1000f64.powi(i as i32);
    format!("{v:.1} {}", UNITS[i])
}

/// Last scan, instantly, if the cache matches the current scan roots.
/// The UI shows this first, then runs a background rescan.
#[tauri::command]
pub fn get_cached_scan(state: State<'_, AppState>) -> Result<Option<ScanSummary>, String> {
    let roots = state.settings.lock().unwrap().scan_roots.clone();
    Ok(devclean_scanner::load_cached_summary(
        &state.scan_cache_path,
        &roots,
    ))
}

/// Re-scan a single provider and merge the result into the stored state.
/// (MVP: implemented as a full scan filtered to one provider.)
#[tauri::command]
pub async fn scan_provider(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: String,
) -> Result<ScanSummary, String> {
    let full = scan_all(app, state.clone()).await?;
    let resources: Vec<Resource> = full
        .resources
        .into_iter()
        .filter(|r| r.provider == provider)
        .collect();
    let total_bytes = resources.iter().map(|r| r.size_bytes).sum();
    let reclaimable_bytes = resources.iter().map(|r| r.reclaimable_bytes).sum();
    Ok(ScanSummary {
        total_bytes,
        reclaimable_bytes,
        providers: full
            .providers
            .into_iter()
            .filter(|p| p.provider == provider)
            .collect(),
        resources,
    })
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
    let engine = state.cleanup.clone();
    let resources = state.snapshot_resources();
    tokio::task::spawn_blocking(move || engine.preview(&ids, &resources, &providers_map()))
        .await
        .map_err(|e| e.to_string())
}

/// Execute cleanup behind the safety validator. Emits `cleanup_started` →
/// N×`cleanup_progress` (one per finished item) → `cleanup_completed`.
/// Each resource reports its own status so partial failure is transparent
/// (G-REL-02).
///
/// Async + `spawn_blocking`: deletion + re-measurement walk the
/// filesystem and previously froze the UI for the whole run (sync
/// commands execute on the Tauri main thread).
#[tauri::command]
pub async fn cleanup(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<Vec<CleanupResult>, String> {
    let settings = state.settings.lock().unwrap().clone();
    if settings.require_confirmation {
        // Confirmation itself is a UI concern; the backend records that the
        // gate exists. (Full "confirmed: bool" param arrives in M2.)
        tracing::debug!("cleanup requires UI confirmation (G-UI-01)");
    }
    app.emit(CLEANUP_STARTED, &ids).map_err(|e| e.to_string())?;

    // Everything the blocking task needs is owned up front so the async
    // handler never holds a lock across the filesystem work.
    let engine = state.cleanup.clone();
    let resources = state.snapshot_resources();
    let roots = settings.scan_roots.clone();
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
        engine.execute(
            &ids,
            &resources,
            &providers,
            &validator,
            lookup,
            Some(on_progress),
        )
    })
    .await
    .map_err(|e| e.to_string())?;

    // Durable per-resource history (M3, JSONL). Best-effort: a failed
    // append is logged, never fatal to the cleanup itself.
    {
        let finished = now_unix();
        for r in &results {
            let src = state.resources.lock().unwrap().get(&r.resource_id).cloned();
            let entry = HistoryEntry {
                finished_at_unix: finished,
                resource_id: r.resource_id.clone(),
                provider: src.as_ref().map(|s| s.provider.clone()).unwrap_or_default(),
                name: src.as_ref().map(|s| s.name.clone()).unwrap_or_default(),
                requested_bytes: r.requested_bytes,
                freed_bytes: r.freed_bytes,
                status: r.status,
                error: r.error.clone(),
            };
            if let Err(e) = devclean_cleanup::append_history_entry(&state.history_path, &entry) {
                tracing::warn!("could not append cleanup history: {e}");
                break;
            }
        }
    }

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

/// Newest cleanup history entries, oldest first (M3). `limit` defaults
/// to 100 and is capped at 1000.
#[tauri::command]
pub fn get_history(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<HistoryEntry>, String> {
    let limit = limit.unwrap_or(100).clamp(1, 1000) as usize;
    Ok(devclean_cleanup::read_history(&state.history_path, limit))
}

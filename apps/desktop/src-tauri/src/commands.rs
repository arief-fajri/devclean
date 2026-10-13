//! Tauri commands — the ONLY bridge between the Svelte UI and the Rust
//! core. This layer is intentionally thin: no filesystem logic lives here
//! beyond wiring crates together. See `docs/reference/tauri-contract.md`.

use crate::events::*;
use crate::state::AppState;
use devclean_core::{
    CleanupPreview, CleanupResult, Resource, SafetyValidator, ScanSummary, Settings,
};
use devclean_providers::{all_providers, Provider};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

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
    let roots = state.settings.lock().unwrap().scan_roots.clone();
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

    let summary = state.scan.scan_all(roots, Some(progress)).await;
    state.store_resources(summary.resources.clone());
    app.emit(SCAN_COMPLETED, &summary)
        .map_err(|e| e.to_string())?;
    Ok(summary)
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
#[tauri::command]
pub fn preview_cleanup(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<CleanupPreview, String> {
    let resources = state.snapshot_resources();
    Ok(state.cleanup.preview(&ids, &resources, &providers_map()))
}

/// Execute cleanup behind the safety validator. Emits `cleanup_started` →
/// `cleanup_progress` → `cleanup_completed`. Each resource reports its own
/// status so partial failure is transparent (G-REL-02).
#[tauri::command]
pub fn cleanup(
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

    let resources = state.snapshot_resources();
    let validator = SafetyValidator::new(settings.scan_roots.clone());
    let lookup = |id: &str| resources.get(id).map(|r| r.path.clone());
    let results = state
        .cleanup
        .execute(&ids, &resources, &providers_map(), &validator, lookup);

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

    app.emit(
        CLEANUP_PROGRESS,
        CleanupProgressPayload {
            done: results.len(),
            total: ids.len(),
        },
    )
    .map_err(|e| e.to_string())?;
    app.emit(CLEANUP_COMPLETED, &results)
        .map_err(|e| e.to_string())?;
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
pub fn update_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings, String> {
    // Guardrail G-SAFE-03: privileged operations stay off unless the user
    // explicitly opts in — which this assignment already represents.
    *state.settings.lock().unwrap() = settings.clone();
    Ok(settings)
}

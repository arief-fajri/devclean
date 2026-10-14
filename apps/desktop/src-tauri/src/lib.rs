//! DevClean desktop shell. All product logic lives in the `devclean-*`
//! crates; this binary only wires Tauri commands, events, and state.

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod events;
mod state;
mod tray;

use commands::*;
use state::{apply_window_background, AppState};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .setup(|app| {
            // Paint the native window background (title bar) from the
            // persisted theme before the first frame, so no unpainted
            // transparent strip is ever visible.
            let theme = app.state::<AppState>().settings.lock().unwrap().theme;
            apply_window_background(app.handle(), theme);
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            // Restrained notifications only (scan/cleanup completion while
            // the window is unfocused — see commands::notify_if_background).
            app.handle().plugin(tauri_plugin_notification::init())?;
            tray::setup(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window hides it and leaves DevClean in the menu
            // bar. Without this the runtime exits as soon as the last window
            // is gone (tauri-runtime-writ), which made every tray action
            // unreachable: the icon simply vanished with the app.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            scan_all,
            scan_provider,
            get_resource_details,
            preview_cleanup,
            cleanup,
            cancel_scan,
            cancel_cleanup,
            get_settings,
            update_settings,
            get_cached_scan,
            get_history,
            get_history_runs,
            get_providers,
            detect_scan_roots,
            get_disk_free,
        ])
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}

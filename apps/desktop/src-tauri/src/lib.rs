//! DevClean desktop shell. All product logic lives in the `devclean-*`
//! crates; this binary only wires Tauri commands, events, and state.

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod events;
mod state;

use commands::*;
use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
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
            update_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}

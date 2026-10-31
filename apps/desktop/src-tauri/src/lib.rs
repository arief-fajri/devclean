//! DevClean desktop shell. All product logic lives in the `devclean-*`
//! crates; this binary only wires Tauri commands, events, and state.

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod disk;
mod events;
mod metrics;
mod state;
mod tray;

use commands::*;
use state::{apply_window_background, AppState};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // A Finder-launched app inherits launchd's minimal PATH — no
    // `/usr/local/bin`, no Homebrew — so `docker`, `go` and the rest were
    // invisible to every provider, which then reported "nothing found"
    // without saying why. Widen it before the builder can spawn anything;
    // the log line lands below, once the log plugin exists.
    let added_to_path = devclean_core::system::augment_process_path();
    let app = tauri::Builder::default()
        .manage(AppState::default())
        .setup(move |app| {
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
            if !added_to_path.is_empty() {
                log::info!(
                    "PATH widened for tool discovery: {}",
                    added_to_path
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(" ")
                );
            }
            // Restrained notifications only (scan/cleanup completion while
            // the window is unfocused — see commands::notify_if_background).
            app.handle().plugin(tauri_plugin_notification::init())?;
            // In-app updates (contract v0.12.0). The plugin verifies every
            // payload against the minisign public key in tauri.conf.json
            // before installing it (G-UPD-01).
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;

            // A startup check runs ONLY when the user opted in: DevClean makes
            // no routine outbound request otherwise. Spawned, never awaited,
            // so the window can never wait on the network (G-UPD-02, G-REL-06).
            let check_on_start = app
                .state::<AppState>()
                .settings
                .lock()
                .unwrap()
                .check_updates_automatically;
            if check_on_start {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<AppState>();
                    let _ = commands::run_update_check(&handle, &state).await;
                });
            }
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
            get_update_status,
            check_for_update,
            install_update,
            restart_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // `Builder::run` is `build()?.run(|_, _| {})` — an empty event handler. The
    // window close handler above leaves DevClean running in the menu bar with
    // its window hidden, and closing that loop is exactly what made the Dock
    // icon dead: macOS sends `applicationShouldHandleReopen` on a Dock click,
    // which becomes `RunEvent::Reopen`, which nothing was listening for. The
    // Dock icon stayed on screen and did nothing at all.
    app.run(|app_handle, event| {
        // macOS only, and deliberately unbranched: when the window is merely
        // minimised macOS reports `has_visible_windows: true`, so gating on
        // `!has_visible_windows` would skip that case. `show_main` is
        // idempotent (show + unminimize + focus).
        #[cfg(target_os = "macos")]
        if matches!(event, tauri::RunEvent::Reopen { .. }) {
            tray::show_main(app_handle);
        }
        // `RunEvent::Reopen` does not exist off macOS, so both parameters go
        // unused there — and clippy runs with `-D warnings`.
        let _ = (app_handle, event);
    });
}

#[cfg(test)]
mod tests {
    /// This module's own source. A platform-delegate event cannot be raised
    /// from a unit test — macOS has to be clicked — so the only thing CI can
    /// do is make sure the handler is still *there*. These tests read the file
    /// rather than testing behaviour, and that limitation is deliberate: a
    /// passing test here means "not deleted", not "works".
    fn source(path: &str) -> String {
        std::fs::read_to_string(format!("{}/src/{path}", env!("CARGO_MANIFEST_DIR")))
            .unwrap_or_else(|e| panic!("reading {path}: {e}"))
    }

    /// The file up to its test module. These guardrails are about production
    /// code, and the assertions below would otherwise match their own source
    /// text — a test that quotes `.set_focus()` must not count itself.
    fn production(path: &str) -> String {
        let src = source(path);
        match src.find("#[cfg(test)]") {
            Some(i) => src[..i].to_string(),
            None => src,
        }
    }

    #[test]
    fn the_dock_reopen_event_is_handled() {
        let lib = production("lib.rs");
        assert!(
            lib.contains("RunEvent::Reopen"),
            "lib.rs must match RunEvent::Reopen, or a Dock click does nothing (G-UI-06)"
        );
        assert!(
            lib.contains("tray::show_main(app_handle)"),
            "the Dock click must go through the same reveal path as the menu bar"
        );
        // The dead-icon bug was an *empty* event handler. `Builder::run` is
        // literally `build()?.run(|_, _| {})`, so going back to it would
        // silently re-break this.
        assert!(
            !lib.contains(".run(tauri::generate_context!())"),
            "use .build(...)? + app.run(...): Builder::run installs an empty handler"
        );
    }

    #[test]
    fn every_reveal_path_goes_through_one_function() {
        let tray = production("tray.rs");
        assert!(
            tray.contains("pub(crate) fn show_main"),
            "show_main must be reachable from lib.rs"
        );
        // Two copies of show/unminimize/focus would drift: the Dock could get
        // a fix the menu bar never sees, or the reverse. `set_focus` may appear
        // exactly once in the tray — inside `show_main` — and never in lib.rs.
        assert_eq!(
            tray.matches(".set_focus()").count(),
            1,
            "focusing the window belongs to show_main and nowhere else"
        );
        let lib = production("lib.rs");
        assert!(
            !lib.contains(".set_focus()"),
            "lib.rs must reveal through show_main, not by reaching for the window itself"
        );
        for (name, src) in [("lib.rs", &lib), ("tray.rs", &tray)] {
            assert!(
                src.contains("show_main"),
                "{name} must reach the window through show_main"
            );
        }
    }
}

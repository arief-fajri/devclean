//! DevClean desktop shell. All product logic lives in the `devclean-*`
//! crates; this binary only wires Tauri commands, events, and state.

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod events;
mod state;

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
            setup_tray(app.handle())?;
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

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// System tray: Show / Scan now / Quit. No autostart, no promo — the tray
/// is quick access only (roadmap: "never adware").
fn setup_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::{
        menu::{Menu, MenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };

    let show = MenuItem::with_id(app, "show", "Show DevClean", true, None::<&str>)?;
    let scan = MenuItem::with_id(app, "scan", "Scan now", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit DevClean", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &scan, &quit])?;
    // The app's own mark, not the default Tauri scaffold icon. The tray
    // uses a macOS *template* image (black + alpha) so the menu bar
    // recolours it for light/dark automatically; the 2x variant is what
    // Retina displays actually sample.
    // `tray.png` is authored as a macOS template image (black + alpha only);
    // `icon_as_template` makes the system recolour it for light/dark menus.
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("DevClean")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "scan" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state: tauri::State<'_, AppState> = handle.state();
                    if let Err(e) = run_scan(&handle, &state).await {
                        tracing::warn!("tray scan failed: {e}");
                    }
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

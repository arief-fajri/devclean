//! The menu bar surface: one click opens a menu that answers "how much can I
//! get back?" without opening the window.
//!
//! Two rules shape this module:
//!
//! * **It never destroys anything (G-UI-02).** The menu may scan, navigate
//!   and quit; the only path to a deletion is the in-app confirmation, and the
//!   ids in [`MENU_ACTION_IDS`] are the complete list of what the menu bar can
//!   do. `src/lib/architecture.test.ts` fails the build if a destructive call
//!   ever appears here.
//! * **It never blocks the UI thread (G-REL-01).** Numbers come from the scan
//!   cache on disk and from `AppState`; opening the menu reads files, it does
//!   not spawn processes and it does not scan.

use crate::state::AppState;
use devclean_core::{format_age, format_bytes, DiskFree, ScanSummary};
use std::time::SystemTime;
use tauri::{
    image::Image,
    menu::{IconMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Wry,
};

/// Everything the menu bar may do. Read by `architecture.test.ts`.
pub const MENU_ACTION_IDS: [&str; 6] = ["open", "scan", "review", "history", "settings", "quit"];

/// Live handles so a completed scan can repaint the menu without rebuilding
/// the tray icon (which would make it blink).
pub struct TrayHandles {
    headline: MenuItem<Wry>,
    age: MenuItem<Wry>,
    /// Fixed three slots for the biggest reclaimers: the set changes between
    /// scans, so the rows are reused and only their text and icon change.
    rows: [IconMenuItem<Wry>; 3],
    warn: MenuItem<Wry>,
    disk: MenuItem<Wry>,
    scan_now: MenuItem<Wry>,
}

/// What the menu shows, derived from data we already have on disk.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TraySnapshot {
    pub headline: String,
    pub age: String,
    /// `(provider id, label, bytes)`, biggest first.
    pub top: Vec<(String, String, u64)>,
    /// `None` when there is nothing stale to say.
    pub warning: Option<String>,
    pub disk_low: Option<String>,
    pub tooltip: String,
    pub scanning: bool,
}

impl TraySnapshot {
    /// The state shown before anything has ever been scanned.
    pub fn empty() -> Self {
        Self {
            headline: "No scan yet".into(),
            age: "Scan to see what is reclaimable".into(),
            tooltip: "DevClean — never scanned".into(),
            ..Default::default()
        }
    }
}

/// Build the menu and register the tray icon. Idempotent enough for setup:
/// it runs once per launch, and every later repaint goes through
/// [`refresh`].
pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let empty = TraySnapshot::empty();

    let headline = info_item(app, "info-headline", &empty.headline)?;
    let age = info_item(app, "info-age", &empty.age)?;
    let warn = info_item(app, "info-warn", "")?;
    let disk = info_item(app, "info-disk", "")?;
    let rows = [
        icon_item(app, "info-row-1", "", None)?,
        icon_item(app, "info-row-2", "", None)?,
        icon_item(app, "info-row-3", "", None)?,
    ];

    let open = MenuItem::with_id(app, "open", "Open DevClean", true, Some("CmdOrCtrl+O"))?;
    let scan_now = MenuItem::with_id(app, "scan", "Scan now", true, Some("CmdOrCtrl+R"))?;
    let review = MenuItem::with_id(app, "review", "Review safe items…", true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", "History…", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    let quit = MenuItem::with_id(app, "quit", "Quit DevClean", true, Some("CmdOrCtrl+Q"))?;

    let menu = Menu::with_items(
        app,
        &[
            &headline,
            &age,
            &PredefinedMenuItem::separator(app)?,
            &rows[0],
            &rows[1],
            &rows[2],
            &warn,
            &disk,
            &PredefinedMenuItem::separator(app)?,
            &open,
            &scan_now,
            &review,
            &PredefinedMenuItem::separator(app)?,
            &history,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    // A macOS *template* image (black + alpha) so the menu bar recolours it
    // for light and dark automatically.
    let icon = Image::from_bytes(include_bytes!("../icons/tray.png"))?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(true)
        .tooltip(&empty.tooltip)
        .menu(&menu)
        // One click, either button, shows the menu: the status item is a menu
        // first and a window launcher second.
        .show_menu_on_left_click(true)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                // The menu handles the click itself; this only makes sure a
                // click that reaches us still shows the window.
                show_main(tray.app_handle());
            }
            // Repaint on hover so the numbers are never older than the moment
            // the user looked at them.
            if let TrayIconEvent::Enter { .. } = event {
                let app = tray.app_handle().clone();
                let state = app.state::<AppState>();
                refresh(&app, &state);
            }
        })
        .build(app)?;

    app.manage(TrayHandles {
        headline,
        age,
        rows,
        warn,
        disk,
        scan_now,
    });

    // First paint: whatever the cache already says, before any interaction.
    let state = app.state::<AppState>();
    refresh(app, &state);
    Ok(())
}

/// A grey, non-clickable line: information, not an action.
fn info_item(app: &AppHandle, id: &str, text: &str) -> tauri::Result<MenuItem<Wry>> {
    MenuItem::with_id(app, id, text, false, None::<&str>)
}

fn icon_item(
    app: &AppHandle,
    id: &str,
    text: &str,
    icon: Option<Image<'static>>,
) -> tauri::Result<IconMenuItem<Wry>> {
    IconMenuItem::with_id(app, id, text, false, icon, None::<&str>)
}

/// The one way DevClean reveals its window. The menu bar, the tray icon and
/// the Dock all call this — so a fix to one cannot leave the others behind
/// (see G-UI-06: closing the window used to leave the Dock icon dead, because
/// this logic lived behind the menu bar's event handler and nothing else).
pub(crate) fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// The menu bar asks the window to navigate. The frontend owns that decision,
/// so the id is validated there (`isRouteId` in `$lib/nav`) and anything
/// unknown is ignored rather than applied.
fn on_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    let id = event.id().as_ref();
    // The menu bar acts only on ids that were reviewed as read-only or
    // navigational (G-UI-02). An id that is not on the list is refused here,
    // so a future item cannot quietly acquire a destructive meaning.
    if !MENU_ACTION_IDS.contains(&id) {
        tracing::debug!("tray: refused unknown menu id {id}");
        return;
    }
    let route: Option<(&str, Option<&str>)> = match id {
        "open" | "history" => {
            show_main(app);
            if event.id().as_ref() == "history" {
                Some(("history", None))
            } else {
                None
            }
        }
        // "Review safe items" is navigation, never a cleanup: it opens the
        // list with the safe filter applied (G-UI-02).
        "review" => {
            show_main(app);
            Some(("resources", Some("safe")))
        }
        "settings" => {
            show_main(app);
            Some(("settings", None))
        }
        "scan" => {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let state: tauri::State<'_, AppState> = handle.state();
                tracing::debug!("tray: scan requested");
                if let Err(e) = crate::commands::run_scan(&handle, &state).await {
                    tracing::warn!("tray scan failed: {e}");
                }
                let state = handle.state::<AppState>();
                refresh(&handle, &state);
            });
            None
        }
        "quit" => {
            app.exit(0);
            None
        }
        _ => None,
    };
    if let Some((route, filter)) = route {
        use tauri::Emitter;
        let payload = crate::events::TrayAction {
            route: route.into(),
            filter: filter.map(str::to_string),
        };
        if let Err(e) = app.emit(crate::events::TRAY_ACTION, payload) {
            tracing::warn!("could not emit tray action: {e}");
        }
    }
}

/// Repaint the menu from the scan cache. Cheap: one small JSON file plus the
/// settings we already hold.
pub fn refresh(app: &AppHandle, state: &AppState) {
    let Some(handles) = app.try_state::<TrayHandles>() else {
        return;
    };
    let snapshot = snapshot(state);
    apply(app, &handles, &snapshot);
}

fn apply(app: &AppHandle, handles: &TrayHandles, s: &TraySnapshot) {
    let set = |result: tauri::Result<()>, what: &str| {
        if let Err(e) = result {
            tracing::warn!("could not update tray {what}: {e}");
        }
    };
    set(handles.headline.set_text(&s.headline), "headline");
    set(handles.age.set_text(&s.age), "age");
    for (slot, (id, label, bytes)) in handles.rows.iter().zip(s.top.iter()) {
        set(
            slot.set_text(format!("{} · {}", label, format_bytes(*bytes))),
            "row text",
        );
        set(slot.set_icon(Some(chip(id))), "row icon");
    }
    // Hide the unused slots: an empty grey row reads as a bug.
    for slot in handles.rows.iter().skip(s.top.len()) {
        set(slot.set_text(""), "row hide");
        set(slot.set_icon(None), "row icon hide");
    }
    set(
        handles.warn.set_text(s.warning.as_deref().unwrap_or("")),
        "warning",
    );
    set(
        handles.disk.set_text(s.disk_low.as_deref().unwrap_or("")),
        "disk free",
    );
    set(
        handles.scan_now.set_text(if s.scanning {
            "Scanning…"
        } else {
            "Scan now"
        }),
        "scan item",
    );
    if let Some(tray) = app.tray_by_id("main") {
        set(tray.set_tooltip(Some(s.tooltip.as_str())), "tooltip");
    }
}

/// Read the numbers the menu shows. No scan, no subprocess: the cache file is
/// the same one the window painted from.
pub fn snapshot(state: &AppState) -> TraySnapshot {
    let settings = state.settings.lock().unwrap();
    let roots = settings.scan_roots.clone();
    drop(settings);

    let cached = devclean_scanner::load_cached_summary(&state.scan_cache_path, &roots);
    let age = cache_age(&state.scan_cache_path);

    let Some(summary) = cached else {
        let mut s = TraySnapshot::empty();
        s.age = age;
        s.disk_low = low_disk(&state.disk_free());
        return s;
    };
    from_summary(&summary, age, &state.disk_free())
}

/// Items older than this are the "stale" signal — a display hint only, never
/// a safety verdict (the frontend parses the same threshold out of `detail`).
const STALE_DAYS: u64 = 90;

/// Everything about the menu contents that can be decided without a window.
pub fn from_summary(summary: &ScanSummary, age: String, disk: &DiskFree) -> TraySnapshot {
    let mut providers: Vec<_> = summary.providers.iter().collect();
    providers.sort_by_key(|p| std::cmp::Reverse(p.reclaimable_bytes));
    let top: Vec<(String, String, u64)> = providers
        .iter()
        .take(3)
        .map(|p| (p.provider.clone(), p.name.clone(), p.reclaimable_bytes))
        .collect();

    let kept = summary.blocked_bytes;
    let headline = if kept > 0 {
        format!(
            "{} reclaimable · {} kept",
            format_bytes(summary.reclaimable_bytes),
            format_bytes(kept)
        )
    } else {
        format!("{} reclaimable", format_bytes(summary.reclaimable_bytes))
    };

    let stale = summary
        .resources
        .iter()
        .filter(|r| r.age_days.is_some_and(|d| d >= STALE_DAYS))
        .count();
    let warning = (stale > 0).then(|| {
        format!(
            "{} stale over 90 days",
            if stale == 1 {
                "1 item".to_string()
            } else {
                format!("{stale} items")
            }
        )
    });

    let tooltip = format!(
        "DevClean — {} · last scan {}",
        format_bytes(summary.reclaimable_bytes),
        age
    );

    TraySnapshot {
        headline,
        age,
        top,
        warning,
        disk_low: low_disk(disk),
        tooltip,
        scanning: false,
    }
}

/// Only worth a menu line when the disk is actually in trouble.
fn low_disk(disk: &DiskFree) -> Option<String> {
    if !disk.known || disk.total_bytes == 0 {
        return None;
    }
    let free = disk.available_bytes as f64 / disk.total_bytes as f64;
    if free >= 0.1 {
        return None;
    }
    Some(format!(
        "Low disk — {} free",
        format_bytes(disk.available_bytes)
    ))
}

/// "2h ago", taken from the cache file's mtime — the scan time is not part of
/// the summary payload.
fn cache_age(path: &std::path::Path) -> String {
    let secs = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_age(secs)
}

/// A provider chip for the menu rows: the app's accent colour with a simple
/// glyph, generated here so no raster assets and no SVG renderer are needed.
///
/// `muda` draws menu icons at a fixed 18pt height and does not template-
/// recolour them, so the pixels are authored square and at 2x (36px) for
/// Retina. A non-square image is what made the first tray icon look squashed.
fn chip(provider: &str) -> Image<'static> {
    const SIZE: u32 = 36;
    // The chip fills ~78% of the canvas. `muda` draws menu images at 18pt, so
    // this lands at ~14pt — a normal menu icon size — and still leaves a
    // transparent margin so it never crowds the label.
    const GLYPH: f32 = 0.78;
    let (r, g, b) = accent(provider);
    let mut px = vec![0u8; (SIZE * SIZE * 4) as usize];
    let pad = (SIZE as f32 * (1.0 - GLYPH) / 2.0).round() as i32;
    let glyph_lo = pad;
    let glyph_hi = SIZE as i32 - pad;

    for y in 0..SIZE as i32 {
        for x in 0..SIZE as i32 {
            let inside_glyph = x >= glyph_lo && x < glyph_hi && y >= glyph_lo && y < glyph_hi;
            if !inside_glyph {
                continue;
            }
            let (fx, fy) = (x as f32, y as f32);
            let c = SIZE as f32 / 2.0;
            let lit = glyph_mask(provider, fx - c, fy - c, c);
            let (pr, pg, pb) = if lit { (255, 255, 255) } else { (r, g, b) };
            let i = ((y as u32 * SIZE + x as u32) * 4) as usize;
            px[i] = pr;
            px[i + 1] = pg;
            px[i + 2] = pb;
            px[i + 3] = 255;
        }
    }
    Image::new_owned(px, SIZE, SIZE)
}

/// The accent colours mirror `providerMeta.ts` (Tailwind 600 steps). A drift
/// between the two is cosmetic, not behavioural.
fn accent(provider: &str) -> (u8, u8, u8) {
    match provider {
        "node_modules" => (0x84, 0xcc, 0x16),
        "rust_target" | "claude" => (0xea, 0x58, 0x0c),
        "go_cache" => (0x06, 0x9e, 0xb6),
        "docker" => (0x02, 0x84, 0xc7),
        "docker_pruned" | "npm_cache" => (0xdc, 0x26, 0x26),
        "python_cache" => (0x25, 0x63, 0xeb),
        "cargo_registry" => (0xd9, 0x77, 0x06),
        "gradle_cache" => (0x05, 0x96, 0x69),
        "global_cache" => (0x0d, 0x94, 0x88),
        "opencode_fs" => (0x4f, 0x46, 0xe5),
        "opencode_db" => (0x93, 0x3c, 0xe8),
        _ => (0x71, 0x71, 0x7a),
    }
}

/// Seven primitives, drawn with rectangles and discs — enough to tell the
/// provider families apart at 18pt without a font or a path rasteriser.
fn glyph_mask(provider: &str, x: f32, y: f32, c: f32) -> bool {
    let box_shape =
        |hw: f32, hh: f32, y0: f32| (x + hw).abs() <= hw && y >= y0 - hh && y <= y0 + hh;
    match provider {
        // stack of boxes (rust target)
        "rust_target" => {
            box_shape(c * 0.62, c * 0.16, -c * 0.45)
                || box_shape(c * 0.62, c * 0.16, c * 0.05)
                || box_shape(c * 0.62, c * 0.16, c * 0.55)
        }
        // container: one box with a lid line
        "docker" => box_shape(c * 0.6, c * 0.5, c * 0.1) || box_shape(c * 0.72, c * 0.1, -c * 0.52),
        // archive box with a slot
        "cargo_registry" => {
            box_shape(c * 0.6, c * 0.5, c * 0.1) || box_shape(c * 0.6, c * 0.12, -c * 0.52)
        }
        // package: a box with a lid band
        "node_modules" | "npm_cache" | "python_cache" | "gradle_cache" => {
            box_shape(c * 0.58, c * 0.55, c * 0.05) || box_shape(c * 0.58, c * 0.1, -c * 0.42)
        }
        // disc (docker prune / caches)
        "go_cache" | "docker_pruned" | "global_cache" => {
            (x * x + y * y) <= (c * 0.55).powi(2) && (x * x + y * y) >= (c * 0.28).powi(2)
        }
        // page with lines (AI history / documents)
        "opencode_fs" | "opencode_db" | "claude" | "xcode_data" => {
            box_shape(c * 0.5, c * 0.6, 0.0)
                || box_shape(c * 0.3, c * 0.06, -c * 0.3)
                || box_shape(c * 0.3, c * 0.06, 0.0)
                || box_shape(c * 0.3, c * 0.06, c * 0.3)
        }
        _ => (x * x + y * y) <= (c * 0.5).powi(2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devclean_core::{ProviderSummary, Resource};

    fn summary(providers: Vec<(&str, &str, u64)>, kept: u64, stale: usize) -> ScanSummary {
        ScanSummary {
            total_bytes: 1_000,
            reclaimable_bytes: providers.iter().map(|p| p.2).sum(),
            blocked_bytes: kept,
            providers: providers
                .into_iter()
                .map(|(id, name, bytes)| ProviderSummary {
                    provider: id.into(),
                    name: name.into(),
                    total_bytes: bytes,
                    reclaimable_bytes: bytes,
                    resource_count: 1,
                })
                .collect(),
            resources: (0..stale)
                .map(|i| Resource {
                    id: format!("s:{i}"),
                    provider: "node_modules".into(),
                    name: format!("stale-{i}"),
                    path: format!("/tmp/{i}").into(),
                    size_bytes: 10,
                    reclaimable_bytes: 10,
                    apparent_bytes: None,
                    safety: devclean_core::SafetyLevel::Safe,
                    detail: Some("· 143 days stale · npm".into()),
                    impact: None,
                    age_days: Some(143),
                    blocked_reason: None,
                    blocked_detail: None,
                })
                .collect(),
        }
    }

    fn disk(free: u64, total: u64) -> DiskFree {
        DiskFree {
            available_bytes: free,
            total_bytes: total,
            known: true,
            mount: None,
        }
    }

    #[test]
    fn headline_reports_reclaimable_and_kept() {
        let s = from_summary(
            &summary(vec![("docker", "Docker", 5_000_000_000)], 1_000_000_000, 0),
            "2h ago".into(),
            &disk(500_000_000_000, 1_000_000_000_000),
        );
        assert_eq!(s.headline, "5.0 GB reclaimable · 1.0 GB kept");
        assert_eq!(s.tooltip, "DevClean — 5.0 GB · last scan 2h ago");
    }

    #[test]
    fn headline_omits_the_kept_clause_when_nothing_is_held_back() {
        let s = from_summary(
            &summary(vec![("docker", "Docker", 5_000)], 0, 0),
            "1m ago".into(),
            &disk(500, 1_000),
        );
        assert_eq!(s.headline, "5.0 KB reclaimable");
    }

    #[test]
    fn rows_are_biggest_first_and_capped_at_three() {
        let s = from_summary(
            &summary(
                vec![
                    ("docker", "Docker", 10),
                    ("node_modules", "node_modules", 90),
                    ("rust_target", "Rust target", 50),
                    ("go_cache", "Go cache", 30),
                    ("cargo_registry", "Cargo registry", 20),
                ],
                0,
                0,
            ),
            "just now".into(),
            &disk(500, 1_000),
        );
        assert_eq!(
            s.top
                .iter()
                .map(|(id, _, _)| id.as_str())
                .collect::<Vec<_>>(),
            ["node_modules", "rust_target", "go_cache"]
        );
    }

    #[test]
    fn stale_items_raise_a_warning_and_a_single_item_is_not_pluralised() {
        let many = from_summary(
            &summary(vec![("docker", "Docker", 10)], 0, 3),
            "just now".into(),
            &disk(500, 1_000),
        );
        assert_eq!(many.warning.as_deref(), Some("3 items stale over 90 days"));

        let one = from_summary(
            &summary(vec![("docker", "Docker", 10)], 0, 1),
            "just now".into(),
            &disk(500, 1_000),
        );
        assert_eq!(one.warning.as_deref(), Some("1 item stale over 90 days"));

        let none = from_summary(
            &summary(vec![("docker", "Docker", 10)], 0, 0),
            "just now".into(),
            &disk(500, 1_000),
        );
        assert_eq!(none.warning, None);
    }

    #[test]
    fn low_disk_only_speaks_up_when_the_disk_is_actually_low() {
        let roomy = from_summary(
            &summary(vec![("docker", "Docker", 10)], 0, 0),
            "just now".into(),
            &disk(500_000, 1_000_000),
        );
        assert_eq!(roomy.disk_low, None);

        let tight = from_summary(
            &summary(vec![("docker", "Docker", 10)], 0, 0),
            "just now".into(),
            &disk(50_000, 1_000_000),
        );
        assert_eq!(tight.disk_low.as_deref(), Some("Low disk — 50.0 KB free"));

        let unknown = from_summary(
            &summary(vec![("docker", "Docker", 10)], 0, 0),
            "just now".into(),
            &DiskFree {
                available_bytes: 0,
                total_bytes: 0,
                known: false,
                mount: None,
            },
        );
        assert_eq!(unknown.disk_low, None);
    }

    #[test]
    fn an_unscanned_machine_says_so_instead_of_showing_zeros() {
        let s = TraySnapshot::empty();
        assert_eq!(s.headline, "No scan yet");
        assert_eq!(s.tooltip, "DevClean — never scanned");
        assert!(s.top.is_empty());
    }

    #[test]
    fn the_menu_can_only_do_the_six_reviewed_actions() {
        // G-UI-02: no entry may be a cleanup.
        assert_eq!(
            MENU_ACTION_IDS,
            ["open", "scan", "review", "history", "settings", "quit"]
        );
    }

    #[test]
    fn chips_are_square_and_actually_paint_something() {
        for provider in [
            "node_modules",
            "rust_target",
            "go_cache",
            "docker",
            "docker_pruned",
            "npm_cache",
            "python_cache",
            "cargo_registry",
            "gradle_cache",
            "global_cache",
            "opencode_fs",
            "opencode_db",
            "claude",
            "xcode_data",
            "something_new",
        ] {
            let img = chip(provider);
            assert_eq!((img.width(), img.height()), (36, 36), "{provider}");
            let px = img.rgba();
            let opaque = px.chunks(4).filter(|c| c[3] > 0).count();
            // Roughly a 28×28 chip inside the 36×36 canvas: opaque enough to
            // read at 18pt, small enough not to crowd the menu label.
            assert!(
                (700..=860).contains(&opaque),
                "{provider}: {opaque} opaque px"
            );
            assert_eq!(px[3], 0, "{provider} corner must stay transparent");
        }
    }
}

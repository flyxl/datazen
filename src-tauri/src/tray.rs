//! System tray for background dashboard monitoring.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use crate::commands::AppState;

pub const TRAY_ID: &str = "datazen-monitor";

static ALLOW_EXIT: AtomicBool = AtomicBool::new(false);

pub fn request_app_exit(app: &AppHandle) {
    ALLOW_EXIT.store(true, Ordering::SeqCst);
    app.exit(0);
}

pub fn should_prevent_exit(app: &AppHandle) -> bool {
    if ALLOW_EXIT.load(Ordering::SeqCst) {
        return false;
    }
    should_close_to_tray(app)
}

pub fn should_close_to_tray(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let settings = tauri::async_runtime::block_on(state.store.get_settings());
    if !settings.monitor.close_to_tray {
        return false;
    }
    state.monitor_engine.is_monitoring_active()
}

fn tray_label(lang: &str, key: &str) -> String {
    crate::menu_labels(lang)
        .get(key)
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

fn build_tray_menu<R: Runtime>(
    app: &AppHandle<R>,
    lang: &str,
    paused: bool,
) -> Result<tauri::menu::Menu<R>, Box<dyn std::error::Error>> {
    let open = MenuItemBuilder::with_id("tray-open-dashboards", tray_label(lang, "tray-open-dashboards"))
        .build(app)?;
    let pause_text = if paused {
        tray_label(lang, "tray-resume-monitoring")
    } else {
        tray_label(lang, "tray-pause-monitoring")
    };
    let pause = MenuItemBuilder::with_id("tray-toggle-pause", pause_text).build(app)?;
    let quit = MenuItemBuilder::with_id("tray-quit", tray_label(lang, "tray-quit")).build(app)?;
    Ok(MenuBuilder::new(app)
        .items(&[&open, &pause, &quit])
        .build()?)
}

fn show_dashboard_windows(app: &AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.unminimize();
        let _ = main.set_focus();
    }
    for (label, window) in app.webview_windows() {
        if label.starts_with("dashboard-") {
            let _ = window.show();
            let _ = window.unminimize();
        }
    }
}

fn handle_tray_menu_event(app: &AppHandle, id: &str) {
    match id {
        "tray-open-dashboards" => show_dashboard_windows(app),
        "tray-toggle-pause" => {
            let state = app.state::<AppState>();
            let paused = !state.monitor_engine.is_paused();
            state.monitor_engine.set_paused(paused);
            sync_tray(app);
        }
        "tray-quit" => request_app_exit(app),
        _ => {}
    }
}

/// Create, update, or remove the monitor tray icon based on settings and schedule.
pub fn sync_tray(app: &AppHandle) {
    let state = app.state::<AppState>();
    let settings = tauri::async_runtime::block_on(state.store.get_settings());
    let show = settings.monitor.tray_enabled && state.monitor_engine.is_monitoring_active();

    if !show {
        if let Some(tray) = app.tray_by_id(TRAY_ID) {
            let _ = tray.set_visible(false);
        }
        return;
    }

    let lang = settings.language.as_str();
    let paused = state.monitor_engine.is_paused();

    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        match build_tray_menu(app, lang, paused) {
            Ok(menu) => {
                let _ = tray.set_menu(Some(menu));
                let _ = tray.set_visible(true);
            }
            Err(e) => tracing::warn!(error = %e, "failed to update tray menu"),
        }
        return;
    }

    let icon = match app.default_window_icon() {
        Some(icon) => icon.clone(),
        None => {
            tracing::warn!("no default window icon for tray");
            return;
        }
    };

    let menu = match build_tray_menu(app, lang, paused) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(error = %e, "failed to build tray menu");
            return;
        }
    };

    let app_clone = app.clone();
    if let Err(e) = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .menu(&menu)
        .tooltip("DataZen")
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| {
            handle_tray_menu_event(app, event.id.as_ref());
        })
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                handle_tray_menu_event(tray.app_handle(), "tray-open-dashboards");
            }
        })
        .build(&app_clone)
    {
        tracing::warn!(error = %e, "failed to create tray icon");
    }
}

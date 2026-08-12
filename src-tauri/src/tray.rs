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
    should_close_main_to_tray(
        settings.monitor.tray_enabled,
        settings.monitor.close_to_tray,
        state.monitor_engine.is_monitoring_active(),
    )
}

pub(crate) fn tray_label(lang: &str, key: &str) -> String {
    crate::menu_labels(lang)
        .get(key)
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

/// Label for the pause/resume tray menu item (pure — safe to unit test).
pub(crate) fn tray_pause_item_label(lang: &str, paused: bool) -> String {
    if paused {
        tray_label(lang, "tray-resume-monitoring")
    } else {
        tray_label(lang, "tray-pause-monitoring")
    }
}

/// Pure mapping from tray menu item id to the action the host should perform.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TrayMenuAction {
    OpenDashboards,
    TogglePause,
    Quit,
    Ignore,
}

pub(crate) fn tray_action_for_id(id: &str) -> TrayMenuAction {
    match id {
        "tray-open-dashboards" => TrayMenuAction::OpenDashboards,
        "tray-toggle-pause" => TrayMenuAction::TogglePause,
        "tray-quit" => TrayMenuAction::Quit,
        _ => TrayMenuAction::Ignore,
    }
}

pub(crate) fn should_show_tray(tray_enabled: bool, monitoring_active: bool) -> bool {
    tray_enabled && monitoring_active
}

pub(crate) fn should_close_main_to_tray(
    tray_enabled: bool,
    close_to_tray: bool,
    monitoring_active: bool,
) -> bool {
    tray_enabled && close_to_tray && monitoring_active
}

fn build_tray_menu<R: Runtime>(
    app: &AppHandle<R>,
    lang: &str,
    paused: bool,
) -> Result<tauri::menu::Menu<R>, Box<dyn std::error::Error>> {
    let open = MenuItemBuilder::with_id(
        "tray-open-dashboards",
        tray_label(lang, "tray-open-dashboards"),
    )
    .build(app)?;
    let pause_text = tray_pause_item_label(lang, paused);
    let pause = MenuItemBuilder::with_id("tray-toggle-pause", pause_text).build(app)?;
    let quit = MenuItemBuilder::with_id("tray-quit", tray_label(lang, "tray-quit")).build(app)?;
    Ok(MenuBuilder::new(app)
        .items(&[&open, &pause, &quit])
        .build()?)
}

/// Bring the main window to the front (Dock / taskbar / tray restore).
pub fn focus_main_window(app: &AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.unminimize();
        let _ = main.set_focus();
    }
}

fn show_dashboard_windows(app: &AppHandle) {
    focus_main_window(app);
    for (label, window) in app.webview_windows() {
        if label.starts_with("dashboard-") {
            let _ = window.show();
            let _ = window.unminimize();
        }
    }
}

fn handle_tray_menu_event(app: &AppHandle, id: &str) {
    match tray_action_for_id(id) {
        TrayMenuAction::OpenDashboards => show_dashboard_windows(app),
        TrayMenuAction::TogglePause => {
            let state = app.state::<AppState>();
            let paused = !state.monitor_engine.is_paused();
            state.monitor_engine.set_paused(paused);
            sync_tray(app);
        }
        TrayMenuAction::Quit => request_app_exit(app),
        TrayMenuAction::Ignore => {}
    }
}

/// Sync tray from a non-async context (window/menu handlers).
/// Prefer [`sync_tray_async`] when already inside a Tokio/Tauri async runtime —
/// calling this from async code will panic (`block_on` nested in a runtime).
pub fn sync_tray(app: &AppHandle) {
    let state = app.state::<AppState>();
    let settings = tauri::async_runtime::block_on(state.store.get_settings());
    apply_tray(app, &settings);
}

/// Async tray sync — safe to call from `MonitorEngine` and other async paths.
pub async fn sync_tray_async(app: &AppHandle) {
    let state = app.state::<AppState>();
    let settings = state.store.get_settings().await;
    apply_tray(app, &settings);
}

fn apply_tray(app: &AppHandle, settings: &crate::store::AppSettings) {
    let state = app.state::<AppState>();
    let show = should_show_tray(
        settings.monitor.tray_enabled,
        state.monitor_engine.is_monitoring_active(),
    );

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_action_for_id_maps_known_items() {
        assert_eq!(
            tray_action_for_id("tray-open-dashboards"),
            TrayMenuAction::OpenDashboards
        );
        assert_eq!(
            tray_action_for_id("tray-toggle-pause"),
            TrayMenuAction::TogglePause
        );
        assert_eq!(tray_action_for_id("tray-quit"), TrayMenuAction::Quit);
        assert_eq!(tray_action_for_id("unknown"), TrayMenuAction::Ignore);
    }

    #[test]
    fn should_show_tray_requires_enabled_and_active() {
        assert!(!should_show_tray(false, true));
        assert!(!should_show_tray(true, false));
        assert!(should_show_tray(true, true));
    }

    #[test]
    fn should_close_main_to_tray_requires_all_flags() {
        assert!(should_close_main_to_tray(true, true, true));
        assert!(!should_close_main_to_tray(false, true, true));
        assert!(!should_close_main_to_tray(true, false, true));
        assert!(!should_close_main_to_tray(true, true, false));
    }

    #[test]
    fn tray_label_resolves_known_keys_for_en() {
        let quit = tray_label("en", "quit");
        assert!(!quit.is_empty());
        assert_ne!(quit, "quit");
    }

    #[test]
    fn tray_label_falls_back_to_key_when_missing() {
        assert_eq!(
            tray_label("en", "tray-nonexistent-key"),
            "tray-nonexistent-key"
        );
    }

    #[test]
    fn tray_pause_item_label_switches_on_paused_state() {
        let running = tray_pause_item_label("en", false);
        let paused = tray_pause_item_label("en", true);
        assert_ne!(running, paused);
        assert_eq!(running, "tray-pause-monitoring");
        assert_eq!(paused, "tray-resume-monitoring");
    }

    #[test]
    fn tray_labels_localized_for_zh_cn_when_present() {
        let en = tray_label("en", "file");
        let zh = tray_label("zh-CN", "file");
        assert_ne!(en, zh);
    }
}

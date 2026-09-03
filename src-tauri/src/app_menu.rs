//! Native macOS menu labels, action mapping, tree build, and event dispatch.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::commands::AppState;
use crate::tray;

#[cfg(target_os = "macos")]
use tauri::menu::{
    AboutMetadata, CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem,
    SubmenuBuilder,
};
#[cfg(target_os = "macos")]
use tauri::Emitter;
use tauri::Manager;

pub(crate) fn menu_labels(lang: &str) -> HashMap<String, String> {
    static MENU_JSON: &str = include_str!("../resources/menu-labels.json");

    let all: HashMap<String, HashMap<String, String>> =
        serde_json::from_str(MENU_JSON).expect("invalid menu-labels.json");

    all.get(lang)
        .or_else(|| all.get("en"))
        .cloned()
        .unwrap_or_default()
}

/// Resolve a single menu label key for the given locale (falls back to the key itself).
pub(crate) fn menu_label(lang: &str, key: &str) -> String {
    menu_labels(lang)
        .get(key)
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

/// Whether a theme submenu check item should be checked for the active theme mode.
pub(crate) fn theme_menu_item_checked(current_theme: &str, item_id: &str) -> bool {
    matches!(
        (current_theme, item_id),
        ("light", "theme-light") | ("dark", "theme-dark") | ("system", "theme-system")
    )
}

/// Pure mapping from native menu item id to an action the frontend or host should perform.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MenuAction {
    ThemeChange(String),
    Emit(&'static str),
    OpenDocs,
    OpenReportIssue,
    AddFavorite,
    OpenMigrationWindow(crate::commands::MigrationSubWindow),
    OpenLogDir,
    Ignore,
}

/// First caller wins. Used so `rebuild_menu` does not stack another `on_menu_event`.
pub(crate) fn take_once_slot(flag: &AtomicBool) -> bool {
    !flag.swap(true, Ordering::SeqCst)
}

/// Menu events that target the main workspace shell and must raise the main window first.
pub(crate) fn menu_emit_needs_main_focus(event: &str) -> bool {
    matches!(
        event,
        "menu:open-settings"
            | "menu:new-connection"
            | "menu:workflow"
            | "menu:dashboard"
            | "menu:export-config"
            | "menu:import-config"
            | "menu:export-connections"
            | "menu:import-connections-file"
            | "menu:import-connections-dbx"
            | "menu:import-connections-navicat"
            | "menu:import-connections-datagrip"
            | "menu:import-connections-dbeaver"
            | "menu:import-connections-tableplus"
    )
}

pub(crate) fn menu_action_for_id(id: &str) -> MenuAction {
    if let Some(theme) = id.strip_prefix("theme-") {
        return MenuAction::ThemeChange(theme.to_string());
    }
    match id {
        "open-settings" => MenuAction::Emit("menu:open-settings"),
        "new-connection" => MenuAction::Emit("menu:new-connection"),
        "schema-diff" => {
            MenuAction::OpenMigrationWindow(crate::commands::MigrationSubWindow::SchemaDiff)
        }
        "data-sync" => {
            MenuAction::OpenMigrationWindow(crate::commands::MigrationSubWindow::DataSync)
        }
        "data-transfer" => {
            MenuAction::OpenMigrationWindow(crate::commands::MigrationSubWindow::DataTransfer)
        }
        "workflow" => MenuAction::Emit("menu:workflow"),
        "dashboard" => MenuAction::Emit("menu:dashboard"),
        "backup" => MenuAction::OpenMigrationWindow(crate::commands::MigrationSubWindow::Backup),
        "restore" => MenuAction::OpenMigrationWindow(crate::commands::MigrationSubWindow::Restore),
        "export-config" => MenuAction::Emit("menu:export-config"),
        "import-config" => MenuAction::Emit("menu:import-config"),
        "export-connections" => MenuAction::Emit("menu:export-connections"),
        "import-connections" | "import-connections-file" => {
            MenuAction::Emit("menu:import-connections-file")
        }
        "import-connections-dbx" => MenuAction::Emit("menu:import-connections-dbx"),
        "import-connections-navicat" => MenuAction::Emit("menu:import-connections-navicat"),
        "import-connections-datagrip" => MenuAction::Emit("menu:import-connections-datagrip"),
        "import-connections-dbeaver" => MenuAction::Emit("menu:import-connections-dbeaver"),
        "import-connections-tableplus" => MenuAction::Emit("menu:import-connections-tableplus"),
        "view-logs" => MenuAction::OpenLogDir,
        "help-docs" => MenuAction::OpenDocs,
        "help-report" => MenuAction::OpenReportIssue,
        "ctx-add-favorite" => MenuAction::AddFavorite,
        _ => MenuAction::Ignore,
    }
}

/// Native menu event handler is registered once. `rebuild_menu` only replaces
/// the menu tree; stacking another `on_menu_event` opened duplicate docs windows.
#[cfg(target_os = "macos")]
mod native_menu {
    use super::*;
    use std::sync::Mutex;
    use tauri::menu::CheckMenuItem;
    use tauri::Wry;

    struct ThemeChecks {
        light: CheckMenuItem<Wry>,
        dark: CheckMenuItem<Wry>,
        system: CheckMenuItem<Wry>,
    }

    static THEME_CHECKS: Mutex<Option<ThemeChecks>> = Mutex::new(None);
    static HANDLER_REGISTERED: AtomicBool = AtomicBool::new(false);

    pub fn store_theme_checks(
        light: CheckMenuItem<Wry>,
        dark: CheckMenuItem<Wry>,
        system: CheckMenuItem<Wry>,
    ) {
        if let Ok(mut guard) = THEME_CHECKS.lock() {
            *guard = Some(ThemeChecks {
                light,
                dark,
                system,
            });
        }
    }

    fn apply_theme_checks(theme: &str) {
        let Ok(guard) = THEME_CHECKS.lock() else {
            return;
        };
        let Some(items) = guard.as_ref() else {
            return;
        };
        let _ = items
            .light
            .set_checked(theme_menu_item_checked(theme, "theme-light"));
        let _ = items
            .dark
            .set_checked(theme_menu_item_checked(theme, "theme-dark"));
        let _ = items
            .system
            .set_checked(theme_menu_item_checked(theme, "theme-system"));
    }

    pub fn register_handler_once(handle: &tauri::AppHandle) {
        if !take_once_slot(&HANDLER_REGISTERED) {
            return;
        }
        handle.on_menu_event(move |app_handle, event| {
            let id = event.id().as_ref();
            match menu_action_for_id(id) {
                MenuAction::ThemeChange(theme) => {
                    apply_theme_checks(&theme);
                    let _ = app_handle.emit("menu:theme-change", theme);
                }
                MenuAction::Emit(event) => {
                    if menu_emit_needs_main_focus(event) {
                        tray::focus_main_window(app_handle);
                    }
                    let _ = app_handle.emit(event, ());
                }
                MenuAction::OpenDocs => {
                    let app = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = crate::commands::open_docs_window(app, None).await {
                            tracing::warn!(error = %e, "menu help-docs: open docs URL failed");
                        }
                    });
                }
                MenuAction::OpenReportIssue => {
                    let _ = open::that("https://github.com/flyxl/datazen/issues/new");
                }
                MenuAction::AddFavorite => {
                    let _ = app_handle.emit("menu:add-favorite", ());
                }
                MenuAction::OpenMigrationWindow(kind) => {
                    let app = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) =
                            crate::commands::open_migration_sub_window(app, kind).await
                        {
                            tracing::warn!(error = %e, ?kind, "menu migration sub-window open failed");
                        }
                    });
                }
                MenuAction::OpenLogDir => {
                    let app = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let state = app.state::<AppState>();
                        if let Err(e) = crate::commands::open_log_dir(state).await {
                            tracing::warn!(error = %e, "menu view-logs: open log dir failed");
                        }
                    });
                }
                MenuAction::Ignore => {}
            }
        });
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn setup_menu(
    handle: &tauri::AppHandle,
    theme: &str,
    lang: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let t = |key: &str| menu_label(lang, key);

    // ── Shared items ──
    let theme_light = CheckMenuItemBuilder::new(t("theme-light"))
        .id("theme-light")
        .checked(theme_menu_item_checked(theme, "theme-light"))
        .build(handle)?;
    let theme_dark = CheckMenuItemBuilder::new(t("theme-dark"))
        .id("theme-dark")
        .checked(theme_menu_item_checked(theme, "theme-dark"))
        .build(handle)?;
    let theme_system = CheckMenuItemBuilder::new(t("theme-system"))
        .id("theme-system")
        .checked(theme_menu_item_checked(theme, "theme-system"))
        .build(handle)?;

    let settings_item = MenuItemBuilder::new(t("open-settings"))
        .id("open-settings")
        .accelerator("CmdOrCtrl+,")
        .build(handle)?;

    let new_conn_item = MenuItemBuilder::new(t("new-connection"))
        .id("new-connection")
        .accelerator("CmdOrCtrl+N")
        .build(handle)?;

    let export_config_item = MenuItemBuilder::new(t("export-config"))
        .id("export-config")
        .build(handle)?;
    let import_config_item = MenuItemBuilder::new(t("import-config"))
        .id("import-config")
        .build(handle)?;
    let export_connections_item = MenuItemBuilder::new(t("export-connections"))
        .id("export-connections")
        .build(handle)?;
    let import_connections_file_item = MenuItemBuilder::new(t("import-connections-file"))
        .id("import-connections-file")
        .build(handle)?;
    let import_dbx_item = MenuItemBuilder::new(t("import-connections-dbx"))
        .id("import-connections-dbx")
        .build(handle)?;
    let import_navicat_item = MenuItemBuilder::new(t("import-connections-navicat"))
        .id("import-connections-navicat")
        .build(handle)?;
    let import_datagrip_item = MenuItemBuilder::new(t("import-connections-datagrip"))
        .id("import-connections-datagrip")
        .build(handle)?;
    let import_dbeaver_item = MenuItemBuilder::new(t("import-connections-dbeaver"))
        .id("import-connections-dbeaver")
        .build(handle)?;
    let import_tableplus_item = MenuItemBuilder::new(t("import-connections-tableplus"))
        .id("import-connections-tableplus")
        .build(handle)?;
    let import_connections_menu = SubmenuBuilder::new(handle, t("import-connections"))
        .item(&import_dbx_item)
        .item(&import_navicat_item)
        .item(&import_datagrip_item)
        .item(&import_dbeaver_item)
        .item(&import_tableplus_item)
        .separator()
        .item(&import_connections_file_item)
        .build()?;

    let workflow_item = MenuItemBuilder::new(t("workflow"))
        .id("workflow")
        .build(handle)?;
    let dashboard_item = MenuItemBuilder::new(t("dashboard"))
        .id("dashboard")
        .build(handle)?;
    let backup_item = MenuItemBuilder::new(t("backup"))
        .id("backup")
        .build(handle)?;
    let restore_item = MenuItemBuilder::new(t("restore"))
        .id("restore")
        .build(handle)?;
    let view_logs_item = MenuItemBuilder::new(t("view-logs"))
        .id("view-logs")
        .build(handle)?;

    // ── Tools ──
    use crate::product_features;
    let mut tools_builder = SubmenuBuilder::new(handle, t("tools"));
    let mut migration_menu = false;
    if product_features::SCHEMA_DIFF_MENU {
        let schema_diff_item = MenuItemBuilder::new(t("schema-diff"))
            .id("schema-diff")
            .build(handle)?;
        tools_builder = tools_builder.item(&schema_diff_item);
        migration_menu = true;
    }
    if product_features::DATA_SYNC_MENU {
        let data_sync_item = MenuItemBuilder::new(t("data-sync"))
            .id("data-sync")
            .build(handle)?;
        tools_builder = tools_builder.item(&data_sync_item);
        migration_menu = true;
    }
    if product_features::DATA_TRANSFER_MENU {
        let data_transfer_item = MenuItemBuilder::new(t("data-transfer"))
            .id("data-transfer")
            .build(handle)?;
        tools_builder = tools_builder.item(&data_transfer_item);
        migration_menu = true;
    }
    if migration_menu {
        tools_builder = tools_builder.separator();
    }
    let tools_menu = tools_builder
        .item(&workflow_item)
        .item(&dashboard_item)
        .separator()
        .item(&backup_item)
        .item(&restore_item)
        .separator()
        .item(&view_logs_item)
        .build()?;

    let docs_item = MenuItemBuilder::new(t("documentation"))
        .id("help-docs")
        .build(handle)?;
    let report_item = MenuItemBuilder::new(t("report-issue"))
        .id("help-report")
        .build(handle)?;

    // ── DataZen (app menu) ──
    let app_menu = SubmenuBuilder::new(handle, t("app-name"))
        .about_with_text(
            t("about"),
            Some(AboutMetadata {
                name: Some("DataZen".into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                copyright: Some("© DataZen".into()),
                ..Default::default()
            }),
        )
        .separator()
        .item(&settings_item)
        .separator()
        .services_with_text(t("services"))
        .separator()
        .hide_with_text(t("hide"))
        .hide_others_with_text(t("hide-others"))
        .show_all_with_text(t("show-all"))
        .separator()
        .quit_with_text(t("quit"))
        .build()?;

    // ── File ──
    let file_menu = SubmenuBuilder::new(handle, t("file"))
        .item(&new_conn_item)
        .separator()
        .item(&export_config_item)
        .item(&import_config_item)
        .item(&export_connections_item)
        .item(&import_connections_menu)
        .build()?;

    // ── Edit ──
    // Restores ⌘X/⌘C/⌘V/⌘A for native inputs (settings, forms, search, …).
    // Undo/Redo are intentionally omitted: CodeMirror handles its own ⌘Z/⌘⇧Z
    // keymap, and the native undo/redo selectors do not target it.
    let edit_menu = SubmenuBuilder::new(handle, t("edit"))
        .item(&PredefinedMenuItem::cut(handle, Some(&t("cut")))?)
        .item(&PredefinedMenuItem::copy(handle, Some(&t("copy")))?)
        .item(&PredefinedMenuItem::paste(handle, Some(&t("paste")))?)
        .separator()
        .item(&PredefinedMenuItem::select_all(
            handle,
            Some(&t("select-all")),
        )?)
        .build()?;

    // ── View ──
    let theme_menu = SubmenuBuilder::new(handle, t("theme"))
        .items(&[&theme_light, &theme_dark, &theme_system])
        .build()?;
    let view_menu = SubmenuBuilder::new(handle, t("view"))
        .item(&theme_menu)
        .separator()
        .fullscreen_with_text(t("fullscreen"))
        .build()?;

    // ── Window ──
    let window_menu = SubmenuBuilder::new(handle, t("window"))
        .item(&PredefinedMenuItem::minimize(handle, Some(&t("minimize")))?)
        .item(&PredefinedMenuItem::maximize(handle, Some(&t("zoom")))?)
        .separator()
        .bring_all_to_front_with_text(t("bring-all-to-front"))
        .separator()
        .item(&PredefinedMenuItem::close_window(
            handle,
            Some(&t("close-window")),
        )?)
        .build()?;
    let _ = window_menu.set_as_windows_menu_for_nsapp();

    // ── Help ──
    let help_menu = SubmenuBuilder::new(handle, t("help"))
        .item(&docs_item)
        .separator()
        .item(&report_item)
        .build()?;

    let menu = MenuBuilder::new(handle)
        .items(&[
            &app_menu,
            &file_menu,
            &edit_menu,
            &view_menu,
            &tools_menu,
            &window_menu,
            &help_menu,
        ])
        .build()?;

    handle.set_menu(menu)?;

    native_menu::store_theme_checks(
        theme_light.clone(),
        theme_dark.clone(),
        theme_system.clone(),
    );
    native_menu::register_handler_once(handle);

    Ok(())
}

#[tauri::command]
pub(crate) async fn rebuild_menu(handle: tauri::AppHandle, language: String) -> Result<(), String> {
    let state = handle.state::<AppState>();
    state.prompt_resolver.ensure_ready(&language).await;
    #[cfg(target_os = "macos")]
    {
        let settings = state.store.get_settings().await;
        setup_menu(&handle, &settings.theme.mode, &language).map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn menu_labels_en_contains_core_keys() {
        let labels = menu_labels("en");
        assert!(labels.contains_key("app-name"));
        assert!(labels.contains_key("quit"));
        assert!(!labels["app-name"].is_empty());
    }

    #[test]
    fn menu_labels_localize_tools_submenu_across_locales() {
        let tools_keys = [
            "schema-diff",
            "data-sync",
            "data-transfer",
            "workflow",
            "dashboard",
            "backup",
            "restore",
            "view-logs",
        ];
        for lang in ["en", "zh-CN"] {
            let labels = menu_labels(lang);
            for key in tools_keys {
                let value = labels
                    .get(key)
                    .unwrap_or_else(|| panic!("menu-labels.json[{lang}] is missing key {key:?}"));
                assert!(
                    !value.is_empty(),
                    "menu-labels.json[{lang}][{key:?}] is empty"
                );
            }
        }
    }

    #[test]
    fn menu_labels_unknown_falls_back_to_en() {
        let fallback = menu_labels("xx-not-a-locale");
        let en = menu_labels("en");
        assert_eq!(fallback.get("app-name"), en.get("app-name"));
    }

    #[test]
    fn menu_labels_zh_cn_localizes_file_menu() {
        let en = menu_labels("en");
        let zh = menu_labels("zh-CN");
        assert_ne!(en.get("file"), zh.get("file"));
    }

    #[test]
    fn menu_label_resolves_or_falls_back() {
        assert_eq!(menu_label("en", "app-name"), menu_labels("en")["app-name"]);
        assert_eq!(menu_label("en", "missing-menu-key"), "missing-menu-key");
    }

    #[test]
    fn theme_menu_item_checked_matches_mode() {
        assert!(theme_menu_item_checked("light", "theme-light"));
        assert!(!theme_menu_item_checked("light", "theme-dark"));
        assert!(theme_menu_item_checked("dark", "theme-dark"));
        assert!(theme_menu_item_checked("system", "theme-system"));
        assert!(!theme_menu_item_checked("system", "theme-light"));
    }

    #[test]
    fn menu_action_for_id_maps_known_items() {
        assert_eq!(
            menu_action_for_id("theme-dark"),
            MenuAction::ThemeChange("dark".into())
        );
        assert_eq!(
            menu_action_for_id("open-settings"),
            MenuAction::Emit("menu:open-settings")
        );
        assert_eq!(menu_action_for_id("help-docs"), MenuAction::OpenDocs);
        assert_eq!(
            menu_action_for_id("help-report"),
            MenuAction::OpenReportIssue
        );
        assert_eq!(
            menu_action_for_id("ctx-add-favorite"),
            MenuAction::AddFavorite
        );
        assert_eq!(menu_action_for_id("unknown-id"), MenuAction::Ignore);
    }

    #[test]
    fn take_once_slot_only_first_caller_wins() {
        let flag = AtomicBool::new(false);
        assert!(take_once_slot(&flag));
        assert!(!take_once_slot(&flag));
        assert!(!take_once_slot(&flag));
    }

    #[test]
    fn menu_emit_needs_main_focus_for_shell_actions_only() {
        assert!(menu_emit_needs_main_focus("menu:open-settings"));
        assert!(menu_emit_needs_main_focus("menu:new-connection"));
        assert!(menu_emit_needs_main_focus("menu:workflow"));
        assert!(!menu_emit_needs_main_focus("menu:backup"));
    }

    #[test]
    fn menu_action_for_id_covers_shell_emit_events() {
        for id in [
            "open-settings",
            "new-connection",
            "workflow",
            "dashboard",
            "export-config",
            "import-config",
            "export-connections",
            "import-connections",
            "import-connections-file",
            "import-connections-dbx",
            "import-connections-navicat",
            "import-connections-datagrip",
            "import-connections-dbeaver",
            "import-connections-tableplus",
        ] {
            match menu_action_for_id(id) {
                MenuAction::Emit(_) => {}
                other => panic!("expected Emit for {id}, got {other:?}"),
            }
        }
    }

    #[test]
    fn menu_action_for_id_opens_migration_windows_in_rust() {
        use crate::commands::MigrationSubWindow;
        assert_eq!(
            menu_action_for_id("schema-diff"),
            MenuAction::OpenMigrationWindow(MigrationSubWindow::SchemaDiff)
        );
        assert_eq!(
            menu_action_for_id("data-sync"),
            MenuAction::OpenMigrationWindow(MigrationSubWindow::DataSync)
        );
        assert_eq!(
            menu_action_for_id("data-transfer"),
            MenuAction::OpenMigrationWindow(MigrationSubWindow::DataTransfer)
        );
        assert_eq!(
            menu_action_for_id("backup"),
            MenuAction::OpenMigrationWindow(MigrationSubWindow::Backup)
        );
        assert_eq!(
            menu_action_for_id("restore"),
            MenuAction::OpenMigrationWindow(MigrationSubWindow::Restore)
        );
        assert_eq!(menu_action_for_id("view-logs"), MenuAction::OpenLogDir);
    }
}

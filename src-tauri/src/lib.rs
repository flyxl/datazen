mod cache;
mod commands;
mod db;
mod services;
pub mod ssh_tunnel;
mod store;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{Emitter, Manager};
use commands::AppState;
use db::init_drivers;
use cache::SchemaCache;
use services::ConnectionManager;
use store::Store;

fn menu_labels(lang: &str) -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    if lang == "en" {
        m.insert("edit", "Edit");
        m.insert("view", "View");
        m.insert("tools", "Tools");
        m.insert("window", "Window");
        m.insert("theme-light", "Light");
        m.insert("theme-dark", "Dark");
        m.insert("theme-system", "System");
        m.insert("open-settings", "Settings…");
        m.insert("new-connection", "New Connection");
        m.insert("data-sync", "Data Sync");
    } else {
        m.insert("edit", "编辑");
        m.insert("view", "显示");
        m.insert("tools", "工具");
        m.insert("window", "窗口");
        m.insert("theme-light", "浅色主题");
        m.insert("theme-dark", "深色主题");
        m.insert("theme-system", "跟随系统");
        m.insert("open-settings", "偏好设置…");
        m.insert("new-connection", "新建连接");
        m.insert("data-sync", "数据同步");
    }
    m
}

fn build_app_menu(
    app: &tauri::App,
    theme: &str,
    lang: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let l = menu_labels(lang);

    let theme_light = CheckMenuItemBuilder::new(l["theme-light"])
        .id("theme-light")
        .checked(theme == "light")
        .build(app)?;
    let theme_dark = CheckMenuItemBuilder::new(l["theme-dark"])
        .id("theme-dark")
        .checked(theme == "dark")
        .build(app)?;
    let theme_system = CheckMenuItemBuilder::new(l["theme-system"])
        .id("theme-system")
        .checked(theme == "system")
        .build(app)?;

    let settings_item = MenuItemBuilder::new(l["open-settings"])
        .id("open-settings")
        .accelerator("CmdOrCtrl+,")
        .build(app)?;

    let new_conn_item = MenuItemBuilder::new(l["new-connection"])
        .id("new-connection")
        .accelerator("CmdOrCtrl+N")
        .build(app)?;

    let data_sync_item = MenuItemBuilder::new(l["data-sync"])
        .id("data-sync")
        .build(app)?;

    let edit_menu = SubmenuBuilder::new(app, l["edit"])
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let view_menu = SubmenuBuilder::new(app, l["view"])
        .items(&[&theme_light, &theme_dark, &theme_system])
        .separator()
        .item(&settings_item)
        .build()?;

    let tools_menu = SubmenuBuilder::new(app, l["tools"])
        .item(&new_conn_item)
        .item(&data_sync_item)
        .build()?;

    let window_menu = SubmenuBuilder::new(app, l["window"])
        .minimize()
        .close_window()
        .build()?;

    let menu = MenuBuilder::new(app)
        .items(&[&edit_menu, &view_menu, &tools_menu, &window_menu])
        .build()?;

    app.set_menu(menu)?;

    let tl = theme_light.clone();
    let td = theme_dark.clone();
    let ts = theme_system.clone();

    app.on_menu_event(move |app_handle, event| {
        let id = event.id().as_ref();
        if let Some(theme) = id.strip_prefix("theme-") {
            let _ = tl.set_checked(id == "theme-light");
            let _ = td.set_checked(id == "theme-dark");
            let _ = ts.set_checked(id == "theme-system");
            let _ = app_handle.emit("menu:theme-change", theme);
        }
        match id {
            "open-settings" => { let _ = app_handle.emit("menu:open-settings", ()); }
            "new-connection" => { let _ = app_handle.emit("menu:new-connection", ()); }
            "data-sync" => { let _ = app_handle.emit("menu:data-sync", ()); }
            _ => {}
        }
    });

    Ok(())
}

fn rebuild_menu_for_handle(
    handle: &tauri::AppHandle,
    lang: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let l = menu_labels(lang);

    let state = handle.state::<AppState>();
    let settings = tauri::async_runtime::block_on(state.store.get_settings());
    let theme = &settings.theme;

    let theme_light = CheckMenuItemBuilder::new(l["theme-light"])
        .id("theme-light")
        .checked(theme == "light")
        .build(handle)?;
    let theme_dark = CheckMenuItemBuilder::new(l["theme-dark"])
        .id("theme-dark")
        .checked(theme == "dark")
        .build(handle)?;
    let theme_system = CheckMenuItemBuilder::new(l["theme-system"])
        .id("theme-system")
        .checked(theme == "system")
        .build(handle)?;

    let settings_item = MenuItemBuilder::new(l["open-settings"])
        .id("open-settings")
        .accelerator("CmdOrCtrl+,")
        .build(handle)?;

    let new_conn_item = MenuItemBuilder::new(l["new-connection"])
        .id("new-connection")
        .accelerator("CmdOrCtrl+N")
        .build(handle)?;

    let data_sync_item = MenuItemBuilder::new(l["data-sync"])
        .id("data-sync")
        .build(handle)?;

    let edit_menu = SubmenuBuilder::new(handle, l["edit"])
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let view_menu = SubmenuBuilder::new(handle, l["view"])
        .items(&[&theme_light, &theme_dark, &theme_system])
        .separator()
        .item(&settings_item)
        .build()?;

    let tools_menu = SubmenuBuilder::new(handle, l["tools"])
        .item(&new_conn_item)
        .item(&data_sync_item)
        .build()?;

    let window_menu = SubmenuBuilder::new(handle, l["window"])
        .minimize()
        .close_window()
        .build()?;

    let menu = MenuBuilder::new(handle)
        .items(&[&edit_menu, &view_menu, &tools_menu, &window_menu])
        .build()?;

    handle.set_menu(menu)?;

    let tl = theme_light.clone();
    let td = theme_dark.clone();
    let ts = theme_system.clone();

    handle.on_menu_event(move |app_handle, event| {
        let id = event.id().as_ref();
        if let Some(theme) = id.strip_prefix("theme-") {
            let _ = tl.set_checked(id == "theme-light");
            let _ = td.set_checked(id == "theme-dark");
            let _ = ts.set_checked(id == "theme-system");
            let _ = app_handle.emit("menu:theme-change", theme);
        }
        match id {
            "open-settings" => { let _ = app_handle.emit("menu:open-settings", ()); }
            "new-connection" => { let _ = app_handle.emit("menu:new-connection", ()); }
            "data-sync" => { let _ = app_handle.emit("menu:data-sync", ()); }
            _ => {}
        }
    });

    Ok(())
}

#[tauri::command]
fn rebuild_menu(handle: tauri::AppHandle, language: String) -> Result<(), String> {
    rebuild_menu_for_handle(&handle, &language).map_err(|e| e.to_string())
}

/// Entry point invoked by `main.rs`.
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init());

    #[cfg(feature = "webdriver")]
    let builder = builder.plugin(tauri_plugin_webdriver::init());

    let t_builder = Instant::now();
    tracing::info!("[startup] builder created");

    builder
        .setup(move |app| {
            let builder_elapsed = t_builder.elapsed();
            let t_setup = Instant::now();
            tracing::info!("[startup] setup begin (builder took {:?})", builder_elapsed);

            let handle = app.handle().clone();

            let t0 = Instant::now();
            let app_state = tauri::async_runtime::block_on(async {
                let t_drv = Instant::now();
                let registry = Arc::new(init_drivers().await);
                tracing::info!("[startup]   init_drivers: {:?}", t_drv.elapsed());

                let t_store = Instant::now();
                let store = Arc::new(Store::init(&handle).await.map_err(|e| e.to_string())?);
                tracing::info!("[startup]   Store::init: {:?}", t_store.elapsed());

                let schema_cache = Arc::new(SchemaCache::new(registry.clone()));
                let connection_manager = Arc::new(ConnectionManager::new(
                    registry.clone(),
                    store.clone(),
                ));

                connection_manager.clone().start_cleanup_task();

                Ok::<AppState, String>(AppState {
                    driver_registry: registry,
                    connection_manager,
                    store,
                    schema_cache,
                })
            })?;
            tracing::info!("[startup]   block_on total: {:?}", t0.elapsed());

            std::thread::spawn(|| {
                let mut sys = sysinfo::System::new();
                sys.refresh_memory();
                tracing::info!(
                    used_mib = sys.used_memory() / 1024 / 1024,
                    total_mib = sys.total_memory() / 1024 / 1024,
                    "Host memory snapshot"
                );
            });

            let t_theme = Instant::now();
            let initial_settings = tauri::async_runtime::block_on(
                app_state.store.get_settings(),
            );
            tracing::info!("[startup]   get_settings: {:?}", t_theme.elapsed());

            app.manage(app_state);

            let t_menu = Instant::now();
            build_app_menu(app, &initial_settings.theme, &initial_settings.language)?;
            tracing::info!("[startup]   build menu: {:?}", t_menu.elapsed());

            tracing::info!("[startup] setup complete: {:?}", t_setup.elapsed());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_connections,
            commands::save_connection,
            commands::delete_connection,
            commands::get_groups,
            commands::save_groups,
            commands::test_connection,
            commands::connect,
            commands::disconnect,
            commands::get_connection_info,
            commands::get_databases,
            commands::get_tables,
            commands::get_table_schema,
            commands::get_table_data,
            commands::execute_query,
            commands::get_explain,
            commands::cancel_query,
            commands::get_query_history,
            commands::clear_query_history,
            commands::get_settings,
            commands::save_settings,
            commands::write_file,
            commands::read_file,
            commands::backup_database,
            commands::restore_database,
            commands::compare_databases,
            commands::sync_table,
            commands::sync_tables,
            commands::get_sync_tasks,
            commands::save_sync_task_direct,
            commands::delete_sync_task,
            commands::check_sync_conflicts,
            rebuild_menu,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

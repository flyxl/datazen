mod cache;
mod commands;
mod db;
mod services;
mod store;

use std::sync::Arc;
use std::time::Instant;

use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{Emitter, Manager};
use commands::AppState;
use db::init_drivers;
use cache::SchemaCache;
use services::ConnectionManager;
use store::Store;

/// Entry point invoked by `main.rs`.
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init());

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
            let initial_theme = tauri::async_runtime::block_on(
                app_state.store.get_settings(),
            ).theme;
            tracing::info!("[startup]   get_settings: {:?}", t_theme.elapsed());

            app.manage(app_state);

            let t_menu = Instant::now();
            // ── Build native menu bar ──
            let theme_light = CheckMenuItemBuilder::new("浅色主题")
                .id("theme-light")
                .checked(initial_theme == "light")
                .build(app)?;
            let theme_dark = CheckMenuItemBuilder::new("深色主题")
                .id("theme-dark")
                .checked(initial_theme == "dark")
                .build(app)?;
            let theme_system = CheckMenuItemBuilder::new("跟随系统")
                .id("theme-system")
                .checked(initial_theme == "system")
                .build(app)?;

            let settings_item = MenuItemBuilder::new("偏好设置…")
                .id("open-settings")
                .accelerator("CmdOrCtrl+,")
                .build(app)?;

            let edit_menu = SubmenuBuilder::new(app, "编辑")
                .undo()
                .redo()
                .separator()
                .cut()
                .copy()
                .paste()
                .select_all()
                .build()?;

            let view_menu = SubmenuBuilder::new(app, "显示")
                .items(&[&theme_light, &theme_dark, &theme_system])
                .separator()
                .item(&settings_item)
                .build()?;

            let window_menu = SubmenuBuilder::new(app, "窗口")
                .minimize()
                .close_window()
                .build()?;

            let menu = MenuBuilder::new(app)
                .items(&[&edit_menu, &view_menu, &window_menu])
                .build()?;

            app.set_menu(menu)?;
            tracing::info!("[startup]   build menu: {:?}", t_menu.elapsed());

            // ── Handle menu events ──
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
                if id == "open-settings" {
                    let _ = app_handle.emit("menu:open-settings", ());
                }
            });

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub mod ai;
mod app_data_archive;
mod dashboard;
mod monitor;
mod theme;
mod cache;
mod commands;
pub mod db;
mod i18n_locale;
pub mod mcp;
mod plugin_init;
mod services;
mod ssh_known_hosts;
pub mod ssh_tunnel;
mod store;
mod tray;
pub mod sync;
pub mod schema_diff;
pub mod workflow;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use std::collections::HashMap;
#[cfg(target_os = "macos")]
use tauri::menu::{
    AboutMetadata, CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem,
    SubmenuBuilder,
};
#[cfg(target_os = "macos")]
use tauri::Emitter;
use tauri::Manager;
use ai::SchemaContextBuilder;
use commands::AppState;
use db::init_drivers;
use cache::SchemaCache;
use monitor::MonitorEngine;
use services::ConnectionManager;
use store::Store;
use sync::adapter_registry::SyncAdapterRegistry;

pub(crate) fn menu_labels(lang: &str) -> HashMap<String, String> {
    static MENU_JSON: &str = include_str!("../resources/menu-labels.json");

    let all: HashMap<String, HashMap<String, String>> =
        serde_json::from_str(MENU_JSON).expect("invalid menu-labels.json");

    all.get(lang)
        .or_else(|| all.get("en"))
        .cloned()
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
fn setup_menu(
    handle: &tauri::AppHandle,
    theme: &str,
    lang: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let l = menu_labels(lang);
    let t = |key: &str| -> String {
        l.get(key).cloned().unwrap_or_else(|| key.to_string())
    };

    // ── Shared items ──
    let theme_light = CheckMenuItemBuilder::new(t("theme-light"))
        .id("theme-light")
        .checked(theme == "light")
        .build(handle)?;
    let theme_dark = CheckMenuItemBuilder::new(t("theme-dark"))
        .id("theme-dark")
        .checked(theme == "dark")
        .build(handle)?;
    let theme_system = CheckMenuItemBuilder::new(t("theme-system"))
        .id("theme-system")
        .checked(theme == "system")
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
    let import_connections_item = MenuItemBuilder::new(t("import-connections"))
        .id("import-connections")
        .build(handle)?;

    let data_sync_item = MenuItemBuilder::new(t("data-sync"))
        .id("data-sync")
        .build(handle)?;
    let schema_diff_item = MenuItemBuilder::new(t("schema-diff"))
        .id("schema-diff")
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
        .item(&import_connections_item)
        .build()?;

    // Edit menu omitted: predefined undo/redo/cut/copy do not target CodeMirror;
    // keep locale keys for a future editor-aware Edit menu.

    // ── View ──
    let theme_menu = SubmenuBuilder::new(handle, t("theme"))
        .items(&[&theme_light, &theme_dark, &theme_system])
        .build()?;
    let view_menu = SubmenuBuilder::new(handle, t("view"))
        .item(&theme_menu)
        .separator()
        .fullscreen_with_text(t("fullscreen"))
        .build()?;

    // ── Tools ──
    let tools_menu = SubmenuBuilder::new(handle, t("tools"))
        .item(&data_sync_item)
        .item(&schema_diff_item)
        .separator()
        .item(&backup_item)
        .item(&restore_item)
        .separator()
        .item(&view_logs_item)
        .build()?;

    // ── Window ──
    let window_menu = SubmenuBuilder::new(handle, t("window"))
        .item(&PredefinedMenuItem::minimize(handle, Some(&t("minimize")))?)
        .item(&PredefinedMenuItem::maximize(handle, Some(&t("zoom")))?)
        .separator()
        .bring_all_to_front_with_text(t("bring-all-to-front"))
        .separator()
        .item(&PredefinedMenuItem::close_window(handle, Some(&t("close-window")))?)
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
            &view_menu,
            &tools_menu,
            &window_menu,
            &help_menu,
        ])
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
            return;
        }
        match id {
            "open-settings" => {
                let _ = app_handle.emit("menu:open-settings", ());
            }
            "new-connection" => {
                let _ = app_handle.emit("menu:new-connection", ());
            }
            "data-sync" => {
                let _ = app_handle.emit("menu:data-sync", ());
            }
            "schema-diff" => {
                let _ = app_handle.emit("menu:schema-diff", ());
            }
            "backup" => {
                let _ = app_handle.emit("menu:backup", ());
            }
            "restore" => {
                let _ = app_handle.emit("menu:restore", ());
            }
            "export-config" => {
                let _ = app_handle.emit("menu:export-config", ());
            }
            "import-config" => {
                let _ = app_handle.emit("menu:import-config", ());
            }
            "export-connections" => {
                let _ = app_handle.emit("menu:export-connections", ());
            }
            "import-connections" => {
                let _ = app_handle.emit("menu:import-connections", ());
            }
            "view-logs" => {
                let _ = app_handle.emit("menu:view-logs", ());
            }
            "help-docs" => {
                // Open directly in Rust — do not emit to every webview (that
                // previously caused concurrent create_sub_window races).
                let app = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = commands::open_docs_window(app, None).await {
                        tracing::warn!(error = %e, "menu help-docs: open docs window failed");
                    }
                });
            }
            "help-report" => {
                let _ = open::that("https://github.com/flyxl/datazen/issues/new");
            }
            "ctx-add-favorite" => {
                let _ = app_handle.emit("menu:add-favorite", ());
            }
            _ => {}
        }
    });

    Ok(())
}

fn resolve_log_settings() -> (String, PathBuf) {
    let data_dir = store::Store::default_app_data_dir().unwrap_or_else(|_| PathBuf::from("."));

    let settings_path = data_dir.join("settings.json");

    let (level, custom_path) = std::fs::read_to_string(&settings_path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .map(|v| {
            let level = v
                .get("logLevel")
                .and_then(|v| v.as_str())
                .unwrap_or("info")
                .to_string();
            let path = v
                .get("logPath")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            (level, path)
        })
        .unwrap_or_else(|| ("info".to_string(), String::new()));

    let log_dir = if custom_path.is_empty() {
        data_dir.join("logs")
    } else {
        PathBuf::from(custom_path)
    };

    (level, log_dir)
}

#[tauri::command]
async fn rebuild_menu(handle: tauri::AppHandle, language: String) -> Result<(), String> {
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

/// Build AppState for the GUI: Store first, then preload drivers used by saved connections.
/// Sync adapters / AI / prompts / workflows / history / MCP load on demand.
async fn build_gui_app_state(
    handle: &tauri::AppHandle,
    prompts_dir: Option<PathBuf>,
) -> Result<AppState, String> {
    let t_core = Instant::now();
    let store = Arc::new(Store::init(handle).await.map_err(|e| e.to_string())?);
    tracing::info!("[startup]   store: {:?}", t_core.elapsed());

    let t_drv = Instant::now();
    let registry = Arc::new(init_drivers());
    let needed: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        store
            .get_connections()
            .await
            .into_iter()
            .filter_map(|c| seen.insert(c.database_type.clone()).then_some(c.database_type))
            .collect()
    };
    registry.ensure_types(&needed).await;
    tracing::info!(
        "[startup]   drivers ({} types): {:?}",
        needed.len(),
        t_drv.elapsed()
    );

    Ok(finish_app_state(
        store,
        registry,
        Arc::new(SyncAdapterRegistry::new()),
        prompts_dir,
    ))
}

/// Build AppState for headless MCP (shells only; AI/prompts/sync load on first use).
async fn build_app_state(
    store: Arc<Store>,
    prompts_dir: Option<PathBuf>,
) -> Result<AppState, String> {
    let t_drv = Instant::now();
    let registry = Arc::new(init_drivers());
    let needed: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        store
            .get_connections()
            .await
            .into_iter()
            .filter_map(|c| seen.insert(c.database_type.clone()).then_some(c.database_type))
            .collect()
    };
    registry.ensure_types(&needed).await;
    tracing::info!(
        "[startup]   drivers ({} types): {:?}",
        needed.len(),
        t_drv.elapsed()
    );
    Ok(finish_app_state(
        store,
        registry,
        Arc::new(SyncAdapterRegistry::new()),
        prompts_dir,
    ))
}

fn finish_app_state(
    store: Arc<Store>,
    registry: Arc<db::DriverRegistry>,
    sync_adapters: Arc<sync::adapter_registry::SyncAdapterRegistry>,
    prompts_dir: Option<PathBuf>,
) -> AppState {
    let schema_cache = Arc::new(SchemaCache::new(registry.clone()));
    let connection_manager = Arc::new(ConnectionManager::new(
        registry.clone(),
        store.clone(),
    ));
    connection_manager.clone().start_cleanup_task();
    let monitor_connections = Arc::new(monitor::MonitorConnectionRegistry::new(
        connection_manager.clone(),
    ));
    let monitor_engine = MonitorEngine::new(store.clone(), monitor_connections.clone());

    let data_dir = store.data_dir().to_path_buf();

    // AI / prompts / workflows / history / MCP client: empty shells.
    // Nothing here touches disk or network — window can show immediately.
    AppState {
        driver_registry: registry,
        connection_manager: connection_manager.clone(),
        monitor_connections,
        monitor_engine,
        store,
        schema_cache: schema_cache.clone(),
        sync_adapters,
        ai_registry: Arc::new(ai::AiProviderRegistry::new()),
        schema_context_builder: Arc::new(SchemaContextBuilder::new(
            schema_cache,
            connection_manager,
        )),
        prompt_resolver: Arc::new(ai::PromptResolver::new(&data_dir, prompts_dir)),
        workflow_registry: Arc::new(workflow::WorkflowRegistry::new(data_dir.join("workflows"))),
        workflow_history: Arc::new(workflow::WorkflowHistoryManager::new(
            data_dir.join("workflow_history"),
        )),
        mcp_client_manager: Arc::new(mcp::McpClientManager::new()),
    }
}

/// Run as a headless MCP stdio server (invoked with `--mcp`).
pub fn run_mcp_stdio() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");

    rt.block_on(async {
        let data_dir = Store::default_app_data_dir().expect("Cannot determine data dir");
        let store = Arc::new(
            Store::init_with_path(&data_dir).await.expect("Failed to init store"),
        );
        let prompts_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/prompts");
        let app_state = match build_app_state(store, Some(prompts_dir)).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to initialize AppState: {e}");
                std::process::exit(1);
            }
        };

        let cancel = tokio_util::sync::CancellationToken::new();
        mcp::start_mcp_stdio(Arc::new(app_state), cancel).await;
    });
}

/// Entry point invoked by `main.rs`.
pub fn run() {
    let (log_level, log_dir) = resolve_log_settings();

    let _ = std::fs::create_dir_all(&log_dir);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "datazen.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&log_level));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
        .init();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_os::init());

    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init());

    #[cfg(feature = "webdriver")]
    let builder = builder.plugin(tauri_plugin_webdriver::init());

    let builder = plugin_init::register_plugins(builder);

    let t_builder = Instant::now();
    tracing::info!("[startup] builder created");

    builder
        .setup(move |app| {
            let builder_elapsed = t_builder.elapsed();
            let t_setup = Instant::now();
            tracing::info!("[startup] setup begin (builder took {:?})", builder_elapsed);

            let handle = app.handle().clone();

            let t0 = Instant::now();
            let prompts_dir = handle.path().resource_dir().ok().map(|d| d.join("prompts"));
            let app_state = tauri::async_runtime::block_on(build_gui_app_state(&handle, prompts_dir))?;
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

            app.manage(app_state);

            {
                let state = handle.state::<AppState>();
                state.monitor_engine.start(handle.clone());
                tray::sync_tray(&handle);
            }

            let _ = app.get_webview_window("main");

            // Optional embedded MCP: only if user explicitly enabled it in settings (default off).
            {
                let state = handle.state::<AppState>();
                let enabled = tauri::async_runtime::block_on(state.store.get_settings()).mcp_server_enabled;
                if enabled {
                    if let Err(e) = tauri::async_runtime::block_on(commands::start_embedded_mcp(state.inner())) {
                        tracing::warn!(error = %e, "Failed to auto-start embedded MCP Server");
                    }
                }
            }

            #[cfg(target_os = "macos")]
            {
                let t_settings = Instant::now();
                let initial_settings = tauri::async_runtime::block_on(
                    handle.state::<AppState>().store.get_settings(),
                );
                tracing::info!("[startup]   get_settings: {:?}", t_settings.elapsed());
                let t_menu = Instant::now();
                setup_menu(&handle, &initial_settings.theme.mode, &initial_settings.language)?;
                tracing::info!("[startup]   build menu: {:?}", t_menu.elapsed());
            }

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
            commands::ping_connection,
            commands::disconnect,
            commands::get_connection_info,
            commands::get_available_drivers,
            commands::get_databases,
            commands::use_database,
            commands::get_tables,
            commands::kv_scan_keys,
            commands::kv_get_key,
            commands::get_columns,
            commands::get_table_schema,
            commands::get_structure_capabilities,
            commands::plan_table_structure_changes,
            commands::get_er_data,
            commands::get_table_data,
            commands::commit_row_updates,
            commands::execute_query,
            commands::get_explain,
            commands::cancel_query,
            commands::get_query_history,
            commands::clear_query_history,
            commands::get_favorite_queries,
            commands::add_favorite_query,
            commands::delete_favorite_query,
            commands::show_editor_context_menu,
            commands::get_settings,
            commands::get_system_ui_language,
            commands::save_settings,
            commands::get_log_path,
            commands::open_path,
            commands::open_log_dir,
            commands::open_workflows_dir,
            commands::open_context_dir,
            commands::export_connections,
            commands::export_connections_with_dialog,
            commands::import_connections_preview,
            commands::import_connections_with_dialog,
            commands::export_app_data,
            commands::export_app_data_with_dialog,
            commands::import_app_data,
            commands::import_app_data_with_dialog,
            commands::save_encryption_key_with_dialog,
            commands::restart_app,
            commands::write_file,
            commands::write_file_base64,
            commands::read_file,
            commands::save_text_with_dialog,
            commands::save_base64_with_dialog,
            commands::open_text_with_dialog,
            commands::open_base64_with_dialog,
            commands::backup_database,
            commands::backup_database_with_dialog,
            commands::restore_database,
            commands::restore_database_with_dialog,
            commands::compare_databases,
            commands::compare_table_schemas,
            commands::compare_table_data,
            commands::prepare_schema_diff_plan,
            commands::execute_schema_diff_deploy,
            commands::sync_table,
            commands::sync_tables,
            commands::get_sync_tasks,
            commands::save_sync_task_direct,
            commands::delete_sync_task,
            commands::check_sync_conflicts,
            commands::ai_get_providers,
            commands::ai_fetch_remote_models,
            commands::ai_validate_config,
            commands::ai_save_config,
            commands::ai_get_config,
            commands::ai_delete_config,
            commands::ai_generate_sql,
            commands::ai_diagnose_error,
            commands::ai_analyze_explain,
            commands::ai_chat,
            commands::ai_parse_filter,
            commands::mcp_get_status,
            commands::mcp_start_stdio,
            commands::mcp_stop,
            commands::mcp_list_all_tools,
            commands::workflow_list,
            commands::workflow_execute,
            commands::workflow_save,
            commands::workflow_delete,
            commands::workflow_reload,
            commands::workflow_get_dir,
            commands::workflow_get,
            commands::ai_generate_schema_doc,
            commands::ai_diagnose_connection,
            commands::ai_analyze_queries,
            commands::mcp_client_connect,
            commands::mcp_client_disconnect,
            commands::mcp_client_list,
            commands::mcp_client_tools,
            commands::mcp_client_call_tool,
            commands::create_sub_window,
            commands::adb_list_packages,
            commands::adb_list_databases,
            commands::adb_pull_database,
            commands::adb_pull_database_with_dialog,
            commands::prompt_list,
            commands::prompt_set_override,
            commands::prompt_remove_override,
            commands::context_get_dir,
            commands::context_list_files,
            commands::context_read_files,
            commands::workflow_history_list,
            commands::workflow_history_get,
            commands::workflow_history_clear,
            commands::list_theme_packs,
            commands::install_theme_pack_with_dialog,
            commands::remove_theme_pack,
            commands::read_theme_pack_file,
            commands::list_dashboards,
            commands::get_dashboard,
            commands::save_dashboard,
            commands::delete_dashboard,
            commands::list_widget_runs,
            commands::get_widget_run,
            commands::run_dashboard_widget,
            commands::export_dashboard_with_dialog,
            commands::import_dashboard_with_dialog,
            commands::get_monitor_paused,
            commands::set_monitor_paused,
            rebuild_menu,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" && tray::should_close_to_tray(window.app_handle()) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            #[cfg(target_os = "macos")]
            if let tauri::WindowEvent::Resized(size) = event {
                let win = window.clone();
                let size = *size;
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    if let Some(monitor) = win.current_monitor().ok().flatten() {
                        let mon = monitor.size();
                        let is_fs = size.width >= mon.width && size.height >= mon.height;
                        let _ = win.emit("fullscreen-changed", is_fs);
                    }
                });
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if tray::should_prevent_exit(app_handle) {
                    api.prevent_exit();
                    return;
                }
                let state = app_handle.state::<AppState>();
                let mgr = state.mcp_client_manager.clone();
                tauri::async_runtime::block_on(mgr.disconnect_all());
            }
        });
}

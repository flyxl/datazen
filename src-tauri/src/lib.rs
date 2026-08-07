pub mod ai;
mod app_data_archive;
mod cache;
mod commands;
mod db;
mod i18n_locale;
pub mod mcp;
mod plugin_init;
mod services;
mod ssh_known_hosts;
pub mod ssh_tunnel;
mod store;
pub mod sync;
pub mod workflow;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use std::collections::HashMap;
#[cfg(target_os = "macos")]
use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
#[cfg(target_os = "macos")]
use tauri::Emitter;
use tauri::Manager;
use ai::SchemaContextBuilder;
use commands::AppState;
use db::init_drivers;
use cache::SchemaCache;
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

    let data_sync_item = MenuItemBuilder::new(t("data-sync"))
        .id("data-sync")
        .build(handle)?;

    let export_config_item = MenuItemBuilder::new(t("export-config"))
        .id("export-config")
        .build(handle)?;

    let import_config_item = MenuItemBuilder::new(t("import-config"))
        .id("import-config")
        .build(handle)?;

    let view_logs_item = MenuItemBuilder::new(t("view-logs"))
        .id("view-logs")
        .build(handle)?;

    let edit_menu = SubmenuBuilder::new(handle, t("edit"))
        .item(&PredefinedMenuItem::undo(handle, Some(&t("undo")))?)
        .item(&PredefinedMenuItem::redo(handle, Some(&t("redo")))?)
        .separator()
        .item(&PredefinedMenuItem::cut(handle, Some(&t("cut")))?)
        .item(&PredefinedMenuItem::copy(handle, Some(&t("copy")))?)
        .item(&PredefinedMenuItem::paste(handle, Some(&t("paste")))?)
        .item(&PredefinedMenuItem::select_all(handle, Some(&t("select-all")))?)
        .build()?;

    let view_menu = SubmenuBuilder::new(handle, t("view"))
        .items(&[&theme_light, &theme_dark, &theme_system])
        .separator()
        .item(&settings_item)
        .build()?;

    let tools_menu = SubmenuBuilder::new(handle, t("tools"))
        .item(&new_conn_item)
        .item(&data_sync_item)
        .separator()
        .item(&view_logs_item)
        .separator()
        .item(&export_config_item)
        .item(&import_config_item)
        .build()?;

    let window_menu = SubmenuBuilder::new(handle, t("window"))
        .item(&PredefinedMenuItem::minimize(handle, Some(&t("minimize")))?)
        .item(&PredefinedMenuItem::close_window(handle, Some(&t("close-window")))?)
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
            "export-config" => { let _ = app_handle.emit("menu:export-config", ()); }
            "import-config" => { let _ = app_handle.emit("menu:import-config", ()); }
            "view-logs" => { let _ = app_handle.emit("menu:view-logs", ()); }
            "ctx-add-favorite" => { let _ = app_handle.emit("menu:add-favorite", ()); }
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
        setup_menu(&handle, &settings.theme, &language).map_err(|e| e.to_string())
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

    let data_dir = store.data_dir().to_path_buf();

    // AI / prompts / workflows / history / MCP client: empty shells.
    // Nothing here touches disk or network — window can show immediately.
    AppState {
        driver_registry: registry,
        connection_manager: connection_manager.clone(),
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
        .plugin(tauri_plugin_os::init());

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
                setup_menu(&handle, &initial_settings.theme, &initial_settings.language)?;
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
            commands::get_tables,
            commands::kv_scan_keys,
            commands::kv_get_key,
            commands::get_columns,
            commands::get_table_schema,
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
            commands::import_connections_preview,
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
            commands::backup_database,
            commands::backup_database_with_dialog,
            commands::restore_database,
            commands::restore_database_with_dialog,
            commands::compare_databases,
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
            rebuild_menu,
        ])
        .on_window_event(|_window, _event| {
            #[cfg(target_os = "macos")]
            if let tauri::WindowEvent::Resized(size) = _event {
                let win = _window.clone();
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
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let state = app_handle.state::<AppState>();
                let mgr = state.mcp_client_manager.clone();
                tauri::async_runtime::block_on(mgr.disconnect_all());
            }
        });
}

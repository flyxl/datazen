//! Application bootstrap: logging, AppState assembly, GUI/MCP entry points.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::ai::SchemaContextBuilder;
use crate::cache::SchemaCache;
use crate::commands::AppState;
use crate::db::init_drivers;
use crate::monitor::MonitorEngine;
use crate::services::ConnectionManager;
use crate::store::Store;
use crate::transfer::adapter_registry::SyncAdapterRegistry;
use tauri::Emitter;
use tauri::Manager;

#[cfg(target_os = "macos")]
use crate::app_menu::setup_menu;
use crate::driver_init;
use crate::extensions;
use crate::mcp;
use crate::redis_flush_gate;
use crate::theme;
use crate::tray;
use crate::workflow;

/// Collect unique driver type ids from saved connections (stable insertion order).
pub(crate) fn unique_driver_types(connections: &[crate::db::ConnectionConfig]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    connections
        .iter()
        .filter_map(|c| {
            seen.insert(c.database_type.clone())
                .then_some(c.database_type.clone())
        })
        .collect()
}

/// Parse log level and custom log path from a settings JSON value.
pub(crate) fn parse_log_settings_fields(v: &serde_json::Value) -> (String, String) {
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
}

/// Resolve the directory used for application logs.
pub(crate) fn resolve_log_dir(data_dir: &std::path::Path, custom_log_path: &str) -> PathBuf {
    if custom_log_path.is_empty() {
        data_dir.join("logs")
    } else {
        PathBuf::from(custom_log_path)
    }
}

/// Resolve the AI context files directory.
pub(crate) fn resolve_context_dir(
    data_dir: &std::path::Path,
    context_dir_setting: &str,
) -> PathBuf {
    if context_dir_setting.is_empty() {
        data_dir.join("contexts")
    } else {
        PathBuf::from(context_dir_setting)
    }
}

/// Whether CLI args request headless MCP stdio mode (`--mcp` / `--mcp-stdio`).
pub fn is_mcp_stdio_mode(args: &[String]) -> bool {
    args.iter().any(|a| a == "--mcp" || a == "--mcp-stdio")
}

/// Build the tracing env filter: honor `RUST_LOG` when set, else use settings log level.
pub(crate) fn build_tracing_env_filter(log_level: &str) -> tracing_subscriber::EnvFilter {
    tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level))
}

/// True when a window's logical size covers the full monitor (used for fullscreen-changed emit).
pub(crate) fn is_fullscreen_for_monitor(
    window_width: u32,
    window_height: u32,
    monitor_width: u32,
    monitor_height: u32,
) -> bool {
    window_width >= monitor_width && window_height >= monitor_height
}

/// Resolve bundled prompt templates directory from Tauri resource dir when available.
pub(crate) fn resolve_prompts_dir(resource_dir: Option<PathBuf>) -> Option<PathBuf> {
    resource_dir.map(|d| d.join("prompts"))
}

/// Whether embedded MCP server should auto-start during GUI setup.
pub(crate) fn should_auto_start_embedded_mcp(mcp_server_enabled: bool) -> bool {
    mcp_server_enabled
}

pub(crate) fn resolve_log_settings() -> (String, PathBuf) {
    let data_dir =
        crate::store::Store::default_app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
    resolve_log_settings_in(&data_dir)
}

/// Core logic extracted so tests can inject a temporary `data_dir`.
pub(crate) fn resolve_log_settings_in(data_dir: &std::path::Path) -> (String, PathBuf) {
    let settings_path = data_dir.join("settings.json");

    let (level, custom_path) = std::fs::read_to_string(&settings_path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .map(|v| parse_log_settings_fields(&v))
        .unwrap_or_else(|| ("info".to_string(), String::new()));

    let log_dir = resolve_log_dir(data_dir, &custom_path);

    (level, log_dir)
}

/// Build AppState for the GUI: Store first, then preload drivers used by saved connections.
/// Sync adapters / AI / prompts / workflows / history / MCP load on demand.
async fn build_gui_app_state(
    handle: &tauri::AppHandle,
    prompts_dir: Option<PathBuf>,
) -> Result<AppState, String> {
    let t_core = Instant::now();
    let store = Arc::new(Store::init(handle).await.map_err(|e| e.to_string())?);
    redis_flush_gate::sync_from_settings(&store.get_settings().await);
    tracing::info!("[startup]   store: {:?}", t_core.elapsed());

    let t_drv = Instant::now();
    let registry = Arc::new(init_drivers());
    let needed = unique_driver_types(&store.get_connections().await);
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
    redis_flush_gate::sync_from_settings(&store.get_settings().await);
    let t_drv = Instant::now();
    let registry = Arc::new(init_drivers());
    let needed = unique_driver_types(&store.get_connections().await);
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

pub(crate) fn finish_app_state(
    store: Arc<Store>,
    registry: Arc<crate::db::DriverRegistry>,
    sync_adapters: Arc<crate::transfer::adapter_registry::SyncAdapterRegistry>,
    prompts_dir: Option<PathBuf>,
) -> AppState {
    let schema_cache = Arc::new(SchemaCache::new(registry.clone()));
    let connection_manager = Arc::new(ConnectionManager::new(registry.clone(), store.clone()));
    connection_manager.clone().start_cleanup_task();
    let monitor_connections = Arc::new(crate::monitor::MonitorConnectionRegistry::new(
        connection_manager.clone(),
    ));
    let monitor_engine = MonitorEngine::new(store.clone());

    let data_dir = store.data_dir().to_path_buf();
    let history_db = store.history_db();
    let app_db = store.app_db();

    // Runtime extensions: scan {appData}/plugins/ for installed packages.
    // Invalid packages are skipped (warn) so one bad install can't break boot.
    let extension_manager = Arc::new(extensions::ExtensionManager::new(data_dir.join("plugins")));
    let extension_count = extension_manager.load_from_disk();
    tracing::info!("[startup]   ui extensions loaded: {extension_count}");

    // AI / prompts / workflows / history / MCP client: empty shells.
    // Nothing here touches disk or network — window can show immediately.
    let state = AppState {
        driver_registry: registry,
        connection_manager: connection_manager.clone(),
        monitor_connections,
        monitor_engine: monitor_engine.clone(),
        store,
        schema_cache: schema_cache.clone(),
        sync_adapters,
        ai_registry: Arc::new(crate::ai::AiProviderRegistry::new()),
        schema_context_builder: Arc::new(SchemaContextBuilder::new(
            schema_cache,
            connection_manager,
        )),
        prompt_resolver: Arc::new(crate::ai::PromptResolver::new(&data_dir, prompts_dir)),
        workflow_registry: Arc::new(workflow::WorkflowRegistry::new(app_db, data_dir.clone())),
        workflow_history: Arc::new(workflow::WorkflowHistoryManager::new(history_db)),
        mcp_client_manager: Arc::new(mcp::McpClientManager::new()),
        session_transactions: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        query_executions: Arc::new(crate::commands::QueryExecutionRegistry::new()),
        workflow_scheduler: workflow::scheduler::WorkflowScheduler::new(),
        extensions: extension_manager,
    };
    monitor_engine.attach_app_state(Arc::new(state.clone()));
    state
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
        if let Err(e) = mcp::auth::verify_stdio_token(&data_dir) {
            tracing::error!("{e}");
            std::process::exit(1);
        }
        let store = Arc::new(
            Store::init_with_path(&data_dir)
                .await
                .expect("Failed to init store"),
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

    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

    let filter = build_tracing_env_filter(&log_level);

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
        .init();

    let surface_bg = theme::surface_bg::SurfaceBgCache::load();
    let builder = tauri::Builder::default()
        .plugin(theme::surface_bg::SurfaceBootPlugin::new(surface_bg))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_os::init());

    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init());

    #[cfg(feature = "webdriver")]
    let builder = builder.plugin(tauri_plugin_webdriver::init());

    // E2E-only dialog-injection state (see commands/dialog.rs). Production
    // builds never manage it — the injection IPCs are compiled out entirely.
    #[cfg(feature = "webdriver")]
    let builder = builder.manage(crate::commands::DialogInjectionQueue::default());

    let builder = driver_init::register_drivers(builder);

    // `datazen://` plugin asset service + deep links (F2). Windows exposes
    // this as `http://datazen./...`; parsing accepts both forms.
    let builder = builder.register_uri_scheme_protocol("datazen", |ctx, request| {
        extensions::protocol::handle_datazen_request(ctx, request)
    });

    let t_builder = Instant::now();
    tracing::info!("[startup] builder created");

    builder
        .setup(move |app| {
            let builder_elapsed = t_builder.elapsed();
            let t_setup = Instant::now();
            tracing::info!("[startup] setup begin (builder took {:?})", builder_elapsed);

            let handle = app.handle().clone();

            let t0 = Instant::now();
            let prompts_dir = resolve_prompts_dir(handle.path().resource_dir().ok());
            let app_state =
                tauri::async_runtime::block_on(build_gui_app_state(&handle, prompts_dir))?;
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
                state.workflow_scheduler.start(handle.clone());
                tray::sync_tray(&handle);
            }

            if let Some(main) = app.get_webview_window("main") {
                crate::commands::window::prepare_main_window(&main);
                let main_for_deferred = main.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    crate::commands::window::prepare_main_window(&main_for_deferred);
                });
            }

            // Optional embedded MCP: only if user explicitly enabled it in settings (default off).
            {
                let state = handle.state::<AppState>();
                let enabled =
                    tauri::async_runtime::block_on(state.store.get_settings()).mcp_server_enabled;
                if should_auto_start_embedded_mcp(enabled) {
                    if let Err(e) = tauri::async_runtime::block_on(
                        crate::commands::start_embedded_mcp(state.inner()),
                    ) {
                        tracing::warn!(error = %e, "Failed to auto-start embedded MCP Server");
                    }
                }

                let client_configs = tauri::async_runtime::block_on(state.store.get_settings())
                    .mcp_client_servers
                    .into_iter()
                    .filter(|c| c.enabled)
                    .collect::<Vec<_>>();
                for config in client_configs {
                    let app_state = state.inner().clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) =
                            crate::commands::mcp_client_connect_impl(&app_state, config.clone())
                                .await
                        {
                            tracing::warn!(
                                error = %e,
                                server_id = %config.id,
                                "Failed to auto-connect external MCP client on startup"
                            );
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            if let Err(retry_err) =
                                crate::commands::mcp_client_connect_impl(&app_state, config).await
                            {
                                tracing::warn!(
                                    error = %retry_err,
                                    "External MCP client startup retry also failed"
                                );
                            }
                        }
                    });
                }
            }

            #[cfg(target_os = "macos")]
            {
                let t_settings = Instant::now();
                let initial_settings =
                    tauri::async_runtime::block_on(handle.state::<AppState>().store.get_settings());
                tracing::info!("[startup]   get_settings: {:?}", t_settings.elapsed());
                let t_menu = Instant::now();
                setup_menu(
                    &handle,
                    &initial_settings.theme.mode,
                    &initial_settings.language,
                )?;
                tracing::info!("[startup]   build menu: {:?}", t_menu.elapsed());
            }

            tracing::info!("[startup] setup complete: {:?}", t_setup.elapsed());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            crate::commands::get_connections,
            crate::commands::save_connection,
            crate::commands::delete_connection,
            crate::commands::reorder_connections,
            crate::commands::get_groups,
            crate::commands::save_groups,
            crate::commands::test_connection,
            crate::commands::connect,
            crate::commands::connect_dedicated,
            crate::commands::ping_connection,
            crate::commands::release_connection,
            crate::commands::disconnect,
            crate::commands::get_connection_info,
            crate::commands::get_available_drivers,
            crate::commands::get_databases,
            crate::commands::get_tables,
            crate::commands::get_columns,
            crate::commands::get_table_schema,
            crate::commands::get_structure_capabilities,
            crate::commands::plan_table_structure_changes,
            crate::commands::get_er_data,
            crate::commands::get_table_data,
            crate::commands::preview_pending_changes,
            crate::commands::commit_pending_changes,
            crate::commands::commit_row_updates,
            crate::commands::commit_row_deletes,
            crate::commands::execute_query,
            crate::commands::execute_query_stream,
            crate::commands::export_tables_stream,
            crate::commands::get_driver_commands,
            crate::commands::get_connection_commands,
            crate::commands::execute_driver_command,
            crate::commands::execute_driver_command_stream,
            crate::commands::get_explain,
            crate::commands::cancel_query,
            crate::commands::begin_session_transaction,
            crate::commands::commit_session_transaction,
            crate::commands::rollback_session_transaction,
            crate::commands::session_transaction_status,
            crate::commands::get_database_objects,
            crate::commands::get_object_ddl,
            crate::commands::get_privileges,
            crate::commands::get_query_history,
            crate::commands::clear_query_history,
            crate::commands::purge_history,
            crate::commands::get_favorite_queries,
            crate::commands::add_favorite_query,
            crate::commands::delete_favorite_query,
            crate::commands::get_settings,
            crate::commands::get_system_ui_language,
            crate::commands::save_settings,
            crate::commands::get_log_path,
            crate::commands::open_path,
            crate::commands::open_log_dir,
            crate::commands::open_workflows_dir,
            crate::commands::open_context_dir,
            crate::commands::export_connections,
            crate::commands::import_connections_preview,
            crate::commands::import_connections_with_dialog,
            crate::commands::pick_connections_import_file,
            crate::commands::import_connections_at_path,
            crate::commands::detect_connection_import_path,
            crate::commands::pick_connection_import_path_with_dialog,
            crate::commands::import_connections_from_app,
            crate::commands::export_app_data,
            crate::commands::pick_app_data_import_file,
            crate::commands::import_app_data,
            crate::commands::save_encryption_key_with_dialog,
            crate::commands::restart_app,
            crate::commands::save_text_with_dialog,
            crate::commands::save_base64_with_dialog,
            crate::commands::begin_save_with_dialog,
            crate::commands::append_save_text,
            crate::commands::finish_save,
            crate::commands::abort_save,
            crate::commands::open_text_with_dialog,
            crate::commands::open_base64_with_dialog,
            crate::commands::backup_database,
            crate::commands::restore_sql_file,
            crate::commands::prepare_schema_diff_plan,
            crate::commands::execute_schema_diff_deploy,
            crate::commands::cancel_schema_diff_deploy,
            crate::commands::compare_table_schemas,
            crate::commands::execute_data_sync,
            crate::commands::cancel_data_sync,
            crate::commands::compare_data_sync,
            crate::commands::apply_data_sync,
            crate::commands::generate_data_sync_sql,
            crate::commands::revalidate_data_sync,
            crate::commands::inspect_data_sync,
            crate::commands::classify_data_sync_pair,
            crate::commands::classify_transfer_pair,
            crate::commands::inspect_data_transfer,
            crate::commands::preview_data_transfer,
            crate::commands::execute_data_transfer,
            crate::commands::cancel_data_transfer,
            crate::commands::get_sync_tasks,
            crate::commands::save_sync_task_direct,
            crate::commands::delete_sync_task,
            crate::commands::check_sync_conflicts,
            crate::commands::ai_get_providers,
            crate::commands::ai_fetch_remote_models,
            crate::commands::ai_validate_config,
            crate::commands::ai_save_config,
            crate::commands::ai_get_config,
            crate::commands::ai_delete_config,
            crate::commands::ai_generate_sql,
            crate::commands::ai_diagnose_error,
            crate::commands::ai_analyze_explain,
            crate::commands::ai_chat,
            crate::commands::ai_parse_filter,
            crate::commands::mcp_get_status,
            crate::commands::mcp_start_stdio,
            crate::commands::mcp_stop,
            crate::commands::mcp_reload,
            crate::commands::mcp_list_all_tools,
            crate::commands::workflow_list,
            crate::commands::workflow_execute,
            crate::commands::workflow_save,
            crate::commands::workflow_save_yaml,
            crate::commands::workflow_get_yaml,
            crate::commands::workflow_delete,
            crate::commands::workflow_reload,
            crate::commands::workflow_get_dir,
            crate::commands::workflow_get,
            crate::commands::ai_generate_schema_doc,
            crate::commands::ai_diagnose_connection,
            crate::commands::ai_analyze_queries,
            crate::commands::mcp_client_connect,
            crate::commands::mcp_client_disconnect,
            crate::commands::mcp_client_list,
            crate::commands::mcp_client_tools,
            crate::commands::mcp_client_call_tool,
            crate::commands::create_sub_window,
            crate::commands::prompt_list,
            crate::commands::prompt_set_override,
            crate::commands::prompt_remove_override,
            crate::commands::context_get_dir,
            crate::commands::context_list_files,
            crate::commands::context_read_files,
            crate::commands::workflow_history_list,
            crate::commands::workflow_history_get,
            crate::commands::workflow_history_clear,
            crate::commands::set_surface_background,
            crate::commands::list_dashboards,
            crate::commands::get_dashboard,
            crate::commands::save_dashboard,
            crate::commands::delete_dashboard,
            crate::commands::list_widget_runs,
            crate::commands::get_widget_run,
            crate::commands::run_dashboard_widget,
            crate::commands::export_dashboard_with_dialog,
            crate::commands::import_dashboard_with_dialog,
            crate::commands::set_dashboard_refresh_paused,
            crate::commands::find_dashboard_workflow_refs,
            crate::commands::create_widget_from_sql,
            crate::commands::create_widget_from_workflow,
            crate::commands::update_hidden_widget_sql,
            crate::commands::list_extensions,
            crate::commands::inspect_extension_package_with_dialog,
            crate::commands::install_extension,
            crate::commands::remove_extension,
            crate::commands::set_extension_enabled,
            crate::commands::get_extension_manifest,
            crate::commands::extension_storage_get,
            crate::commands::extension_storage_set,
            crate::commands::extension_storage_remove,
            crate::commands::read_extension_file,
            crate::commands::extension_audit_log,
            crate::app_menu::rebuild_menu,
            // E2E-only dialog-injection surface (commands/dialog.rs): each
            // entry carries its own cfg gate so production registration
            // surfaces never contain these commands.
            #[cfg(feature = "webdriver")]
            crate::commands::test_inject_dialog_result,
            #[cfg(feature = "webdriver")]
            crate::commands::test_reset_dialog_queue,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    if crate::commands::window::main_close_blocked_by_child_windows(
                        window.app_handle().webview_windows().keys().cloned(),
                    ) {
                        api.prevent_close();
                        let _ = window.minimize();
                        return;
                    }
                    if tray::should_close_to_tray(window.app_handle()) {
                        api.prevent_close();
                        // macOS: hide so the Dock icon remains; Reopen restores MainWindow.
                        // Windows/Linux: minimize so the taskbar entry can restore MainWindow.
                        #[cfg(target_os = "macos")]
                        {
                            let _ = window.hide();
                        }
                        #[cfg(not(target_os = "macos"))]
                        {
                            let _ = window.minimize();
                        }
                    }
                }
            }
            #[cfg(target_os = "macos")]
            if let tauri::WindowEvent::Resized(size) = event {
                let win = window.clone();
                let width = size.width;
                let height = size.height;
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    if let Some(monitor) = win.current_monitor().ok().flatten() {
                        let mon = monitor.size();
                        let is_fs = is_fullscreen_for_monitor(width, height, mon.width, mon.height);
                        let _ = win.emit("fullscreen-changed", is_fs);
                    }
                });
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            match event {
                tauri::RunEvent::ExitRequested { api, .. } => {
                    if tray::should_prevent_exit(app_handle) {
                        api.prevent_exit();
                        return;
                    }
                    let state = app_handle.state::<AppState>();
                    tauri::async_runtime::block_on(async {
                        state.connection_manager.shutdown().await;
                        state.mcp_client_manager.disconnect_all().await;
                    });
                }
                // macOS Dock click (applicationShouldHandleReopen): raise MainWindow.
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen { .. } => {
                    tray::focus_main_window(app_handle);
                }
                _ => {}
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use std::sync::{Arc, Mutex};

    /// Guards tests that read/write the shared global `settings.json` so they
    /// don't race when `cargo test` runs them in parallel.
    static SETTINGS_FILE_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn resolve_log_settings_defaults_without_settings_file() {
        let _guard = SETTINGS_FILE_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path();
        let (level, log_dir) = resolve_log_settings_in(data_dir);
        assert_eq!(level, "info");
        assert_eq!(log_dir, data_dir.join("logs"));
    }

    #[test]
    fn resolve_log_settings_reads_custom_level_and_path() {
        let _guard = SETTINGS_FILE_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path();
        std::fs::create_dir_all(data_dir).unwrap();
        let settings_path = data_dir.join("settings.json");
        let custom_log = data_dir.join("custom-logs");
        let settings = serde_json::json!({
            "logLevel": "debug",
            "logPath": custom_log.to_string_lossy(),
        });
        std::fs::write(&settings_path, settings.to_string()).unwrap();

        let (level, log_dir) = resolve_log_settings_in(data_dir);
        assert_eq!(level, "debug");
        assert_eq!(log_dir, custom_log);
    }

    #[test]
    fn unique_driver_types_deduplicates_preserving_order() {
        use crate::db::{ConnectionConfig, SslMode};

        fn conn(id: &str, db_type: &str) -> ConnectionConfig {
            ConnectionConfig {
                id: id.into(),
                name: id.into(),
                database_type: db_type.into(),
                host: None,
                port: None,
                database: None,
                schema: None,
                username: None,
                password: None,
                ssl_mode: SslMode::default(),
                connection_timeout: 30,
                max_pool_size: 10,
                ssh_tunnel: None,
                color_tag: None,
                group: None,
                last_connected_at: None,
                server_version: None,
                options: None,
                read_only: false,
                pinned: false,
            }
        }

        let connections = vec![
            conn("a", "postgres"),
            conn("b", "mysql"),
            conn("c", "postgres"),
            conn("d", "redis"),
            conn("e", "mysql"),
        ];
        assert_eq!(
            unique_driver_types(&connections),
            vec!["postgres", "mysql", "redis"]
        );
    }

    #[test]
    fn parse_log_settings_fields_defaults() {
        let (level, path) = parse_log_settings_fields(&serde_json::json!({}));
        assert_eq!(level, "info");
        assert_eq!(path, "");
        let (level, path) = parse_log_settings_fields(&serde_json::json!({
            "logLevel": "warn",
            "logPath": "/var/log/datazen"
        }));
        assert_eq!(level, "warn");
        assert_eq!(path, "/var/log/datazen");
    }

    #[test]
    fn resolve_log_dir_uses_default_or_custom() {
        let data = PathBuf::from("/data/app");
        assert_eq!(resolve_log_dir(&data, ""), PathBuf::from("/data/app/logs"));
        assert_eq!(
            resolve_log_dir(&data, "/tmp/custom"),
            PathBuf::from("/tmp/custom")
        );
    }

    #[test]
    fn resolve_context_dir_uses_default_or_custom() {
        let data = PathBuf::from("/data/app");
        assert_eq!(
            resolve_context_dir(&data, ""),
            PathBuf::from("/data/app/contexts")
        );
        assert_eq!(
            resolve_context_dir(&data, "/home/user/ctx"),
            PathBuf::from("/home/user/ctx")
        );
    }

    #[test]
    fn is_mcp_stdio_mode_detects_flags() {
        assert!(!is_mcp_stdio_mode(&["datazen".into()]));
        assert!(is_mcp_stdio_mode(&["datazen".into(), "--mcp".into()]));
        assert!(is_mcp_stdio_mode(&["datazen".into(), "--mcp-stdio".into()]));
        assert!(!is_mcp_stdio_mode(&["datazen".into(), "--other".into()]));
    }

    #[test]
    fn build_tracing_env_filter_uses_log_level_when_env_unset() {
        let filter = build_tracing_env_filter("warn");
        assert!(filter.to_string().contains("warn") || filter.to_string().contains("WARN"));
    }

    #[test]
    fn is_fullscreen_for_monitor_compares_dimensions() {
        assert!(is_fullscreen_for_monitor(1920, 1080, 1920, 1080));
        assert!(is_fullscreen_for_monitor(2000, 1200, 1920, 1080));
        assert!(!is_fullscreen_for_monitor(800, 600, 1920, 1080));
        assert!(!is_fullscreen_for_monitor(1920, 900, 1920, 1080));
    }

    #[test]
    fn resolve_prompts_dir_appends_prompts_subdir() {
        assert_eq!(resolve_prompts_dir(None), None);
        assert_eq!(
            resolve_prompts_dir(Some(PathBuf::from("/app/resources"))),
            Some(PathBuf::from("/app/resources/prompts"))
        );
    }

    #[test]
    fn should_auto_start_embedded_mcp_follows_setting() {
        assert!(should_auto_start_embedded_mcp(true));
        assert!(!should_auto_start_embedded_mcp(false));
    }

    #[tokio::test]
    async fn finish_app_state_wires_core_services() {
        use crate::db::registry::DriverRegistry;
        use crate::testing::mock_driver::{MockDriver, MockDriverOptions};
        use crate::transfer::adapter_registry::SyncAdapterRegistry;

        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(
            Store::init_with_path(temp.path())
                .await
                .expect("store init"),
        );
        let registry = Arc::new(DriverRegistry::new());
        let mock = MockDriver::new("postgres", MockDriverOptions::default());
        registry.register_test_driver("postgres", mock).await;

        let state = finish_app_state(
            store.clone(),
            registry,
            Arc::new(SyncAdapterRegistry::new()),
            None,
        );
        assert!(state
            .workflow_registry
            .workflows_dir()
            .starts_with(temp.path()));
        assert_eq!(state.store.data_dir(), temp.path());
    }

    /// [tester] `lib.rs` preserves public crate entry points after the split.
    #[test]
    fn test_tester_lib_reexports_public_entry_points() {
        let lib = include_str!("lib.rs");
        assert!(lib.contains("pub use bootstrap::{"));
        assert!(lib.contains("run_mcp_stdio"));
        assert!(lib.contains("run,"));
        // is_mcp_stdio_mode must be publicly re-exported so the `datazen` bin
        // can call it from main.rs (see fix(bootstrap) re-export commit).
        assert!(lib.contains("is_mcp_stdio_mode"));
        assert!(lib.contains("mod app_menu"));
        assert!(lib.contains("mod bootstrap"));
    }

    /// [tester] `finish_app_state` wires extension manager and driver registry shells.
    #[tokio::test]
    async fn test_tester_finish_app_state_initializes_extensions_and_registry() {
        use crate::db::registry::DriverRegistry;
        use crate::testing::mock_driver::{MockDriver, MockDriverOptions};
        use crate::transfer::adapter_registry::SyncAdapterRegistry;

        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(
            Store::init_with_path(temp.path())
                .await
                .expect("store init"),
        );
        let registry = Arc::new(DriverRegistry::new());
        let db_type = "postgres".to_string();
        registry
            .register_test_driver(
                &db_type,
                MockDriver::new("postgres", MockDriverOptions::default()),
            )
            .await;

        let state = finish_app_state(
            store.clone(),
            registry.clone(),
            Arc::new(SyncAdapterRegistry::new()),
            None,
        );

        assert_eq!(
            state.extensions.extensions_dir(),
            temp.path().join("plugins")
        );
        assert!(Arc::ptr_eq(&state.driver_registry, &registry));
        assert!(registry.get(&db_type).await.is_some());
        assert!(state.extensions.list().is_empty());
    }

    fn invoke_handler_registration_block() -> &'static str {
        let src = include_str!("bootstrap.rs");
        let start = src
            .find(".invoke_handler(tauri::generate_handler![")
            .expect("invoke_handler block");
        let rest = &src[start..];
        let end = rest.find("])").expect("invoke_handler closing");
        &rest[..end]
    }

    /// [tester] Critical IPC commands remain registered after bootstrap extraction.
    #[test]
    fn test_tester_invoke_handler_registers_critical_commands() {
        let block = invoke_handler_registration_block();
        for needle in [
            "commands::get_connections,",
            "commands::execute_driver_command,",
            "commands::connect,",
            "commands::mcp_start_stdio,",
            "commands::list_extensions,",
            "app_menu::rebuild_menu,",
        ] {
            assert!(
                block.contains(needle),
                "invoke_handler missing registration: {needle}"
            );
        }
        let registered = block.matches("commands::").count() + block.matches("app_menu::").count();
        assert!(
            registered >= 150,
            "expected a large IPC surface, got {registered} command registrations"
        );
    }

    /// [tester] MCP stdio headless entry remains wired from `main` through `bootstrap`.
    #[test]
    fn test_tester_run_mcp_stdio_entry_chain_wiring() {
        let bootstrap = include_str!("bootstrap.rs");
        assert!(bootstrap.contains("pub fn run_mcp_stdio()"));
        assert!(bootstrap.contains("mcp::auth::verify_stdio_token"));
        assert!(bootstrap.contains("build_app_state"));
        assert!(bootstrap.contains("mcp::start_mcp_stdio"));

        let main_rs = include_str!("main.rs");
        assert!(main_rs.contains("datazen::is_mcp_stdio_mode"));
        assert!(main_rs.contains("datazen::run_mcp_stdio()"));
        assert!(main_rs.contains("datazen::run()"));
    }

    /// [tester] GUI bootstrap registers driver plugins and the datazen URI scheme.
    #[test]
    fn test_tester_run_registers_plugins_and_uri_scheme() {
        let src = include_str!("bootstrap.rs");
        assert!(src.contains("driver_init::register_drivers"));
        assert!(src.contains("register_uri_scheme_protocol(\"datazen\""));
        assert!(src.contains("build_gui_app_state"));
        assert!(src.contains("setup_menu"));
    }
}

//! GUI and headless MCP process entry points (`run_mcp_stdio`, `run`).
//!
//! `generate_handler!` lives here so registration-contract tests can
//! `include_str!("../bootstrap/run.rs")` and see the full IPC surface.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tauri::Emitter;
use tauri::Manager;

use crate::commands::AppState;
use crate::store::Store;
use crate::{driver_init, mcp, theme, tray, wapps};

#[cfg(target_os = "macos")]
use crate::app_menu::setup_menu;

use super::app_state::{build_app_state, build_gui_app_state};
use super::helpers::{
    build_tracing_env_filter, is_fullscreen_for_monitor, resolve_log_settings, resolve_prompts_dir,
    should_auto_start_embedded_mcp,
};

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
        .plugin(tauri_plugin_clipboard_manager::init())
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
        wapps::protocol::handle_datazen_request(ctx, request)
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

            // ── Window creation ───────────────────────────────────────────
            // No windows are defined in tauri.conf.json — we create the
            // appropriate one here based on the onboarding state so the two
            // never coexist.
            {
                let state = handle.state::<AppState>();
                let settings = tauri::async_runtime::block_on(state.store.get_settings());
                let needs_onboarding = settings.onboarding.as_ref().map_or(true, |o| !o.completed);

                if needs_onboarding {
                    tracing::info!("onboarding not completed — creating wizard window");
                    if let Err(e) = crate::commands::window::create_onboarding_window(&handle) {
                        tracing::error!(error = %e, "failed to create onboarding window");
                    }
                } else {
                    tracing::info!("onboarding completed — creating main window");
                    match crate::commands::window::create_main_window(&handle) {
                        Ok(main) => {
                            let main_for_deferred = main.clone();
                            std::thread::spawn(move || {
                                std::thread::sleep(std::time::Duration::from_millis(500));
                                crate::commands::window::prepare_main_window(&main_for_deferred);
                            });
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "failed to create main window");
                        }
                    }
                }
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
            crate::commands::get_tunnels,
            crate::commands::get_tunnel,
            crate::commands::save_tunnel,
            crate::commands::delete_tunnel,
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
            crate::commands::get_all_columns,
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
            crate::commands::write_clipboard,
            crate::commands::read_clipboard,
            crate::commands::get_privileges,
            crate::commands::get_query_history,
            crate::commands::clear_query_history,
            crate::commands::purge_history,
            crate::commands::get_favorite_queries,
            crate::commands::add_favorite_query,
            crate::commands::delete_favorite_query,
            crate::commands::backup_database,
            crate::commands::restore_sql_file,
            crate::commands::get_settings,
            crate::commands::save_settings,
            crate::commands::export_connections,
            crate::commands::import_connections_preview,
            crate::commands::import_connections_with_dialog,
            crate::commands::pick_connections_import_file,
            crate::commands::import_connections_at_path,
            crate::commands::detect_connection_import_path,
            crate::commands::pick_connection_import_path_with_dialog,
            crate::commands::import_connections_from_app,
            crate::commands::mcp_start_stdio,
            crate::commands::mcp_stop,
            crate::commands::mcp_get_status,
            crate::commands::mcp_client_connect,
            crate::commands::mcp_client_disconnect,
            crate::commands::mcp_client_list,
            crate::commands::mcp_client_tools,
            crate::commands::mcp_client_call_tool,
            crate::commands::create_sub_window,
            crate::commands::onboarding_complete,
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
            crate::commands::list_wapps,
            crate::commands::inspect_wapp_package_with_dialog,
            crate::commands::install_wapp,
            crate::commands::remove_wapp,
            crate::commands::set_wapp_enabled,
            crate::commands::get_wapp_manifest,
            crate::commands::wapp_storage_get,
            crate::commands::wapp_storage_set,
            crate::commands::wapp_storage_remove,
            crate::commands::read_wapp_file,
            crate::commands::wapp_audit_log,
            crate::app_menu::rebuild_menu,
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
            if let tauri::WindowEvent::Resized(_) = event {
                let win = window.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    let is_fs = win.is_fullscreen().unwrap_or_else(|_| {
                        if let (Ok(cur_size), Ok(Some(monitor))) =
                            (win.inner_size(), win.current_monitor())
                        {
                            let mon = monitor.size();
                            is_fullscreen_for_monitor(
                                cur_size.width,
                                cur_size.height,
                                mon.width,
                                mon.height,
                            )
                        } else {
                            false
                        }
                    });
                    let _ = win.emit("fullscreen-changed", is_fs);

                    // Re-check after animation fully settles (macOS fullscreen animation is ~400ms)
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    let is_fs_settled = win.is_fullscreen().unwrap_or_else(|_| {
                        if let (Ok(cur_size), Ok(Some(monitor))) =
                            (win.inner_size(), win.current_monitor())
                        {
                            let mon = monitor.size();
                            is_fullscreen_for_monitor(
                                cur_size.width,
                                cur_size.height,
                                mon.width,
                                mon.height,
                            )
                        } else {
                            false
                        }
                    });
                    if is_fs_settled != is_fs {
                        let _ = win.emit("fullscreen-changed", is_fs_settled);
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

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

pub mod ai;
mod app_data_archive;
mod cache;
mod commands;
mod dashboard;
pub mod db;
mod i18n_locale;
mod log_redact;
pub mod mcp;
mod monitor;
mod plugin_init;
mod redis_flush_gate;
pub mod schema_diff;
mod schema_objects;
mod services;
mod sql_guard;
mod ssh_known_hosts;
pub mod ssh_tunnel;
mod store;
pub mod sync;
mod theme;
mod tray;
pub mod workflow;

pub use store::{AppDb, HistoryDb};

#[cfg(test)]
pub(crate) mod testing;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use ai::SchemaContextBuilder;
use cache::SchemaCache;
use commands::AppState;
use db::init_drivers;
use monitor::MonitorEngine;
use services::ConnectionManager;
use std::collections::HashMap;
use store::Store;
use sync::adapter_registry::SyncAdapterRegistry;
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
    Ignore,
}

/// First caller wins. Used so `rebuild_menu` does not stack another `on_menu_event`.
pub(crate) fn take_once_slot(flag: &AtomicBool) -> bool {
    !flag.swap(true, Ordering::SeqCst)
}

pub(crate) fn menu_action_for_id(id: &str) -> MenuAction {
    if let Some(theme) = id.strip_prefix("theme-") {
        return MenuAction::ThemeChange(theme.to_string());
    }
    match id {
        "open-settings" => MenuAction::Emit("menu:open-settings"),
        "new-connection" => MenuAction::Emit("menu:new-connection"),
        "data-sync" => MenuAction::Emit("menu:data-sync"),
        "schema-diff" => MenuAction::Emit("menu:schema-diff"),
        "backup" => MenuAction::Emit("menu:backup"),
        "restore" => MenuAction::Emit("menu:restore"),
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
        "view-logs" => MenuAction::Emit("menu:view-logs"),
        "help-docs" => MenuAction::OpenDocs,
        "help-report" => MenuAction::OpenReportIssue,
        "ctx-add-favorite" => MenuAction::AddFavorite,
        _ => MenuAction::Ignore,
    }
}

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
                    let _ = app_handle.emit(event, ());
                }
                MenuAction::OpenDocs => {
                    // Open directly in Rust — do not emit to every webview (that
                    // previously caused concurrent create_sub_window races).
                    let app = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = commands::open_docs_window(app, None).await {
                            tracing::warn!(error = %e, "menu help-docs: open docs window failed");
                        }
                    });
                }
                MenuAction::OpenReportIssue => {
                    let _ = open::that("https://github.com/flyxl/datazen/issues/new");
                }
                MenuAction::AddFavorite => {
                    let _ = app_handle.emit("menu:add-favorite", ());
                }
                MenuAction::Ignore => {}
            }
        });
    }
}

#[cfg(target_os = "macos")]
fn setup_menu(
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

pub(crate) fn resolve_log_settings() -> (String, PathBuf) {
    let data_dir = store::Store::default_app_data_dir().unwrap_or_else(|_| PathBuf::from("."));

    let settings_path = data_dir.join("settings.json");

    let (level, custom_path) = std::fs::read_to_string(&settings_path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .map(|v| parse_log_settings_fields(&v))
        .unwrap_or_else(|| ("info".to_string(), String::new()));

    let log_dir = resolve_log_dir(&data_dir, &custom_path);

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
    registry: Arc<db::DriverRegistry>,
    sync_adapters: Arc<sync::adapter_registry::SyncAdapterRegistry>,
    prompts_dir: Option<PathBuf>,
) -> AppState {
    let schema_cache = Arc::new(SchemaCache::new(registry.clone()));
    let connection_manager = Arc::new(ConnectionManager::new(registry.clone(), store.clone()));
    connection_manager.clone().start_cleanup_task();
    let monitor_connections = Arc::new(monitor::MonitorConnectionRegistry::new(
        connection_manager.clone(),
    ));
    let monitor_engine = MonitorEngine::new(store.clone());

    let data_dir = store.data_dir().to_path_buf();
    let history_db = store.history_db();
    let app_db = store.app_db();

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
        ai_registry: Arc::new(ai::AiProviderRegistry::new()),
        schema_context_builder: Arc::new(SchemaContextBuilder::new(
            schema_cache,
            connection_manager,
        )),
        prompt_resolver: Arc::new(ai::PromptResolver::new(&data_dir, prompts_dir)),
        workflow_registry: Arc::new(workflow::WorkflowRegistry::new(app_db, data_dir.clone())),
        workflow_history: Arc::new(workflow::WorkflowHistoryManager::new(history_db)),
        mcp_client_manager: Arc::new(mcp::McpClientManager::new()),
        session_transactions: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        workflow_scheduler: workflow::scheduler::WorkflowScheduler::new(),
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

            let _ = app.get_webview_window("main");

            // Optional embedded MCP: only if user explicitly enabled it in settings (default off).
            {
                let state = handle.state::<AppState>();
                let enabled =
                    tauri::async_runtime::block_on(state.store.get_settings()).mcp_server_enabled;
                if should_auto_start_embedded_mcp(enabled) {
                    if let Err(e) =
                        tauri::async_runtime::block_on(commands::start_embedded_mcp(state.inner()))
                    {
                        tracing::warn!(error = %e, "Failed to auto-start embedded MCP Server");
                    }
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
            commands::commit_row_deletes,
            commands::execute_query,
            commands::execute_query_stream,
            commands::get_driver_commands,
            commands::get_connection_commands,
            commands::execute_driver_command,
            commands::get_explain,
            commands::cancel_query,
            commands::begin_session_transaction,
            commands::commit_session_transaction,
            commands::rollback_session_transaction,
            commands::session_transaction_status,
            commands::get_database_objects,
            commands::get_object_ddl,
            commands::get_privileges,
            commands::get_query_history,
            commands::clear_query_history,
            commands::purge_history,
            commands::get_favorite_queries,
            commands::add_favorite_query,
            commands::delete_favorite_query,
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
            commands::detect_connection_import_path,
            commands::pick_connection_import_path_with_dialog,
            commands::import_connections_from_app,
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
            commands::classify_sync_pair,
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
            commands::workflow_save_yaml,
            commands::workflow_get_yaml,
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
            commands::set_surface_background,
            commands::list_dashboards,
            commands::get_dashboard,
            commands::save_dashboard,
            commands::delete_dashboard,
            commands::list_widget_runs,
            commands::get_widget_run,
            commands::run_dashboard_widget,
            commands::export_dashboard_with_dialog,
            commands::import_dashboard_with_dialog,
            commands::set_dashboard_refresh_paused,
            commands::find_dashboard_workflow_refs,
            commands::create_widget_from_sql,
            commands::create_widget_from_workflow,
            commands::update_hidden_widget_sql,
            commands::get_monitor_paused,
            commands::set_monitor_paused,
            rebuild_menu,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" && tray::should_close_to_tray(window.app_handle()) {
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
            #[cfg(target_os = "macos")]
            if let tauri::WindowEvent::Resized(size) = event {
                let win = window.clone();
                let size = *size;
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    if let Some(monitor) = win.current_monitor().ok().flatten() {
                        let mon = monitor.size();
                        let is_fs = is_fullscreen_for_monitor(
                            size.width,
                            size.height,
                            mon.width,
                            mon.height,
                        );
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
                    let mgr = state.mcp_client_manager.clone();
                    tauri::async_runtime::block_on(mgr.disconnect_all());
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
    use std::sync::Arc;

    #[test]
    fn menu_labels_en_contains_core_keys() {
        let labels = menu_labels("en");
        assert!(labels.contains_key("app-name"));
        assert!(labels.contains_key("quit"));
        assert!(!labels["app-name"].is_empty());
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
    fn resolve_log_settings_defaults_without_settings_file() {
        let data_dir = Store::default_app_data_dir().unwrap();
        let settings_path = data_dir.join("settings.json");
        let backup = settings_path
            .exists()
            .then(|| std::fs::read(&settings_path).unwrap());
        let _ = std::fs::remove_file(&settings_path);

        let (level, log_dir) = resolve_log_settings();
        assert_eq!(level, "info");
        assert_eq!(log_dir, data_dir.join("logs"));

        if let Some(bytes) = backup {
            std::fs::write(settings_path, bytes).unwrap();
        }
    }

    #[test]
    fn resolve_log_settings_reads_custom_level_and_path() {
        let data_dir = Store::default_app_data_dir().unwrap();
        std::fs::create_dir_all(&data_dir).unwrap();
        let settings_path = data_dir.join("settings.json");
        let backup = settings_path
            .exists()
            .then(|| std::fs::read(&settings_path).unwrap());
        let custom_log = tempfile::tempdir().unwrap().path().join("custom-logs");
        let settings = serde_json::json!({
            "logLevel": "debug",
            "logPath": custom_log.to_string_lossy(),
        });
        std::fs::write(&settings_path, settings.to_string()).unwrap();

        let (level, log_dir) = resolve_log_settings();
        assert_eq!(level, "debug");
        assert_eq!(log_dir, custom_log);

        if let Some(bytes) = backup {
            std::fs::write(settings_path, bytes).unwrap();
        } else {
            let _ = std::fs::remove_file(settings_path);
        }
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

    #[test]
    fn menu_action_for_id_covers_all_emit_events() {
        for id in [
            "open-settings",
            "new-connection",
            "data-sync",
            "schema-diff",
            "backup",
            "restore",
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
            "view-logs",
        ] {
            match menu_action_for_id(id) {
                MenuAction::Emit(_) => {}
                other => panic!("expected Emit for {id}, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn finish_app_state_wires_core_services() {
        use crate::db::registry::DriverRegistry;
        use crate::sync::adapter_registry::SyncAdapterRegistry;
        use crate::testing::mock_driver::{MockDriver, MockDriverOptions};

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
}

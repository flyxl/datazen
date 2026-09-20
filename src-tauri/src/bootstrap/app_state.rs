//! AppState assembly for GUI and headless MCP entry points.

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
use crate::{mcp, redis_flush_gate, wapps, workflow};

use super::helpers::unique_driver_types;

/// Build AppState for the GUI: Store first, then preload drivers used by saved connections.
/// Sync adapters / AI / prompts / workflows / history / MCP load on demand.
pub(super) async fn build_gui_app_state(
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
pub(super) async fn build_app_state(
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

    // Runtime wapps: scan {appData}/wapps/ for installed packages.
    let wapps_dir = data_dir.join("wapps");
    // Invalid packages are skipped (warn) so one bad install can't break boot.
    let wapp_manager = Arc::new(wapps::WappManager::new(wapps_dir));
    let wapp_count = wapp_manager.load_from_disk();
    tracing::info!("[startup]   ui wapps loaded: {wapp_count}");

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
        wapps: wapp_manager,
    };
    monitor_engine.attach_app_state(Arc::new(state.clone()));
    state
}

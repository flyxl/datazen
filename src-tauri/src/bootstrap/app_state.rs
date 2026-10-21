//! AppState assembly for GUI and headless MCP entry points.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::commands::AppState;
use crate::monitor::engine::MonitorEngine;
use crate::store::Store;
use crate::transfer::adapter_registry::SyncAdapterRegistry;
use crate::{mcp, redis_flush_gate, wapps, workflow};
use datazen_feature_core::ai::context::SchemaContextBuilder;
use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::job::RecoveryFilter;
use datazen_platform_api::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};
use datazen_runtime::budget::{BudgetConfig, BudgetLedger};
use datazen_runtime::driver_registry::init_drivers;
use datazen_runtime::job::{JobClock, SystemJobClock};
use datazen_runtime::schema_cache::SchemaCache;

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

    let state = finish_app_state(
        store,
        registry,
        Arc::new(SyncAdapterRegistry::new()),
        prompts_dir,
    );
    recover_desktop_jobs_at_startup(&state).await?;
    Ok(state)
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
    let state = finish_app_state(
        store,
        registry,
        Arc::new(SyncAdapterRegistry::new()),
        prompts_dir,
    );
    recover_desktop_jobs_at_startup(&state).await?;
    Ok(state)
}

pub(crate) fn build_desktop_job_host(
    app_db: Arc<crate::store::AppDb>,
) -> Arc<datazen_runtime::job::JobHost> {
    let clock: Arc<dyn JobClock> = Arc::new(SystemJobClock);
    let repository: Arc<dyn datazen_runtime::job::JobRuntimeRepository> = Arc::new(
        crate::store::app_db::jobs::SqliteJobRepository::new(app_db, clock.clone(), 300),
    );
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(BudgetConfig::team_default())));
    Arc::new(datazen_runtime::job::JobHost::new(
        repository, ledger, clock,
    ))
}

pub(crate) async fn recover_desktop_jobs_at_startup(state: &AppState) -> Result<(), String> {
    // Native Jobs use the real desktop identity, unlike historical migration
    // adapters. Mark only those jobs for inspection; never rebuild a handler.
    if let Ok(adapter) = state.platform_require() {
        let context = adapter.begin_call().context().clone();
        state
            .desktop_job_host
            .mark_restart_candidates_for_verification(
                &context,
                RecoveryFilter {
                    kinds: vec!["nativeToolProbe".into()],
                    older_than: None,
                    limit: None,
                },
            )
            .await
            .map_err(|_| "native job recovery scan failed".to_string())?;
    }

    let ctx = RequestContext::new(
        OrganizationId::new("datazen-local"),
        PrincipalId::new("datazen-local-user"),
        None,
        ClientInstanceId::new("datazen-local-client"),
        RequestId::new(format!("startup-{}", uuid::Uuid::new_v4())),
        None,
    );
    let candidates = state
        .desktop_job_host
        .mark_restart_candidates_for_verification(&ctx, RecoveryFilter::default())
        .await
        .map_err(|_| "desktop job recovery scan failed".to_string())?;
    let interrupted = candidates
        .iter()
        .filter(|job| job.view.pending_verification_reason.is_some())
        .count();
    if interrupted > 0 {
        tracing::info!(interrupted, "interrupted desktop jobs require verification");
    }
    for grant in state
        .store
        .app_db()
        .list_monitor_grants()
        .map_err(|_| "monitor job recovery identity lookup failed".to_string())?
    {
        let service_ctx = grant.context();
        let monitor_jobs = state
            .desktop_job_host
            .mark_restart_candidates_for_verification(
                &service_ctx,
                RecoveryFilter {
                    kinds: vec!["monitorOccurrence".into()],
                    ..Default::default()
                },
            )
            .await
            .map_err(|_| "monitor job recovery scan failed".to_string())?;
        let interrupted = monitor_jobs
            .iter()
            .filter(|job| job.view.pending_verification_reason.is_some())
            .count();
        if interrupted > 0 {
            tracing::info!(
                interrupted,
                "interrupted monitor occurrences require verification"
            );
        }
    }
    Ok(())
}

pub(crate) fn finish_app_state(
    store: Arc<Store>,
    registry: Arc<datazen_runtime::driver_registry::DriverRegistry>,
    sync_adapters: Arc<crate::transfer::adapter_registry::SyncAdapterRegistry>,
    prompts_dir: Option<PathBuf>,
) -> AppState {
    let schema_cache = Arc::new(SchemaCache::new(registry.clone()));
    let monitor_engine = MonitorEngine::new(store.clone());

    let data_dir = store.data_dir().to_path_buf();
    let history_db = store.history_db();
    let app_db = store.app_db();
    let desktop_job_host = build_desktop_job_host(app_db.clone());

    // Runtime wapps: scan {appData}/wapps/ for installed packages.
    let wapps_dir = data_dir.join("wapps");
    // Invalid packages are skipped (warn) so one bad install can't break boot.
    let wapp_manager = Arc::new(wapps::WappManager::new(wapps_dir));
    let wapp_count = wapp_manager.load_from_disk();
    tracing::info!("[startup]   ui wapps loaded: {wapp_count}");

    // AI / prompts / workflows / history / MCP client: empty shells.
    // Nothing here touches disk or network — window can show immediately.
    let platform = crate::platform::PlatformEntry::launch();
    let schema_source = Arc::new(crate::ai::metadata_source::RuntimeSchemaSource::new(
        registry.clone(),
        store.clone(),
        platform.clone(),
    ));
    let state = AppState {
        driver_registry: registry,
        monitor_engine: monitor_engine.clone(),
        store,
        schema_cache: schema_cache.clone(),
        sync_adapters,
        ai_registry: Arc::new(datazen_ai_providers::AiProviderRegistry::with_http_client(
            datazen_ai_providers::http::HttpClient::desktop(),
        )),
        schema_context_builder: Arc::new(SchemaContextBuilder::new(schema_cache, schema_source)),
        prompt_resolver: Arc::new(
            datazen_feature_core::ai::prompt_resolver::PromptResolver::new(&data_dir, prompts_dir),
        ),
        workflow_registry: Arc::new(workflow::WorkflowRegistry::new(
            Arc::new(workflow::registry::AppDbWorkflowStore::new(app_db.clone())),
            data_dir.clone(),
        )),
        workflow_history: Arc::new(workflow::WorkflowHistoryManager::new(history_db)),
        mcp_client_manager: Arc::new(mcp::McpClientManager::new()),
        // 概要 §6.2 第 8 步：身份在启动期组装一次。组装失败时把原因存进
        // PlatformEntry，依赖身份的 IPC 调用会拿到明确错误，而不是静默继续。
        platform,
        query_executions: Arc::new(crate::commands::QueryExecutionRegistry::new()),
        workflow_scheduler: workflow::scheduler::WorkflowScheduler::new(),
        wapps: wapp_manager,
        cancel_registry: datazen_feature_core::ai::cancel::CancellationRegistry::default(),
        ai_grants: Arc::new(crate::commands::ai::AiGrantService::new()),
        schema_diff_jobs: Arc::new(crate::commands::schema_diff::job::SchemaDiffJobInfra::new()),
        desktop_job_host,
        // Data Transfer 的计划/检查点存储：宿主在组装期创建一个实例并注入。
        // 包内不再有进程级全局，存储的生命周期到这里为止。
        transfer_plans: Arc::new(
            crate::data_transfer::plan_store::InMemoryTransferPlanStore::new(),
        ),
    };
    if let Ok(adapter) = state.platform.require() {
        if let Err(error) = adapter.inject(state.store.clone(), state.driver_registry.clone()) {
            tracing::error!(code = ?error.code, "desktop platform injection failed");
        }
    }
    monitor_engine.attach_app_state(Arc::new(state.clone()));
    state
}

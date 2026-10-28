//! AppState assembly for GUI and headless MCP entry points.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::commands::AppState;
use crate::monitor::engine::MonitorEngine;
use crate::store::DesktopRepository;
use crate::transfer::adapter_registry::SyncAdapterRegistry;
use crate::{mcp, redis_flush_gate, workflow};
use datazen::ai::context::SchemaContextBuilder;
use datazen::execution::runtime::budget::{BudgetConfig, BudgetLedger};
use datazen::execution::runtime::driver_registry::init_drivers;
use datazen::jobs::domain::job::RecoveryFilter;
use datazen::jobs::{JobClock, SystemJobClock};
use datazen::schema::cache::SchemaCache;
use datazen::shared::context::RequestContext;
use datazen::shared::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};

use super::helpers::unique_driver_types;

/// Build AppState for the GUI: DesktopRepository first, then preload drivers used by saved connections.
/// Sync adapters / AI / prompts / workflows / history / MCP load on demand.
pub(super) async fn build_gui_app_state(
    handle: &tauri::AppHandle,
    prompts_dir: Option<PathBuf>,
) -> Result<AppState, String> {
    let t_core = Instant::now();
    let store = Arc::new(
        DesktopRepository::init(handle)
            .await
            .map_err(|e| e.to_string())?,
    );
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
    store: Arc<DesktopRepository>,
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
) -> Arc<datazen::jobs::JobHost> {
    let clock: Arc<dyn JobClock> = Arc::new(SystemJobClock);
    let repository: Arc<dyn datazen::jobs::JobRuntimeRepository> = Arc::new(
        crate::store::app_db::jobs::SqliteJobRepository::new(app_db, clock.clone(), 300),
    );
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(BudgetConfig::team_default())));
    Arc::new(datazen::jobs::JobHost::new(repository, ledger, clock))
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
        OrganizationId::new(crate::platform::LOCAL_ORGANIZATION_ID),
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
    store: Arc<DesktopRepository>,
    registry: Arc<datazen::execution::runtime::driver_registry::DriverRegistry>,
    sync_adapters: Arc<crate::transfer::adapter_registry::SyncAdapterRegistry>,
    prompts_dir: Option<PathBuf>,
) -> AppState {
    let schema_cache = Arc::new(SchemaCache::new());
    let monitor_engine = MonitorEngine::new(store.clone());

    let data_dir = store.data_dir().to_path_buf();
    let history_db = store.history_db();
    let app_db = store.app_db();
    let desktop_job_host = build_desktop_job_host(app_db.clone());

    // Runtime wapps: scan {appData}/wapps/ for installed packages.
    let wapps_dir = data_dir.join("wapps");
    // Invalid packages are skipped (warn) so one bad install can't break boot.
    let wapp_manager = Arc::new(datazen::workspace_apps::manager::WappManager::new(
        wapps_dir,
    ));
    let wapp_count = wapp_manager.load_from_disk();
    tracing::info!("[startup]   ui wapps loaded: {wapp_count}");

    // AI / prompts / workflows / history / MCP client: empty shells.
    // Nothing here touches disk or network — window can show immediately.
    let platform = crate::platform::PlatformEntry::launch();
    // 隧道相关的两个**存储端口**实例在组装期创建一次：
    //
    // * `tunnels`：隧道定义的仓储。桌面实现就是存储层本身（`DesktopRepository` 实现了
    //   `TunnelRepository`：加密落盘 `{appData}/tunnels.json`）。
    // * `known_hosts`：SSH known-hosts 的存储。端口契约住在形态中立的
    //   `modules/src/tunnel`，**文件 I/O** 留在宿主（桌面读
    //   `{appData}/ssh_known_hosts.json`）。
    //
    // 同一份实例既注入会话后端（建连时解引用 `tunnelId`、启动隧道），也进
    // `AppState`（命令侧取用）：整个进程只有一份，存储的生命周期由宿主决定。
    let tunnels: Arc<dyn datazen::tunnel::TunnelRepository> = store.clone();
    let known_hosts: Arc<dyn datazen::tunnel::KnownHostsRepository> =
        crate::platform::known_hosts::desktop_known_hosts_repository(&data_dir);
    // 平台装配必须**先于**依赖它的功能绑定：共享的元数据源要拿到 Runtime 门面
    // （解析会话归属）与执行宿主（profile / 驱动 / 命令执行），两者都出自装配。
    //
    // 装配失败不崩应用——与既有行为一致——但依赖平台的调用会明确失败。注意
    // `inject` 的 `Err` 现在只表示**装配冲突**（同一进程重复注入）：可选接线
    // （执行身份解析器、结果制品存储）失败是**降级**，已在 `inject` 内部记 warn
    // 并可通过 `degraded_capabilities()` 枚举；内核（Runtime / ApplicationServices）
    // 照常可用，因此设置、连接列表、建连这些主路径不受降级牵连。
    if let Ok(adapter) = platform.require() {
        if let Err(error) = adapter.inject(
            store.clone(),
            registry.clone(),
            tunnels.clone(),
            known_hosts.clone(),
        ) {
            tracing::error!(code = ?error.code, "desktop platform assembly conflict");
        }
    }
    // Schema Diff 评审计划的存储：桌面单租户，只承认桌面自己的组织。
    // 桌面身份组装失败时不编造组织身份——该存储不承认任何组织，一切访问按
    // 「不存在」处理（依赖身份的 IPC 调用本来也会明确失败）。
    let schema_diff_reviewed_plans =
        crate::commands::schema_diff::reviewed_repository::desktop_reviewed_plan_repository(
            &platform,
        );
    // AI 元数据源：共享实现住在 modules，桌面只提供两个跨形态件——Runtime
    // 门面与执行宿主。桌面不再有自己的 AI 适配层。
    let schema_source = crate::services::schema_context_source::desktop_schema_context_source(
        registry.clone(),
        store.clone(),
        platform.clone(),
    );
    let state = AppState {
        driver_registry: registry,
        monitor_engine: monitor_engine.clone(),
        store,
        schema_cache,
        sync_adapters,
        ai_registry: Arc::new(datazen::ai::AiProviderRegistry::with_http_client(
            datazen::ai::http::HttpClient::desktop(),
        )),
        schema_context_builder: Arc::new(SchemaContextBuilder::new(schema_source)),
        prompt_resolver: Arc::new(datazen::ai::prompt_resolver::PromptResolver::new(
            &data_dir,
            prompts_dir,
        )),
        workflow_registry: Arc::new(workflow::WorkflowRegistry::new(
            Arc::new(workflow::registry::AppDbWorkflowRepository::new(
                app_db.clone(),
            )),
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
        cancel_registry: datazen::ai::cancel::CancellationRegistry::default(),
        ai_grants: Arc::new(crate::commands::ai::AiGrantService::new()),
        schema_diff_jobs: Arc::new(crate::commands::schema_diff::job::SchemaDiffJobInfra::new()),
        desktop_job_host,
        // 组织作用域在**启动期**确定一次：桌面是单租户，组织的值是安装级常量，
        // 与「当前 OS 用户名能不能取到」无关。请求期直接取这份值（INV-01）。
        //
        // scope-gate: desktop-composition-root
        // 这是**桌面形态自己的作用域来源**：宿主组装期在这里回答「本形态是哪个组织」。
        // 请求期的调用方一律走 `AppState::organization_scope()`，不得再直读常量。
        // 别的形态（多租户服务端）把这一行换成「按请求解析组织」即可，其余调用点不用动。
        organization_scope: crate::platform::local_organization_scope(),
        schema_diff_reviewed_plans,
        // Data Sync 的存储端口实例在组装期创建一次：宿主决定「实例从哪来」，
        // 规则与落盘实现留在共享包里。
        sync_stores: crate::commands::SyncRepositoryPorts::desktop(),
        // Data Transfer 的计划/检查点存储：宿主在组装期创建一个实例并注入。
        // 包内不再有进程级全局，存储的生命周期到这里为止。
        transfer_plans: Arc::new(
            crate::data_transfer::plan_store::InMemoryTransferPlanRepository::new(),
        ),
        // `tunnels` 与 `known_hosts` 都不在这里：它们由 `ApplicationServices` 持有
        // （§5.1.2 第三步与第五步）。局部变量保留，因为要把同一份实例交给 `inject`。
    };
    monitor_engine.attach_app_state(Arc::new(state.clone()));
    state
}

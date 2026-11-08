//! Data Sync 执行面的服务端受理管线（W8 刀3e-4a）。
//!
//! 桌面对照 `datazen-desktop/src/commands/sync/`：`inspect.rs`（检视管线）、
//! `apply.rs`（compare/prepare 受理 + 复核投影物化）、`exec.rs`（apply 受理 /
//! execute 投影 / SQL 预览）、`mod.rs` 的 `check_sync_conflicts` 与
//! `revalidate_data_sync`、`job_api.rs` 的 `verify_data_sync_recovery`。
//! 形态差异按预登记收敛：
//!
//! - **会话解析**走 [`SessionEndpoints`]（目录可见性闸 + 活句柄借出 +
//!   档案 config 回读——schema_diff 域刀3b-2 的同一条服务端路径）；
//! - **受理上下文**来自 HTTP 认证链（组织隔离 INV-01；桌面是本地单租户
//!   `durable_context`）；
//! - **无预受理取消记忆**（3e-3 已登记的服务端语义差：job id 受理时
//!   铸造，客户端无从在受理前指名一个还不存在的 id）；
//! - **工件/选区**挂共享层 plan 仓储（组织键隔离端口，见 [`host`]）。
//!
//! 本刀只立服务层与直驱测试；HTTP 路由随刀3e-4b 点亮（九方法：
//! inspect / compare / prepare / apply / execute / SQL 预览 / 复核 /
//! 冲突检查 / 恢复核验）。

use std::collections::{HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use serde::Deserialize;

use datazen::execution::EffectOutcome;
use datazen::history::{MigrationProfileRef, MigrationRunRecord};
use datazen::jobs::domain::job::{
    JobDefinition, JobDetails, JobRecoveryRequest, JobRecoveryResult, JobRecoveryTarget,
    JobRecoveryVerdict, JobRecoveryVerification, JobState, JobView,
};
use datazen::jobs::{EndpointRef, JobHost, JobRecoveryVerifier};
use datazen::migration::data_sync::artifact_view::comparison_from_artifact;
use datazen::migration::data_sync::comparison_store::{ComparisonStorage, ComparisonTableMetadata};
use datazen::migration::data_sync::job::body::{
    ApplySpec, PrepareSpec, APPLY_KIND, CHECKPOINT_VERSION, HANDLER_VERSION, PLAN_VERSION,
    PREPARE_KIND,
};
use datazen::migration::data_sync::job::DataSyncHandler;
use datazen::migration::data_sync::plan_store::{
    self as plans, SelectionMatcher, StoredSyncPlan, SyncPlanIssue, SyncPlanRepository,
    SyncRunRequest, SyncRunSelection,
};
use datazen::migration::data_sync::{
    api, classify_tables, require_data_sync_family, ChangeSet, ComparisonResult,
    DataSyncExecutionResponse, Endpoint, ExecutionResult, SqlStatement, SyncOptions,
    SyncSourceFilter, TableMapping, TableResult,
};
use datazen::migration::data_transfer::transfer::adapter_registry::SyncAdapterRegistry;
use datazen::services::migration_endpoint::EndpointIdentity;
use datazen::services::schema_scope::metadata_schema;
use datazen::shared::context::{OwnerRef, RequestContext};
use datazen::shared::error::PortError;
use datazen::shared::id::{IdempotencyKey, JobId, WorkerId};
use datazen::shared::scope::OrganizationScope;
use datazen_driver_api::DatabaseDriver;
use futures_util::FutureExt;

use crate::schema_diff::endpoints::SessionEndpoints;
use crate::storage::connections::ServerConnections;
use crate::storage::migration_runs::ServerMigrationRuns;
use crate::storage::sync_assets::{ServerSyncProfiles, ServerSyncTasks};

use super::host::server_execution_host;
use super::runtime_state::SyncRuntimeState;
use super::sessions::{
    close_dedicated_session, open_dedicated_session, own_sessions, release_sessions, scope_of,
    scoped_identity,
};

/// One worker identity for the whole server form: the accepting request owns
/// the Job and the Job never spawns another worker（桌面 `data-sync-window`
/// 的服务端对应物）。
const WORKER: &str = "data-sync-server";

/// 执行面服务错误（3e-4b 的路由层按桶映射 wire 码：NotFound→404 /
/// Validation→400 `invalidArgument` / Storage→503）。
#[derive(Debug, thiserror::Error)]
pub enum SyncExecutionError {
    #[error("Data Sync resource '{0}' was not found")]
    NotFound(String),
    #[error("{0}")]
    Validation(String),
    #[error("Data Sync execution storage is unavailable: {0}")]
    Storage(String),
}

impl SyncExecutionError {
    pub(crate) fn unavailable(message: impl Into<String>) -> Self {
        Self::Storage(message.into())
    }
}

impl From<PortError> for SyncExecutionError {
    fn from(error: PortError) -> Self {
        match error {
            PortError::NotFound(id) => Self::NotFound(id),
            other => Self::Storage(other.to_string()),
        }
    }
}

/// `inspectDataSync` / `revalidateDataSync` 的请求形状（桌面 IPC 命令参数
/// 的 camelCase 同构；`tables` 在 inspect 是映射覆盖，在 revalidate 是
/// 待复核表名）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncInspectRequest {
    pub source_db_session_id: String,
    pub target_db_session_id: String,
    #[serde(default)]
    pub source_database: Option<String>,
    #[serde(default)]
    pub target_database: Option<String>,
    #[serde(default)]
    pub source_schema: Option<String>,
    #[serde(default)]
    pub target_schema: Option<String>,
    #[serde(default)]
    pub tables: Option<Vec<TableMapping>>,
}

/// `startDataSyncPrepareJob` / `compareDataSync` 的请求形状。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPrepareRequest {
    pub source_db_session_id: String,
    pub target_db_session_id: String,
    #[serde(default)]
    pub tables: Option<Vec<String>>,
    /// 客户端铸定的 job id（web 前端 `data-sync-{uuid}`；缺省时服务端铸造）。
    #[serde(default)]
    pub job_id: Option<String>,
    #[serde(default)]
    pub source_database: Option<String>,
    #[serde(default)]
    pub target_database: Option<String>,
    #[serde(default)]
    pub source_schema: Option<String>,
    #[serde(default)]
    pub target_schema: Option<String>,
    #[serde(default)]
    pub options: Option<api::SyncOptionsInput>,
    #[serde(default)]
    pub filters: Option<HashMap<String, SyncSourceFilter>>,
    /// compare 语义的映射覆盖（prepare 路径不需要；`compare_data_sync` 携带）。
    #[serde(default)]
    pub mappings: Option<Vec<TableMapping>>,
}

/// `verifyDataSyncRecovery` 的请求形状（桌面 `DataSyncRecoveryRequest`
/// 同构）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncRecoveryRequest {
    pub job_id: String,
    pub source_db_session_id: String,
    pub target_db_session_id: String,
    pub source_database: String,
    pub target_database: String,
    #[serde(default)]
    pub source_schema: Option<String>,
    #[serde(default)]
    pub target_schema: Option<String>,
}

/// Data Sync 执行面服务（composition root 注入；`host`/`plans`/
/// `runtime_state` 与读面 [`super::jobs::SyncJobs`] 共享同一批实例）。
#[derive(Clone)]
pub struct SyncExecution {
    host: Arc<JobHost>,
    plans: Arc<dyn SyncPlanRepository>,
    runtime_state: SyncRuntimeState,
    endpoints: SessionEndpoints,
    connections: Arc<ServerConnections>,
    adapters: Arc<SyncAdapterRegistry>,
    comparison_root:
        Arc<dyn datazen::migration::data_sync::comparison_store::ComparisonStorageRoot>,
    runs: Arc<ServerMigrationRuns>,
    sync_profiles: Arc<ServerSyncProfiles>,
    tasks: Arc<ServerSyncTasks>,
}

impl SyncExecution {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        host: Arc<JobHost>,
        plans: Arc<dyn SyncPlanRepository>,
        runtime_state: SyncRuntimeState,
        endpoints: SessionEndpoints,
        connections: Arc<ServerConnections>,
        adapters: Arc<SyncAdapterRegistry>,
        comparison_root: Arc<
            dyn datazen::migration::data_sync::comparison_store::ComparisonStorageRoot,
        >,
        runs: Arc<ServerMigrationRuns>,
        sync_profiles: Arc<ServerSyncProfiles>,
        tasks: Arc<ServerSyncTasks>,
    ) -> Self {
        Self {
            host,
            plans,
            runtime_state,
            endpoints,
            connections,
            adapters,
            comparison_root,
            runs,
            sync_profiles,
            tasks,
        }
    }

    // ── inspect / revalidate ────────────────────────────────────────────

    /// `inspectDataSync`：一对会话的表映射与硬门闸（无行比较）。桌面
    /// `inspect::run_inspect` 逐字对齐——分类之后的收尾投影在共享层
    /// `finalize_inspect_results`。
    pub async fn inspect(
        &self,
        ctx: &RequestContext,
        request: &SyncInspectRequest,
    ) -> Result<Vec<TableResult>, SyncExecutionError> {
        self.inspect_sessions(
            ctx,
            &request.source_db_session_id,
            &request.target_db_session_id,
            request.source_database.as_deref(),
            request.target_database.as_deref(),
            request.source_schema.as_deref(),
            request.target_schema.as_deref(),
            request.tables.as_deref().unwrap_or(&[]),
        )
        .await
    }

    /// `revalidateDataSync`：重跑 inspect 门闸，返回过期表投影（结构/主键
    /// 漂移的表名）。
    pub async fn revalidate(
        &self,
        ctx: &RequestContext,
        request: &SyncInspectRequest,
    ) -> Result<serde_json::Value, SyncExecutionError> {
        let tables = request
            .tables
            .as_ref()
            .map(|mappings| {
                mappings
                    .iter()
                    .map(|mapping| mapping.source_table.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let inspected = self
            .inspect_sessions(
                ctx,
                &request.source_db_session_id,
                &request.target_db_session_id,
                request.source_database.as_deref(),
                request.target_database.as_deref(),
                request.source_schema.as_deref(),
                request.target_schema.as_deref(),
                &[],
            )
            .await?;
        Ok(api::revalidation_report(tables, inspected))
    }

    #[allow(clippy::too_many_arguments)]
    async fn inspect_sessions(
        &self,
        ctx: &RequestContext,
        source_db_session_id: &str,
        target_db_session_id: &str,
        source_database: Option<&str>,
        target_database: Option<&str>,
        source_schema: Option<&str>,
        target_schema: Option<&str>,
        mappings: &[TableMapping],
    ) -> Result<Vec<TableResult>, SyncExecutionError> {
        let source = self
            .endpoints
            .resolve(
                ctx,
                source_db_session_id,
                datazen::execution::application::LivePurpose::Inspection,
            )
            .await
            .map_err(endpoint_unavailable)?;
        let target = self
            .endpoints
            .resolve(
                ctx,
                target_db_session_id,
                datazen::execution::application::LivePurpose::Inspection,
            )
            .await
            .map_err(endpoint_unavailable)?;
        let src_config = source.config.clone();
        let tgt_config = target.config.clone();

        require_data_sync_family(&src_config.database_type, &tgt_config.database_type)
            .map_err(|error| SyncExecutionError::Validation(error.to_string()))?;

        let src_db = api::resolve_db_name(source_database, src_config.database.as_deref());
        let tgt_db = api::resolve_db_name(target_database, tgt_config.database.as_deref());

        let source_schema = metadata_schema(
            source.driver.as_ref(),
            source_schema,
            None,
            src_config.schema.as_deref(),
        );
        let target_schema = metadata_schema(
            target.driver.as_ref(),
            target_schema,
            None,
            tgt_config.schema.as_deref(),
        );

        if api::is_self_sync(
            source_db_session_id,
            target_db_session_id,
            &src_db,
            &tgt_db,
            source_schema.as_deref(),
            target_schema.as_deref(),
        ) {
            return Err(SyncExecutionError::Validation(
                "self-sync of the same database is not allowed".into(),
            ));
        }

        let src_tables = api::filter_tables_by_schema(
            source
                .driver
                .get_tables(&source.handle, &src_db, source_schema.as_deref())
                .await
                .map_err(|error| SyncExecutionError::Validation(error.to_string()))?,
            source_schema.as_deref(),
        );
        let tgt_tables = api::filter_tables_by_schema(
            target
                .driver
                .get_tables(&target.handle, &tgt_db, target_schema.as_deref())
                .await
                .map_err(|error| SyncExecutionError::Validation(error.to_string()))?,
            target_schema.as_deref(),
        );

        let mut source_schemas = HashMap::new();
        let mut source_schema_errors = HashMap::new();
        for table in src_tables
            .iter()
            .filter(|t| matches!(t.table_type, datazen_driver_api::TableType::Table))
        {
            let table_schema = metadata_schema(
                source.driver.as_ref(),
                source_schema.as_deref(),
                table.schema.as_deref(),
                None,
            );
            match source
                .driver
                .get_table_schema(
                    &source.handle,
                    &table.name,
                    &src_db,
                    table_schema.as_deref(),
                )
                .await
            {
                Ok(schema) => {
                    source_schemas.insert(table.name.clone(), schema);
                }
                Err(error) if src_config.database_type == "sqlserver" => {
                    source_schema_errors.insert(table.name.clone(), error.to_string());
                }
                Err(_) => {}
            }
        }
        let mut target_schemas = HashMap::new();
        let mut target_schema_errors = HashMap::new();
        for table in tgt_tables
            .iter()
            .filter(|t| matches!(t.table_type, datazen_driver_api::TableType::Table))
        {
            let table_schema = metadata_schema(
                target.driver.as_ref(),
                target_schema.as_deref(),
                table.schema.as_deref(),
                None,
            );
            match target
                .driver
                .get_table_schema(
                    &target.handle,
                    &table.name,
                    &tgt_db,
                    table_schema.as_deref(),
                )
                .await
            {
                Ok(schema) => {
                    target_schemas.insert(table.name.clone(), schema);
                }
                Err(error) if tgt_config.database_type == "sqlserver" => {
                    target_schema_errors.insert(table.name.clone(), error.to_string());
                }
                Err(_) => {}
            }
        }

        let family = require_data_sync_family(&src_config.database_type, &tgt_config.database_type)
            .map_err(|error| SyncExecutionError::Validation(error.to_string()))?;
        let results = classify_tables(
            &family,
            &src_tables,
            &tgt_tables,
            mappings,
            &source_schemas,
            &target_schemas,
        );
        Ok(api::finalize_inspect_results(
            results,
            mappings,
            &source_schemas,
            &source_schema_errors,
            &target_schema_errors,
        ))
    }

    // ── compare / prepare（durable Job 受理面）────────────────────────────

    /// `compareDataSync`：statement-only 预检后由 `dataSyncPrepare` Job 冻结
    /// ChangeSet，终态后读复核投影（桌面 `run_comparison` 同步语义）。
    pub async fn compare(
        &self,
        ctx: &RequestContext,
        request: &SyncPrepareRequest,
    ) -> Result<datazen::migration::data_sync::plan_store::SyncComparisonPreview, SyncExecutionError>
    {
        let job_id = request
            .job_id
            .clone()
            .filter(|id| !id.trim().is_empty())
            .unwrap_or_else(|| format!("data-sync-{}", uuid::Uuid::new_v4()));
        let accepted = self.start_prepare(ctx, request, job_id.clone()).await?;
        let finished = self
            .wait_for_terminal(ctx, accepted.job_id.as_str())
            .await?;
        if finished.state != JobState::Succeeded {
            return Err(SyncExecutionError::Validation(
                finished.error.unwrap_or_else(|| {
                    "the Data Sync comparison job did not complete; compare again".to_string()
                }),
            ));
        }
        self.preview_of(ctx, finished.job_id.as_str()).await
    }

    /// `startDataSyncPrepareJob`：受理 durable prepare Job，受理回执先于任何
    /// 读取返回，分离的派发 worker 做行比较并物化进程内复核投影。
    pub async fn start_prepare(
        &self,
        ctx: &RequestContext,
        request: &SyncPrepareRequest,
        job_id: String,
    ) -> Result<JobView, SyncExecutionError> {
        let source = self
            .endpoints
            .resolve(
                ctx,
                &request.source_db_session_id,
                datazen::execution::application::LivePurpose::Inspection,
            )
            .await
            .map_err(endpoint_unavailable)?;
        let target = self
            .endpoints
            .resolve(
                ctx,
                &request.target_db_session_id,
                datazen::execution::application::LivePurpose::Inspection,
            )
            .await
            .map_err(endpoint_unavailable)?;
        let source_config = source.config.clone();
        let target_config = target.config.clone();
        let source_database_name = api::resolve_db_name(
            request.source_database.as_deref(),
            source_config.database.as_deref(),
        );
        let target_database_name = api::resolve_db_name(
            request.target_database.as_deref(),
            target_config.database.as_deref(),
        );
        require_data_sync_family(&source_config.database_type, &target_config.database_type)
            .map_err(|error| SyncExecutionError::Validation(error.to_string()))?;
        let source_schema_name = metadata_schema(
            source.driver.as_ref(),
            request.source_schema.as_deref(),
            None,
            source_config.schema.as_deref(),
        );
        let target_schema_name = metadata_schema(
            target.driver.as_ref(),
            request.target_schema.as_deref(),
            None,
            target_config.schema.as_deref(),
        );

        let options = api::resolve_options(request.options.clone());
        let source_filters = request.filters.clone().unwrap_or_default();
        let sessions = own_sessions(
            &self.endpoints,
            ctx,
            &request.source_db_session_id,
            &request.target_db_session_id,
            &source_database_name,
            &target_database_name,
            source_schema_name.as_deref(),
            target_schema_name.as_deref(),
        )
        .await?;

        let mapping_overrides = request.mappings.clone().unwrap_or_default();
        let preflight = async {
            let inspected = self
                .inspect_sessions(
                    ctx,
                    &sessions.source_id,
                    &sessions.target_id,
                    Some(source_database_name.as_str()),
                    Some(target_database_name.as_str()),
                    source_schema_name.as_deref(),
                    target_schema_name.as_deref(),
                    &mapping_overrides,
                )
                .await?;
            self.adapters
                .ensure_pair(&source_config.database_type, &target_config.database_type)
                .map_err(|error| SyncExecutionError::Validation(error))?;
            if self
                .adapters
                .get_source(&source_config.database_type)
                .is_none()
                || self
                    .adapters
                    .get_source(&target_config.database_type)
                    .is_none()
            {
                return Err(SyncExecutionError::Validation(
                    "a Data Sync endpoint has no compatible key contract".into(),
                ));
            }
            options
                .validate()
                .map_err(|error| SyncExecutionError::Validation(error.to_string()))?;
            let wanted = request
                .tables
                .clone()
                .unwrap_or_default()
                .into_iter()
                .collect::<HashSet<_>>();
            let order = inspected
                .iter()
                .map(|table| (table.source_table.clone(), table.target_table.clone()))
                .collect::<Vec<_>>();
            let selected = inspected
                .iter()
                .filter(|table| {
                    table.status == datazen::migration::data_sync::TableMappingStatus::Matched
                        && (wanted.is_empty() || wanted.contains(&table.source_table))
                })
                .map(|table| (table.source_table.clone(), table.target_table.clone()))
                .collect::<Vec<_>>();
            let skipped = inspected
                .into_iter()
                .filter(|table| {
                    table.status != datazen::migration::data_sync::TableMappingStatus::Matched
                        || (!wanted.is_empty() && !wanted.contains(&table.source_table))
                })
                .collect::<Vec<_>>();
            let mappings = selected
                .iter()
                .map(|pair| {
                    let mut mapping = mapping_overrides
                        .iter()
                        .find(|mapping| {
                            mapping.enabled
                                && mapping.source_table == pair.0
                                && mapping.target_table == pair.1
                        })
                        .cloned()
                        .unwrap_or_else(|| mapping_of(pair, &source_filters));
                    if let Some(filter) = source_filters.get(&pair.0) {
                        mapping.source_filter = Some(filter.clone());
                    }
                    mapping
                })
                .collect::<Vec<_>>();
            Ok::<_, SyncExecutionError>((order, skipped, mappings))
        }
        .await;
        let (order, skipped, mappings) = match preflight {
            Ok(value) => value,
            Err(error) => {
                release_sessions(&self.endpoints, ctx, &sessions.ids()).await;
                return Err(error);
            }
        };

        let plan_id = uuid::Uuid::new_v4().to_string();
        let source_endpoint = Endpoint {
            db_session_id: sessions.source_id.clone(),
            database: source_database_name.clone(),
            schema: source_schema_name.clone(),
        };
        let target_endpoint = Endpoint {
            db_session_id: sessions.target_id.clone(),
            database: target_database_name.clone(),
            schema: target_schema_name.clone(),
        };
        let spec = PrepareSpec {
            source: source_endpoint.clone(),
            target: target_endpoint.clone(),
            mappings,
            options: options.clone(),
            filters: source_filters.clone(),
            plan_id: Some(plan_id.clone()),
        };
        let handler = server_execution_host(
            &self.endpoints,
            ctx,
            self.connections.clone(),
            self.plans.clone(),
            self.runtime_state.clone(),
            self.adapters.clone(),
            source_endpoint,
            target_endpoint,
            source_filters,
            &job_id,
        )
        .await
        .map_err(SyncExecutionError::unavailable)?;
        let handler = Arc::new(DataSyncHandler::for_prepare(spec, handler));
        let endpoints = sessions.endpoints();
        let payload = serde_json::json!({
            "planVersion": PLAN_VERSION,
            "handlerVersion": HANDLER_VERSION,
            "checkpointVersion": CHECKPOINT_VERSION,
            "planId": plan_id.clone(),
            "recoveryTargets": sessions.recovery_targets(),
            "recoveryPolicy": "verify-only",
        });
        let admission = match self
            .admit(
                ctx,
                &job_id,
                PREPARE_KIND,
                payload,
                &format!("data-sync-prepare:{job_id}"),
                &endpoints,
            )
            .await
        {
            Ok(admission) => admission,
            Err(error) => {
                release_sessions(&self.endpoints, ctx, &sessions.ids()).await;
                return Err(error);
            }
        };
        if !admission.dispatch {
            release_sessions(&self.endpoints, ctx, &sessions.ids()).await;
            return Ok(admission.view);
        }

        let runtime_state = self.runtime_state.clone();
        let prepared_job_id = admission.job_id.as_str().to_string();
        runtime_state.preview_started(&prepared_job_id);
        let worker = Worker {
            host: Arc::clone(&self.host),
            plans: Arc::clone(&self.plans),
            comparison_root: Arc::clone(&self.comparison_root),
            endpoints: self.endpoints.clone(),
            ctx: ctx.clone(),
        };
        let owned_ids = sessions.ids();
        let source_plan_session_id = request.source_db_session_id.clone();
        let target_plan_session_id = request.target_db_session_id.clone();
        let source_identity = sessions.source_identity.clone();
        let target_identity = sessions.target_identity.clone();
        let target_read_only = target_config.read_only;
        tokio::spawn(async move {
            let work = AssertUnwindSafe(async {
                dispatch(
                    &worker.host,
                    &worker.ctx,
                    admission.job_id.clone(),
                    &admission.endpoints,
                    handler,
                )
                .await;
                match worker.host.get(&worker.ctx, admission.job_id.clone()).await {
                    Ok(record) if record.view.state == JobState::Succeeded => {
                        if finish_prepare_preview(
                            &worker,
                            &prepared_job_id,
                            &plan_id,
                            &source_plan_session_id,
                            &target_plan_session_id,
                            &owned_ids[0],
                            &owned_ids[1],
                            &source_database_name,
                            &target_database_name,
                            source_schema_name.as_deref(),
                            target_schema_name.as_deref(),
                            options,
                            skipped,
                            order,
                            target_read_only,
                            source_identity,
                            target_identity,
                        )
                        .await
                        .is_err()
                        {
                            runtime_state.record_failure(
                                &prepared_job_id,
                                "preparePreviewUnavailable".to_string(),
                            );
                        }
                    }
                    _ => {}
                }
            })
            .catch_unwind()
            .await;
            if work.is_err() {
                runtime_state.record_failure(&prepared_job_id, "prepareWorkerPanicked".to_string());
            }
            runtime_state.preview_finished(&prepared_job_id);
            release_sessions(&worker.endpoints, &worker.ctx, &owned_ids).await;
        });
        Ok(admission.view)
    }

    /// `getDataSyncJobPreview` 的执行面共享实现（读面 [`super::jobs::SyncJobs`]
    /// 的 `preview` 同语义——60s 物化等待窗口）。
    async fn preview_of(
        &self,
        ctx: &RequestContext,
        job_id: &str,
    ) -> Result<datazen::migration::data_sync::plan_store::SyncComparisonPreview, SyncExecutionError>
    {
        let details = self.details(ctx, job_id).await?;
        if details.job.kind != PREPARE_KIND {
            return Err(SyncExecutionError::Validation(
                "job is not a Data Sync prepare job".into(),
            ));
        }
        let Some(plan_id) = details.plan_id else {
            return Err(SyncExecutionError::Validation(
                "Data Sync prepare job has no durable plan receipt".into(),
            ));
        };
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
        while self.runtime_state.preview_pending(job_id) {
            if tokio::time::Instant::now() >= deadline {
                return Err(SyncExecutionError::Validation(
                    "Data Sync review projection is still being prepared; retry loading the preview"
                        .into(),
                ));
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        plans::preview_for_plan(self.plans.as_ref(), &scope_of(ctx), &plan_id)
            .map_err(SyncExecutionError::Validation)
    }

    // ── apply / execute ─────────────────────────────────────────────────

    /// `startDataSyncApplyJob`：对确切的已复核 ChangeSet 受理 apply Job，跑在
    /// 分离的 job 自有会话上；选区进程内安装，durable 回执取胜后才可见。
    pub async fn start_apply(
        &self,
        ctx: &RequestContext,
        source_db_session_id: &str,
        target_db_session_id: &str,
        mut request: SyncRunRequest,
    ) -> Result<JobView, SyncExecutionError> {
        if request.plan_id.trim().is_empty() {
            return Err(SyncExecutionError::Validation(
                "apply requires a comparison plan; compare again".into(),
            ));
        }
        if let Some(receipt) = self.find_apply_receipt(ctx, &request.plan_id).await? {
            return Ok(receipt);
        }
        let scope = scope_of(ctx);
        let plan = self
            .plans
            .peek(&scope, &request.plan_id)
            .map_err(SyncExecutionError::Validation)?;
        if request.selection.revision != plan.selection_revision {
            return Err(SyncExecutionError::Validation(
                "selection revision is stale; return to comparison".into(),
            ));
        }
        validate_requested_options(&plan, &request.options)?;
        plans::validate_selection_streaming(&plan.comparison, &request.selection, &request.options)
            .map_err(SyncExecutionError::Validation)?;
        self.validate_plan_context_sessions(ctx, &plan, source_db_session_id, target_db_session_id)
            .await?;
        let artifact = self
            .plans
            .load_artifact(&scope, &request.plan_id)
            .ok_or_else(|| {
                SyncExecutionError::Validation(
                    "reviewed ChangeSet is unavailable; compare again".into(),
                )
            })?;
        if artifact.blocks.is_empty() {
            return Err(SyncExecutionError::Validation(
                "reviewed ChangeSet is empty; compare again".into(),
            ));
        }

        let sessions = own_sessions(
            &self.endpoints,
            ctx,
            source_db_session_id,
            target_db_session_id,
            &plan.source_database,
            &plan.target_database,
            plan.source_schema.as_deref(),
            plan.target_schema.as_deref(),
        )
        .await?;
        let job_id = request
            .job_id
            .take()
            .filter(|id| !id.trim().is_empty())
            .unwrap_or_else(|| format!("data-sync-{}", uuid::Uuid::new_v4()));
        let source_endpoint = Endpoint {
            db_session_id: sessions.source_id.clone(),
            database: plan.source_database.clone(),
            schema: plan.source_schema.clone(),
        };
        let target_endpoint = Endpoint {
            db_session_id: sessions.target_id.clone(),
            database: plan.target_database.clone(),
            schema: plan.target_schema.clone(),
        };
        let spec = ApplySpec {
            plan_id: request.plan_id.clone(),
            selection_revision: plan.selection_revision,
            options: request.options.clone(),
        };
        let handler = server_execution_host(
            &self.endpoints,
            ctx,
            self.connections.clone(),
            self.plans.clone(),
            self.runtime_state.clone(),
            self.adapters.clone(),
            source_endpoint.clone(),
            target_endpoint.clone(),
            HashMap::new(),
            &job_id,
        )
        .await
        .map_err(SyncExecutionError::unavailable)?;
        let handler = Arc::new(DataSyncHandler::for_apply_with_endpoints(
            spec,
            handler,
            source_endpoint,
            target_endpoint,
        ));
        let endpoints = sessions.endpoints();
        let payload = serde_json::json!({
            "planVersion": PLAN_VERSION,
            "handlerVersion": HANDLER_VERSION,
            "checkpointVersion": CHECKPOINT_VERSION,
            "planId": request.plan_id.clone(),
            "consumedPlanId": request.plan_id.clone(),
            "selectionRevision": plan.selection_revision,
            "planDigest": format!("sha256:{}", artifact.digest),
            "targetBeforeFingerprint": format!("sha256:{}", artifact.structure_fingerprint),
            "recoveryTargets": sessions.recovery_targets(),
            "recoveryPolicy": "verify-only",
        });
        let admission = match self
            .admit(
                ctx,
                &job_id,
                APPLY_KIND,
                payload,
                &format!("data-sync-apply:{}", request.plan_id),
                &endpoints,
            )
            .await
        {
            Ok(admission) => admission,
            Err(error) => {
                release_sessions(&self.endpoints, ctx, &sessions.ids()).await;
                return Err(error);
            }
        };
        if !admission.dispatch {
            release_sessions(&self.endpoints, ctx, &sessions.ids()).await;
            return Ok(admission.view);
        }
        if self.plans.claim(&scope, &request.plan_id).is_err() {
            let _ = self
                .host
                .repository()
                .mark_failed_unstarted(ctx, &admission.job_id, "planUnavailableAfterAccept")
                .await;
            release_sessions(&self.endpoints, ctx, &sessions.ids()).await;
            return Err(SyncExecutionError::Validation(
                "reviewed plan is no longer available; compare again".into(),
            ));
        }
        self.plans.install_selection(
            &scope,
            &request.plan_id,
            plans::StoredSyncSelection {
                selection: request.selection,
                options: request.options,
            },
        );

        let worker_host = Arc::clone(&self.host);
        let runtime_state = self.runtime_state.clone();
        let worker_endpoints = self.endpoints.clone();
        let worker_ctx = ctx.clone();
        let worker_job_id = admission.job_id.as_str().to_string();
        let dispatch_endpoints = admission.endpoints.clone();
        let owned_ids = sessions.ids();
        tokio::spawn(async move {
            let work = AssertUnwindSafe(dispatch(
                &worker_host,
                &worker_ctx,
                JobId::new(worker_job_id.clone()),
                &dispatch_endpoints,
                handler,
            ))
            .catch_unwind()
            .await;
            if work.is_err() {
                runtime_state.record_failure(&worker_job_id, "applyWorkerPanicked".to_string());
            }
            release_sessions(&worker_endpoints, &worker_ctx, &owned_ids).await;
        });
        Ok(admission.view)
    }

    /// `executeDataSync`：apply 的同步投影（受理 → 终态 → IPC 契约形状）+
    /// 迁移运行历史两段落账。桌面 `execute_data_sync` 命令同链。
    pub async fn execute(
        &self,
        ctx: &RequestContext,
        mut request: SyncRunRequest,
        profile: Option<&MigrationProfileRef>,
    ) -> Result<DataSyncExecutionResponse, SyncExecutionError> {
        if request.plan_id.trim().is_empty() {
            return Err(SyncExecutionError::Validation(
                "execute_data_sync requires a comparison planId".into(),
            ));
        }
        // A consumed-plan receipt identifies an existing execution. Replaying it
        // must neither write again nor invent a second migration-history run.
        let replay = self
            .find_apply_receipt(ctx, &request.plan_id)
            .await?
            .is_some();
        let scope = scope_of(ctx);
        let mut run = if replay {
            None
        } else {
            Some(self.start_run(ctx, "dataSync", profile).await)
        };
        if let Some(run) = run.as_mut() {
            run.selected_count = request.selection.rows.len() as u64;
            if let Ok(plan) = self.plans.peek(&scope, &request.plan_id) {
                run.source_connection_id = plan
                    .source_endpoint_identity
                    .as_ref()
                    .map(|identity| identity.connection_id.as_str().to_owned());
                run.target_connection_id = plan
                    .target_endpoint_identity
                    .as_ref()
                    .map(|identity| identity.connection_id.as_str().to_owned());
            }
        }
        let (response, cancelled) = match self.validate_profile_ref(ctx, profile).await {
            Err(error) => (
                DataSyncExecutionResponse::failed_before_start(error.clone()),
                false,
            ),
            Ok(()) => {
                let outcome = self.execute_plan(ctx, &mut request).await;
                execution_response_and_cancelled(outcome)
            }
        };
        if let Some(run) = run {
            self.finish_run(ctx, run, &response, cancelled).await;
        }
        Ok(response)
    }

    /// 桌面 `execute_data_sync_plan` 的服务端投影（受理 → 等终态 → 域结果）。
    async fn execute_plan(
        &self,
        ctx: &RequestContext,
        request: &mut SyncRunRequest,
    ) -> Result<ExecutionResult, ApplyFailure> {
        let scope = scope_of(ctx);
        let (source_session_id, target_session_id) = self
            .plans
            .peek(&scope, &request.plan_id)
            .map(|plan| (plan.source_db_session_id, plan.target_db_session_id))
            .unwrap_or_default();
        if request
            .job_id
            .as_deref()
            .map_or(true, |job_id| job_id.trim().is_empty())
        {
            request.job_id = Some(format!("data-sync-{}", uuid::Uuid::new_v4()));
        }
        let accepted = self
            .start_apply(ctx, &source_session_id, &target_session_id, request.clone())
            .await?;
        let finished = self
            .wait_for_terminal(ctx, accepted.job_id.as_str())
            .await?;
        let details = self.details(ctx, finished.job_id.as_str()).await?;
        let job = details.job;
        let message = match job.state {
            JobState::Succeeded => None,
            JobState::Cancelled => Some("execute cancelled".to_string()),
            JobState::Failed => Some(job.error.unwrap_or_else(|| {
                "the Data Sync apply job failed; compare current data before continuing".to_string()
            })),
            JobState::Queued | JobState::Running => Some(
                "the Data Sync apply job was still running when the request returned".to_string(),
            ),
        };
        applied_outcome(Ok(SyncJobOutcome {
            state: job.state,
            effect: job.effect_outcome.unwrap_or(EffectOutcome::NotStarted),
            committed: job.progress.committed.get(),
            message,
        }))
    }

    // ── SQL 预览 ────────────────────────────────────────────────────────

    /// `generateDataSyncSql`：已复核 ChangeSet 的 DML 预览（行级分页源 +
    /// IPC 上限；执行路径用同一分页源、从不累积这个预览向量）。桌面
    /// `generate_data_sync_sql_for_plan` 同链。
    pub async fn generate_sql(
        &self,
        ctx: &RequestContext,
        plan_id: &str,
        selection: SyncRunSelection,
        options: SyncOptions,
    ) -> Result<Vec<SqlStatement>, SyncExecutionError> {
        let scope = scope_of(ctx);
        let plan = self
            .plans
            .peek(&scope, plan_id)
            .map_err(SyncExecutionError::Validation)?;
        if selection.revision != plan.selection_revision {
            return Err(SyncExecutionError::Validation(
                "selection revision is stale; return to comparison".into(),
            ));
        }
        validate_requested_options(&plan, &options)?;
        let matcher = plans::validate_selection_streaming(&plan.comparison, &selection, &options)
            .map_err(SyncExecutionError::Validation)?;
        self.validate_plan_context(ctx, &plan).await?;
        let mut source = ComparisonPageSource {
            execution: self,
            ctx,
            target_db_session_id: plan.target_db_session_id.clone(),
            comparison: plan.comparison.clone(),
            matcher,
            options: options.clone(),
            target_database: plan.target_database.clone(),
            target_schema: plan.target_schema.clone(),
            summaries: plan
                .comparison
                .summaries()
                .map_err(SyncExecutionError::Validation)?,
            table_index: 0,
            row_offset: 0,
        };
        let mut statements = Vec::new();
        let mut response_bytes = 2usize; // JSON array brackets
        while let Some(batch) = source.next_sql_batch().await? {
            for statement in batch {
                api::append_bounded_preview_statement(
                    &mut statements,
                    &mut response_bytes,
                    statement,
                )
                .map_err(|error| SyncExecutionError::Validation(error.to_string()))?;
            }
        }
        Ok(statements)
    }

    // ── 冲突检查 ────────────────────────────────────────────────────────

    /// `checkSyncConflicts`：持久化任务按 connectionId 重开专用会话，源侧
    /// 行数与冻结计数比对（桌面 `check_sync_conflicts` 同链）。
    pub async fn check_conflicts(
        &self,
        ctx: &RequestContext,
        task_id: &str,
    ) -> Result<serde_json::Value, SyncExecutionError> {
        let scope = scope_of(ctx);
        let tasks = self
            .tasks
            .list(&scope)
            .await
            .map_err(|error| SyncExecutionError::Storage(error.to_string()))?;
        let task = tasks
            .iter()
            .find(|t| t.id == task_id)
            .ok_or_else(|| SyncExecutionError::NotFound("Sync task not found".into()))?;

        let source = self
            .resolve_task_endpoint(
                ctx,
                &task.source_connection_id,
                task.source_database.as_deref(),
                task.source_schema.as_deref(),
                "source",
            )
            .await?;
        // Resolve the target as well so a stale or deleted target cannot make the
        // task appear safe to resume. No target rows are read by this command.
        let target = match self
            .resolve_task_endpoint(
                ctx,
                &task.target_connection_id,
                task.target_database.as_deref(),
                task.target_schema.as_deref(),
                "target",
            )
            .await
        {
            Ok(target) => target,
            Err(error) => {
                self.release_task_endpoint(ctx, &source).await;
                return Err(error);
            }
        };

        let result = async {
            let mut conflicts = Vec::<serde_json::Value>::new();
            for table in &task.tables {
                if task.completed_tables.contains(table) {
                    continue;
                }
                let original_count = task.source_row_counts.get(table).copied().unwrap_or(0);
                let current_count = api::count_table_rows(
                    source.driver.as_ref(),
                    &source.handle,
                    &source.config.database_type,
                    task.source_database
                        .as_deref()
                        .or(source.config.database.as_deref()),
                    task.source_schema
                        .as_deref()
                        .or(source.config.schema.as_deref()),
                    table,
                )
                .await
                .map_err(|error| SyncExecutionError::Validation(error.to_string()))?;
                if current_count != original_count {
                    conflicts.push(serde_json::json!({
                        "table": table,
                        "originalRows": original_count,
                        "currentRows": current_count,
                    }));
                }
            }
            Ok(serde_json::json!({
                "hasConflicts": !conflicts.is_empty(),
                "conflicts": conflicts,
            }))
        }
        .await;

        // Dedicated sessions are task-local resources. Release them on both the
        // successful result and every count error path.
        self.release_task_endpoint(ctx, &source).await;
        self.release_task_endpoint(ctx, &target).await;
        result
    }

    // ── 恢复核验 ────────────────────────────────────────────────────────

    /// `verifyDataSyncRecovery`：显式只读核验（受理 apply/prepare 类 Job 的
    /// PendingVerification 态；端点重新授权 + 证据记录，永不续写）。桌面
    /// `verify_data_sync_recovery` 同链。
    pub async fn verify_recovery(
        &self,
        ctx: &RequestContext,
        request: &SyncRecoveryRequest,
    ) -> Result<JobDetails, SyncExecutionError> {
        let details = self.details(ctx, &request.job_id).await?;
        if details.job.kind != APPLY_KIND && details.job.kind != PREPARE_KIND {
            return Err(SyncExecutionError::NotFound(
                "Data Sync recovery candidate was not found".into(),
            ));
        }
        if !details
            .recovery
            .as_ref()
            .is_some_and(|recovery| recovery.verdict == JobRecoveryVerdict::PendingVerification)
        {
            return Err(SyncExecutionError::Validation(
                "Data Sync job is not awaiting recovery verification".into(),
            ));
        }
        let sessions = own_sessions(
            &self.endpoints,
            ctx,
            &request.source_db_session_id,
            &request.target_db_session_id,
            &request.source_database,
            &request.target_database,
            request.source_schema.as_deref(),
            request.target_schema.as_deref(),
        )
        .await?;
        let verifier = Arc::new(ServerSyncRecoveryVerifier {
            endpoints: self.endpoints.clone(),
            ctx: ctx.clone(),
            sessions: sessions.ids(),
            targets: sessions.recovery_targets(),
            kind: details.job.kind.clone(),
        });
        let result = AssertUnwindSafe(self.host.verify_recovery(
            ctx,
            JobId::new(request.job_id.clone()),
            verifier,
        ))
        .catch_unwind()
        .await;
        release_sessions(&self.endpoints, ctx, &sessions.ids()).await;
        match result {
            Ok(result) => result.map_err(SyncExecutionError::from),
            Err(_) => Err(SyncExecutionError::Storage(
                "Data Sync recovery verification failed safely".into(),
            )),
        }
    }

    // ── 受理 / 读回基础件 ───────────────────────────────────────────────

    /// durable Job 受理（桌面 `admit_durable` 的服务端投影：先持久化后取
    /// 资源；**无预受理取消记忆**——服务端 job id 受理时铸造）。
    async fn admit(
        &self,
        ctx: &RequestContext,
        job_id: &str,
        kind: &'static str,
        payload: serde_json::Value,
        idempotency_key: &str,
        endpoints: &[EndpointRef],
    ) -> Result<Admission, SyncExecutionError> {
        self.host
            .ensure_endpoint_services(endpoints)
            .map_err(SyncExecutionError::from)?;
        let requested_job_id = JobId::new(job_id.to_string());
        let definition = JobDefinition {
            job_id: requested_job_id.clone(),
            organization_id: ctx.organization_id.clone(),
            kind: kind.to_string(),
            owner: OwnerRef::ClientSession {
                client_instance_id: ctx.client_instance_id.clone(),
                purpose: format!("data-sync:{kind}"),
            },
            payload,
            created_at: self.host.now(),
        };
        let record = self
            .host
            .accept(
                ctx,
                definition,
                &IdempotencyKey::new(idempotency_key.to_string()),
            )
            .await
            .map_err(SyncExecutionError::from)?;
        // 重放（同幂等键）返回既有 Job——job id 不同即不再派发第二个 worker。
        let dispatch =
            record.view.job_id == requested_job_id && record.view.state == JobState::Queued;
        Ok(Admission {
            job_id: record.view.job_id.clone(),
            view: record.view,
            endpoints: endpoints.to_vec(),
            dispatch,
        })
    }

    /// 读回（Failed 态宿主侧消息覆写——durable 记录只落安全原因码）。
    pub async fn view_of(
        &self,
        ctx: &RequestContext,
        job_id: &str,
    ) -> Result<JobView, SyncExecutionError> {
        let record = self
            .host
            .get(ctx, JobId::new(job_id.to_owned()))
            .await
            .map_err(SyncExecutionError::from)?;
        if !matches!(record.view.kind.as_str(), PREPARE_KIND | APPLY_KIND) {
            return Err(SyncExecutionError::NotFound(job_id.to_owned()));
        }
        let mut view = record.view;
        if view.state == JobState::Failed {
            if let Some(message) = self.runtime_state.safe_failure_of(job_id) {
                view.error = Some(message);
            }
        }
        Ok(view)
    }

    /// 详情读回（kind 过滤 + Failed 覆写；桌面 `details_durable_job`）。
    pub async fn details(
        &self,
        ctx: &RequestContext,
        job_id: &str,
    ) -> Result<JobDetails, SyncExecutionError> {
        let mut details = self
            .host
            .details(ctx, JobId::new(job_id.to_owned()))
            .await
            .map_err(SyncExecutionError::from)?;
        if !matches!(details.job.kind.as_str(), PREPARE_KIND | APPLY_KIND) {
            return Err(SyncExecutionError::NotFound(job_id.to_owned()));
        }
        if details.job.state == JobState::Failed {
            if let Some(message) = self.runtime_state.safe_failure_of(job_id) {
                details.job.error = Some(message);
            }
        }
        Ok(details)
    }

    /// 解析既有 apply 回执（消费计划的单次不变式：重放返回原 Job）。
    async fn find_apply_receipt(
        &self,
        ctx: &RequestContext,
        plan_id: &str,
    ) -> Result<Option<JobView>, SyncExecutionError> {
        let records = self
            .host
            .list(
                ctx,
                datazen::jobs::domain::job::JobFilter {
                    organization_id: ctx.organization_id.clone(),
                    states: Vec::new(),
                    owner: None,
                    after: None,
                    limit: None,
                },
            )
            .await
            .map_err(SyncExecutionError::from)?;
        Ok(records
            .into_iter()
            .find(|record| {
                record.view.kind == APPLY_KIND
                    && record
                        .definition
                        .payload
                        .get("consumedPlanId")
                        .and_then(serde_json::Value::as_str)
                        == Some(plan_id)
            })
            .map(|record| record.view))
    }

    /// 等待终态（桌面 `wait_for_durable_terminal` 同款轮询）。
    pub async fn wait_for_terminal(
        &self,
        ctx: &RequestContext,
        job_id: &str,
    ) -> Result<JobView, SyncExecutionError> {
        loop {
            let view = self.view_of(ctx, job_id).await?;
            if view.state.is_terminal() {
                return Ok(view);
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    // ── apply 前置校验 ──────────────────────────────────────────────────

    /// 桌面 `validate_plan_context`（计划自带的会话）。
    async fn validate_plan_context(
        &self,
        ctx: &RequestContext,
        plan: &StoredSyncPlan,
    ) -> Result<(), SyncExecutionError> {
        self.validate_plan_context_sessions(
            ctx,
            plan,
            &plan.source_db_session_id,
            &plan.target_db_session_id,
        )
        .await
    }

    /// Refuse an apply whose target can no longer be trusted, before the Job is
    /// submitted（桌面 `validate_plan_context_sessions` 逐字对齐）。
    async fn validate_plan_context_sessions(
        &self,
        ctx: &RequestContext,
        plan: &StoredSyncPlan,
        source_session_id: &str,
        target_session_id: &str,
    ) -> Result<(), SyncExecutionError> {
        let comparison = plan
            .comparison
            .summaries()
            .map_err(SyncExecutionError::Validation)?;
        if plan.target_read_only_at_preview {
            return Err(SyncExecutionError::Validation(
                "target connection was read-only during comparison; return to comparison".into(),
            ));
        }
        let source = self
            .endpoints
            .resolve(
                ctx,
                source_session_id,
                datazen::execution::application::LivePurpose::Inspection,
            )
            .await
            .map_err(endpoint_unavailable)?;
        let target = self
            .endpoints
            .resolve(
                ctx,
                target_session_id,
                datazen::execution::application::LivePurpose::Inspection,
            )
            .await
            .map_err(endpoint_unavailable)?;
        let source_config = source.config.clone();
        let target_config = target.config.clone();
        if target_config.read_only {
            return Err(SyncExecutionError::Validation(
                "target connection is now read-only; return to comparison".into(),
            ));
        }
        api::validate_active_database(
            source_config.database.as_deref(),
            &plan.source_database,
            "source",
        )
        .map_err(|error| SyncExecutionError::Validation(error.to_string()))?;
        api::validate_active_database(
            target_config.database.as_deref(),
            &plan.target_database,
            "target",
        )
        .map_err(|error| SyncExecutionError::Validation(error.to_string()))?;
        if source.driver.driver_type() != plan.source_driver_type
            || target.driver.driver_type() != plan.target_driver_type
            || plans::driver_protocol_version(source.driver.as_ref()) != plan.source_driver_protocol
            || plans::driver_protocol_version(target.driver.as_ref()) != plan.target_driver_protocol
        {
            return Err(SyncExecutionError::Validation(
                "driver contract changed since comparison; return to comparison".into(),
            ));
        }
        let source_identity = scoped_identity(
            &self.endpoints,
            ctx,
            source_session_id,
            Some((&plan.source_database, plan.source_schema.as_deref())),
        )
        .await?;
        let target_identity = scoped_identity(
            &self.endpoints,
            ctx,
            target_session_id,
            Some((&plan.target_database, plan.target_schema.as_deref())),
        )
        .await?;
        if plan
            .source_endpoint_identity
            .as_ref()
            .is_some_and(|expected| expected != &source_identity)
            || plan
                .target_endpoint_identity
                .as_ref()
                .is_some_and(|expected| expected != &target_identity)
        {
            return Err(SyncExecutionError::Validation(
                "Data Sync endpoint changed since comparison; compare again".into(),
            ));
        }
        let source_fingerprint = current_schema_fingerprint(
            source.driver.as_ref(),
            &source.handle,
            &plan.source_database,
            plan.source_schema.as_deref(),
            &comparison,
            true,
        )
        .await?;
        let target_fingerprint = current_schema_fingerprint(
            target.driver.as_ref(),
            &target.handle,
            &plan.target_database,
            plan.target_schema.as_deref(),
            &comparison,
            false,
        )
        .await?;
        if source_fingerprint != plan.source_schema_fingerprint {
            tracing::warn!(
                expected = %plan.source_schema_fingerprint,
                actual = %source_fingerprint,
                "sync plan source schema fingerprint changed"
            );
            return Err(SyncExecutionError::Validation(
                "source schema/key changed since comparison; return to comparison".into(),
            ));
        }
        if target_fingerprint != plan.target_schema_fingerprint {
            tracing::warn!(
                expected = %plan.target_schema_fingerprint,
                actual = %target_fingerprint,
                "sync plan target schema fingerprint changed"
            );
            return Err(SyncExecutionError::Validation(
                "target schema/key changed since comparison; return to comparison".into(),
            ));
        }
        Ok(())
    }

    // ── 冲突检查的端点解析 ──────────────────────────────────────────────

    /// 持久化任务端点解析（桌面 `tasks::resolve_task_endpoint`：按
    /// connectionId 重开专用会话；任务里的 runtime 会话 id 有意忽略——
    /// 它们是进程局部的，重启后可能指向另一个库）。
    async fn resolve_task_endpoint(
        &self,
        ctx: &RequestContext,
        connection_id: &str,
        database: Option<&str>,
        _schema: Option<&str>,
        label: &str,
    ) -> Result<ResolvedTaskEndpoint, SyncExecutionError> {
        let connection_id = connection_id.trim();
        if connection_id.is_empty() {
            return Err(SyncExecutionError::Validation(format!(
                "Sync task is missing its {label} connectionId; reopen the task and choose a connection"
            )));
        }
        use datazen::connection::domain::catalog::ConnectionRepository as _;
        let config = self
            .connections
            .get(
                &datazen::connection::domain::ConnectionScope::new(ctx.organization_id.as_str()),
                connection_id,
            )
            .await
            .map_err(|error| SyncExecutionError::Storage(error.to_string()))?
            .ok_or_else(|| {
                SyncExecutionError::NotFound(format!("connection '{connection_id}' not found"))
            })?;
        let requested_database = database
            .filter(|value| !value.trim().is_empty())
            .or(config.database.as_deref())
            .ok_or_else(|| {
                SyncExecutionError::Validation(format!(
                    "Sync task is missing its {label} database; reopen the task and choose a database"
                ))
            })?;
        let db_session_id = open_dedicated_session(
            &self.endpoints,
            ctx,
            connection_id,
            Some(requested_database),
            "syncTask",
        )
        .await
        .map_err(|_| {
            SyncExecutionError::Validation(format!(
                "could not open the persisted Sync task {label} session"
            ))
        })?;
        let context = match self
            .endpoints
            .resolve(
                ctx,
                &db_session_id,
                datazen::execution::application::LivePurpose::Inspection,
            )
            .await
        {
            Ok(context) => context,
            Err(error) => {
                let _ = close_dedicated_session(&self.endpoints, ctx, &db_session_id).await;
                return Err(endpoint_unavailable(error));
            }
        };
        let mut resolved_config = context.config.clone();
        resolved_config.database = context
            .view
            .observed_context
            .namespace
            .database
            .clone()
            .or(resolved_config.database);
        // Keep database/schema explicit in the count and in the validation error.
        // A persisted task may outlive an edited connection config; silently
        // counting a different catalog would produce a false conflict result.
        if let Some(expected) = database.filter(|value| !value.trim().is_empty()) {
            if resolved_config.database.as_deref() != Some(expected) {
                let _ = close_dedicated_session(&self.endpoints, ctx, &db_session_id).await;
                return Err(SyncExecutionError::Validation(format!(
                    "Sync task {label} database '{}' is not active on connection '{}'; reopen the task and compare again",
                    expected, connection_id
                )));
            }
        }
        Ok(ResolvedTaskEndpoint {
            db_session_id,
            driver: context.driver.clone(),
            handle: context.handle.clone(),
            config: resolved_config,
        })
    }

    async fn release_task_endpoint(&self, ctx: &RequestContext, endpoint: &ResolvedTaskEndpoint) {
        if let Err(error) =
            close_dedicated_session(&self.endpoints, ctx, &endpoint.db_session_id).await
        {
            tracing::warn!(
                db_session_id = %endpoint.db_session_id,
                %error,
                "Failed to release dedicated sync task session"
            );
        }
    }

    // ── 迁移运行历史（桌面 commands/history.rs 的 dataSync 臂）───────────

    /// 开始一段运行：落 `running` 行（best-effort），返回可覆写的记录。
    async fn start_run(
        &self,
        ctx: &RequestContext,
        operation: &str,
        profile: Option<&MigrationProfileRef>,
    ) -> MigrationRunRecord {
        let run = MigrationRunRecord {
            id: uuid::Uuid::new_v4().to_string(),
            operation: operation.into(),
            status: "running".into(),
            outcome: "pending".into(),
            phase: "execute".into(),
            profile_id: profile.map(|value| value.id.clone()),
            profile_revision: profile.map(|value| value.revision.clone()),
            source_connection_id: None,
            target_connection_id: None,
            started_at: chrono::Utc::now().to_rfc3339(),
            finished_at: None,
            selected_count: 0,
            committed_count: 0,
            failed_count: 0,
            conflict_count: 0,
            cancelled: false,
            rollback_outcome: "notRequired".into(),
            error_summary: None,
        };
        if let Err(error) = self
            .runs
            .save(ctx.organization_id.as_str(), run.clone())
            .await
        {
            tracing::warn!(%error, operation, "failed to persist migration run start");
        }
        run
    }

    /// Persist Data Sync's evidence-based outcome（桌面
    /// `finish_data_sync_migration_run` 逐字对齐——不改 Transfer / Schema
    /// Diff 对 `outcome` / `rollback_outcome` 的既有解释）。
    async fn finish_run(
        &self,
        ctx: &RequestContext,
        mut run: MigrationRunRecord,
        response: &DataSyncExecutionResponse,
        cancelled: bool,
    ) {
        let outcome = response.outcome;
        run.status = if cancelled {
            "cancelled"
        } else if outcome == datazen::migration::data_sync::ExecutionOutcome::Committed {
            "completed"
        } else {
            "failed"
        }
        .into();
        run.outcome = match outcome {
            datazen::migration::data_sync::ExecutionOutcome::NotStarted => "not_started",
            datazen::migration::data_sync::ExecutionOutcome::Committed => "committed",
            datazen::migration::data_sync::ExecutionOutcome::PartiallyApplied => {
                "partially_applied"
            }
            datazen::migration::data_sync::ExecutionOutcome::RolledBack => "rolled_back",
            datazen::migration::data_sync::ExecutionOutcome::Unknown => "unknown",
        }
        .into();
        run.phase = "finished".into();
        run.finished_at = Some(chrono::Utc::now().to_rfc3339());
        run.committed_count = if matches!(
            outcome,
            datazen::migration::data_sync::ExecutionOutcome::Committed
                | datazen::migration::data_sync::ExecutionOutcome::PartiallyApplied
        ) {
            response.result.applied as u64
        } else {
            0
        };
        run.failed_count = u64::from(matches!(
            outcome,
            datazen::migration::data_sync::ExecutionOutcome::NotStarted
                | datazen::migration::data_sync::ExecutionOutcome::RolledBack
                | datazen::migration::data_sync::ExecutionOutcome::PartiallyApplied
        ));
        run.conflict_count = response.result.conflicts.len() as u64;
        run.cancelled = cancelled;
        run.rollback_outcome = match outcome {
            datazen::migration::data_sync::ExecutionOutcome::NotStarted
            | datazen::migration::data_sync::ExecutionOutcome::Committed => "notRequired",
            datazen::migration::data_sync::ExecutionOutcome::RolledBack => "completed",
            datazen::migration::data_sync::ExecutionOutcome::PartiallyApplied => "partial",
            datazen::migration::data_sync::ExecutionOutcome::Unknown => "unknown",
        }
        .into();
        run.error_summary = match outcome {
            datazen::migration::data_sync::ExecutionOutcome::Committed => None,
            datazen::migration::data_sync::ExecutionOutcome::PartiallyApplied => Some(
                "Some batches committed before execution stopped. Compare current data before continuing.".into(),
            ),
            datazen::migration::data_sync::ExecutionOutcome::NotStarted => Some(
                "Execution did not start. Review the endpoint context and run a fresh comparison."
                    .into(),
            ),
            datazen::migration::data_sync::ExecutionOutcome::RolledBack => {
                Some("Execution did not commit; rollback was confirmed.".into())
            }
            datazen::migration::data_sync::ExecutionOutcome::Unknown => Some(
                "Commit or rollback could not be confirmed. Compare current data before continuing."
                    .into(),
            ),
        };
        if let Err(error) = self.runs.save(ctx.organization_id.as_str(), run).await {
            tracing::warn!(%error, "failed to persist Data Sync migration run completion");
        }
    }

    /// profile 引用校验（桌面 `validate_migration_profile_ref` 的 dataSync
    /// 臂：id 缺席 → 过；`updated_at`（RFC3339 解析等值）不匹配 → 拒绝）。
    async fn validate_profile_ref(
        &self,
        ctx: &RequestContext,
        reference: Option<&MigrationProfileRef>,
    ) -> Result<(), String> {
        let Some(reference) = reference else {
            return Ok(());
        };
        let profiles = self
            .sync_profiles
            .list(&scope_of(ctx))
            .await
            .map_err(|error| error.to_string())?;
        let actual = profiles
            .iter()
            .find(|profile| profile.id == reference.id)
            .map(|profile| profile.updated_at.to_rfc3339());
        let revisions_match = |actual: &str, expected: &str| {
            chrono::DateTime::parse_from_rfc3339(actual).ok()
                == chrono::DateTime::parse_from_rfc3339(expected).ok()
        };
        match actual.as_deref() {
            Some(actual) if revisions_match(actual, reference.revision.as_str()) => Ok(()),
            Some(_) => Err("Migration profile changed; review and prepare the plan again".into()),
            None => Err("Migration profile is missing; execution was blocked".into()),
        }
    }
}

/// 受理结果（dispatch = false 即幂等回执已指向既有 Job，重放不派发）。
struct Admission {
    job_id: JobId,
    view: JobView,
    endpoints: Vec<EndpointRef>,
    dispatch: bool,
}

/// prepare worker 的共享依赖束。
struct Worker {
    host: Arc<JobHost>,
    plans: Arc<dyn SyncPlanRepository>,
    comparison_root:
        Arc<dyn datazen::migration::data_sync::comparison_store::ComparisonStorageRoot>,
    endpoints: SessionEndpoints,
    ctx: RequestContext,
}

/// 派发 + 派发失败收尾（桌面 `dispatch_durable` 的 Err 路径逐字复刻：
/// Queued → `mark_failed_unstarted`；Running → `mark_pending_verification`
/// + `persist_recovery(dispatchNeedsVerification)`）。
async fn dispatch(
    host: &Arc<JobHost>,
    ctx: &RequestContext,
    job_id: JobId,
    endpoints: &[EndpointRef],
    handler: Arc<dyn datazen::jobs::JobHandler>,
) {
    let result = host
        .dispatch(ctx, &job_id, &WorkerId::new(WORKER), endpoints, handler)
        .await;
    let Err(error) = result else { return };
    let reason_code = match &error {
        PortError::QuotaExceeded(_) => "budgetDenied",
        PortError::StaleClaim => "staleClaim",
        PortError::CasConflict { .. } => "stateConflict",
        PortError::UnsupportedVersion(_) => "unsupportedHandler",
        PortError::PlanAlreadyConsumed(_) => "planAlreadyConsumed",
        PortError::IdempotencyConflict => "idempotencyConflict",
        _ => "dispatchNotStarted",
    };
    let repository = host.repository();
    match host.get(ctx, job_id.clone()).await {
        Ok(record) if record.view.state == JobState::Queued => {
            let _ = repository
                .mark_failed_unstarted(ctx, &job_id, reason_code)
                .await;
        }
        Ok(record) if record.view.state == JobState::Running => {
            if repository
                .mark_pending_verification(ctx, &job_id, "dispatchNeedsVerification")
                .await
                .is_ok()
            {
                let _ = repository
                    .persist_recovery(
                        ctx,
                        &job_id,
                        JobRecoveryResult {
                            verdict: JobRecoveryVerdict::PendingVerification,
                            resume_through: None,
                            reason_code: Some("dispatchNeedsVerification".into()),
                        },
                    )
                    .await;
            }
        }
        _ => {}
    }
    tracing::error!(
        job_id = %job_id.as_str(),
        reason_code,
        cause = ?error,
        "Data Sync Job dispatch failed"
    );
}

/// 冲突检查的已解析任务端点（专用会话 + 驱动句柄）。
struct ResolvedTaskEndpoint {
    db_session_id: String,
    driver: Arc<dyn DatabaseDriver>,
    handle: datazen_driver_api::ConnectionHandle,
    config: datazen_driver_api::ConnectionConfig,
}

fn endpoint_unavailable(error: crate::schema_diff::endpoints::EndpointError) -> SyncExecutionError {
    match error {
        crate::schema_diff::endpoints::EndpointError::NotVisible => SyncExecutionError::Validation(
            "The addressed resource does not exist or is not visible to you.".into(),
        ),
        crate::schema_diff::endpoints::EndpointError::ProfileUnavailable => {
            SyncExecutionError::Validation("Connection profile unavailable".into())
        }
        crate::schema_diff::endpoints::EndpointError::Unavailable(source) => {
            SyncExecutionError::Validation(format!("session endpoint unavailable: {source}"))
        }
    }
}

/// Project one already-inspected matched table back into the frozen
/// `TableMapping` carried by the prepare Job（桌面 `mapping_of` 同款）。
fn mapping_of(
    table: &(String, String),
    source_filters: &HashMap<String, SyncSourceFilter>,
) -> TableMapping {
    TableMapping {
        source_table: table.0.clone(),
        target_table: table.1.clone(),
        enabled: true,
        matching_columns: Vec::new(),
        source_filter: source_filters.get(&table.0).cloned(),
    }
}

/// 比较请求的选项必须与评审时一致（桌面 `validate_requested_options`）。
fn validate_requested_options(
    plan: &StoredSyncPlan,
    options: &SyncOptions,
) -> Result<(), SyncExecutionError> {
    options
        .validate()
        .map_err(|error| SyncExecutionError::Validation(error.to_string()))?;
    if options.matching_strategy != plan.options.matching_strategy
        || options.large_value_mode != plan.options.large_value_mode
        || options.conflict_policy != plan.options.conflict_policy
        || plans::fingerprint_conflict_policy(options.conflict_policy)
            != plan.conflict_policy_fingerprint
    {
        return Err(SyncExecutionError::Validation(
            "comparison options changed; return to comparison".into(),
        ));
    }
    Ok(())
}

/// 当前 schema 指纹（桌面 `current_schema_fingerprint`：逐表快照 +
/// `fingerprint_relations_with_filters`）。
async fn current_schema_fingerprint(
    driver: &dyn DatabaseDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    database: &str,
    schema: Option<&str>,
    comparison: &[ComparisonTableMetadata],
    source: bool,
) -> Result<String, SyncExecutionError> {
    let mut entries = Vec::new();
    for table in comparison.iter().filter(|table| {
        table.table.status == datazen::migration::data_sync::TableMappingStatus::Matched
    }) {
        let relation = if source {
            &table.table.source_table
        } else {
            &table.table.target_table
        };
        let schema_snapshot = driver
            .get_table_schema(handle, relation, database, schema)
            .await;
        let schema_snapshot = schema_snapshot.map_err(|_| {
            SyncExecutionError::Validation(format!(
                "cannot confirm current {} schema metadata; return to comparison",
                if source { "source" } else { "target" }
            ))
        })?;
        entries.push((
            relation.clone(),
            Some(schema_snapshot),
            table.table.source_filter.clone(),
        ));
    }
    plans::fingerprint_relations_with_filters(database, schema, entries)
        .map_err(SyncExecutionError::Validation)
}

/// SQL 预览的分页行源（桌面 `ComparisonStorageSqlPageSource`）。
struct ComparisonPageSource<'a> {
    execution: &'a SyncExecution,
    ctx: &'a RequestContext,
    target_db_session_id: String,
    comparison: ComparisonStorage,
    matcher: SelectionMatcher,
    options: SyncOptions,
    target_database: String,
    target_schema: Option<String>,
    summaries: Vec<ComparisonTableMetadata>,
    table_index: usize,
    row_offset: usize,
}

impl ComparisonPageSource<'_> {
    async fn next_sql_batch(&mut self) -> Result<Option<Vec<SqlStatement>>, SyncExecutionError> {
        while self.table_index < self.summaries.len() {
            let table = self.summaries[self.table_index].clone();
            if table.table.status != datazen::migration::data_sync::TableMappingStatus::Matched
                || self.row_offset >= table.row_count
            {
                self.table_index += 1;
                self.row_offset = 0;
                continue;
            }

            let rows = self
                .comparison
                .load_table_page(
                    &table.table.source_table,
                    &table.table.target_table,
                    self.row_offset,
                    plans::SYNC_COMPARISON_STREAM_PAGE_SIZE,
                )
                .map_err(SyncExecutionError::Validation)?;
            if rows.is_empty() {
                return Err(SyncExecutionError::Validation(
                    "comparison page did not advance while generating SQL".into(),
                ));
            }
            self.row_offset = self.row_offset.saturating_add(rows.len());
            let Some(table_page) =
                plans::selected_table_page(&table, rows, &self.matcher, &self.options)
                    .map_err(SyncExecutionError::Validation)?
            else {
                continue;
            };
            let statements = self
                .execution
                .generate_table_sql(
                    self.ctx,
                    &self.target_db_session_id,
                    vec![table_page],
                    self.options.clone(),
                    Some(self.target_database.clone()),
                    self.target_schema.clone(),
                )
                .await?;
            if !statements.is_empty() {
                return Ok(Some(statements));
            }
        }
        Ok(None)
    }
}

/// 终态投影（桌面 `SyncJobOutcome` 减 job_id——服务端读回按受理返回的
/// id，不在结果里回带）。
struct SyncJobOutcome {
    state: JobState,
    effect: EffectOutcome,
    committed: u64,
    message: Option<String>,
}

/// apply 终态的失败分类（桌面 `CommandError` 的 DataSync 三臂：
/// `DataSyncPartiallyApplied` / `DataSyncOutcomeUnknown` / `DataSyncNotStarted`）。
enum ApplyFailure {
    NotStarted(String),
    PartiallyApplied(u64),
    OutcomeUnknown(String),
    StillRunning(String),
}

impl From<SyncExecutionError> for ApplyFailure {
    fn from(error: SyncExecutionError) -> Self {
        ApplyFailure::NotStarted(error.to_string())
    }
}

/// Map the Job's terminal projection onto the IPC contract（桌面
/// `applied_outcome` 逐字对齐——§7 决定每一条分支：NotStarted/RolledBack 是
/// 拒绝，Unknown 是栅栏（永不报「未开始」），PartiallyApplied 是带警告的
/// 成功（已提交的行无法由调用方回卷））。
fn applied_outcome(
    outcome: Result<SyncJobOutcome, SyncExecutionError>,
) -> Result<ExecutionResult, ApplyFailure> {
    let applied = outcome.map_err(|error| ApplyFailure::NotStarted(error.to_string()))?;
    let committed = applied.committed as usize;
    let message = applied.message.clone();
    match applied.state {
        JobState::Succeeded => Ok(ExecutionResult {
            applied: committed,
            affected_rows: applied.committed,
            rolled_back: false,
            rollback_reason: None,
            skipped: 0,
            conflicts: Vec::new(),
        }),
        JobState::Cancelled => Ok(ExecutionResult {
            applied: committed,
            affected_rows: applied.committed,
            rolled_back: applied.effect == EffectOutcome::RolledBack,
            rollback_reason: message,
            skipped: 0,
            conflicts: Vec::new(),
        }),
        JobState::Failed if applied.effect == EffectOutcome::RolledBack => Ok(ExecutionResult {
            applied: 0,
            affected_rows: 0,
            rolled_back: true,
            rollback_reason: Some("Execution did not commit; rollback was confirmed.".into()),
            skipped: 0,
            conflicts: Vec::new(),
        }),
        JobState::Failed => Err(match applied.effect {
            EffectOutcome::Unknown => ApplyFailure::OutcomeUnknown(message.unwrap_or_else(|| {
                "commit or rollback could not be confirmed; compare current data before continuing"
                    .to_string()
            })),
            EffectOutcome::PartiallyApplied => ApplyFailure::PartiallyApplied(applied.committed),
            _ => ApplyFailure::NotStarted(
                message.unwrap_or_else(|| "the apply job did not start; compare again".to_string()),
            ),
        }),
        JobState::Queued | JobState::Running => {
            Err(ApplyFailure::StillRunning(message.unwrap_or_else(|| {
                "the apply job was still running when the request returned".to_string()
            })))
        }
    }
}

/// execute 的响应 + 取消标记（桌面 `execution_response_from_result` +
/// `execution_response_and_cancelled` 的折叠：PartiallyApplied/Unknown 是
/// **带分类的成功应答**，不是错误——响应体携带证据，调用方决定下一步）。
fn execution_response_and_cancelled(
    result: Result<ExecutionResult, ApplyFailure>,
) -> (DataSyncExecutionResponse, bool) {
    match result {
        Ok(result) => {
            let cancelled = result
                .rollback_reason
                .as_deref()
                .is_some_and(|message| message.starts_with("execute cancelled"));
            (DataSyncExecutionResponse::from_result(result), cancelled)
        }
        Err(ApplyFailure::PartiallyApplied(committed)) => (
            DataSyncExecutionResponse::partially_applied(committed),
            false,
        ),
        Err(ApplyFailure::OutcomeUnknown(message)) => {
            (DataSyncExecutionResponse::unknown(message), false)
        }
        Err(ApplyFailure::NotStarted(message)) => {
            let cancelled = message.starts_with("execute cancelled");
            (
                DataSyncExecutionResponse::failed_before_start(message),
                cancelled,
            )
        }
        Err(ApplyFailure::StillRunning(message)) => (
            DataSyncExecutionResponse::failed_before_start(message),
            false,
        ),
    }
}

impl SyncExecution {
    /// 表级 SQL 装配（桌面 `generate_data_sync_sql_for_tables`：会话/适配器
    /// 解析、逐表读 schema、投影查找；装配本身在共享层
    /// `api::generate_write_sql_for_table`，与复核批次共用）。
    #[allow(clippy::too_many_arguments)]
    async fn generate_table_sql(
        &self,
        ctx: &RequestContext,
        target_db_session_id: &str,
        tables: Vec<TableResult>,
        options: SyncOptions,
        target_database: Option<String>,
        target_schema: Option<String>,
    ) -> Result<Vec<SqlStatement>, SyncExecutionError> {
        options
            .validate()
            .map_err(|error| SyncExecutionError::Validation(error.to_string()))?;
        let comparison = ComparisonResult::new(tables);
        let set = ChangeSet::from_comparison("server-preview", &comparison, &options);
        set.validate_executable()
            .map_err(|error| SyncExecutionError::Validation(error.to_string()))?;

        let target = self
            .endpoints
            .resolve(
                ctx,
                target_db_session_id,
                datazen::execution::application::LivePurpose::Inspection,
            )
            .await
            .map_err(endpoint_unavailable)?;
        let tgt_config = target.config.clone();
        let family = require_data_sync_family(&tgt_config.database_type, &tgt_config.database_type)
            .map_err(|error| SyncExecutionError::Validation(error.to_string()))?;
        let tgt_driver = target.driver.clone();
        // 占位符规则来自 SqlDialect 契约，旧驱动 API 逐条转发。
        let dialect = datazen_driver_api::LegacySqlDialect::new(tgt_driver.as_ref());
        let target_database_name =
            api::resolve_db_name(target_database.as_deref(), tgt_config.database.as_deref());
        let target_schema = metadata_schema(
            tgt_driver.as_ref(),
            target_schema.as_deref(),
            None,
            tgt_config.schema.as_deref(),
        );
        self.adapters
            .ensure_type(&tgt_config.database_type)
            .map_err(SyncExecutionError::Validation)?;
        let preview_source = self
            .adapters
            .get_source(&tgt_config.database_type)
            .ok_or_else(|| {
                SyncExecutionError::Validation(
                    "target driver has no SQL preview type adapter".into(),
                )
            })?;
        let preview_target = self
            .adapters
            .get_target(&tgt_config.database_type)
            .ok_or_else(|| {
                SyncExecutionError::Validation(
                    "target driver has no SQL preview literal renderer".into(),
                )
            })?;

        let mut statements = Vec::new();
        for table in &set.tables {
            let schema = tgt_driver
                .get_table_schema(
                    &target.handle,
                    &table.target_table,
                    &target_database_name,
                    target_schema.as_deref(),
                )
                .await
                .map_err(|error| SyncExecutionError::Validation(error.to_string()))?;
            let projection = comparison
                .tables
                .iter()
                .find(|t| {
                    t.source_table == table.source_table && t.target_table == table.target_table
                })
                .ok_or_else(|| {
                    SyncExecutionError::Validation(
                        "comparison projection missing; compare again".into(),
                    )
                })?;
            let ctx = api::WriteSqlContext {
                family: &family,
                database: &target_database_name,
                schema: target_schema.as_deref(),
                schema_obj: &schema,
                preview_source: preview_source.as_ref(),
                preview_target: preview_target.as_ref(),
                dialect: &dialect,
                identity_insert_requires_session_toggle: tgt_driver
                    .explicit_identity_insert_requires_session_toggle(),
            };
            // sqlserver 预检、投影复验、preview 字面量、限定名与 identity 盖章
            // 全部在 modules 的同一份装配里（F4c-1），与复核批次路径共用。
            let stmts = api::generate_write_sql_for_table(
                &ctx,
                table,
                &projection.columns,
                &projection.column_types,
                &projection.primary_keys,
                options.conflict_policy,
            )
            .map_err(|error| SyncExecutionError::Validation(error.to_string()))?;
            statements.extend(stmts);
        }
        Ok(statements)
    }
}

/// 恢复核验器（桌面 `DataSyncRecoveryVerifier` 同款：端点重授权 + 活会话
/// 证据；durable 回执不含行值与 SQL，端点匹配证明不了行效果——记录证据
/// 并要求人工比较，永不续写）。
struct ServerSyncRecoveryVerifier {
    endpoints: SessionEndpoints,
    ctx: RequestContext,
    sessions: Vec<String>,
    targets: Vec<JobRecoveryTarget>,
    kind: String,
}

#[async_trait::async_trait]
impl JobRecoveryVerifier for ServerSyncRecoveryVerifier {
    fn kind(&self) -> &str {
        &self.kind
    }

    async fn verify(
        &self,
        _context: &RequestContext,
        request: &JobRecoveryRequest,
    ) -> Result<JobRecoveryVerification, datazen::PortError> {
        if request.details.recovery_targets != self.targets || self.sessions.len() != 2 {
            return Ok(manual_review("reauthorizedEndpointScopeMismatch"));
        }
        let runtime = self
            .endpoints
            .lease
            .resolve()
            .await
            .map_err(|error| PortError::BackendUnavailable(error))?;
        let live = futures_util::future::try_join_all(
            self.sessions
                .iter()
                .map(|id| runtime.get_session_by_id(&self.ctx, id)),
        )
        .await;
        if live.is_err() {
            return Ok(manual_review("reauthorizedSessionUnavailable"));
        }
        // The durable Sync receipt intentionally omits row values and SQL, so
        // an endpoint match cannot prove the row effects. Record the explicit
        // live-session evidence and require a human compare before any write.
        Ok(manual_review("syncEffectsRequireFreshComparison"))
    }
}

fn manual_review(reason_code: &str) -> JobRecoveryVerification {
    JobRecoveryVerification {
        result: JobRecoveryResult {
            verdict: JobRecoveryVerdict::RequireManualReview,
            resume_through: None,
            reason_code: Some(reason_code.to_string()),
        },
        confirmed_boundaries: Vec::new(),
        domain_results: Vec::new(),
    }
}

/// prepare 成功后的复核投影物化（桌面 `finish_async_prepare_preview`
/// 逐字对齐：工件 → Comparison 重建 → 跳过表合并 → 顺序恢复 → 指纹 →
/// 计划签发 → 端点身份绑定）。
#[allow(clippy::too_many_arguments)]
async fn finish_prepare_preview(
    worker: &Worker,
    job_id: &str,
    plan_id: &str,
    source_plan_session_id: &str,
    target_plan_session_id: &str,
    source_job_session_id: &str,
    target_job_session_id: &str,
    source_database: &str,
    target_database: &str,
    source_schema: Option<&str>,
    target_schema: Option<&str>,
    options: SyncOptions,
    skipped: Vec<TableResult>,
    order: Vec<(String, String)>,
    target_read_only: bool,
    source_identity: EndpointIdentity,
    target_identity: EndpointIdentity,
) -> Result<(), SyncExecutionError> {
    let scope = OrganizationScope {
        organization_id: worker.ctx.organization_id.clone(),
    };
    let artifact = worker.plans.load_artifact(&scope, plan_id).ok_or_else(|| {
        SyncExecutionError::Validation("the prepare job produced no frozen ChangeSet".into())
    })?;
    let mut rebuilt = comparison_from_artifact(&artifact);
    rebuilt.tables.extend(skipped);
    rebuilt.tables.sort_by_key(|table| {
        order
            .iter()
            .position(|(source, target)| {
                *source == table.source_table && *target == table.target_table
            })
            .unwrap_or(usize::MAX)
    });
    // 一次调用只取一次作用域：比较存储与计划注册表必须落在同一个组织维度上。
    let comparison =
        ComparisonStorage::from_comparison(rebuilt, worker.comparison_root.as_ref(), &scope)
            .map_err(SyncExecutionError::Validation)?;
    let source = worker
        .endpoints
        .resolve(
            &worker.ctx,
            source_job_session_id,
            datazen::execution::application::LivePurpose::Inspection,
        )
        .await
        .map_err(endpoint_unavailable)?;
    let target = worker
        .endpoints
        .resolve(
            &worker.ctx,
            target_job_session_id,
            datazen::execution::application::LivePurpose::Inspection,
        )
        .await
        .map_err(endpoint_unavailable)?;
    let source_driver = source.driver.clone();
    let target_driver = target.driver.clone();
    let mut source_entries = Vec::new();
    let mut target_entries = Vec::new();
    for table in comparison
        .summaries()
        .map_err(SyncExecutionError::Validation)?
        .into_iter()
        .filter(|table| {
            table.table.status == datazen::migration::data_sync::TableMappingStatus::Matched
        })
    {
        let source_schema_snapshot = source_driver
            .get_table_schema(
                &source.handle,
                &table.table.source_table,
                source_database,
                source_schema,
            )
            .await
            .map_err(|error| SyncExecutionError::Validation(error.to_string()))?;
        let target_schema_snapshot = target_driver
            .get_table_schema(
                &target.handle,
                &table.table.target_table,
                target_database,
                target_schema,
            )
            .await
            .map_err(|error| SyncExecutionError::Validation(error.to_string()))?;
        source_entries.push((
            table.table.source_table.clone(),
            Some(source_schema_snapshot),
            table.table.source_filter.clone(),
        ));
        target_entries.push((
            table.table.target_table.clone(),
            Some(target_schema_snapshot),
            table.table.source_filter.clone(),
        ));
    }
    let source_fingerprint =
        plans::fingerprint_relations_with_filters(source_database, source_schema, source_entries)
            .map_err(SyncExecutionError::Validation)?;
    let target_fingerprint =
        plans::fingerprint_relations_with_filters(target_database, target_schema, target_entries)
            .map_err(SyncExecutionError::Validation)?;
    worker
        .plans
        .issue(
            &scope,
            source_driver.as_ref(),
            target_driver.as_ref(),
            SyncPlanIssue {
                plan_id: Some(plan_id.to_string()),
                selection_revision: 1,
                source_db_session_id: source_plan_session_id.to_string(),
                target_db_session_id: target_plan_session_id.to_string(),
                source_database: source_database.to_string(),
                target_database: target_database.to_string(),
                source_schema: source_schema.map(str::to_string),
                target_schema: target_schema.map(str::to_string),
                source_schema_fingerprint: source_fingerprint,
                target_schema_fingerprint: target_fingerprint,
                comparison,
                options,
                target_read_only_at_preview: target_read_only,
            },
        )
        .map_err(SyncExecutionError::Validation)?;
    worker
        .plans
        .attach_endpoint_identities(&scope, plan_id, source_identity, target_identity)
        .map_err(SyncExecutionError::Validation)?;
    tracing::debug!(job_id, "Data Sync review plan is ready");
    Ok(())
}

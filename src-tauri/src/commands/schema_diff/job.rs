//! P5 Wave-1：Schema Diff 窗口命令经 JobRuntime 执行（`p5-schema-diff` 轨）。
//!
//! 结构：
//!
//! * [`SchemaDiffJobInfra`]：进程级 Job 基础设施（仓储/预算/计划库）。
//! * [`AppStateBackend`]：[`SchemaDiffJobBackend`] 的宿主实现。
//! * [`run_prepare_job`] / [`run_apply_job`]：IPC 命令的 JobRuntime 入口。
//!
//! 不变量（§2.1）：同一请求只走一种管理器——IPC 一律构造 Job、注册 handler、
//! 跑 JobRuntime，不直接调旧 prepare/deploy 管理器。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{JobDefinition, JobState};
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{
    ClientInstanceId, ConnectionId, IdempotencyKey, JobId, OrganizationId, PrincipalId, RequestId,
    WorkerId,
};
use datazen_platform_api::ports::budget::ServiceQuota;
use datazen_platform_api::ports::job::JobRepository;

use datazen_runtime::budget::{BudgetConfig, BudgetLedger};
use datazen_runtime::job::{
    EndpointRef, EndpointRole, HandlerRegistry, InMemoryJobRepository, JobClock, JobHandler,
    JobRuntime,
};

use datazen_schema_diff::job::{
    ApplyRequest, PlanStore, PrepareRequest, PreparedPlan, ReadOnlyVerdict, SchemaDiffFrozenPlan,
    SchemaDiffHandler, SchemaDiffJobBackend, SchemaDiffPlanError, StoredPlan,
};
use datazen_schema_diff::types::{SchemaDiffDeployResult, SchemaDiffPlan};

use crate::commands::error::{CmdExt, CommandError};
use crate::commands::schema_diff::{
    execute_schema_diff_deploy_impl, fetch_target_table_schema,
    prepare_schema_diff_plan_with_schemas_impl,
};
use crate::commands::schema_diff::unified_plan::prepare_schema_unified_plan_impl;
use crate::AppState;

/// 进程级 Job 基础设施。置于 [`AppState`] 中，驻留整个应用生命周期：
/// Job 关闭后仍保留终态与 effect 投影（§8：窗口关闭不释放 Job 资源）。
pub struct SchemaDiffJobInfra {
    pub repo: Arc<InMemoryJobRepository>,
    pub ledger: Arc<Mutex<BudgetLedger>>,
    pub plans: Arc<PlanStore>,
}

impl SchemaDiffJobInfra {
    pub fn new() -> Self {
        let clock = Arc::new(SystemJobClock);
        let quota = ServiceQuota::new(64, [4, 4, 4, 0])
            .expect("schema-diff job budget quota must be valid");
        Self {
            repo: Arc::new(InMemoryJobRepository::new(clock, 300)),
            ledger: Arc::new(Mutex::new(BudgetLedger::new(BudgetConfig::new(quota)))),
            plans: Arc::new(PlanStore::new()),
        }
    }

    fn now_ms(&self) -> u64 {
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        epoch.as_millis() as u64
    }
}

/// 真实时间 JobClock（生产路径）。
pub struct SystemJobClock;

impl JobClock for SystemJobClock {
    fn now(&self) -> datazen_platform_api::id::Timestamp {
        datazen_platform_api::id::Timestamp::new(
            chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string(),
        )
    }
}

fn fnv_hex_of_json<T: serde::Serialize>(v: &T) -> String {
    let bytes = serde_json::to_vec(v).unwrap_or_default();
    datazen_schema_diff::job::fnv1a64_hex(&bytes)
}


fn source_session_id(request: &PrepareRequest) -> String {
    match request {
        PrepareRequest::Table {
            source_db_session_id, ..
        } => source_db_session_id.clone(),
        PrepareRequest::Unified {
            source_db_session_id, ..
        } => source_db_session_id.clone(),
    }
}

fn target_session_id(request: &PrepareRequest) -> String {
    match request {
        PrepareRequest::Table {
            target_db_session_id, ..
        } => target_db_session_id.clone(),
        PrepareRequest::Unified {
            target_db_session_id, ..
        } => target_db_session_id.clone(),
    }
}

/// §4.2 桌面确认边界：运行时仓配化的恢复策略字符串（§2.2）。
fn recovery_policy_str(transactional_ddl: bool) -> &'static str {
    if transactional_ddl {
        "inTransactionBatch"
    } else {
        "readOnlyVerify"
    }
}

/// 目标结构的 before 指纹：当前目标表的 TableSchema 集合哈希。
/// 空表集（纯对象计划）退化为渲染语句集合的哈希（弱指纹，在目标结构
/// 无表对象可读时采用）。
async fn compute_target_fingerprint(
    state: &AppState,
    target_db_session_id: &str,
    target_database: Option<&str>,
    target_schema: Option<&str>,
    tables: &[String],
    fallback_fns: &[String],
) -> Result<String, CommandError> {
    let (driver, handle) = state
        .connection_manager
        .get_session(target_db_session_id)
        .await
        .cmd_err("compute_target_fingerprint")?;
    let mut schemas = Vec::new();
    for table in tables {
        let schema = fetch_target_table_schema(
            driver.as_ref(),
            &handle,
            table,
            target_database.unwrap_or(""),
            target_schema,
        )
        .await?;
        schemas.push(schema);
    }
    if schemas.is_empty() {
        return Ok(fnv_hex_of_json(&fallback_fns));
    }
    Ok(fnv_hex_of_json(&schemas))
}

/// capability/版本/credential 快照（§2.2）：当前连接配置的可核验身份。
async fn current_capability_hash(
    state: &AppState,
    target_db_session_id: &str,
) -> Result<String, CommandError> {
    let config = state
        .connection_manager
        .get_session_config(target_db_session_id)
        .await
        .cmd_err("current_capability_hash")?;
    let owner = state
        .connection_manager
        .owner_connection_id(target_db_session_id)
        .await;
    let read_only = match owner.as_ref() {
        Some(owner) => {
            let persisted = state.store.get_connection(owner).await;
            config.read_only
                || persisted
                    .as_ref()
                    .map(|p| p.read_only)
                    .unwrap_or(false)
        }
        None => config.read_only,
    };
    Ok(fnv_hex_of_json(&serde_json::json!({
        "databaseType": config.database_type,
        "database": config.database,
        "schema": config.schema,
        "readOnly": read_only,
        "owner": owner,
    })))
}

/// 宿主 [`SchemaDiffJobBackend`] 实现。
pub struct AppStateBackend {
    state: Arc<AppState>,
    plans: Arc<PlanStore>,
    last_prepared: Arc<Mutex<Option<(String, SchemaDiffFrozenPlan)>>>,
}

impl AppStateBackend {
    pub fn new(state: Arc<AppState>, plans: Arc<PlanStore>) -> Self {
        Self {
            state,
            plans,
            last_prepared: Arc::new(Mutex::new(None)),
        }
    }

    pub fn last_prepared(&self) -> Option<(String, SchemaDiffFrozenPlan)> {
        self.last_prepared
            .lock()
            .ok()
            .and_then(|g| g.clone())
    }
}

#[async_trait]
impl SchemaDiffJobBackend for AppStateBackend {
    async fn prepare_plan(
        &self,
        request: &PrepareRequest,
        _cancel: &datazen_runtime::job::CancelToken,
    ) -> Result<PreparedPlan, SchemaDiffPlanError> {
        let (plan, tgt_sess, target_db, target_schema) = match request {
            PrepareRequest::Table {
                source_db_session_id,
                target_db_session_id,
                table_names,
                target_table_names,
                target_only_table_names,
                source_schema,
                target_schema,
                allow_destructive,
                include_indexes,
                type_overrides,
            } => {
                let plan = prepare_schema_diff_plan_with_schemas_impl(
                    &self.state,
                    source_db_session_id.clone(),
                    target_db_session_id.clone(),
                    table_names.clone(),
                    target_table_names.clone(),
                    target_only_table_names.clone(),
                    *allow_destructive,
                    *include_indexes,
                    Some(type_overrides.clone()),
                    source_schema.clone(),
                    target_schema.clone(),
                )
                .await
                .map_err(|e| SchemaDiffPlanError::PlanNotFound(e.to_string()))?;
                (plan, target_db_session_id.clone(), None, target_schema.clone())
            }
            PrepareRequest::Unified {
                source_db_session_id,
                target_db_session_id,
                table_names,
                target_table_names,
                target_only_table_names,
                source_objects,
                target_objects,
                source_schema,
                target_schema,
                allow_destructive,
                include_indexes,
                type_overrides,
            } => {
                let plan = prepare_schema_unified_plan_impl(
                    &self.state,
                    source_db_session_id.clone(),
                    target_db_session_id.clone(),
                    table_names.clone(),
                    Some(target_table_names.clone()),
                    Some(target_only_table_names.clone()),
                    Some(source_objects.clone()),
                    Some(target_objects.clone()),
                    source_schema.clone(),
                    target_schema.clone(),
                    *allow_destructive,
                    *include_indexes,
                    Some(type_overrides.clone()),
                )
                .await
                .map_err(|e| SchemaDiffPlanError::PlanNotFound(e.to_string()))?;
                (plan, target_db_session_id.clone(), None, target_schema.clone())
            }
        };
        let plan_id = plan
            .plan_id
            .clone()
            .ok_or_else(|| SchemaDiffPlanError::PlanNotFound("planId missing".into()))?;
        let fingerprint = compute_target_fingerprint(
            &self.state,
            &tgt_sess,
            target_db.as_deref(),
            target_schema.as_deref(),
            &plan.tables,
            &plan
                .statements
                .iter()
                .map(|s| s.sql.clone())
                .collect::<Vec<_>>(),
        )
        .await
        .map_err(|e| SchemaDiffPlanError::PlanStale(e.to_string()))?;
        let capability_hash = current_capability_hash(&self.state, &tgt_sess)
            .await
            .map_err(|e| SchemaDiffPlanError::CapabilityChanged)?;
        let transactional_ddl = plan
            .statements
            .iter()
            .any(|s| s.requires_transaction)
            || plan.source_dialect.contains("sqlite")
            || plan.source_dialect.contains("postgres");
        let meta = SchemaDiffFrozenPlan {
            plan_id: plan_id.clone(),
            plan_version: 1,
            handler_version: 1,
            checkpoint_version: 1,
            selection_revision: 1,
            created_at: datazen_platform_api::id::Timestamp::new(
                chrono::Utc::now()
                    .format("%Y-%m-%dT%H:%M:%SZ")
                    .to_string(),
            ),
            expires_at: datazen_platform_api::id::Timestamp::new(
                (chrono::Utc::now() + chrono::Duration::hours(2))
                    .format("%Y-%m-%dT%H:%M:%SZ")
                    .to_string(),
            ),
            source_connection_id: source_session_id(request),
            target_connection_id: target_session_id(request),
            source_database: None,
            target_database: target_db,
            source_schema: None,
            target_schema,
            endpoint_evidence: vec![
                format!("target:{}", tgt_sess),
                format!("planTables:{}", plan.tables.join(",")),
            ],
            capability_snapshot_hash: capability_hash,
            schema_fingerprint: fingerprint,
            mapping_fingerprint: fnv_hex_of_json(&plan.tables),
            consistency: "tableSnapshot".into(),
            transaction_scope: if transactional_ddl {
                "task".into()
            } else {
                "nonAtomicDDL".into()
            },
            recovery_policy: datazen_schema_diff::job::RecoveryPolicy::parse(
                recovery_policy_str(transactional_ddl),
            )
            .ok_or(SchemaDiffPlanError::RecoveryPolicyInvalid("policy parse".into()))?,
            body_artifact_ids: vec![format!("artifact:plan:{plan_id}")],
            body_digests: vec![fnv_hex_of_json(&plan.statements)],
            confirmed_actions: Vec::new(),
        };
        let body_digest = fnv_hex_of_json(&plan.statements);
        *self.last_prepared.lock().map_err(|_| {
            SchemaDiffPlanError::PlanNotFound("last_prepared lock".into())
        })? = Some((plan_id.clone(), meta.clone()));
        Ok(PreparedPlan {
            meta,
            plan,
            body_digest,
        })
    }

    async fn verify_authorization(
        &self,
        plan_meta: &SchemaDiffFrozenPlan,
    ) -> Result<(), SchemaDiffPlanError> {
        // verify_authorization 需要目标会话：从 plan_meta 中读取。
        let target_session = plan_meta
            .endpoint_evidence
            .iter()
            .find_map(|e| e.strip_prefix("target:"))
            .ok_or(SchemaDiffPlanError::CapabilityChanged)?;
        let current = current_capability_hash(&self.state, target_session)
            .await
            .map_err(|_| SchemaDiffPlanError::CapabilityChanged)?;
        if current != plan_meta.capability_snapshot_hash {
            return Err(SchemaDiffPlanError::CapabilityChanged);
        }
        Ok(())
    }

    async fn read_target_fingerprint(
        &self,
        plan_meta: &SchemaDiffFrozenPlan,
    ) -> Result<String, SchemaDiffPlanError> {
        let target_session = plan_meta
            .endpoint_evidence
            .iter()
            .find_map(|e| e.strip_prefix("target:"))
            .ok_or(SchemaDiffPlanError::PlanStale("target evidence missing".into()))?;
        let tables: Vec<String> = plan_meta
            .endpoint_evidence
            .iter()
            .find_map(|e| e.strip_prefix("planTables:"))
            .map(|s| s.split(',').filter(|t| !t.is_empty()).map(String::from).collect())
            .unwrap_or_default();
        compute_target_fingerprint(
            &self.state,
            target_session,
            plan_meta.target_database.as_deref(),
            plan_meta.target_schema.as_deref(),
            &tables,
            &[],
        )
        .await
        .map_err(|e| SchemaDiffPlanError::PlanStale(e.to_string()))
    }

    async fn deploy(
        &self,
        plan: &SchemaDiffPlan,
        request: &ApplyRequest,
        _cancel: &datazen_runtime::job::CancelToken,
    ) -> Result<SchemaDiffDeployResult, SchemaDiffPlanError> {
        let result = execute_schema_diff_deploy_impl(
            &self.state,
            request.target_db_session_id.clone(),
            plan.clone(),
            Some(request.use_transaction),
            Some(request.require_rollback),
            request.confirm_destructive.clone(),
            request.job_id.clone(),
            request.target_database.clone(),
            request.target_schema.clone(),
            request.profile.as_ref().map(|(id, rev)| {
                crate::store::MigrationProfileRef {
                    id: id.clone(),
                    revision: rev.clone(),
                }
            }),
        )
        .await;
        match result {
            Ok(r) => Ok(r),
            Err(e) => Err(SchemaDiffPlanError::PlanStale(e.to_string())),
        }
    }

    async fn read_only_verify(
        &self,
        _plan_meta: &SchemaDiffFrozenPlan,
        _operation_id: &str,
    ) -> Result<ReadOnlyVerdict, SchemaDiffPlanError> {
        // P5 Wave-1：只读核验由 execute_schema_diff_deploy_impl 的重复快照校验兜底；
        // 独立的只读核验记录器随工件台账补齐，这里返回「无法唯一证明」作为保守裁决。
        Ok(ReadOnlyVerdict::Indeterminate)
    }

}

// ----------------------------------------------------------------- 载荷/上下文

fn job_ctx() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("local"),
        PrincipalId::new("owner"),
        None,
        ClientInstanceId::new("desktop"),
        RequestId::new("req"),
        None,
    )
}

fn apply_payload(plan_id: &str, selection_revision: u64) -> serde_json::Value {
    serde_json::json!({
        "consumedPlanId": plan_id,
        "planVersion": 1,
        "handlerVersion": 1,
        "checkpointVersion": 1,
        "selectionRevision": selection_revision,
    })
}

fn prepare_payload() -> serde_json::Value {
    serde_json::json!({
        "planVersion": 1,
        "handlerVersion": 1,
        "checkpointVersion": 1,
    })
}

fn owner_ref(state_unused: &AppState) -> datazen_platform_api::OwnerRef {
    datazen_platform_api::OwnerRef::ClientSession {
        client_instance_id: ClientInstanceId::new("desktop"),
        purpose: "migration".into(),
    }
}

async fn endpoints_from_session_pair(
    state: &AppState,
    source_session: Option<&str>,
    target_session: &str,
    objects: &[String],
) -> Result<Vec<EndpointRef>, CommandError> {
    let tgt_cfg = state
        .connection_manager
        .get_session_config(target_session)
        .await
        .cmd_err("endpoints")?;
    let src_owner = match source_session {
        Some(s) => state
            .connection_manager
            .owner_connection_id(s)
            .await
            .unwrap_or_default(),
        None => String::new(),
    };
    let tgt_owner = state
        .connection_manager
        .owner_connection_id(target_session)
        .await
        .unwrap_or_default();
    let mut out = Vec::new();
    if let Some(source_session) = source_session {
        let src_cfg = state
            .connection_manager
            .get_session_config(source_session)
            .await
            .cmd_err("endpoints")?;
        out.push(EndpointRef {
            connection_id: ConnectionId::new(src_owner),
            service_key: format!(
                "{}|{}|{}",
                src_cfg.database_type,
                src_cfg.host.as_deref().unwrap_or(""),
                src_cfg.database.as_deref().unwrap_or("")
            ),
            objects: objects.to_vec(),
            role: EndpointRole::SourceReader,
        });
    }
    out.push(EndpointRef {
            connection_id: ConnectionId::new(tgt_owner),
            service_key: format!(
                "{}|{}|{}",
                tgt_cfg.database_type,
                tgt_cfg.host.as_deref().unwrap_or(""),
                tgt_cfg.database.as_deref().unwrap_or("")
            ),
            objects: objects.to_vec(),
            role: EndpointRole::TargetWriter,
        });
    Ok(out)
}

/// Envelope：prepare 的 IPC 出参（计划 Artifact + 冻结元数据）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiffPrepareEnvelope {
    pub plan: SchemaDiffPlan,
    pub plan_id: String,
    pub selection_revision: u64,
    pub plan_version: u64,
    pub handler_version: u64,
    pub checkpoint_version: u64,
    pub expires_at: String,
    pub recovery_policy: String,
}

/// 执行 prepare JobRuntime 旅程；成功时返回 envelope。
pub async fn run_prepare_job(
    state: &AppState,
    request: PrepareRequest,
) -> Result<SchemaDiffPrepareEnvelope, CommandError> {
    let infra = &state.schema_diff_jobs;
    let ctx = job_ctx();
    let clock = Arc::new(SystemJobClock);
    let owned = Arc::new(state.clone());
    let backend = Arc::new(AppStateBackend::new(owned, infra.plans.clone()));
    let handler = Arc::new(SchemaDiffHandler::for_prepare(
        backend.clone(),
        infra.plans.clone(),
        request.clone(),
    ));
    let mut registry = HandlerRegistry::new();
    registry.register(handler);
    let runtime = JobRuntime::new(
        infra.repo.clone(),
        Arc::new(registry),
        infra.ledger.clone(),
        clock,
        infra.now_ms(),
    );
    let (source_session, target_session, objects) = match &request {
        PrepareRequest::Table {
            source_db_session_id,
            target_db_session_id,
            table_names,
            ..
        } => (source_db_session_id.clone(), target_db_session_id.clone(), table_names.clone()),
        PrepareRequest::Unified {
            source_db_session_id,
            target_db_session_id,
            table_names,
            ..
        } => (source_db_session_id.clone(), target_db_session_id.clone(), table_names.clone()),
    };
    let endpoints = endpoints_from_session_pair(state, Some(&source_session), &target_session, &objects)
        .await?;
    {
        let mut ledger = infra.ledger.lock().map_err(|_| CommandError::Internal("budget ledger lock".into()))?;
        for ep in &endpoints {
            ledger.ensure_service(&ep.connection_id);
        }
    }
    let job_id = JobId::new(format!("schemaDiffPrepare-{}", uuid::Uuid::new_v4()));
    let definition = JobDefinition {
        job_id: job_id.clone(),
        kind: "schemaDiffPrepare".into(),
        owner: owner_ref(state),
        payload: prepare_payload(),
        created_at: datazen_platform_api::id::Timestamp::new(
            chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string(),
        ),
    };
    infra
        .repo
        .accept(&ctx, definition, &IdempotencyKey::new(format!("idem-{job_id}")))
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    let result = runtime
        .run(&ctx, &job_id, &WorkerId::new("schema-diff-host"), &endpoints)
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    if result.state != JobState::Succeeded {
        return Err(CommandError::Internal(format!(
            "schemaDiffPrepare failed: {:?}",
            result.error
        )));
    }
    let (plan_id, meta) = backend
        .last_prepared()
        .ok_or_else(|| CommandError::Internal("prepared plan missing".into()))?;
    let stored = infra
        .plans
        .get(&plan_id)
        .ok_or_else(|| CommandError::Internal("plan artifact missing".into()))?;
    Ok(SchemaDiffPrepareEnvelope {
        plan: stored.plan,
        plan_id,
        selection_revision: meta.selection_revision,
        plan_version: meta.plan_version,
        handler_version: meta.handler_version,
        checkpoint_version: meta.checkpoint_version,
        expires_at: meta.expires_at.as_str().to_string(),
        recovery_policy: meta.recovery_policy.as_str().to_string(),
    })
}

/// 执行 apply JobRuntime 旅程；成功时返回 deploy 结果投影。
pub async fn run_apply_job(
    state: &AppState,
    plan_id: &str,
    selection_revision: u64,
    apply_request: ApplyRequest,
) -> Result<SchemaDiffDeployResult, CommandError> {
    let infra = &state.schema_diff_jobs;
    let ctx = job_ctx();
    let clock = Arc::new(SystemJobClock);
    let owned = Arc::new(state.clone());
    let backend = Arc::new(AppStateBackend::new(owned, infra.plans.clone()));
    let handler = Arc::new(SchemaDiffHandler::for_apply(
        backend.clone(),
        infra.plans.clone(),
        plan_id.to_string(),
        selection_revision,
        apply_request.clone(),
    ));
    let mut registry = HandlerRegistry::new();
    registry.register(handler);
    let runtime = JobRuntime::new(
        infra.repo.clone(),
        Arc::new(registry),
        infra.ledger.clone(),
        clock,
        infra.now_ms(),
    );
    let stored = infra
        .plans
        .get(plan_id)
        .ok_or_else(|| CommandError::NotFound(format!("plan `{plan_id}` not found")))?;
    let source_session = if stored.meta.source_connection_id.is_empty() {
        None
    } else {
        Some(stored.meta.source_connection_id.as_str())
    };
    let endpoints = endpoints_from_session_pair(
        state,
        source_session,
        &apply_request.target_db_session_id,
        &stored.meta.endpoint_evidence
            .iter()
            .find_map(|e| e.strip_prefix("planTables:"))
            .map(|s| s.split(',').map(String::from).collect::<Vec<_>>())
            .unwrap_or_default(),
    )
    .await?;
    {
        let mut ledger = infra.ledger.lock().map_err(|_| CommandError::Internal("budget ledger lock".into()))?;
        for ep in &endpoints {
            ledger.ensure_service(&ep.connection_id);
        }
    }
    let job_id = JobId::new(format!("schemaDiffApply-{}", uuid::Uuid::new_v4()));
    let definition = JobDefinition {
        job_id: job_id.clone(),
        kind: "schemaDiffApply".into(),
        owner: owner_ref(state),
        payload: apply_payload(plan_id, selection_revision),
        created_at: datazen_platform_api::id::Timestamp::new(
            chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string(),
        ),
    };
    infra
        .repo
        .accept(&ctx, definition, &IdempotencyKey::new(format!("idem-{job_id}")))
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    let result = runtime
        .run(&ctx, &job_id, &WorkerId::new("schema-diff-host"), &endpoints)
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    // 效果结局投影：映射 JobState/EffectOutcome 回既有 DeployStatus。
    let status = match (result.state, result.effect_outcome) {
        (JobState::Succeeded, _) => datazen_schema_diff::types::DeployStatus::Committed,
        (JobState::Failed, _) if result.error.is_some() => datazen_schema_diff::types::DeployStatus::Failed,
        (JobState::Failed, EffectOutcome::PartiallyApplied) => datazen_schema_diff::types::DeployStatus::Mixed,
        (JobState::Failed, EffectOutcome::Unknown) => datazen_schema_diff::types::DeployStatus::Unknown,
        (JobState::Failed, EffectOutcome::RolledBack) => datazen_schema_diff::types::DeployStatus::RolledBack,
        (JobState::Cancelled, EffectOutcome::PartiallyApplied) => datazen_schema_diff::types::DeployStatus::Mixed,
        (JobState::Cancelled, EffectOutcome::Unknown) => datazen_schema_diff::types::DeployStatus::Unknown,
        (JobState::Cancelled, _) => datazen_schema_diff::types::DeployStatus::Cancelled,
        _ => datazen_schema_diff::types::DeployStatus::Failed,
    };
    Ok(SchemaDiffDeployResult {
        status,
        executed_count: result.progress.attempted.get() as usize,
        statement_count: result.progress.converted.get() as usize,
        errors: result.error.into_iter().collect(),
        statement_results: Vec::new(),
    })
}

/// 旧 IPC 兼容路径：客户端直接携带计划正文时，先注册进 PlanStore 再走 apply Job。
/// 两种受理（prepare 产物 / 直接携带正文）都满足「只经 JobRuntime」。
pub fn register_plan_for_apply(
    state: &AppState,
    plan: SchemaDiffPlan,
    target_db_session_id: &str,
) -> Result<(String, u64), CommandError> {
    let plan_id = plan
        .plan_id
        .clone()
        .ok_or_else(|| CommandError::Validation("plan requires planId".into()))?;
    let meta = SchemaDiffFrozenPlan {
        plan_id: plan_id.clone(),
        plan_version: 1,
        handler_version: 1,
        checkpoint_version: 1,
        selection_revision: 1,
        created_at: datazen_platform_api::id::Timestamp::new(
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        ),
        expires_at: datazen_platform_api::id::Timestamp::new(
            (chrono::Utc::now() + chrono::Duration::hours(2))
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string(),
        ),
        source_connection_id: String::new(),
        target_connection_id: target_db_session_id.to_string(),
        source_database: None,
        target_database: None,
        source_schema: None,
        target_schema: None,
        endpoint_evidence: vec![
            format!("target:{target_db_session_id}"),
            format!("planTables:{}", plan.tables.join(",")),
        ],
        capability_snapshot_hash: String::new(),
        schema_fingerprint: datazen_schema_diff::job::fnv1a64_hex(
            &serde_json::to_vec(&plan.expected_target_schemas).unwrap_or_default(),
        ),
        mapping_fingerprint: datazen_schema_diff::job::fnv1a64_hex(
            &serde_json::to_vec(&plan.tables).unwrap_or_default(),
        ),
        consistency: "tableSnapshot".into(),
        transaction_scope: "task".into(),
        recovery_policy: datazen_schema_diff::job::RecoveryPolicy::ReadOnlyVerify,
        body_artifact_ids: vec![],
        body_digests: vec![],
        confirmed_actions: Vec::new(),
    };
    state
        .schema_diff_jobs
        .plans
        .insert(StoredPlan {
            meta,
            plan,
            body_digest: String::new(),
        });
    Ok((plan_id, 1))
}

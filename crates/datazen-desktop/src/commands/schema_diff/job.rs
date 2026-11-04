//! P5 Wave-1：Schema Diff 窗口命令经 JobRuntime 执行（`p5-schema-diff` 轨）。
//!
//! 结构：
//!
//! * [`SchemaDiffJobInfra`]：进程级 Job 基础设施（仓储/预算/计划库）。
//! * [`AppStateBackend`]：[`SchemaDiffJobBackend`] 的宿主实现。
//! * [`run_prepare_job`] / [`run_apply_job`]：IPC 命令的 JobRuntime 入口。
//! * [`prepare_table_plan_via_sessions`] / [`prepare_unified_plan_via_sessions`] /
//!   [`deploy_reviewed_plan_with_cancel`]：表级/统一计划准备与部署的宿主侧
//!   编排（B2 schema_diff 域刀自 `plan_commands.rs` / `unified_plan.rs` /
//!   `deploy_commands.rs` 的 `_impl` 转发体逐字迁入——它们的生产调用方
//!   就是本模块的后端 trait 实现）。
//!
//! 不变量（§2.1）：同一请求只走一种管理器——IPC 一律构造 Job、注册 handler、
//! 跑 JobRuntime，不直接调旧 prepare/deploy 管理器。

use std::sync::Arc;

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use datazen::jobs::{EndpointRef, EndpointRole};
use datazen::shared::context::RequestContext;
use datazen::shared::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};
use datazen::shared::scope::OrganizationScope;

use datazen::migration::schema_diff::api as schema_diff_api;
use datazen::migration::schema_diff::job::{
    ApplyRequest, PlanRepository, PrepareRequest, PreparedPlan, ReadOnlyVerdict,
    SchemaDiffFrozenPlan, SchemaDiffJobBackend, SchemaDiffPlanError, StoredPlan,
};
use datazen::migration::schema_diff::types::{
    ColumnTypeOverride, PlanRequirement, SchemaDiffDeployResult, SchemaDiffPlan,
};

use crate::commands::error::{CmdExt, CommandError};
use crate::AppState;
// `LegacySchemaMigration::ddl_atomicity` 是该 trait 的方法，部署编排里要用。
use datazen_driver_api::SchemaMigration as _;

mod recovery;

/// 进程级 Job 基础设施。置于 [`AppState`] 中，驻留整个应用生命周期：
/// Job 关闭后仍保留终态与 effect 投影（§8：窗口关闭不释放 Job 资源）。
pub struct SchemaDiffJobInfra {
    pub plans: Arc<PlanRepository>,
}

impl SchemaDiffJobInfra {
    pub fn new() -> Self {
        Self {
            plans: Arc::new(PlanRepository::new()),
        }
    }
}

fn fnv_hex_of_json<T: serde::Serialize>(v: &T) -> String {
    let bytes = serde_json::to_vec(v).unwrap_or_default();
    datazen::migration::schema_diff::job::fnv1a64_hex(&bytes)
}

fn source_session_id(request: &PrepareRequest) -> String {
    match request {
        PrepareRequest::Table {
            source_db_session_id,
            ..
        } => source_db_session_id.clone(),
        PrepareRequest::Unified {
            source_db_session_id,
            ..
        } => source_db_session_id.clone(),
    }
}

fn target_session_id(request: &PrepareRequest) -> String {
    match request {
        PrepareRequest::Table {
            target_db_session_id,
            ..
        } => target_db_session_id.clone(),
        PrepareRequest::Unified {
            target_db_session_id,
            ..
        } => target_db_session_id.clone(),
    }
}

async fn owner_connection_id(state: &AppState, session_id: &str) -> Result<String, CommandError> {
    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let runtime = adapter
        .runtime()
        .map_err(crate::platform::into_command_error)?;
    runtime
        .connection_id_for_session(&datazen::execution::runtime::connection::DbSessionId::new(
            session_id,
        ))
        .map(|id| id.to_string())
        .ok_or_else(|| CommandError::Validation("Migration connection owner is unavailable".into()))
}

/// §4.2 桌面确认边界：运行时仓配化的恢复策略字符串（§2.2）。
fn recovery_policy_str(transactional_ddl: bool) -> &'static str {
    if transactional_ddl {
        "inTransactionBatch"
    } else {
        "readOnlyVerify"
    }
}

/// capability/版本/credential 快照（§2.2）：当前连接配置的可核验身份。
async fn current_capability_hash(
    state: &AppState,
    target_db_session_id: &str,
) -> Result<String, CommandError> {
    let owner = owner_connection_id(state, target_db_session_id).await?;
    let config = crate::commands::schema_diff::persisted_connection(
        state,
        &owner,
        "schema_diff_current_capability_hash",
    )
    .await?
    .ok_or_else(|| CommandError::Validation("Migration connection owner is unavailable".into()))?;
    let read_only = config.read_only;
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
    target_session_id: Option<String>,
}

impl AppStateBackend {
    pub fn new(state: Arc<AppState>, target_session_id: Option<String>) -> Self {
        Self {
            state,
            target_session_id,
        }
    }
}

#[async_trait]
impl SchemaDiffJobBackend for AppStateBackend {
    async fn prepare_plan(
        &self,
        request: &PrepareRequest,
        _cancel: &datazen::jobs::CancelToken,
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
                let plan = prepare_table_plan_via_sessions(
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
                (
                    plan,
                    target_db_session_id.clone(),
                    target_database_from_config(&self.state, &target_db_session_id).await,
                    target_schema.clone(),
                )
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
                let plan = prepare_unified_plan_via_sessions(
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
                (
                    plan,
                    target_db_session_id.clone(),
                    target_database_from_config(&self.state, &target_db_session_id).await,
                    target_schema.clone(),
                )
            }
        };
        let recovery_targets = recovery::prepare_target_objects(&self.state, request)
            .await
            .map_err(|_| SchemaDiffPlanError::PlanStale("target identity unavailable".into()))?;
        let plan_id = plan
            .plan_id
            .clone()
            .ok_or_else(|| SchemaDiffPlanError::PlanNotFound("planId missing".into()))?;
        let fingerprint = recovery::fingerprint_targets(
            &self.state,
            &tgt_sess,
            target_db.as_deref(),
            &recovery_targets,
        )
        .await
        .map_err(|e| SchemaDiffPlanError::PlanStale(e.to_string()))?;
        let capability_hash = current_capability_hash(&self.state, &tgt_sess)
            .await
            .map_err(|_| SchemaDiffPlanError::CapabilityChanged)?;
        let transactional_ddl = plan.statements.iter().any(|s| s.requires_transaction)
            || plan.source_dialect.contains("sqlite")
            || plan.source_dialect.contains("postgres");
        let mut endpoint_evidence = vec![format!("planTables:{}", plan.tables.join(","))];
        recovery::record_target_objects(&mut endpoint_evidence, &recovery_targets);
        let meta = SchemaDiffFrozenPlan {
            plan_id: plan_id.clone(),
            plan_version: 1,
            handler_version: 1,
            checkpoint_version: 1,
            selection_revision: 1,
            created_at: datazen::shared::id::Timestamp::new(
                chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            ),
            expires_at: datazen::shared::id::Timestamp::new(
                (chrono::Utc::now() + chrono::Duration::hours(2))
                    .format("%Y-%m-%dT%H:%M:%SZ")
                    .to_string(),
            ),
            source_connection_id: owner_connection_id(&self.state, &source_session_id(request))
                .await
                .map_err(|_| SchemaDiffPlanError::CapabilityChanged)?,
            target_connection_id: owner_connection_id(&self.state, &target_session_id(request))
                .await
                .map_err(|_| SchemaDiffPlanError::CapabilityChanged)?,
            source_database: None,
            target_database: target_db,
            source_schema: None,
            target_schema,
            endpoint_evidence,
            capability_snapshot_hash: capability_hash,
            schema_fingerprint: fingerprint,
            mapping_fingerprint: fnv_hex_of_json(&plan.tables),
            consistency: "tableSnapshot".into(),
            transaction_scope: if transactional_ddl {
                "task".into()
            } else {
                "nonAtomicDDL".into()
            },
            recovery_policy: datazen::migration::schema_diff::job::RecoveryPolicy::parse(
                recovery_policy_str(transactional_ddl),
            )
            .ok_or(SchemaDiffPlanError::RecoveryPolicyInvalid(
                "policy parse".into(),
            ))?,
            body_artifact_ids: vec![format!("artifact:plan:{plan_id}")],
            body_digests: vec![fnv_hex_of_json(&plan.statements)],
            confirmed_actions: Vec::new(),
        };
        let body_digest = fnv_hex_of_json(&plan.statements);
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
        let target_session = self
            .target_session_id
            .as_deref()
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
        _plan: &SchemaDiffPlan,
        plan_meta: &SchemaDiffFrozenPlan,
    ) -> Result<String, SchemaDiffPlanError> {
        let target_session =
            self.target_session_id
                .as_deref()
                .ok_or(SchemaDiffPlanError::PlanStale(
                    "target session unavailable".into(),
                ))?;
        let targets = recovery::targets_from_evidence(&plan_meta.endpoint_evidence).ok_or(
            SchemaDiffPlanError::PlanStale("target identity unavailable".into()),
        )?;
        recovery::fingerprint_targets(
            &self.state,
            target_session,
            plan_meta.target_database.as_deref(),
            &targets,
        )
        .await
        .map_err(|e| SchemaDiffPlanError::PlanStale(e.to_string()))
    }

    async fn deploy(
        &self,
        plan: &SchemaDiffPlan,
        request: &ApplyRequest,
        cancel: &datazen::jobs::CancelToken,
    ) -> Result<SchemaDiffDeployResult, SchemaDiffPlanError> {
        let result = deploy_reviewed_plan_with_cancel(
            &self.state,
            request.target_db_session_id.clone(),
            plan.clone(),
            Some(request.use_transaction),
            Some(request.require_rollback),
            request.confirm_destructive.clone(),
            request.job_id.clone(),
            request.target_database.clone(),
            request.target_schema.clone(),
            request
                .profile
                .as_ref()
                .map(|(id, rev)| crate::store::MigrationProfileRef {
                    id: id.clone(),
                    revision: rev.clone(),
                }),
            Some(cancel.flag()),
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
        // A fresh session may be supplied by an explicit recovery command. Until a
        // safe after-state projection is available, do not infer a committed outcome
        // from a changed target fingerprint alone.
        Ok(ReadOnlyVerdict::Indeterminate)
    }
}

// ------------------------------------------------- B2 域刀：迁入的宿主编排
//
// 以下三个函数是原 `plan_commands.rs` / `unified_plan.rs` / `deploy_commands.rs`
// 里 `_impl` 转发体的接线本体，逐字迁入（唯一的生产调用方就是本模块的
// `AppStateBackend`）。错误顺序与文本、日志与历史记账语义逐字节不变。

/// 表级计划的宿主侧准备：入参校验 → 会话/作用域解析 → modules 用例
/// `schema_diff::api::prepare_table_plan`。
async fn prepare_table_plan_via_sessions(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    table_names: Vec<String>,
    target_table_names: Vec<String>,
    target_only_table_names: Vec<String>,
    allow_destructive: bool,
    include_indexes: Option<bool>,
    type_overrides: Option<Vec<ColumnTypeOverride>>,
    source_schema_override: Option<String>,
    target_schema_override: Option<String>,
) -> Result<SchemaDiffPlan, CommandError> {
    // 入参校验先行，保持本传输层的错误顺序（api 会再校验一次，供未来
    // Web 宿主独立调用——同 F4b 的双检纪律）。
    if table_names.is_empty() && target_only_table_names.is_empty() {
        return Err(CommandError::Validation(
            "table_names and target_only_table_names must not both be empty".into(),
        ));
    }
    if table_names.len() != target_table_names.len() {
        return Err(CommandError::Validation(
            "table_names and target_table_names must have the same length".into(),
        ));
    }

    let source_context = crate::services::migration_endpoint::runtime_session_context(
        state,
        &source_db_session_id,
        datazen::execution::application::LivePurpose::Inspection,
    )
    .await?;
    let target_context = crate::services::migration_endpoint::runtime_session_context(
        state,
        &target_db_session_id,
        datazen::execution::application::LivePurpose::Inspection,
    )
    .await?;
    let scope: OrganizationScope = super::reviewed_scope(state)?;
    let adapters = super::adapter_ports(state);
    schema_diff_api::prepare_table_plan(schema_diff_api::PrepareTablePlanRequest {
        source: &super::schema_diff_endpoint(source_context),
        target: &super::schema_diff_endpoint(target_context),
        source_session_id: &source_db_session_id,
        target_session_id: &target_db_session_id,
        table_names,
        target_table_names,
        target_only_table_names,
        allow_destructive,
        include_indexes,
        type_overrides,
        source_schema: source_schema_override.as_deref(),
        target_schema: target_schema_override.as_deref(),
        repository: state.schema_diff_reviewed_plans.as_ref(),
        scope: &scope,
        adapters: &adapters,
    })
    .await
    .cmd_err("prepare_schema_diff_plan")
}

/// 统一（表 + schema 对象）计划的宿主侧准备：入参校验 → 会话/作用域解析 →
/// modules 用例 `schema_diff::api::prepare_unified_plan`。
async fn prepare_unified_plan_via_sessions(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    table_names: Vec<String>,
    target_table_names: Option<Vec<String>>,
    target_only_table_names: Option<Vec<String>>,
    source_objects: Option<Vec<datazen_driver_api::DatabaseObject>>,
    target_objects: Option<Vec<datazen_driver_api::DatabaseObject>>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    allow_destructive: bool,
    include_indexes: Option<bool>,
    type_overrides: Option<Vec<ColumnTypeOverride>>,
) -> Result<SchemaDiffPlan, CommandError> {
    let target_table_names = target_table_names.unwrap_or_else(|| table_names.clone());
    let target_only_table_names = target_only_table_names.unwrap_or_default();
    let source_objects = source_objects.unwrap_or_default();
    let target_objects = target_objects.unwrap_or_default();
    // 入参校验先行，保持本传输层的错误顺序（api 会再校验一次）。
    crate::schema_diff::plan_request::validate_plan_selection(
        &table_names,
        &target_table_names,
        &target_only_table_names,
        &source_objects,
        &target_objects,
    )
    .map_err(|error| CommandError::Validation(error.to_string()))?;

    let source_context = crate::services::migration_endpoint::runtime_session_context(
        state,
        &source_db_session_id,
        datazen::execution::application::LivePurpose::Inspection,
    )
    .await?;
    let target_context = crate::services::migration_endpoint::runtime_session_context(
        state,
        &target_db_session_id,
        datazen::execution::application::LivePurpose::Inspection,
    )
    .await?;
    let scope: OrganizationScope = super::reviewed_scope(state)?;
    let adapters = super::adapter_ports(state);
    schema_diff_api::prepare_unified_plan(schema_diff_api::PrepareUnifiedPlanRequest {
        source: &super::schema_diff_endpoint(source_context),
        target: &super::schema_diff_endpoint(target_context),
        source_session_id: &source_db_session_id,
        target_session_id: &target_db_session_id,
        table_names,
        target_table_names,
        target_only_table_names,
        source_objects,
        target_objects,
        source_schema: source_schema.as_deref(),
        target_schema: target_schema.as_deref(),
        allow_destructive,
        include_indexes,
        type_overrides,
        repository: state.schema_diff_reviewed_plans.as_ref(),
        scope: &scope,
        adapters: &adapters,
    })
    .await
    .cmd_err("prepare_schema_unified_plan")
}

/// 部署失败收尾：把历史记账行落成失败后原样返回错误。
async fn fail_schema_diff_deploy<T>(
    state: &AppState,
    run: crate::store::MigrationRunRecord,
    error: CommandError,
) -> Result<T, CommandError> {
    crate::commands::history::finish_migration_run(state, run, false, false, 0, 1, 0, "unknown")
        .await;
    Err(error)
}

/// 已评审计划的进程内部署编排（JobRuntime 后端 `deploy` 的本体）：
/// 历史记账 → 只读/破坏性/回滚门 → 复核消费与部署前验证 → 源侧快照复核 →
/// DDL 执行（modules `execute_schema_diff_deploy_at`）→ 终态记账。
async fn deploy_reviewed_plan_with_cancel(
    state: &AppState,
    target_db_session_id: String,
    plan: SchemaDiffPlan,
    use_transaction: Option<bool>,
    require_rollback: Option<bool>,
    confirm_destructive: Option<String>,
    // Legacy client key, kept only for the historical IPC signature: the
    // window-registry lookup it used to arm is gone (see the `cancelled`
    // comment below).
    _job_id: Option<String>,
    target_database: Option<String>,
    target_schema: Option<String>,
    profile: Option<crate::store::MigrationProfileRef>,
    cancel_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<SchemaDiffDeployResult, CommandError> {
    crate::commands::history::validate_migration_profile_ref(
        &state,
        "schemaDiff",
        profile.as_ref(),
    )
    .await?;
    let mut history_run =
        crate::commands::history::start_migration_run(&state, "schemaDiff", profile.as_ref()).await;
    history_run.selected_count = plan.statements.len() as u64;
    tracing::info!(
        %target_db_session_id,
        statements = plan.statements.len(),
        ?_job_id,
        "execute_schema_diff_deploy"
    );

    let target_context = match crate::services::migration_endpoint::runtime_session_context(
        &state,
        &target_db_session_id,
        datazen::execution::application::LivePurpose::LongLivedHostWork,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return fail_schema_diff_deploy(&state, history_run, error).await,
    };
    let driver = target_context.driver.clone();
    let handle = target_context.handle.clone();
    let config = target_context.config.clone();
    let owner = target_context.view.connection_id.as_str().to_owned();
    history_run.target_connection_id = Some(owner.clone());
    let persisted =
        match super::persisted_connection(&state, &owner, "execute_schema_diff_deploy").await? {
            Some(value) => value,
            None => {
                return fail_schema_diff_deploy(
                    &state,
                    history_run,
                    CommandError::Validation("Target connection was removed".into()),
                )
                .await;
            }
        };
    if config.read_only || persisted.read_only {
        return fail_schema_diff_deploy(
            &state,
            history_run,
            CommandError::Validation("Target connection is read-only".into()),
        )
        .await;
    }
    let effective_target_database = target_database
        .as_deref()
        .or(config.database.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let effective_target_schema = target_schema
        .as_deref()
        .or(config.schema.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if require_rollback.unwrap_or(false)
        && (!plan.rollback_completeness.complete
            || !use_transaction.unwrap_or(true)
            || !matches!(
                datazen_driver_api::LegacySchemaMigration::new(driver.as_ref()).ddl_atomicity(),
                datazen_driver_api::DdlAtomicity::Transactional
            ))
    {
        return fail_schema_diff_deploy(
            &state,
            history_run,
            CommandError::Validation(
                "Complete rollback requires transactional DDL and a complete rollback plan".into(),
            ),
        )
        .await;
    }
    // Only `Unsupported` blocks deployment. `Backfill` is a follow-up the plan
    // performed *safely*: a NOT NULL column without a default is added nullable
    // and the requirement tells the user to populate existing rows before
    // enforcing NOT NULL. Refusing the deploy on it left the plan permanently
    // undeployable — preparing again cannot clear it, because the statement is
    // already the safe one.
    if plan
        .requirements
        .iter()
        .any(|requirement| matches!(requirement, PlanRequirement::Unsupported { .. }))
    {
        return fail_schema_diff_deploy(
            &state,
            history_run,
            CommandError::Validation(
                "Resolve unsupported plan requirements and prepare again before deploying".into(),
            ),
        )
        .await;
    }
    if plan
        .requirements
        .iter()
        .any(|requirement| matches!(requirement, PlanRequirement::Backfill { .. }))
    {
        tracing::warn!(
            requirements = plan.requirements.len(),
            "deploying a plan with pending backfills; the added columns stay nullable"
        );
    }
    if crate::schema_diff::deploy::plan_has_destructive(&plan) {
        let token = confirm_destructive.as_deref().unwrap_or("");
        if token != crate::schema_diff::deploy::DESTRUCTIVE_CONFIRM_TOKEN {
            return fail_schema_diff_deploy(
                &state,
                history_run,
                CommandError::Validation(format!(
                    "destructive plan requires confirm_destructive = \"{}\"",
                    crate::schema_diff::deploy::DESTRUCTIVE_CONFIRM_TOKEN
                )),
            )
            .await;
        }
    }

    // 复核消费 + 部署前目标侧验证（含表 DROP 的完整目录复核、逐表/对象
    // 快照复核、对象目录与表身份复核、MySQL 视图创建上下文复核）。
    let target_endpoint = schema_diff_api::SchemaDiffEndpoint {
        driver: driver.clone(),
        handle: handle.clone(),
        config: config.clone(),
    };
    let reviewed = match schema_diff_api::consume_and_verify_reviewed_plan(
        state.schema_diff_reviewed_plans.as_ref(),
        &super::reviewed_scope(&state)?,
        &plan,
        &target_endpoint,
        &target_db_session_id,
        effective_target_database,
        effective_target_schema,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            return fail_schema_diff_deploy(&state, history_run, error.into()).await;
        }
    };
    // 源侧快照复核按原顺序随后执行（源会话从复核记录解析）。
    if let Some(source_snapshot) = reviewed.source_snapshot.as_ref() {
        let source_context = match crate::services::migration_endpoint::runtime_session_context(
            &state,
            &source_snapshot.session,
            datazen::execution::application::LivePurpose::Inspection,
        )
        .await
        {
            Ok(value) => value,
            Err(error) => return fail_schema_diff_deploy(&state, history_run, error).await,
        };
        if let Err(error) = schema_diff_api::revalidate_source_snapshot(
            &super::schema_diff_endpoint(source_context),
            &reviewed,
        )
        .await
        {
            return fail_schema_diff_deploy(&state, history_run, error.into()).await;
        }
    }
    let plan = reviewed.plan;
    // The durable Job path always passes a cancel flag (`run_apply_job` drives
    // the deploy through the runtime), and the unattended workflow bridge
    // passes no job id at all, so the legacy window-registry lookup retired
    // together with the old connection manager: `cancelled` is just the
    // external flag when present, `None` otherwise.
    let cancelled = cancel_flag;

    let opts = crate::schema_diff::deploy::DeployOptions {
        use_transaction: use_transaction.unwrap_or(true),
        stop_on_error: true,
    };

    let result = crate::schema_diff::deploy::execute_schema_diff_deploy_at(
        driver.as_ref(),
        &handle,
        &plan,
        opts,
        cancelled,
        datazen_driver_api::SqlTarget::new(effective_target_database, effective_target_schema),
    )
    .await;

    tracing::info!(
        ?result.status,
        executed = result.executed_count,
        "execute_schema_diff_deploy OK"
    );
    let cancelled_outcome = matches!(result.status, crate::schema_diff::DeployStatus::Cancelled);
    let success_outcome = matches!(result.status, crate::schema_diff::DeployStatus::Committed);
    let rollback_outcome = match result.status {
        crate::schema_diff::DeployStatus::RolledBack => "completed",
        crate::schema_diff::DeployStatus::Unknown | crate::schema_diff::DeployStatus::Mixed => {
            "unknown"
        }
        _ => "notRequired",
    };
    crate::commands::history::finish_migration_run(
        &state,
        history_run,
        success_outcome,
        cancelled_outcome,
        result.executed_count as u64,
        result.errors.len() as u64,
        0,
        rollback_outcome,
    )
    .await;
    Ok(result)
}

// ----------------------------------------------------------------- 载荷/上下文

pub(super) fn job_ctx() -> RequestContext {
    RequestContext::new(
        OrganizationId::new(crate::platform::LOCAL_ORGANIZATION_ID),
        PrincipalId::new("datazen-local-user"),
        None,
        ClientInstanceId::new("datazen-local-client"),
        RequestId::new(format!("schema-diff-{}", uuid::Uuid::new_v4())),
        None,
    )
}

pub(super) fn apply_payload(
    stored: &StoredPlan,
    request: &ApplyRequest,
) -> Result<serde_json::Value, CommandError> {
    let plan_bytes = serde_json::to_vec(&stored.plan)
        .map_err(|_| CommandError::Internal("Schema Diff plan digest failed".into()))?;
    let plan_digest = format!("sha256:{:x}", Sha256::digest(plan_bytes));
    let profile_revision_digest = request
        .profile
        .as_ref()
        .map(|profile| {
            serde_json::to_vec(profile)
                .map(|bytes| format!("sha256:{:x}", Sha256::digest(bytes)))
                .map_err(|_| CommandError::Internal("Schema Diff profile digest failed".into()))
        })
        .transpose()?;
    let object_ids = recovery::target_ids_from_evidence(&stored.meta.endpoint_evidence);
    let recovery_targets = serde_json::json!([{
        "connectionId": stored.meta.target_connection_id,
        "objectIds": object_ids,
    }]);
    let mut payload = serde_json::json!({
        "consumedPlanId": stored.meta.plan_id,
        "planDigest": plan_digest,
        "planVersion": 1,
        "handlerVersion": 1,
        "checkpointVersion": 1,
        "selectionRevision": stored.meta.selection_revision,
        "recoveryTargets": recovery_targets,
        "targetBeforeFingerprint": stored.meta.schema_fingerprint,
        "recoveryPolicy": stored.meta.recovery_policy.as_str(),
        "useTransaction": request.use_transaction,
        "requireRollback": request.require_rollback,
        "confirmedDestructive": request.confirm_destructive.as_deref()
            == Some(crate::schema_diff::deploy::DESTRUCTIVE_CONFIRM_TOKEN),
    });
    if let Some(digest) = profile_revision_digest {
        payload["profileRevisionDigest"] = serde_json::Value::String(digest);
    }
    Ok(payload)
}

pub(super) fn prepare_payload() -> serde_json::Value {
    serde_json::json!({
        "planVersion": 1,
        "handlerVersion": 1,
        "checkpointVersion": 1,
    })
}

async fn target_database_from_config(state: &AppState, session: &str) -> Option<String> {
    let owner = owner_connection_id(state, session).await.ok()?;
    crate::commands::schema_diff::persisted_connection(state, &owner, "schema_diff_target_database")
        .await
        .ok()
        .flatten()
        .and_then(|c| c.database)
}

async fn target_schema_from_config(state: &AppState, session: &str) -> Option<String> {
    let adapter = state.platform_require().ok()?;
    let runtime = adapter.runtime().ok()?;
    let call = adapter.begin_call();
    let observed = runtime
        .get_session_by_id(call.context(), session)
        .await
        .ok()
        .and_then(|view| view.observed_context.namespace.schema);
    if observed.is_some() {
        return observed;
    }
    let owner = owner_connection_id(state, session).await.ok()?;
    crate::commands::schema_diff::persisted_connection(state, &owner, "schema_diff_target_schema")
        .await
        .ok()
        .flatten()
        .and_then(|c| c.schema)
}

pub(super) fn owner_ref(_state_unused: &AppState) -> datazen::OwnerRef {
    datazen::OwnerRef::ClientSession {
        // 描述性字段：与本链 ctx 的客户端实例取值一致（落在 `owner_json` 里，从不被比对）。
        client_instance_id: ClientInstanceId::new("datazen-local-client"),
        purpose: "migration".into(),
    }
}

pub(super) async fn endpoints_from_session_pair(
    state: &AppState,
    source_session: Option<&str>,
    target_session: &str,
    objects: &[String],
) -> Result<Vec<EndpointRef>, CommandError> {
    let mut out = Vec::new();
    if let Some(session) = source_session {
        let identity =
            crate::services::migration_endpoint::session_identity(state, session, None).await?;
        out.push(EndpointRef {
            connection_id: identity.connection_id,
            service_key: identity.service_key,
            objects: objects.to_vec(),
            role: EndpointRole::SourceReader,
        });
    }
    let identity =
        crate::services::migration_endpoint::session_identity(state, target_session, None).await?;
    out.push(EndpointRef {
        connection_id: identity.connection_id,
        service_key: identity.service_key,
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
    pub source_connection_id: String,
    pub target_connection_id: String,
    pub selection_revision: u64,
    pub plan_version: u64,
    pub handler_version: u64,
    pub checkpoint_version: u64,
    pub expires_at: String,
    pub recovery_policy: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen::migration::schema_diff::job::{RecoveryPolicy, SchemaDiffFrozenPlan};
    use datazen::migration::schema_diff::types::{
        PlanRequirement, PlanStatement, RollbackCompleteness, StatementRisk,
    };
    use datazen::shared::id::Timestamp;

    #[test]
    fn apply_payload_persists_only_allowlisted_schema_diff_receipt_fields() {
        let target = recovery::table_target("users", Some("public"));
        let mut evidence = vec!["planTables:users".into()];
        recovery::record_target_objects(&mut evidence, &[target]);
        let stored = StoredPlan {
            meta: SchemaDiffFrozenPlan {
                plan_id: "plan-opaque-1".into(),
                plan_version: 1,
                handler_version: 1,
                checkpoint_version: 1,
                selection_revision: 4,
                created_at: Timestamp::new("2026-10-08T00:00:00Z"),
                expires_at: Timestamp::new("2026-10-08T02:00:00Z"),
                source_connection_id: "source-connection".into(),
                target_connection_id: "target-connection".into(),
                source_database: None,
                target_database: Some("app".into()),
                source_schema: None,
                target_schema: Some("public".into()),
                endpoint_evidence: evidence,
                capability_snapshot_hash: "capability-digest".into(),
                schema_fingerprint: format!("sha256:{}", "a".repeat(64)),
                mapping_fingerprint: "mapping-digest".into(),
                consistency: "tableSnapshot".into(),
                transaction_scope: "task".into(),
                recovery_policy: RecoveryPolicy::ReadOnlyVerify,
                body_artifact_ids: vec!["artifact:plan:plan-opaque-1".into()],
                body_digests: vec!["body-digest".into()],
                confirmed_actions: Vec::new(),
            },
            plan: SchemaDiffPlan {
                plan_id: Some("plan-opaque-1".into()),
                table: "users".into(),
                tables: vec!["users".into()],
                source_dialect: "postgresql".into(),
                target_dialect: "postgresql".into(),
                same_dialect: true,
                statements: vec![PlanStatement {
                    sql: "DROP TABLE private_data".into(),
                    risk: StatementRisk::Destructive,
                    rollback_sql: None,
                    summary: "Remove private data".into(),
                    requires_transaction: false,
                }],
                warnings: Vec::new(),
                requirements: vec![PlanRequirement::Unsupported {
                    operation: "private_data".into(),
                    reason: "not selected".into(),
                }],
                rollback_completeness: RollbackCompleteness {
                    complete: false,
                    missing: vec!["private_data".into()],
                },
                type_suggestions: Vec::new(),
                expected_target_schemas: Vec::new(),
            },
            body_digest: "body-digest".into(),
        };
        let request = ApplyRequest {
            target_db_session_id: "target-live-session".into(),
            use_transaction: true,
            require_rollback: true,
            confirm_destructive: Some("DEPLOY".into()),
            job_id: Some("schemaDiffApply-live-id".into()),
            target_database: None,
            target_schema: None,
            profile: Some(("private-profile-id".into(), "revision-7".into())),
        };

        let payload = apply_payload(&stored, &request).expect("safe payload projection succeeds");
        assert_eq!(payload["useTransaction"], true);
        assert_eq!(payload["requireRollback"], true);
        assert_eq!(payload["selectionRevision"], 4);
        assert!(payload["planDigest"]
            .as_str()
            .is_some_and(|value| { value.starts_with("sha256:") && value.len() == 71 }));
        assert!(payload["profileRevisionDigest"]
            .as_str()
            .is_some_and(|value| { value.starts_with("sha256:") && value.len() == 71 }));
        let encoded = payload.to_string();
        for forbidden in [
            "DROP TABLE private_data",
            "target-live-session",
            "schemaDiffApply-live-id",
            "private-profile-id",
            "revision-7",
        ] {
            assert!(!encoded.contains(forbidden), "payload contains {forbidden}");
        }
        assert_eq!(
            payload["recoveryTargets"][0]["connectionId"],
            "target-connection"
        );
        assert!(payload["recoveryTargets"][0]["objectIds"][0]
            .as_str()
            .is_some_and(|id| id.starts_with("obj-") && id.len() <= 512));
    }
}

pub mod desktop;
pub use desktop::{cancel_schema_diff_job, run_apply_job, run_prepare_job, SchemaDiffJobAccepted};

/// 旧 IPC 兼容路径：客户端直接携带计划正文时，先注册进 PlanRepository 再走 apply Job。
/// 两种受理（prepare 产物 / 直接携带正文）都满足「只经 JobRuntime」。
/// D1：登记的 meta 与 verify_authorization/read_target_fingerprint 使用
/// **相同**的哈希计算方式（current_capability_hash / compute_target_fingerprint），
/// 保证兼容路径调用能跑通。
pub async fn register_plan_for_apply(
    state: &AppState,
    plan: SchemaDiffPlan,
    target_db_session_id: &str,
) -> Result<(String, u64), CommandError> {
    let plan_id = plan
        .plan_id
        .clone()
        .ok_or_else(|| CommandError::Validation("plan requires planId".into()))?;
    let target_schema = target_schema_from_config(state, target_db_session_id).await;
    let targets = plan
        .tables
        .iter()
        .map(|table| recovery::table_target(table, target_schema.as_deref()))
        .collect::<Vec<_>>();
    let fingerprint = recovery::fingerprint_targets(
        state,
        target_db_session_id,
        target_database_from_config(state, target_db_session_id)
            .await
            .as_deref(),
        &targets,
    )
    .await?;
    let mut endpoint_evidence = vec![format!("planTables:{}", plan.tables.join(","))];
    recovery::record_target_objects(&mut endpoint_evidence, &targets);
    let meta = SchemaDiffFrozenPlan {
        plan_id: plan_id.clone(),
        plan_version: 1,
        handler_version: 1,
        checkpoint_version: 1,
        selection_revision: 1,
        created_at: datazen::shared::id::Timestamp::new(
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        ),
        expires_at: datazen::shared::id::Timestamp::new(
            (chrono::Utc::now() + chrono::Duration::hours(2))
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string(),
        ),
        source_connection_id: String::new(),
        target_connection_id: owner_connection_id(state, target_db_session_id).await?,
        source_database: None,
        target_database: target_database_from_config(state, target_db_session_id).await,
        source_schema: None,
        target_schema,
        endpoint_evidence,
        capability_snapshot_hash: current_capability_hash(state, target_db_session_id)
            .await
            .map_err(|e| e)?,
        schema_fingerprint: fingerprint,
        mapping_fingerprint: datazen::migration::schema_diff::job::fnv1a64_hex(
            &serde_json::to_vec(&plan.tables).unwrap_or_default(),
        ),
        consistency: "tableSnapshot".into(),
        transaction_scope: "task".into(),
        recovery_policy: datazen::migration::schema_diff::job::RecoveryPolicy::ReadOnlyVerify,
        body_artifact_ids: vec![],
        body_digests: vec![],
        confirmed_actions: Vec::new(),
    };
    state.schema_diff_jobs.plans.insert(StoredPlan {
        meta,
        plan,
        body_digest: String::new(),
    });
    Ok((plan_id, 1))
}

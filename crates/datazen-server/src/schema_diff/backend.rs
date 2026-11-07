//! [`SchemaDiffJobBackend`] 的服务端实现（W8 刀3b-2）。
//!
//! 桌面对照 `datazen-desktop/src/commands/schema_diff/job.rs` 的
//! `AppStateBackend` + 迁入的三个宿主编排（prepare_table_plan_via_sessions
//! / prepare_unified_plan_via_sessions / deploy_reviewed_plan_with_cancel）：
//! 业务主体（api 用例、复核消费、DDL 执行）全在共享层；本件替换的只有
//! 「会话解析（活句柄借出）→ 端点」「历史记账（v12 表）」「能力快照
//! （连接目录）」三块宿主胶水。
//!
//! 错误面：五方法全收窄到 [`SchemaDiffPlanError`]（JobRuntime 契约），
//! 与桌面同臂映射——准备编排失败 → `PlanNotFound`；目标身份/指纹失败 →
//! `PlanStale`；能力快照失败/漂移 → `CapabilityChanged`。

use std::sync::Arc;

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use datazen::migration::schema_diff::api as schema_diff_api;
use datazen::migration::schema_diff::api::SchemaDiffAdapterHost;
use datazen::migration::schema_diff::deploy::{
    plan_has_destructive, DeployOptions, DESTRUCTIVE_CONFIRM_TOKEN,
};
use datazen::migration::schema_diff::job::{
    ApplyRequest, PrepareRequest, PreparedPlan, ReadOnlyVerdict, SchemaDiffFrozenPlan,
    SchemaDiffJobBackend, SchemaDiffPlanError, StoredPlan,
};
use datazen::migration::schema_diff::plan_request::validate_plan_selection;
use datazen::migration::schema_diff::reviewed::ReviewedPlanRepository;
use datazen::migration::schema_diff::types::{
    PlanRequirement, SchemaDiffDeployResult, SchemaDiffPlan,
};
use datazen::shared::context::RequestContext;
use datazen::shared::scope::OrganizationScope;
use datazen_driver_api::SchemaMigration as _;

use super::endpoints::{capability_hash, EndpointError, SessionEndpoints};
use super::history::MigrationHistory;
use super::recovery;

/// 服务端 backend 的装配依赖（composition root 注入）。
#[derive(Clone)]
pub struct ServerSchemaDiffBackendDeps {
    pub endpoints: SessionEndpoints,
    pub history: MigrationHistory,
    pub reviewed: Arc<dyn ReviewedPlanRepository>,
    pub adapters:
        Arc<datazen::migration::data_transfer::transfer::adapter_registry::SyncAdapterRegistry>,
}

/// 宿主 [`SchemaDiffJobBackend`]：`ctx` 是**受理时刻**的请求身份（HTTP
/// 认证链事实），随 job 派发贯穿执行面（P3 授权语：执行面审计归因落在
/// 真实请求主体，不落合成服务主体）。
pub struct ServerSchemaDiffBackend {
    deps: ServerSchemaDiffBackendDeps,
    ctx: RequestContext,
    /// apply 旅程的目标会话（受理时捕获；prepare 旅程为 None——会话从
    /// 请求体解析）。
    target_session_id: Option<String>,
}

impl ServerSchemaDiffBackend {
    pub fn new(deps: ServerSchemaDiffBackendDeps, ctx: RequestContext) -> Self {
        Self {
            deps,
            ctx,
            target_session_id: None,
        }
    }

    /// apply 旅程构造（目标会话在场）。
    pub fn for_apply(
        deps: ServerSchemaDiffBackendDeps,
        ctx: RequestContext,
        target_session_id: String,
    ) -> Self {
        Self {
            deps,
            ctx,
            target_session_id: Some(target_session_id),
        }
    }

    fn scope(&self) -> OrganizationScope {
        OrganizationScope {
            organization_id: self.ctx.organization_id.clone(),
        }
    }

    /// 桌面 `owner_connection_id` 的直取版：会话视图自带归属连接
    /// （桌面绕 runtime 反查；两语义等价——视图即登记事实）。
    fn target_session(&self) -> Result<&str, SchemaDiffPlanError> {
        self.target_session_id
            .as_deref()
            .ok_or(SchemaDiffPlanError::CapabilityChanged)
    }

    async fn current_capability_hash(&self) -> Result<String, SchemaDiffPlanError> {
        let session = self
            .deps
            .endpoints
            .resolve(
                &self.ctx,
                self.target_session()?,
                datazen::execution::application::LivePurpose::Inspection,
            )
            .await
            .map_err(|_| SchemaDiffPlanError::CapabilityChanged)?;
        Ok(capability_hash(&session.config, session.connection_id()))
    }
}

#[async_trait]
impl SchemaDiffJobBackend for ServerSchemaDiffBackend {
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
                let plan = self
                    .prepare_table_plan_via_sessions(
                        source_db_session_id,
                        target_db_session_id,
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
                    .map_err(|error| SchemaDiffPlanError::PlanNotFound(error.to_string()))?;
                let target_db = self
                    .database_of_session(target_db_session_id)
                    .await
                    .map_err(|error| SchemaDiffPlanError::PlanNotFound(error.to_string()))?;
                (
                    plan,
                    target_db_session_id.clone(),
                    target_db,
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
                let plan = self
                    .prepare_unified_plan_via_sessions(
                        source_db_session_id,
                        target_db_session_id,
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
                    .map_err(|error| SchemaDiffPlanError::PlanNotFound(error.to_string()))?;
                let target_db = self
                    .database_of_session(target_db_session_id)
                    .await
                    .map_err(|error| SchemaDiffPlanError::PlanNotFound(error.to_string()))?;
                (
                    plan,
                    target_db_session_id.clone(),
                    target_db,
                    target_schema.clone(),
                )
            }
        };
        let recovery_targets =
            recovery::prepare_target_objects(&self.deps.endpoints, &self.ctx, request)
                .await
                .map_err(|_| {
                    SchemaDiffPlanError::PlanStale("target identity unavailable".into())
                })?;
        let plan_id = plan
            .plan_id
            .clone()
            .ok_or_else(|| SchemaDiffPlanError::PlanNotFound("planId missing".into()))?;
        let fingerprint = recovery::fingerprint_targets(
            &self.deps.endpoints,
            &self.ctx,
            &tgt_sess,
            target_db.as_deref(),
            &recovery_targets,
        )
        .await
        .map_err(|error| SchemaDiffPlanError::PlanStale(error.to_string()))?;
        // 能力快照：目标会话的当前配置（桌面同款 fail-closed——解析失败
        // 即 CapabilityChanged，不带病出计划）。
        let target_session = self
            .deps
            .endpoints
            .resolve(
                &self.ctx,
                &tgt_sess,
                datazen::execution::application::LivePurpose::Inspection,
            )
            .await
            .map_err(|_| SchemaDiffPlanError::CapabilityChanged)?;
        let capability_snapshot =
            capability_hash(&target_session.config, target_session.connection_id());
        let transactional_ddl = plan.statements.iter().any(|s| s.requires_transaction)
            || plan.source_dialect.contains("sqlite")
            || plan.source_dialect.contains("postgres");
        let mut endpoint_evidence = vec![format!("planTables:{}", plan.tables.join(","))];
        datazen::migration::schema_diff::job::recovery_targets::record_target_objects(
            &mut endpoint_evidence,
            &recovery_targets,
        );
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
            source_connection_id: self
                .connection_id_of(&source_session_id_of(request))
                .await
                .map_err(|_| SchemaDiffPlanError::CapabilityChanged)?,
            target_connection_id: self
                .connection_id_of(&tgt_sess)
                .await
                .map_err(|_| SchemaDiffPlanError::CapabilityChanged)?,
            source_database: None,
            target_database: target_db,
            source_schema: None,
            target_schema,
            endpoint_evidence,
            capability_snapshot_hash: capability_snapshot,
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
        let current = self.current_capability_hash().await?;
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
        let target_session = self.target_session()?;
        let targets =
            datazen::migration::schema_diff::job::recovery_targets::targets_from_evidence(
                &plan_meta.endpoint_evidence,
            )
            .ok_or(SchemaDiffPlanError::PlanStale(
                "target identity unavailable".into(),
            ))?;
        recovery::fingerprint_targets(
            &self.deps.endpoints,
            &self.ctx,
            target_session,
            plan_meta.target_database.as_deref(),
            &targets,
        )
        .await
        .map_err(|error| SchemaDiffPlanError::PlanStale(error.to_string()))
    }

    async fn deploy(
        &self,
        plan: &SchemaDiffPlan,
        request: &ApplyRequest,
        cancel: &datazen::jobs::CancelToken,
    ) -> Result<SchemaDiffDeployResult, SchemaDiffPlanError> {
        let result = self
            .deploy_reviewed_plan(plan, request, Some(cancel.flag()))
            .await;
        match result {
            Ok(result) => Ok(result),
            Err(error) => Err(SchemaDiffPlanError::PlanStale(error.to_string())),
        }
    }

    async fn read_only_verify(
        &self,
        _plan_meta: &SchemaDiffFrozenPlan,
        _operation_id: &str,
    ) -> Result<ReadOnlyVerdict, SchemaDiffPlanError> {
        // 桌面同款：显式恢复命令可提供新会话；在安全的事后态投影可用前，
        // 不从变化的目标指纹单方面推断提交结局。
        Ok(ReadOnlyVerdict::Indeterminate)
    }
}

impl ServerSchemaDiffBackend {
    /// 表级计划的宿主侧准备（桌面 `prepare_table_plan_via_sessions` 同款：
    /// 入参校验先行 → 会话解析 → api 用例）。
    #[allow(clippy::too_many_arguments)]
    async fn prepare_table_plan_via_sessions(
        &self,
        source_db_session_id: &str,
        target_db_session_id: &str,
        table_names: Vec<String>,
        target_table_names: Vec<String>,
        target_only_table_names: Vec<String>,
        allow_destructive: bool,
        include_indexes: Option<bool>,
        type_overrides: Option<Vec<datazen::migration::schema_diff::types::ColumnTypeOverride>>,
        source_schema_override: Option<String>,
        target_schema_override: Option<String>,
    ) -> Result<SchemaDiffPlan, EndpointError> {
        // 入参校验先行（api 会再校验一次——双检纪律）。
        if table_names.is_empty() && target_only_table_names.is_empty() {
            return Err(EndpointError::Unavailable(
                "table_names and target_only_table_names must not both be empty".into(),
            ));
        }
        if table_names.len() != target_table_names.len() {
            return Err(EndpointError::Unavailable(
                "table_names and target_table_names must have the same length".into(),
            ));
        }
        let source = self
            .deps
            .endpoints
            .resolve(
                &self.ctx,
                source_db_session_id,
                datazen::execution::application::LivePurpose::Inspection,
            )
            .await?;
        let target = self
            .deps
            .endpoints
            .resolve(
                &self.ctx,
                target_db_session_id,
                datazen::execution::application::LivePurpose::Inspection,
            )
            .await?;
        let scope = self.scope();
        let adapters = ServerSchemaDiffAdapters {
            registry: Arc::clone(&self.deps.adapters),
        };
        schema_diff_api::prepare_table_plan(schema_diff_api::PrepareTablePlanRequest {
            source: &source.endpoint(),
            target: &target.endpoint(),
            source_session_id: source_db_session_id,
            target_session_id: target_db_session_id,
            table_names,
            target_table_names,
            target_only_table_names,
            allow_destructive,
            include_indexes,
            type_overrides,
            source_schema: source_schema_override.as_deref(),
            target_schema: target_schema_override.as_deref(),
            repository: self.deps.reviewed.as_ref(),
            scope: &scope,
            adapters: &adapters,
        })
        .await
        .map_err(|error| EndpointError::Unavailable(error.to_string()))
    }

    /// 统一（表 + schema 对象）计划的宿主侧准备。
    #[allow(clippy::too_many_arguments)]
    async fn prepare_unified_plan_via_sessions(
        &self,
        source_db_session_id: &str,
        target_db_session_id: &str,
        table_names: Vec<String>,
        target_table_names: Option<Vec<String>>,
        target_only_table_names: Option<Vec<String>>,
        source_objects: Option<Vec<datazen_driver_api::DatabaseObject>>,
        target_objects: Option<Vec<datazen_driver_api::DatabaseObject>>,
        source_schema: Option<String>,
        target_schema: Option<String>,
        allow_destructive: bool,
        include_indexes: Option<bool>,
        type_overrides: Option<Vec<datazen::migration::schema_diff::types::ColumnTypeOverride>>,
    ) -> Result<SchemaDiffPlan, EndpointError> {
        let target_table_names = target_table_names.unwrap_or_else(|| table_names.clone());
        let target_only_table_names = target_only_table_names.unwrap_or_default();
        let source_objects = source_objects.unwrap_or_default();
        let target_objects = target_objects.unwrap_or_default();
        validate_plan_selection(
            &table_names,
            &target_table_names,
            &target_only_table_names,
            &source_objects,
            &target_objects,
        )
        .map_err(|error| EndpointError::Unavailable(error.to_string()))?;
        let source = self
            .deps
            .endpoints
            .resolve(
                &self.ctx,
                source_db_session_id,
                datazen::execution::application::LivePurpose::Inspection,
            )
            .await?;
        let target = self
            .deps
            .endpoints
            .resolve(
                &self.ctx,
                target_db_session_id,
                datazen::execution::application::LivePurpose::Inspection,
            )
            .await?;
        let scope = self.scope();
        let adapters = ServerSchemaDiffAdapters {
            registry: Arc::clone(&self.deps.adapters),
        };
        schema_diff_api::prepare_unified_plan(schema_diff_api::PrepareUnifiedPlanRequest {
            source: &source.endpoint(),
            target: &target.endpoint(),
            source_session_id: source_db_session_id,
            target_session_id: target_db_session_id,
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
            repository: self.deps.reviewed.as_ref(),
            scope: &scope,
            adapters: &adapters,
        })
        .await
        .map_err(|error| EndpointError::Unavailable(error.to_string()))
    }

    /// 已评审计划的部署编排（桌面 `deploy_reviewed_plan_with_cancel` 的
    /// server 版：历史记账 → 只读/破坏性/回滚门 → 复核消费与部署前验证 →
    /// 源侧快照复核 → DDL 执行 → 终态记账）。错误顺序与文本逐字对齐。
    async fn deploy_reviewed_plan(
        &self,
        plan: &SchemaDiffPlan,
        request: &ApplyRequest,
        cancel_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<SchemaDiffDeployResult, EndpointError> {
        let profile_ref =
            request
                .profile
                .as_ref()
                .map(|(id, revision)| datazen::history::MigrationProfileRef {
                    id: id.clone(),
                    revision: revision.clone(),
                });
        self.deps
            .history
            .validate_profile_ref(self.ctx.organization_id.as_str(), profile_ref.as_ref())
            .await
            .map_err(EndpointError::Unavailable)?;
        let mut history_run = self
            .deps
            .history
            .start_run(
                self.ctx.organization_id.as_str(),
                "schemaDiff",
                profile_ref.as_ref(),
            )
            .await;
        history_run.selected_count = plan.statements.len() as u64;

        let target_session = match self
            .deps
            .endpoints
            .resolve(
                &self.ctx,
                &request.target_db_session_id,
                datazen::execution::application::LivePurpose::LongLivedHostWork,
            )
            .await
        {
            Ok(session) => session,
            Err(error) => {
                return self
                    .fail_deploy(history_run, EndpointError::Unavailable(error.to_string()))
                    .await;
            }
        };
        let owner = target_session.connection_id().to_owned();
        history_run.target_connection_id = Some(owner.clone());
        // 会话视图自带归属连接；档案缺席（归属连接已删）在 resolve 阶段
        // 已按 ProfileUnavailable 拒绝——这里补终态记账后拒绝。
        let config = target_session.config.clone();
        if config.read_only {
            return self
                .fail_deploy(
                    history_run,
                    EndpointError::Unavailable("Target connection is read-only".into()),
                )
                .await;
        }
        let effective_target_database = request
            .target_database
            .as_deref()
            .or(config.database.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let effective_target_schema = request
            .target_schema
            .as_deref()
            .or(config.schema.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if request.require_rollback
            && (!plan.rollback_completeness.complete
                || !request.use_transaction
                || !matches!(
                    datazen_driver_api::LegacySchemaMigration::new(target_session.driver.as_ref())
                        .ddl_atomicity(),
                    datazen_driver_api::DdlAtomicity::Transactional
                ))
        {
            return self
                .fail_deploy(
                    history_run,
                    EndpointError::Unavailable(
                        "Complete rollback requires transactional DDL and a complete rollback plan"
                            .into(),
                    ),
                )
                .await;
        }
        // 只有 `Unsupported` 阻断部署（`Backfill` 是计划已安全执行的后续动作：
        // 无默认值的 NOT NULL 列以可空加入，要求用户回填后再收紧——在它
        // 上拒绝会让计划永久不可部署）。
        if plan
            .requirements
            .iter()
            .any(|requirement| matches!(requirement, PlanRequirement::Unsupported { .. }))
        {
            return self
                .fail_deploy(
                    history_run,
                    EndpointError::Unavailable(
                        "Resolve unsupported plan requirements and prepare again before deploying"
                            .into(),
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
        if plan_has_destructive(plan) {
            let token = request.confirm_destructive.as_deref().unwrap_or("");
            if token != DESTRUCTIVE_CONFIRM_TOKEN {
                return self
                    .fail_deploy(
                        history_run,
                        EndpointError::Unavailable(format!(
                            "destructive plan requires confirm_destructive = \"{DESTRUCTIVE_CONFIRM_TOKEN}\""
                        )),
                    )
                    .await;
            }
        }
        // 复核消费 + 部署前目标侧验证。
        let target_endpoint = target_session.endpoint();
        let reviewed = match schema_diff_api::consume_and_verify_reviewed_plan(
            self.deps.reviewed.as_ref(),
            &self.scope(),
            plan,
            &target_endpoint,
            &request.target_db_session_id,
            effective_target_database,
            effective_target_schema,
        )
        .await
        {
            Ok(reviewed) => reviewed,
            Err(error) => {
                return self
                    .fail_deploy(history_run, EndpointError::Unavailable(error.to_string()))
                    .await;
            }
        };
        // 源侧快照复核（源会话从复核记录解析）。
        if let Some(source_snapshot) = reviewed.source_snapshot.as_ref() {
            let source_session = match self
                .deps
                .endpoints
                .resolve(
                    &self.ctx,
                    &source_snapshot.session,
                    datazen::execution::application::LivePurpose::Inspection,
                )
                .await
            {
                Ok(session) => session,
                Err(error) => {
                    return self
                        .fail_deploy(history_run, EndpointError::Unavailable(error.to_string()))
                        .await;
                }
            };
            if let Err(error) =
                schema_diff_api::revalidate_source_snapshot(&source_session.endpoint(), &reviewed)
                    .await
            {
                return self
                    .fail_deploy(history_run, EndpointError::Unavailable(error.to_string()))
                    .await;
            }
        }
        let opts = DeployOptions {
            use_transaction: request.use_transaction,
            stop_on_error: true,
        };
        let result = datazen::migration::schema_diff::execute_schema_diff_deploy_at(
            target_session.driver.as_ref(),
            &target_session.handle,
            plan,
            opts,
            cancel_flag,
            datazen_driver_api::SqlTarget::new(effective_target_database, effective_target_schema),
        )
        .await;
        let cancelled_outcome = matches!(
            result.status,
            datazen::migration::schema_diff::types::DeployStatus::Cancelled
        );
        let success_outcome = matches!(
            result.status,
            datazen::migration::schema_diff::types::DeployStatus::Committed
        );
        let rollback_outcome = match result.status {
            datazen::migration::schema_diff::types::DeployStatus::RolledBack => "completed",
            datazen::migration::schema_diff::types::DeployStatus::Unknown
            | datazen::migration::schema_diff::types::DeployStatus::Mixed => "unknown",
            _ => "notRequired",
        };
        self.deps
            .history
            .finish_run(
                self.ctx.organization_id.as_str(),
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

    /// 部署失败收尾（桌面 `fail_schema_diff_deploy` 同款：把历史记账行落
    /// 成失败后原样返回错误）。
    async fn fail_deploy<T>(
        &self,
        run: datazen::history::MigrationRunRecord,
        error: EndpointError,
    ) -> Result<T, EndpointError> {
        self.deps
            .history
            .finish_run(
                self.ctx.organization_id.as_str(),
                run,
                false,
                false,
                0,
                1,
                0,
                "unknown",
            )
            .await;
        Err(error)
    }

    async fn database_of_session(&self, session_id: &str) -> Result<Option<String>, EndpointError> {
        let session = self
            .deps
            .endpoints
            .resolve(
                &self.ctx,
                session_id,
                datazen::execution::application::LivePurpose::Inspection,
            )
            .await?;
        Ok(session.config.database.clone())
    }

    async fn connection_id_of(&self, session_id: &str) -> Result<String, EndpointError> {
        let session = self
            .deps
            .endpoints
            .resolve(
                &self.ctx,
                session_id,
                datazen::execution::application::LivePurpose::Inspection,
            )
            .await?;
        Ok(session.connection_id().to_owned())
    }
}

fn source_session_id_of(request: &PrepareRequest) -> String {
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

/// §4.2 服务端确认边界：运行时仓配化的恢复策略字符串（桌面同款）。
fn recovery_policy_str(transactional_ddl: bool) -> &'static str {
    if transactional_ddl {
        "inTransactionBatch"
    } else {
        "readOnlyVerify"
    }
}

fn fnv_hex_of_json<T: serde::Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    datazen::migration::schema_diff::job::fnv1a64_hex(&bytes)
}

/// sync 适配器端口（`SchemaDiffAdapterHost`）的服务端实现：注册表挂共享层
/// `SyncAdapterRegistry`（与桌面同一实例语义——懒注册，按方言解析）。
pub struct ServerSchemaDiffAdapters {
    /// 注册表（bootstrap 需读它装配 backend 依赖；与 data_transfer 同一
    /// 实例语义）。
    pub registry:
        Arc<datazen::migration::data_transfer::transfer::adapter_registry::SyncAdapterRegistry>,
}

impl SchemaDiffAdapterHost for ServerSchemaDiffAdapters {
    fn resolve_plan_pair(
        &self,
        source_type: &str,
        target_type: &str,
    ) -> Result<datazen::migration::schema_diff::api::PlanSyncAdapters, String> {
        // 注册表以 `&DatabaseType`（= &String）为键；端口入参是 &str，在
        // 此各自提升一次（桌面同款，查找与失败文本不变）。
        let source_type = source_type.to_owned();
        let target_type = target_type.to_owned();
        self.registry.ensure_pair(&source_type, &target_type)?;
        let source = self
            .registry
            .get_source(&source_type)
            .ok_or_else(|| "missing source sync adapter".to_string())?;
        let target = self
            .registry
            .get_target(&target_type)
            .ok_or_else(|| "missing target sync adapter".to_string())?;
        Ok(datazen::migration::schema_diff::api::PlanSyncAdapters { source, target })
    }

    fn comparison_adapters(
        &self,
        source_type: &str,
        target_type: &str,
    ) -> Option<datazen::migration::schema_diff::api::ComparisonSyncAdapters> {
        let source_type = source_type.to_owned();
        let target_type = target_type.to_owned();
        self.registry.ensure_pair(&source_type, &target_type).ok()?;
        Some(
            datazen::migration::schema_diff::api::ComparisonSyncAdapters {
                source_of_source: self.registry.get_source(&source_type)?,
                source_of_target: self.registry.get_source(&target_type)?,
                target_of_source: self.registry.get_target(&source_type)?,
                target_of_target: self.registry.get_target(&target_type)?,
            },
        )
    }
}

/// 对比预览端口的无状态服务端实现（桌面 `DesktopComparisonPreviewHost`
/// 同款：DDL 渲染与全类型读取由共享层 data_transfer 提供）。
pub struct ServerComparisonPreviewHost;

#[async_trait]
impl datazen::migration::schema_diff::api::ComparisonPreviewHost for ServerComparisonPreviewHost {
    async fn full_column_types(
        &self,
        adapter: &dyn datazen_driver_api::sync::SyncSourceAdapter,
        driver: &dyn datazen_driver_api::DatabaseDriver,
        handle: &datazen_driver_api::ConnectionHandle,
        table: &str,
    ) -> Option<std::collections::HashMap<String, String>> {
        datazen::migration::data_transfer::transfer::full_types::fetch_full_column_types(
            adapter, driver, handle, table,
        )
        .await
        .ok()
    }

    fn create_table_ddl(
        &self,
        ir_table: &datazen_driver_api::sync::IRTable,
        target: &dyn datazen_driver_api::sync::SyncTargetAdapter,
    ) -> String {
        datazen::migration::data_transfer::transfer::ddl::build_create_table_ddl(ir_table, target)
    }
}

/// apply 前的兼容路径载荷（桌面 `apply_payload` 同款：安全投影白名单——
/// 计划正文/会话句柄/秘密一律不进 payload）。
pub(crate) fn apply_payload(
    stored: &StoredPlan,
    request: &ApplyRequest,
) -> Result<serde_json::Value, String> {
    let plan_bytes = serde_json::to_vec(&stored.plan)
        .map_err(|_| "Schema Diff plan digest failed".to_string())?;
    let plan_digest = format!("sha256:{:x}", Sha256::digest(plan_bytes));
    let profile_revision_digest = request
        .profile
        .as_ref()
        .map(|profile| {
            serde_json::to_vec(profile)
                .map(|bytes| format!("sha256:{:x}", Sha256::digest(bytes)))
                .map_err(|_| "Schema Diff profile digest failed".to_string())
        })
        .transpose()?;
    let object_ids =
        datazen::migration::schema_diff::job::recovery_targets::target_ids_from_evidence(
            &stored.meta.endpoint_evidence,
        );
    let recovery_targets = serde_json::json!([{
        "connectionId": stored.meta.target_connection_id,
        "objectIds": object_ids,
    }]);
    let mut payload = serde_json::json!({
        "planVersion": 1,
        "handlerVersion": 1,
        "checkpointVersion": 1,
        "consumedPlanId": stored.meta.plan_id,
        "planDigest": plan_digest,
        "selectionRevision": stored.meta.selection_revision,
        "recoveryTargets": recovery_targets,
        "targetBeforeFingerprint": stored.meta.schema_fingerprint,
        "recoveryPolicy": stored.meta.recovery_policy.as_str(),
        "useTransaction": request.use_transaction,
        "requireRollback": request.require_rollback,
        "confirmedDestructive": request.confirm_destructive.as_deref()
            == Some(DESTRUCTIVE_CONFIRM_TOKEN),
    });
    if let Some(digest) = profile_revision_digest {
        payload["profileRevisionDigest"] = serde_json::Value::String(digest);
    }
    Ok(payload)
}

/// prepare 载荷（桌面 `prepare_payload` 同款）。
pub(crate) fn prepare_payload() -> serde_json::Value {
    serde_json::json!({
        "planVersion": 1,
        "handlerVersion": 1,
        "checkpointVersion": 1,
    })
}

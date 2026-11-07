//! Schema Diff 的 JobRuntime 接线（W8 刀3b-2）。
//!
//! 桌面对照 `datazen-desktop/src/commands/schema_diff/job/desktop.rs`：
//! 受理（先持久化后取资源）→ 派发（tokio spawn，桌面 tauri::async_runtime
//! 的 server 对应物）→ 读回/取消/恢复核验。**派发失败收尾逐字复刻**：
//! Queued → `mark_failed_unstarted`；Running → `mark_pending_verification`
//! + `persist_recovery`（dispatchNeedsVerification）。
//!
//! SQL 正文只活在进程内 [`PlanRepository`]（评审期产物，桌面同款——重启
//! 后 `plan_unavailable_after_restart` 如实回答）。

use std::sync::Arc;

use serde::Serialize;

use datazen::jobs::domain::job::{
    JobDetails, JobFilter, JobRecoveryRequest, JobRecoveryResult, JobRecoveryVerdict,
    JobRecoveryVerification, JobResultCounter, JobResultItem, JobState, JobView,
};
use datazen::jobs::{EndpointRef, EndpointRole, JobHost};
use datazen::migration::schema_diff::job::{
    ApplyRequest, PlanRepository, PrepareRequest, SchemaDiffHandler, StoredPlan,
};
use datazen::migration::schema_diff::types::{DeployStatus, SchemaDiffDeployResult};
use datazen::shared::context::OwnerRef;
use datazen::shared::context::RequestContext;
use datazen::shared::error::PortError;
use datazen::shared::id::{JobId, StageId};

use super::backend::{
    apply_payload, prepare_payload, ServerSchemaDiffBackend, ServerSchemaDiffBackendDeps,
};
use super::endpoints::SessionEndpoints;
use super::recovery;

const PREPARE_KIND: &str = "schemaDiffPrepare";
const APPLY_KIND: &str = "schemaDiffApply";

/// 受理回执（202 应答体；桌面 `SchemaDiffJobAccepted` 同形）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiffJobAccepted {
    pub job_id: String,
    pub kind: String,
    pub state: JobState,
    pub progress: datazen::jobs::domain::job::JobProgress,
}

/// 读回（含进程内计划正文与部署结果投影；桌面 `SchemaDiffJobDetails`
/// 同形——`prepared` 为 None 即「重启后不可得」，如实回答）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiffJobDetails {
    pub details: JobDetails,
    pub prepared: Option<SchemaDiffPrepareEnvelope>,
    pub plan_unavailable_after_restart: bool,
    pub deploy_result: Option<SchemaDiffDeployResult>,
}

/// prepare 产物信封（桌面 `SchemaDiffPrepareEnvelope` 同形）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiffPrepareEnvelope {
    pub plan: datazen::migration::schema_diff::types::SchemaDiffPlan,
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

/// 服务错误（路由层映射：NotFound→404 / StaleClaim 与 CasConflict→409 /
/// 其余→503 / 校验→400）。
#[derive(Debug, thiserror::Error)]
pub enum SchemaDiffJobError {
    #[error("schema diff job '{0}' was not found")]
    NotFound(String),
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Conflict(String),
    #[error("Schema Diff job storage is unavailable")]
    Storage,
}

/// Schema Diff 的 Job 服务（composition root 注入；`plans` 是进程级
/// PlanRepository——桌面 `SchemaDiffJobInfra` 同款驻留）。
#[derive(Clone)]
pub struct SchemaDiffJobs {
    host: Arc<JobHost>,
    plans: Arc<PlanRepository>,
    backend_deps: ServerSchemaDiffBackendDeps,
}

impl SchemaDiffJobs {
    pub fn new(
        host: Arc<JobHost>,
        plans: Arc<PlanRepository>,
        backend_deps: ServerSchemaDiffBackendDeps,
    ) -> Self {
        Self {
            host,
            plans,
            backend_deps,
        }
    }

    /// `listSchemaDiffJobs`（states/limit 过滤；桌面 `read_schema_diff_jobs`
    /// 同款——按 kind 收敛，limit clamp 100..=500）。
    pub async fn list(
        &self,
        ctx: &RequestContext,
        states: Option<Vec<JobState>>,
        limit: Option<u32>,
    ) -> Result<Vec<JobView>, SchemaDiffJobError> {
        let records = self
            .host
            .list(
                ctx,
                JobFilter {
                    organization_id: ctx.organization_id.clone(),
                    states: states.unwrap_or_default(),
                    owner: None,
                    after: None,
                    limit: Some(limit.unwrap_or(100).min(500)),
                },
            )
            .await
            .map_err(host_error)?;
        Ok(records
            .into_iter()
            .filter(|record| record.view.kind == PREPARE_KIND || record.view.kind == APPLY_KIND)
            .map(|record| record.view)
            .collect())
    }

    /// `getSchemaDiffJobDetails`（桌面 `read_schema_diff_job_details` 同款：
    /// 计划正文从进程内 PlanRepository 折叠；`plan_unavailable_after_restart`
    /// 只对 prepare·Succeeded 且正文缺席的情形成真）。
    pub async fn details(
        &self,
        ctx: &RequestContext,
        job_id: &str,
    ) -> Result<SchemaDiffJobDetails, SchemaDiffJobError> {
        let mut details = self
            .host
            .details(ctx, JobId::new(job_id.to_owned()))
            .await
            .map_err(host_error)?;
        if details.job.kind != PREPARE_KIND && details.job.kind != APPLY_KIND {
            return Err(SchemaDiffJobError::NotFound(job_id.to_owned()));
        }
        let plan_id = details
            .job
            .artifact_ids
            .iter()
            .find_map(|artifact| artifact.as_str().strip_prefix("artifact:plan:"));
        if details.plan_id.is_none() {
            details.plan_id = plan_id.map(str::to_owned);
        }
        let prepared = plan_id.and_then(|id| self.prepare_envelope(id));
        let plan_unavailable_after_restart = details.job.kind == PREPARE_KIND
            && details.job.state == JobState::Succeeded
            && prepared.is_none();
        let deploy_result = (details.job.kind == APPLY_KIND && details.job.state.is_terminal())
            .then(|| deploy_result_of(&details));
        Ok(SchemaDiffJobDetails {
            details,
            prepared,
            plan_unavailable_after_restart,
            deploy_result,
        })
    }

    /// `cancelSchemaDiffDeploy`（桌面 `cancel_schema_diff_job` 同款：未知
    /// id / 他类 job → false（幂等 no-op），终态 → true，在途 → 登记取消
    /// 意图并回 true）。
    pub async fn cancel(
        &self,
        ctx: &RequestContext,
        job_id: &str,
    ) -> Result<bool, SchemaDiffJobError> {
        let id = JobId::new(job_id.to_owned());
        match self.host.get(ctx, id.clone()).await {
            Err(PortError::NotFound(_)) => Ok(false),
            Err(error) => Err(host_error(error)),
            Ok(record) if record.view.kind != PREPARE_KIND && record.view.kind != APPLY_KIND => {
                Ok(false)
            }
            Ok(record) if record.view.state.is_terminal() => Ok(true),
            Ok(_) => self
                .host
                .cancel(ctx, &id)
                .await
                .map(|_| true)
                .map_err(host_error),
        }
    }

    /// `verifySchemaDiffJobRecovery`（桌面 `verify_schema_diff_recovery`
    /// 同款：apply 类 job + 目标连接归属核对 → 只读核验（指纹比对 →
    /// RequireManualReview 结论族）→ 读回）。
    pub async fn verify_recovery(
        &self,
        endpoints: &SessionEndpoints,
        ctx: &RequestContext,
        job_id: &str,
        target_db_session_id: &str,
    ) -> Result<SchemaDiffJobDetails, SchemaDiffJobError> {
        let id = JobId::new(job_id.to_owned());
        let details = self
            .host
            .details(ctx, id.clone())
            .await
            .map_err(host_error)?;
        if details.job.kind != APPLY_KIND {
            return Err(SchemaDiffJobError::NotFound(format!(
                "schema diff apply job '{job_id}' was not found"
            )));
        }
        let connection_id = endpoints
            .identity_of(ctx, target_db_session_id)
            .await
            .map_err(|error| SchemaDiffJobError::Validation(error.to_string()))?
            .connection_id
            .as_str()
            .to_owned();
        if !details
            .recovery_targets
            .iter()
            .any(|target| target.connection_id.as_str() == connection_id)
        {
            return Err(SchemaDiffJobError::Validation(
                "Reconnect the target connection used by this job before read-only verification"
                    .into(),
            ));
        }
        let verifier = Arc::new(RecoveryVerifier {
            endpoints: endpoints.clone(),
            target_db_session_id: target_db_session_id.to_owned(),
            connection_id,
        });
        self.host
            .verify_recovery(ctx, id, verifier)
            .await
            .map_err(host_error)?;
        self.details(ctx, job_id).await
    }

    /// `prepareSchemaDiffPlan` / `prepareSchemaUnifiedPlan` 的受理+派发。
    pub async fn run_prepare_job(
        &self,
        ctx: &RequestContext,
        request: PrepareRequest,
    ) -> Result<SchemaDiffJobAccepted, SchemaDiffJobError> {
        let (source_session, target_session, objects) = prepare_endpoints(&request);
        let endpoints = self
            .endpoints_from_sessions(ctx, Some(&source_session), &target_session, &objects)
            .await?;
        self.host
            .ensure_endpoint_services(&endpoints)
            .map_err(host_error)?;
        let job_id = JobId::new(format!("schemaDiffPrepare-{}", uuid::Uuid::new_v4()));
        let definition = datazen::jobs::domain::job::JobDefinition {
            job_id: job_id.clone(),
            organization_id: ctx.organization_id.clone(),
            kind: PREPARE_KIND.into(),
            owner: server_owner_ref(ctx),
            payload: prepare_payload(),
            created_at: self.host.now(),
        };
        let record = self
            .host
            .accept(
                ctx,
                definition,
                &datazen::shared::id::IdempotencyKey::new(format!("schema-diff-prepare-{job_id}")),
            )
            .await
            .map_err(host_error)?;
        if record.view.job_id == job_id {
            let host = Arc::clone(&self.host);
            let deps = self.backend_deps.clone();
            let ctx = ctx.clone();
            let plans = Arc::clone(&self.plans);
            tokio::spawn(async move {
                let backend = Arc::new(ServerSchemaDiffBackend::new(deps, ctx.clone()));
                let handler = Arc::new(SchemaDiffHandler::for_prepare(backend, plans, request));
                dispatch(host, ctx, job_id, endpoints, handler).await;
            });
        }
        Ok(accepted(&record.view))
    }

    /// `executeSchemaDiffDeploy` 的受理+派发（先持久化后取资源：回执先于
    /// 任何 DDL 执行返回）。
    pub async fn run_apply_job(
        &self,
        ctx: &RequestContext,
        plan_id: &str,
        selection_revision: u64,
        mut apply_request: ApplyRequest,
    ) -> Result<SchemaDiffJobAccepted, SchemaDiffJobError> {
        let stored = self.plans.get(plan_id).ok_or_else(|| {
            SchemaDiffJobError::NotFound(format!("plan '{plan_id}' is unavailable; compare again"))
        })?;
        if stored.meta.selection_revision != selection_revision {
            return Err(SchemaDiffJobError::Validation(
                "Schema Diff plan selection changed; compare again".into(),
            ));
        }
        let endpoints = self
            .endpoints_from_sessions(
                ctx,
                None,
                &apply_request.target_db_session_id,
                &plan_objects(&stored.meta),
            )
            .await?;
        self.host
            .ensure_endpoint_services(&endpoints)
            .map_err(host_error)?;
        let job_id = JobId::new(format!("schemaDiffApply-{}", uuid::Uuid::new_v4()));
        apply_request.job_id = Some(job_id.as_str().to_string());
        let definition = datazen::jobs::domain::job::JobDefinition {
            job_id: job_id.clone(),
            organization_id: ctx.organization_id.clone(),
            kind: APPLY_KIND.into(),
            owner: server_owner_ref(ctx),
            payload: apply_payload(&stored, &apply_request)
                .map_err(SchemaDiffJobError::Validation)?,
            created_at: self.host.now(),
        };
        let receipt_key = format!("schema-diff-apply-{plan_id}-{selection_revision}");
        let record = self
            .host
            .accept(
                ctx,
                definition,
                &datazen::shared::id::IdempotencyKey::new(receipt_key),
            )
            .await
            .map_err(host_error)?;
        if record.view.job_id == job_id {
            let host = Arc::clone(&self.host);
            let deps = self.backend_deps.clone();
            let ctx = ctx.clone();
            let plans = Arc::clone(&self.plans);
            let target_session_id = apply_request.target_db_session_id.clone();
            let plan_id = plan_id.to_owned();
            tokio::spawn(async move {
                let backend = Arc::new(ServerSchemaDiffBackend::for_apply(
                    deps,
                    ctx.clone(),
                    target_session_id,
                ));
                let handler = Arc::new(SchemaDiffHandler::for_apply(
                    backend,
                    plans,
                    plan_id,
                    selection_revision,
                    apply_request,
                ));
                dispatch(host, ctx, job_id, endpoints, handler).await;
            });
        }
        Ok(accepted(&record.view))
    }

    /// 旧 IPC 兼容路径：客户端直接携带计划正文时，先注册进
    /// PlanRepository 再走 apply Job（桌面 `register_plan_for_apply`
    /// 同款——D1：登记的 meta 与核验用同一套哈希计算）。
    pub async fn register_plan_for_apply(
        &self,
        endpoints: &SessionEndpoints,
        ctx: &RequestContext,
        plan: datazen::migration::schema_diff::types::SchemaDiffPlan,
        target_db_session_id: &str,
    ) -> Result<(String, u64), SchemaDiffJobError> {
        let plan_id = plan
            .plan_id
            .clone()
            .ok_or_else(|| SchemaDiffJobError::Validation("plan requires planId".into()))?;
        let session = endpoints
            .resolve(
                ctx,
                target_db_session_id,
                datazen::execution::application::LivePurpose::Inspection,
            )
            .await
            .map_err(|error| SchemaDiffJobError::Validation(error.to_string()))?;
        let target_schema = session.config.schema.clone();
        let targets = plan
            .tables
            .iter()
            .map(|table| {
                datazen::migration::schema_diff::job::recovery_targets::table_target(
                    table,
                    target_schema.as_deref(),
                )
            })
            .collect::<Vec<_>>();
        let fingerprint = recovery::fingerprint_session_targets(
            &session,
            session.config.database.as_deref(),
            &targets,
        )
        .await
        .map_err(|error| SchemaDiffJobError::Validation(error.to_string()))?;
        let mut endpoint_evidence = vec![format!("planTables:{}", plan.tables.join(","))];
        datazen::migration::schema_diff::job::recovery_targets::record_target_objects(
            &mut endpoint_evidence,
            &targets,
        );
        let meta = datazen::migration::schema_diff::job::SchemaDiffFrozenPlan {
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
            target_connection_id: session.connection_id().to_owned(),
            source_database: None,
            target_database: session.config.database.clone(),
            source_schema: None,
            target_schema,
            endpoint_evidence,
            capability_snapshot_hash: super::endpoints::capability_hash(
                &session.config,
                session.connection_id(),
            ),
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
        self.plans.insert(StoredPlan {
            meta,
            plan,
            body_digest: String::new(),
        });
        Ok((plan_id, 1))
    }

    fn prepare_envelope(&self, plan_id: &str) -> Option<SchemaDiffPrepareEnvelope> {
        let stored = self.plans.get(plan_id)?;
        let meta = stored.meta;
        Some(SchemaDiffPrepareEnvelope {
            plan: stored.plan,
            plan_id: meta.plan_id,
            source_connection_id: meta.source_connection_id,
            target_connection_id: meta.target_connection_id,
            selection_revision: meta.selection_revision,
            plan_version: meta.plan_version,
            handler_version: meta.handler_version,
            checkpoint_version: meta.checkpoint_version,
            expires_at: meta.expires_at.as_str().to_string(),
            recovery_policy: meta.recovery_policy.as_str().to_string(),
        })
    }

    /// 端点登记（桌面 `endpoints_from_session_pair` 同款：源端（apply 缺席）
    /// + 目标端，service_key 走物理身份 fail-closed）。
    async fn endpoints_from_sessions(
        &self,
        ctx: &RequestContext,
        source_session: Option<&str>,
        target_session: &str,
        objects: &[String],
    ) -> Result<Vec<EndpointRef>, SchemaDiffJobError> {
        let mut out = Vec::new();
        if let Some(session) = source_session {
            let identity = self
                .backend_deps
                .endpoints
                .identity_of(ctx, session)
                .await
                .map_err(|error| SchemaDiffJobError::Validation(error.to_string()))?;
            out.push(EndpointRef {
                connection_id: identity.connection_id,
                service_key: identity.service_key,
                objects: objects.to_vec(),
                role: EndpointRole::SourceReader,
            });
        }
        let identity = self
            .backend_deps
            .endpoints
            .identity_of(ctx, target_session)
            .await
            .map_err(|error| SchemaDiffJobError::Validation(error.to_string()))?;
        out.push(EndpointRef {
            connection_id: identity.connection_id,
            service_key: identity.service_key,
            objects: objects.to_vec(),
            role: EndpointRole::TargetWriter,
        });
        Ok(out)
    }
}

/// 派发失败收尾（桌面 `dispatch` 的 Err 路径逐字复刻）。
async fn dispatch(
    host: Arc<JobHost>,
    ctx: RequestContext,
    job_id: JobId,
    endpoints: Vec<EndpointRef>,
    handler: Arc<dyn datazen::jobs::JobHandler>,
) {
    let result = host
        .dispatch(
            &ctx,
            &job_id,
            &datazen::shared::id::WorkerId::new("schema-diff-server"),
            &endpoints,
            handler,
        )
        .await;
    let Err(error) = result else { return };
    let reason_code = dispatch_error_code(&error);
    let repository = host.repository();
    match host.get(&ctx, job_id.clone()).await {
        Ok(record) if record.view.state == JobState::Queued => {
            let _ = repository
                .mark_failed_unstarted(&ctx, &job_id, reason_code)
                .await;
        }
        Ok(record) if record.view.state == JobState::Running => {
            if repository
                .mark_pending_verification(&ctx, &job_id, "dispatchNeedsVerification")
                .await
                .is_ok()
            {
                let _ = repository
                    .persist_recovery(
                        &ctx,
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
        "schema diff Job dispatch failed"
    );
}

fn dispatch_error_code(error: &PortError) -> &'static str {
    match error {
        PortError::QuotaExceeded(_) => "budgetDenied",
        PortError::StaleClaim => "staleClaim",
        PortError::CasConflict { .. } => "stateConflict",
        PortError::UnsupportedVersion(_) => "unsupportedHandler",
        PortError::PlanAlreadyConsumed(_) => "planAlreadyConsumed",
        PortError::IdempotencyConflict => "idempotencyConflict",
        _ => "dispatchFailed",
    }
}

fn host_error(error: PortError) -> SchemaDiffJobError {
    match error {
        PortError::NotFound(id) => SchemaDiffJobError::NotFound(id),
        PortError::IdempotencyConflict => SchemaDiffJobError::Validation(
            "Schema Diff job receipt conflicts with an earlier request".into(),
        ),
        PortError::BackendUnavailable(_) => SchemaDiffJobError::Storage,
        other => SchemaDiffJobError::Validation(format!("Schema Diff job failed: {other}")),
    }
}

fn accepted(view: &JobView) -> SchemaDiffJobAccepted {
    SchemaDiffJobAccepted {
        job_id: view.job_id.as_str().to_owned(),
        kind: view.kind.clone(),
        state: view.state,
        progress: view.progress.clone(),
    }
}

/// 桌面 `owner_ref` 的 server 版：ClientSession owner 落受理主体的客户端
/// 实例（描述性字段；落在 `owner_json` 里从不被比对）。
fn server_owner_ref(ctx: &RequestContext) -> OwnerRef {
    OwnerRef::ClientSession {
        client_instance_id: ctx.client_instance_id.clone(),
        purpose: "migration".into(),
    }
}

fn prepare_endpoints(request: &PrepareRequest) -> (String, String, Vec<String>) {
    match request {
        PrepareRequest::Table {
            source_db_session_id,
            target_db_session_id,
            target_table_names,
            target_only_table_names,
            ..
        } => (
            source_db_session_id.clone(),
            target_db_session_id.clone(),
            target_table_names
                .iter()
                .chain(target_only_table_names)
                .cloned()
                .collect(),
        ),
        PrepareRequest::Unified {
            source_db_session_id,
            target_db_session_id,
            target_table_names,
            target_only_table_names,
            target_objects,
            ..
        } => {
            let mut objects = target_table_names
                .iter()
                .chain(target_only_table_names)
                .cloned()
                .collect::<Vec<_>>();
            objects.extend(target_objects.iter().map(|object| object.name.clone()));
            (
                source_db_session_id.clone(),
                target_db_session_id.clone(),
                objects,
            )
        }
    }
}

fn plan_objects(meta: &datazen::migration::schema_diff::job::SchemaDiffFrozenPlan) -> Vec<String> {
    meta.endpoint_evidence
        .iter()
        .find_map(|evidence| evidence.strip_prefix("planTables:"))
        .map(|tables| {
            tables
                .split(',')
                .filter(|table| !table.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn deploy_result_of(details: &JobDetails) -> SchemaDiffDeployResult {
    let effect = details
        .job
        .effect_outcome
        .unwrap_or(datazen::execution::EffectOutcome::NotStarted);
    let status = match (details.job.state, effect) {
        (_, datazen::execution::EffectOutcome::Completed) => DeployStatus::Committed,
        (_, datazen::execution::EffectOutcome::Unknown) => DeployStatus::Unknown,
        (JobState::Failed, datazen::execution::EffectOutcome::PartiallyApplied) => {
            DeployStatus::Mixed
        }
        (JobState::Cancelled, datazen::execution::EffectOutcome::PartiallyApplied) => {
            DeployStatus::Mixed
        }
        (JobState::Failed, datazen::execution::EffectOutcome::RolledBack) => {
            DeployStatus::RolledBack
        }
        (JobState::Cancelled, _) => DeployStatus::Cancelled,
        (JobState::Succeeded, _) => DeployStatus::Committed,
        _ => DeployStatus::Failed,
    };
    SchemaDiffDeployResult {
        status,
        executed_count: details.job.progress.attempted.get() as usize,
        statement_count: details.job.progress.converted.get() as usize,
        errors: details.job.error.clone().into_iter().collect(),
        statement_results: Vec::new(),
    }
}

/// 恢复核验器（桌面 `SchemaDiffRecoveryVerifier` 同款：指纹比对 →
/// RequireManualReview 结论族；读不到 → targetReadUnavailable）。
struct RecoveryVerifier {
    endpoints: SessionEndpoints,
    target_db_session_id: String,
    connection_id: String,
}

#[async_trait::async_trait]
impl datazen::jobs::JobRecoveryVerifier for RecoveryVerifier {
    fn kind(&self) -> &str {
        APPLY_KIND
    }

    async fn verify(
        &self,
        context: &RequestContext,
        request: &JobRecoveryRequest,
    ) -> Result<JobRecoveryVerification, PortError> {
        let targets = request
            .details
            .recovery_targets
            .iter()
            .find(|target| target.connection_id.as_str() == self.connection_id);
        let Some(target_row) = targets else {
            return Ok(manual_review("targetIdentityUnavailable", 0));
        };
        let decoded = target_row
            .object_ids
            .iter()
            .map(|id| datazen::migration::schema_diff::job::recovery_targets::decode_target_id(id))
            .collect::<Option<Vec<_>>>();
        let Some(decoded) = decoded.filter(|items| !items.is_empty()) else {
            return Ok(manual_review("targetIdentityUnavailable", 0));
        };
        let Some(before) = request.details.target_before_fingerprint.as_deref() else {
            return Ok(manual_review("targetFingerprintUnavailable", decoded.len()));
        };
        if !is_sha256_fingerprint(before) {
            return Ok(manual_review("targetFingerprintUnavailable", decoded.len()));
        }
        let session = match self
            .endpoints
            .resolve(
                context,
                &self.target_db_session_id,
                datazen::execution::application::LivePurpose::Inspection,
            )
            .await
        {
            Ok(session) => session,
            Err(_) => return Ok(manual_review("targetReadUnavailable", decoded.len())),
        };
        let current = match recovery::fingerprint_session_targets(&session, None, &decoded).await {
            Ok(fingerprint) => fingerprint,
            Err(_) => return Ok(manual_review("targetReadUnavailable", decoded.len())),
        };
        let reason_code = if current == before {
            "targetStateUnchangedReprepareRequired"
        } else {
            "targetStateChangedManualReview"
        };
        Ok(manual_review(reason_code, decoded.len()))
    }
}

fn manual_review(reason_code: &str, checked: usize) -> JobRecoveryVerification {
    let outcome_code = match reason_code {
        "targetStateUnchangedReprepareRequired" => "beforeStateUnchanged",
        "targetStateChangedManualReview" => "targetStateChanged",
        "targetReadUnavailable" => "targetReadUnavailable",
        "targetFingerprintUnavailable" => "fingerprintUnavailable",
        _ => "targetIdentityUnavailable",
    };
    JobRecoveryVerification {
        result: JobRecoveryResult {
            verdict: JobRecoveryVerdict::RequireManualReview,
            resume_through: None,
            reason_code: Some(reason_code.to_string()),
        },
        confirmed_boundaries: Vec::new(),
        domain_results: vec![datazen::jobs::domain::job::JobDomainResult {
            stage_id: StageId::new("apply"),
            result_code: "schemaDiffRecovery".into(),
            outcome_code: outcome_code.into(),
            counters: vec![JobResultCounter {
                code: "targetsChecked".into(),
                value: datazen::shared::id::Counter::new(checked.min(u64::MAX as usize) as u64),
            }],
            items: vec![JobResultItem {
                item_id: "target".into(),
                outcome_code: outcome_code.into(),
                reason_code: Some(reason_code.to_string()),
            }],
            artifact_ids: Vec::new(),
        }],
    }
}

fn is_sha256_fingerprint(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

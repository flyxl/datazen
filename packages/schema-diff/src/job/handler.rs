//! SchemaDiffHandler：JobHandler 三态协议（validatePlan / runStage / verifyRecovery）。
//!
//! 拆成两个受理实例：`schemaDiffPrepare` 与 `schemaDiffApply`（§2.1）。
//! handler 按请求构造（携带准备请求或应用请求），JobRuntime 负责 claim、
//! 预算、阶段调度、取消意图、事件与 cleanup；handler 只汇报阶段结局。

use std::sync::Arc;

use datazen_platform_api::dto::execution::{EffectOutcome, ExecutionErrorCode};
use datazen_platform_api::dto::job::{Checkpoint, CommitBoundary, JobProgress};
use datazen_platform_api::id::{ArtifactId, Counter, StageId, Timestamp};

use datazen_runtime::job::{
    CancelToken, JobError, JobHandler, RecoveryVerdict, StageOutcome, StageSpec, StageTerminal,
};

use crate::job::backend::{ApplyRequest, PrepareRequest, SchemaDiffJobBackend};
use crate::job::plan::{fnv1a64_hex, PlanStore, SchemaDiffPlanError, StoredPlan};
use crate::job::recovery::decide_recovery;
use crate::types::{DeployStatus, SchemaDiffDeployResult};

/// handler 角色：prepare 与 apply 是两个 kind。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerRole {
    Prepare,
    Apply,
}

/// Schema Diff 的 JobHandler 实现。
///
/// 构造时携带一次受理的最小上下文：prepare 携带 [`PrepareRequest`]，
/// apply 携带 [`ApplyRequest`] + planId/selectionRevision。物理资源访问一律
/// 经 [`SchemaDiffJobBackend`]（§2.3：handler 不持有 live session）。
pub struct SchemaDiffHandler<B: SchemaDiffJobBackend> {
    role: HandlerRole,
    backend: Arc<B>,
    plans: Arc<PlanStore>,
    prepare_request: Option<PrepareRequest>,
    apply_request: Option<ApplyRequest>,
    consumed_plan_id: Option<String>,
    selection_revision: Option<u64>,
}

impl<B: SchemaDiffJobBackend> SchemaDiffHandler<B> {
    pub fn for_prepare(
        backend: Arc<B>,
        plans: Arc<PlanStore>,
        request: PrepareRequest,
    ) -> Self {
        Self {
            role: HandlerRole::Prepare,
            backend,
            plans,
            prepare_request: Some(request),
            apply_request: None,
            consumed_plan_id: None,
            selection_revision: None,
        }
    }

    pub fn for_apply(
        backend: Arc<B>,
        plans: Arc<PlanStore>,
        consumed_plan_id: String,
        selection_revision: u64,
        request: ApplyRequest,
    ) -> Self {
        Self {
            role: HandlerRole::Apply,
            backend,
            plans,
            prepare_request: None,
            apply_request: Some(request),
            consumed_plan_id: Some(consumed_plan_id),
            selection_revision: Some(selection_revision),
        }
    }

    fn now() -> Timestamp {
        Timestamp::new(
            chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string(),
        )
    }

    fn role_kind(&self) -> &'static str {
        match self.role {
            HandlerRole::Prepare => "schemaDiffPrepare",
            HandlerRole::Apply => "schemaDiffApply",
        }
    }

    fn check_versions(plan: &datazen_runtime::job::FrozenPlan) -> Result<(), JobError> {
        let expected = datazen_runtime::job::SUPPORTED_PLAN_MAJOR;
        for got in [plan.plan_version, plan.handler_version, plan.checkpoint_version] {
            if got != expected {
                return Err(JobError::VersionIncompatible {
                    got,
                    supported: expected,
                });
            }
        }
        Ok(())
    }

    fn failed_outcome(&self, stage: &str, e: &SchemaDiffPlanError) -> StageOutcome {
        tracing::warn!(stage, error = %e, "schema-diff job stage rejected");
        StageOutcome {
            stage_id: StageId::new(stage),
            terminal: StageTerminal::Failed,
            progress: JobProgress::default(),
            commit_boundaries: Vec::new(),
            execution_ids: Vec::new(),
            artifact_ids: Vec::new(),
            effect_outcome: EffectOutcome::NotStarted,
            error_code: Some(ExecutionErrorCode::HostRejected),
        }
    }

    /// §4.2 结果语义映射。
    fn deploy_outcome(
        &self,
        deploy: SchemaDiffDeployResult,
        fingerprint: &str,
    ) -> StageOutcome {
        let ok_boundaries: Vec<CommitBoundary> = deploy
            .statement_results
            .iter()
            .filter(|r| r.ok)
            .map(|r| CommitBoundary {
                stage_id: StageId::new("apply"),
                stable_target_fingerprint: fingerprint.to_string(),
                committed_at: Self::now(),
                operation_id: Some(format!("stmt-{}", r.index)),
                batch_id: None,
                payload_digest: Some(fnv1a64_hex(r.sql.as_bytes())),
                evidence: vec!["stmtOk".into()],
                verified_at: None,
            })
            .collect();
        let ok_count = ok_boundaries.len();
        let (terminal, effect, error_code) = match deploy.status {
            DeployStatus::Committed => (StageTerminal::Succeeded, EffectOutcome::Completed, None),
            DeployStatus::RolledBack => (
                StageTerminal::Failed,
                EffectOutcome::RolledBack,
                Some(ExecutionErrorCode::SqlError),
            ),
            DeployStatus::Mixed => (
                StageTerminal::Failed,
                EffectOutcome::PartiallyApplied,
                Some(ExecutionErrorCode::PipelineAborted),
            ),
            DeployStatus::Unknown => (
                StageTerminal::Unknown,
                EffectOutcome::Unknown,
                Some(ExecutionErrorCode::ResourceLost),
            ),
            DeployStatus::Failed => (
                StageTerminal::Failed,
                EffectOutcome::RolledBack,
                Some(ExecutionErrorCode::SqlError),
            ),
            DeployStatus::Cancelled => (
                StageTerminal::Cancelled,
                if ok_count > 0 {
                    EffectOutcome::PartiallyApplied
                } else {
                    EffectOutcome::RolledBack
                },
                Some(ExecutionErrorCode::Cancelled),
            ),
        };
        let progress = JobProgress {
            read: Counter::new(0),
            converted: Counter::new(deploy.statement_count as u64),
            attempted: Counter::new(deploy.executed_count as u64),
            committed: Counter::new(ok_count as u64),
            unknown: Counter::new(if deploy.status == DeployStatus::Unknown {
                1
            } else {
                0
            }),
        };
        StageOutcome {
            stage_id: StageId::new("apply"),
            terminal,
            progress,
            commit_boundaries: ok_boundaries,
            execution_ids: Vec::new(),
            artifact_ids: Vec::new(),
            effect_outcome: effect,
            error_code,
        }
    }

    async fn run_prepare(&self, cancel: &CancelToken) -> Result<StageOutcome, JobError> {
        let request = self
            .prepare_request
            .as_ref()
            .ok_or_else(|| JobError::PlanProjectionInvalid("prepare request missing".into()))?;
        let prepared = self
            .backend
            .prepare_plan(request, cancel)
            .await
            .map_err(|e| JobError::PlanProjectionInvalid(e.to_string()))?;
        let artifact_ids: Vec<ArtifactId> = prepared
            .meta
            .body_artifact_ids
            .iter()
            .map(|s| ArtifactId::new(s.clone()))
            .collect();
        let progress = JobProgress {
            read: Counter::new(prepared.plan.tables.len() as u64),
            converted: Counter::new(prepared.plan.statements.len() as u64),
            attempted: Counter::new(0),
            committed: Counter::new(0),
            unknown: Counter::new(0),
        };
        self.plans.insert(StoredPlan {
            meta: prepared.meta,
            plan: prepared.plan,
            body_digest: prepared.body_digest,
        });
        Ok(StageOutcome {
            stage_id: StageId::new("prepare"),
            terminal: StageTerminal::Succeeded,
            progress,
            commit_boundaries: Vec::new(),
            execution_ids: Vec::new(),
            artifact_ids,
            effect_outcome: EffectOutcome::Completed,
            error_code: None,
        })
    }

    async fn run_apply(&self, cancel: &CancelToken) -> Result<StageOutcome, JobError> {
        let request = self
            .apply_request
            .as_ref()
            .ok_or_else(|| JobError::PlanProjectionInvalid("apply request missing".into()))?;
        let plan_id = self
            .consumed_plan_id
            .as_ref()
            .ok_or_else(|| JobError::PlanProjectionInvalid("planId missing".into()))?;
        let selection_revision = self
            .selection_revision
            .ok_or_else(|| JobError::PlanProjectionInvalid("selectionRevision missing".into()))?;
        // 一次性消费 + 版本守卫 + 过期/选择版本校验（§2.1 / CM-41）。
        let stored = match self
            .plans
            .take_for_apply(plan_id, selection_revision, &Self::now())
        {
            Ok(s) => s,
            Err(e) => return Ok(self.failed_outcome("apply", &e)),
        };
        // 抑制重复授权：capability/版本/credential 快照复验（§2.2）。
        if let Err(e) = self.backend.verify_authorization(&stored.meta).await {
            return Ok(self.failed_outcome("apply", &e));
        }
        // 执行前重读目标结构比对 before fingerprint → PlanStale（§4.2）。
        let before = match self.backend.read_target_fingerprint(&stored.meta).await {
            Ok(fp) => fp,
            Err(e) => return Ok(self.failed_outcome("apply", &e)),
        };
        if before != stored.meta.schema_fingerprint {
            let stale = SchemaDiffPlanError::PlanStale(format!(
                "expected {}, got {}",
                stored.meta.schema_fingerprint, before
            ));
            return Ok(self.failed_outcome("apply", &stale));
        }
        if cancel.is_cancelled() {
            return Ok(StageOutcome {
                stage_id: StageId::new("apply"),
                terminal: StageTerminal::Cancelled,
                progress: JobProgress::default(),
                commit_boundaries: Vec::new(),
                execution_ids: Vec::new(),
                artifact_ids: Vec::new(),
                effect_outcome: EffectOutcome::RolledBack,
                error_code: Some(ExecutionErrorCode::Cancelled),
            });
        }
        let deploy = match self.backend.deploy(&stored.plan, request, cancel).await {
            Ok(d) => d,
            Err(e) => return Ok(self.failed_outcome("apply", &e)),
        };
        Ok(self.deploy_outcome(deploy, &stored.meta.schema_fingerprint))
    }
}

#[async_trait::async_trait]
impl<B: SchemaDiffJobBackend> JobHandler for SchemaDiffHandler<B> {
    fn kind(&self) -> &str {
        self.role_kind()
    }

    fn handler_version(&self) -> u64 {
        1
    }

    fn validate_plan(&self, plan: &datazen_runtime::job::FrozenPlan) -> Result<Vec<StageSpec>, JobError> {
        Self::check_versions(plan)?;
        match self.role {
            HandlerRole::Prepare => Ok(vec![StageSpec {
                stage_id: StageId::new("prepare"),
                kind: "prepare".into(),
                depends_on: vec![],
            }]),
            HandlerRole::Apply => {
                // planId 必填、selectionRevision 必填（§2.1）。
                let plan_id = plan.consumed_plan_id.as_deref().ok_or_else(|| {
                    JobError::PlanProjectionInvalid("consumedPlanId missing".into())
                })?;
                let selection_revision = plan.selection_revision.ok_or_else(|| {
                    JobError::PlanProjectionInvalid("selectionRevision missing".into())
                })?;
                // PlanStore 校验：存在、未过期、版本守卫、选择版本一致（不消费）。
                match self
                    .plans
                    .validate_for_apply(plan_id, selection_revision, &Self::now())
                {
                    Ok(()) => Ok(vec![StageSpec {
                        stage_id: StageId::new("apply"),
                        kind: "apply".into(),
                        depends_on: vec![],
                    }]),
                    Err(e) => Err(JobError::PlanProjectionInvalid(e.to_string())),
                }
            }
        }
    }

    async fn run_stage(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
    ) -> Result<StageOutcome, JobError> {
        match (self.role, spec.kind.as_str()) {
            (HandlerRole::Prepare, "prepare") => self.run_prepare(cancel).await,
            (HandlerRole::Apply, "apply") => self.run_apply(cancel).await,
            _ => Err(JobError::PlanProjectionInvalid(format!(
                "unexpected stage kind {} for role",
                spec.kind
            ))),
        }
    }

    fn verify_recovery(&self, checkpoint: &Checkpoint) -> RecoveryVerdict {
        decide_recovery(checkpoint)
    }
}


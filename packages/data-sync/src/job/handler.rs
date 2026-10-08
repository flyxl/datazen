//! DataSyncHandler：Data Sync 的 JobHandler 实现（P5 Wave-1）。
//!
//! 协议要点（§10.1.1 / §5、§7）：
//!
//! * `validate_plan` 只做静态检查：版本守卫（未知 major 拒绝）、kind 匹配、
//!   apply 的 consumedPlanId/selectionRevision 与受理体一致、选项合法性。
//! * `run_stage(prepare)` 做动态门闸（family/结构/PK/目标不重叠）→ 完整 PK
//!   keyset 读取与比较 → 产出不可变 ChangeSet Artifact → 释放快照资源。
//! * `run_stage(apply)` 重验结构/PK（指纹比对）→ 选中子集核验 → 权限 →
//!   默认冲突政策（UPDATE/DELETE 用 PK+旧值条件，影响行数不符即冲突；
//!   INSERT 重验证键不存在）→ 逐批 begin/verify/execute/commit/boundary。
//! * 无法证明批次事务支持（TransactionScope::NonAtomic）的目标拒绝执行。
//! * `verify_recovery` 只读裁决：unknown → 人工核验；planStale/sourceChanged →
//!   拒绝续写；否则按已确认 boundary 数续跑。

use std::sync::Arc;

use async_trait::async_trait;
use datazen_platform_api::dto::execution::{EffectOutcome, ExecutionErrorCode};
use datazen_platform_api::dto::job::{
    Checkpoint, CommitBoundary, JobDomainResult, JobProgress, JobResultCounter,
};
use datazen_platform_api::id::StageId;
use datazen_runtime::job::{
    CancelToken, FrozenPlan, JobError, JobHandler, RecoveryVerdict, StageOutcome, StageSpec,
    StageTerminal,
};

use crate::error::DataSyncError;
use crate::model::ChangeOperation;

use super::body::{ApplySpec, PrepareSpec};
use super::host::DataSyncHost;

mod apply;
mod prepare;

/// Data Sync JobHandler。每个 Job 由 host 构造一个独立实例并注入受理体
/// （payload 正文不回传给 runtime；runtime 只从 FrozenPlan 读版本投影）。
pub enum DataSyncHandler {
    Prepare {
        spec: PrepareSpec,
        host: Arc<dyn DataSyncHost>,
    },
    Apply {
        spec: ApplySpec,
        host: Arc<dyn DataSyncHost>,
    },
}

impl DataSyncHandler {
    pub fn for_prepare(spec: PrepareSpec, host: Arc<dyn DataSyncHost>) -> Self {
        Self::Prepare { spec, host }
    }

    pub fn for_apply(spec: ApplySpec, host: Arc<dyn DataSyncHost>) -> Self {
        Self::Apply { spec, host }
    }

    fn kind_str(&self) -> &'static str {
        match self {
            Self::Prepare { .. } => super::body::PREPARE_KIND,
            Self::Apply { .. } => super::body::APPLY_KIND,
        }
    }
}
#[async_trait]
impl JobHandler for DataSyncHandler {
    fn kind(&self) -> &str {
        self.kind_str()
    }

    fn handler_version(&self) -> u64 {
        super::body::HANDLER_VERSION
    }

    fn validate_plan(&self, plan: &FrozenPlan) -> Result<Vec<StageSpec>, JobError> {
        if plan.kind != self.kind_str() {
            return Err(JobError::PlanProjectionInvalid(format!(
                "kind mismatch: plan={} handler={}",
                plan.kind,
                self.kind_str()
            )));
        }
        for (_name, got) in [
            ("planVersion", plan.plan_version),
            ("handlerVersion", plan.handler_version),
            ("checkpointVersion", plan.checkpoint_version),
        ] {
            if got != super::body::HANDLER_VERSION {
                return Err(JobError::VersionIncompatible {
                    got,
                    supported: super::body::HANDLER_VERSION,
                });
            }
        }
        match self {
            Self::Prepare { spec, .. } => {
                if plan.is_apply {
                    return Err(JobError::PlanProjectionInvalid(
                        "prepare handler got an apply plan".into(),
                    ));
                }
                if !spec.options.validate().is_ok() {
                    return Err(JobError::PlanProjectionInvalid(
                        "invalid sync options".into(),
                    ));
                }
                if spec.source.same_database_as(&spec.target) {
                    return Err(JobError::EndpointOverlap(
                        "source and target are the same database".into(),
                    ));
                }
                Ok(vec![StageSpec {
                    stage_id: StageId::new("prepare"),
                    kind: "prepare".into(),
                    depends_on: vec![],
                }])
            }
            Self::Apply { spec, .. } => {
                if !plan.is_apply {
                    return Err(JobError::PlanProjectionInvalid(
                        "apply handler got a prepare plan".into(),
                    ));
                }
                if plan.consumed_plan_id.as_deref() != Some(spec.plan_id.as_str()) {
                    return Err(JobError::PlanProjectionInvalid(
                        "consumedPlanId mismatch with apply body".into(),
                    ));
                }
                if plan.selection_revision != Some(spec.selection_revision) {
                    return Err(JobError::PlanProjectionInvalid(
                        "selectionRevision mismatch with apply body".into(),
                    ));
                }
                if spec.options.validate().is_err() {
                    return Err(JobError::PlanProjectionInvalid(
                        "invalid sync options".into(),
                    ));
                }
                Ok(vec![StageSpec {
                    stage_id: StageId::new("apply"),
                    kind: "apply".into(),
                    depends_on: vec![],
                }])
            }
        }
    }

    async fn run_stage(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
    ) -> Result<StageOutcome, JobError> {
        match self {
            Self::Prepare { .. } if spec.kind == "prepare" => self.run_prepare(cancel).await,
            Self::Apply { .. } if spec.kind == "apply" => self.run_apply(cancel).await,
            _ => Err(JobError::PlanProjectionInvalid(format!(
                "stage kind {} not served by this handler",
                spec.kind
            ))),
        }
    }

    fn verify_recovery(&self, checkpoint: &Checkpoint) -> RecoveryVerdict {
        let evidence = &checkpoint.verification_evidence;
        if evidence.iter().any(|e| e.contains("outcomeUnknown")) {
            return RecoveryVerdict::RequireManualReview {
                reason: "unknownCommitBoundary".into(),
            };
        }
        if evidence
            .iter()
            .any(|e| e.contains("planStale") || e.contains("sourceChanged"))
        {
            return RecoveryVerdict::Reject {
                reason: "plan/source changed; requires re-review, no auto-resume".into(),
            };
        }
        if evidence.iter().any(|e| e.contains("cleanupNotConfirmed")) {
            return RecoveryVerdict::RequireManualReview {
                reason: "cleanupNotConfirmed".into(),
            };
        }
        RecoveryVerdict::ResumeAfterVerify {
            resume_through: checkpoint.committed.len(),
        }
    }

    /// Persist only a bounded result projection. Row values, filters, generated
    /// SQL and driver messages remain in the process-local plan/handler state.
    fn durable_result(&self, outcome: &StageOutcome) -> Option<JobDomainResult> {
        let (result_code, outcome_code) = match self {
            Self::Prepare { .. } => (
                "comparisonPrepared",
                match outcome.terminal {
                    StageTerminal::Succeeded => "completed",
                    StageTerminal::Cancelled => "cancelled",
                    StageTerminal::Failed => "failed",
                    StageTerminal::Unknown => "unknown",
                },
            ),
            Self::Apply { .. } => (
                "changeSetApplied",
                match outcome.terminal {
                    StageTerminal::Succeeded => "completed",
                    StageTerminal::Cancelled => "cancelled",
                    StageTerminal::Failed => "failed",
                    StageTerminal::Unknown => "unknown",
                },
            ),
        };
        Some(JobDomainResult {
            stage_id: outcome.stage_id.clone(),
            result_code: result_code.into(),
            outcome_code: outcome_code.into(),
            counters: vec![
                JobResultCounter {
                    code: "read".into(),
                    value: outcome.progress.read,
                },
                JobResultCounter {
                    code: "converted".into(),
                    value: outcome.progress.converted,
                },
                JobResultCounter {
                    code: "attempted".into(),
                    value: outcome.progress.attempted,
                },
                JobResultCounter {
                    code: "committed".into(),
                    value: outcome.progress.committed,
                },
                JobResultCounter {
                    code: "unknown".into(),
                    value: outcome.progress.unknown,
                },
            ],
            items: Vec::new(),
            artifact_ids: outcome.artifact_ids.clone(),
        })
    }

    // ----- internal -----
}
/// 失败/取消/未知的阶段结果，边界列表只保留已确认提交的部分。
///
/// `host` 不是可选参数：runtime 不保留错误文本，所以每一次阶段失败都必须
/// 经由 host 端口写下原因，否则上层只能回一句「未知失败」。
fn stage_failed(
    host: &dyn DataSyncHost,
    stage: &'static str,
    effect: EffectOutcome,
    code: ExecutionErrorCode,
    err: DataSyncError,
    boundaries: Vec<CommitBoundary>,
    progress: JobProgress,
) -> StageOutcome {
    tracing::warn!(stage, error = %err, "data-sync stage failed");
    host.record_stage_failure(stage, err.to_string());
    StageOutcome {
        stage_id: StageId::new(stage),
        terminal: StageTerminal::Failed,
        progress,
        commit_boundaries: boundaries,
        execution_ids: Vec::new(),
        artifact_ids: Vec::new(),
        effect_outcome: effect,
        error_code: Some(code),
    }
}

fn op_name(op: ChangeOperation) -> &'static str {
    match op {
        ChangeOperation::Insert => "insert",
        ChangeOperation::Update => "update",
        ChangeOperation::Delete => "delete",
        ChangeOperation::Unchanged => "unchanged",
    }
}

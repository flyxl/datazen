//! Restore is an apply Job, with an immutable, authorized Artifact input.
//! Drivers must implement a real Gateway binding; no legacy-handle fallback exists.
//!
//! D6 item 5 切片 b 自 modules 归位（行为用例留 modules 集成测试
//! ——持有 runtime/kernel 引擎 dev-dep，垫片保留先例）。
use crate::execution::domain::target::ExecutionTarget;
use crate::execution::domain::{
    ArtifactLifecycle, ArtifactRepository, EffectOutcome, ExecutionErrorCode,
};
use crate::jobs::domain::job::{Checkpoint, CommitBoundary, JobDomainResult, JobProgress};
use crate::jobs::{
    CancelToken, FrozenPlan, JobError, JobHandler, RecoveryVerdict, StageOutcome, StageSpec,
    StageTerminal,
};
use crate::shared::context::RequestContext;
use crate::shared::id::{ArtifactId, Counter, ExecutionId, StageId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

pub const RESTORE_APPLY_KIND: &str = "databaseRestoreApply";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestorePlan {
    pub plan_id: String,
    pub target: ExecutionTarget,
    pub config_revision: Counter,
    pub artifact_id: ArtifactId,
    pub input_sha256: String,
    pub input_bytes: Counter,
    pub single_transaction: bool,
    pub overwrite: bool,
}
impl RestorePlan {
    /// Includes connection and every submitted namespace/object field.
    pub fn target_fingerprint(target: &ExecutionTarget) -> String {
        // ExecutionTarget contains only infallible serde data types.
        let encoded = serde_json::json!(target).to_string();
        format!("restore:v1:{:x}", Sha256::digest(encoded.as_bytes()))
    }
    pub fn payload(&self) -> serde_json::Value {
        serde_json::json!({"consumedPlanId":self.plan_id,"planVersion":1,"handlerVersion":1,"checkpointVersion":1,"selectionRevision":self.config_revision.get(),"restorePlan":self})
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreRejection {
    Unsupported,
    NotAuthorized,
    InvalidInput,
    RuntimeUnavailable,
    AdmissionRejected,
    TargetMismatch,
}
#[async_trait::async_trait]
pub trait RestoreInput: Send + Sync {
    /// Every read rechecks Artifact ACL/TTL. No filesystem path crosses this port.
    async fn chunk(&self, index: Counter) -> Result<Vec<u8>, RestoreRejection>;
    fn chunks(&self) -> u64;
}
/// Trusted Gateway completion evidence; never infer rollback from a cancel request.
pub struct RestoreEvidence {
    /// Only for rejection before submission; never use after any external write.
    pub rejection: Option<RestoreRejection>,
    pub terminal: StageTerminal,
    pub effects: EffectOutcome,
    pub progress: JobProgress,
    pub executions: Vec<ExecutionId>,
    pub boundaries: Vec<CommitBoundary>,
}
#[async_trait::async_trait]
pub trait RestoreGateway: Send + Sync {
    /// Reauthorize target/revision/capability before acquiring any resource.
    async fn validate(
        &self,
        context: &RequestContext,
        plan: &RestorePlan,
    ) -> Result<(), RestoreRejection>;
    /// Must use a counted, explicit Gateway target. Errors after submission must
    /// return Unknown evidence rather than a pre-dispatch rejection.
    async fn execute(
        &self,
        context: &RequestContext,
        plan: &RestorePlan,
        input: Arc<dyn RestoreInput>,
        cancel: &CancelToken,
    ) -> RestoreEvidence;
}
struct ArtifactInput {
    store: Arc<dyn ArtifactRepository>,
    context: RequestContext,
    artifact: ArtifactId,
    chunks: u64,
}
#[async_trait::async_trait]
impl RestoreInput for ArtifactInput {
    async fn chunk(&self, index: Counter) -> Result<Vec<u8>, RestoreRejection> {
        if index.get() >= self.chunks {
            return Err(RestoreRejection::InvalidInput);
        }
        self.store
            .read_chunk(&self.context, self.artifact.clone(), index)
            .await
            .map(|chunk| chunk.bytes)
            .map_err(|_| RestoreRejection::NotAuthorized)
    }
    fn chunks(&self) -> u64 {
        self.chunks
    }
}
pub struct RestoreApplyJob {
    context: RequestContext,
    plan: RestorePlan,
    artifacts: Arc<dyn ArtifactRepository>,
    gateway: Arc<dyn RestoreGateway>,
    rejection: Mutex<Option<RestoreRejection>>,
}
impl RestoreApplyJob {
    pub fn new(
        context: RequestContext,
        plan: RestorePlan,
        artifacts: Arc<dyn ArtifactRepository>,
        gateway: Arc<dyn RestoreGateway>,
    ) -> Self {
        Self {
            context,
            plan,
            artifacts,
            gateway,
            rejection: Mutex::new(None),
        }
    }
    async fn input(&self, cancel: &CancelToken) -> Result<Arc<dyn RestoreInput>, RestoreRejection> {
        let metadata = self
            .artifacts
            .describe(&self.context, self.plan.artifact_id.clone())
            .await
            .map_err(|_| RestoreRejection::NotAuthorized)?;
        if metadata.target_fingerprint != RestorePlan::target_fingerprint(&self.plan.target)
            || metadata.lifecycle != ArtifactLifecycle::Complete
            || metadata.published_byte_size != self.plan.input_bytes
        {
            tracing::warn!(
                fingerprint_matches = metadata.target_fingerprint
                    == RestorePlan::target_fingerprint(&self.plan.target),
                lifecycle = ?metadata.lifecycle,
                stored_bytes = ?metadata.published_byte_size,
                plan_bytes = ?self.plan.input_bytes,
                "restore input rejected: artifact metadata disagrees with the plan"
            );
            return Err(RestoreRejection::InvalidInput);
        }
        let input = Arc::new(ArtifactInput {
            store: self.artifacts.clone(),
            context: self.context.clone(),
            artifact: self.plan.artifact_id.clone(),
            chunks: metadata.published_chunk_count.get(),
        });
        let mut hash = Sha256::new();
        for index in 0..input.chunks() {
            if cancel.is_cancelled() {
                return Err(RestoreRejection::InvalidInput);
            }
            hash.update(input.chunk(Counter::new(index)).await?);
        }
        let digest = format!("{:x}", hash.finalize());
        if digest != self.plan.input_sha256 {
            tracing::warn!(
                stored_digest = %digest,
                plan_digest = %self.plan.input_sha256,
                chunks = input.chunks(),
                "restore input rejected: artifact digest disagrees with the plan"
            );
            return Err(RestoreRejection::InvalidInput);
        }
        Ok(input)
    }
    fn rejected(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
        reason: RestoreRejection,
    ) -> StageOutcome {
        if let Ok(mut rejection) = self.rejection.lock() {
            *rejection = Some(reason);
        }
        StageOutcome {
            stage_id: spec.stage_id.clone(),
            terminal: if cancel.is_cancelled() {
                StageTerminal::Cancelled
            } else {
                StageTerminal::Failed
            },
            progress: Default::default(),
            commit_boundaries: vec![],
            execution_ids: vec![],
            artifact_ids: vec![],
            effect_outcome: EffectOutcome::NotStarted,
            error_code: Some(if cancel.is_cancelled() {
                ExecutionErrorCode::Cancelled
            } else {
                ExecutionErrorCode::HostRejected
            }),
        }
    }
}
#[async_trait::async_trait]
impl JobHandler for RestoreApplyJob {
    fn kind(&self) -> &str {
        RESTORE_APPLY_KIND
    }
    fn handler_version(&self) -> u64 {
        1
    }
    fn validate_plan(&self, plan: &FrozenPlan) -> Result<Vec<StageSpec>, JobError> {
        if !plan.is_apply
            || plan.kind != RESTORE_APPLY_KIND
            || plan.consumed_plan_id.as_deref() != Some(self.plan.plan_id.as_str())
            || plan.selection_revision != Some(self.plan.config_revision.get())
        {
            return Err(JobError::PlanProjectionInvalid(
                "restore plan binding mismatch".into(),
            ));
        }
        if self.plan.target.connection_id.as_str().trim().is_empty()
            || self.plan.input_sha256.len() != 64
            || self.plan.input_bytes.get() == 0
        {
            return Err(JobError::PlanProjectionInvalid(
                "restore target/input is incomplete".into(),
            ));
        }
        Ok(vec![StageSpec {
            stage_id: StageId::new("restoreArtifact"),
            kind: RESTORE_APPLY_KIND.into(),
            depends_on: vec![],
        }])
    }
    async fn run_stage(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
    ) -> Result<StageOutcome, JobError> {
        if cancel.is_cancelled() {
            return Ok(self.rejected(spec, cancel, RestoreRejection::InvalidInput));
        }
        let input = match self.input(cancel).await {
            Ok(input) => input,
            Err(reason) => return Ok(self.rejected(spec, cancel, reason)),
        };
        if let Err(reason) = self.gateway.validate(&self.context, &self.plan).await {
            return Ok(self.rejected(spec, cancel, reason));
        }
        if cancel.is_cancelled() {
            return Ok(self.rejected(spec, cancel, RestoreRejection::InvalidInput));
        }
        let evidence = self
            .gateway
            .execute(&self.context, &self.plan, input, cancel)
            .await;
        if evidence.effects == EffectOutcome::NotStarted {
            if let Ok(mut rejection) = self.rejection.lock() {
                *rejection = evidence.rejection;
            }
        }
        Ok(StageOutcome {
            stage_id: spec.stage_id.clone(),
            terminal: evidence.terminal,
            progress: evidence.progress,
            commit_boundaries: evidence.boundaries,
            execution_ids: evidence.executions,
            artifact_ids: vec![self.plan.artifact_id.clone()],
            effect_outcome: evidence.effects,
            error_code: match evidence.terminal {
                StageTerminal::Succeeded => None,
                StageTerminal::Cancelled => Some(ExecutionErrorCode::Cancelled),
                StageTerminal::Unknown => Some(ExecutionErrorCode::ResourceLost),
                StageTerminal::Failed if evidence.effects == EffectOutcome::NotStarted => {
                    Some(ExecutionErrorCode::HostRejected)
                }
                StageTerminal::Failed => Some(ExecutionErrorCode::SqlError),
            },
        })
    }
    fn durable_result(&self, outcome: &StageOutcome) -> Option<JobDomainResult> {
        let code = if outcome.effect_outcome == EffectOutcome::NotStarted
            && outcome.terminal == StageTerminal::Cancelled
        {
            "restoreCancelledBeforeSubmission"
        } else {
            match self.rejection.lock().ok().and_then(|reason| *reason) {
                Some(RestoreRejection::Unsupported) => "restoreUnsupported",
                Some(RestoreRejection::NotAuthorized) => "restoreNotAuthorized",
                Some(RestoreRejection::InvalidInput) => "restoreInvalidInput",
                Some(RestoreRejection::RuntimeUnavailable) => "restoreRuntimeUnavailable",
                Some(RestoreRejection::AdmissionRejected) => "restoreAdmissionRejected",
                Some(RestoreRejection::TargetMismatch) => "restoreTargetMismatch",
                None if outcome.effect_outcome == EffectOutcome::NotStarted => {
                    "restoreNotSubmitted"
                }
                None => "restoreExecuted",
            }
        };
        Some(JobDomainResult {
            stage_id: outcome.stage_id.clone(),
            result_code: code.into(),
            outcome_code: match outcome.effect_outcome {
                EffectOutcome::NotStarted => "notStarted",
                EffectOutcome::Completed => "completed",
                EffectOutcome::RolledBack => "rolledBack",
                EffectOutcome::PartiallyApplied => "partiallyApplied",
                EffectOutcome::Unknown => "unknown",
            }
            .into(),
            counters: vec![],
            items: vec![],
            artifact_ids: outcome.artifact_ids.clone(),
        })
    }
    fn verify_recovery(&self, _: &Checkpoint) -> RecoveryVerdict {
        RecoveryVerdict::RequireManualReview {
            reason: "restore effects require driver verification; never automatically replay"
                .into(),
        }
    }
}

/// Hosts without a verified ResourceProvider restore binding must inject this
/// explicit refusal, never adapt a legacy ConnectionHandle into Gateway evidence.
pub struct UnsupportedRestoreGateway;
#[async_trait::async_trait]
impl RestoreGateway for UnsupportedRestoreGateway {
    async fn validate(&self, _: &RequestContext, _: &RestorePlan) -> Result<(), RestoreRejection> {
        Err(RestoreRejection::Unsupported)
    }
    async fn execute(
        &self,
        _: &RequestContext,
        _: &RestorePlan,
        _: Arc<dyn RestoreInput>,
        _: &CancelToken,
    ) -> RestoreEvidence {
        RestoreEvidence {
            rejection: Some(RestoreRejection::Unsupported),
            terminal: StageTerminal::Failed,
            effects: EffectOutcome::NotStarted,
            progress: Default::default(),
            executions: vec![],
            boundaries: vec![],
        }
    }
}

//! Ephemeral handler binding: hosts freeze selection server-side before accepting
//! a nativeToolProbe Job. No executable/argv/environment enter the durable plan.
use super::*;
use crate::job::{
    FrozenPlan, JobError, JobHandler, RecoveryVerdict, StageOutcome, StageSpec, StageTerminal,
};
use datazen_platform_api::{
    dto::job::{JobDomainResult, JobProgress, JobResultCounter},
    id::{Counter, JobId, StageId},
};
// Checkpoint lives in platform-api; the Job module does not re-export it.
use datazen_platform_api::dto::job::Checkpoint as NativeCheckpoint;

pub struct NativeToolJobHandler {
    job: JobId,
    selection: NativeToolSelection,
    runtime: Arc<NativeToolRuntime>,
    last: Mutex<Option<NativeToolResult>>,
}
impl NativeToolJobHandler {
    /// Safe durable projection; the owner permit checks it against this handler
    /// again before admission. The executable and environment stay provider-only.
    pub fn plan_payload(selection: &NativeToolSelection) -> serde_json::Value {
        serde_json::json!({"planVersion":1,"handlerVersion":1,"checkpointVersion":1,"nativeToolSelection":selection})
    }

    pub fn new(
        job: JobId,
        selection: NativeToolSelection,
        runtime: Arc<NativeToolRuntime>,
    ) -> Self {
        Self {
            job,
            selection,
            runtime,
            last: Mutex::new(None),
        }
    }
}
#[async_trait::async_trait]
impl JobHandler for NativeToolJobHandler {
    fn kind(&self) -> &str {
        "nativeToolProbe"
    }
    fn handler_version(&self) -> u64 {
        1
    }
    fn validate_plan(&self, plan: &FrozenPlan) -> Result<Vec<StageSpec>, JobError> {
        if plan.kind != self.kind() || plan.is_apply || plan.handler_version != 1 {
            return Err(JobError::CapabilityUnsupported("native plan".into()));
        }
        // Capability resolution is repeated at physical dispatch.
        let tool = self
            .runtime
            .port
            .resolve(&self.selection)
            .map_err(|_| JobError::CapabilityUnsupported("native tool unavailable".into()))?;
        if tool.max_processes != Some(1) || tool.max_database_sockets.is_none() {
            return Err(JobError::CapabilityUnsupported(
                "unproven native resource bound".into(),
            ));
        }
        Ok(vec![StageSpec {
            stage_id: StageId::new("native-probe"),
            kind: "nativeProbe".into(),
            depends_on: vec![],
        }])
    }
    async fn run_stage(&self, _: &StageSpec, _: &CancelToken) -> Result<StageOutcome, JobError> {
        Err(JobError::CapabilityUnsupported(
            "runtime-issued owner required".into(),
        ))
    }
    async fn run_stage_authorized(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
        owner: StageOwnerPermit,
    ) -> Result<StageOutcome, JobError> {
        if owner.job_id() != &self.job || owner.stage_id() != &spec.stage_id {
            return Err(JobError::StaleClaim);
        }
        let result = self
            .runtime
            .execute(owner, self.selection.clone(), cancel.clone())
            .await
            .map_err(|_| JobError::StaleClaim)?;
        let terminal = if result.effect == EffectOutcome::Unknown {
            StageTerminal::Unknown
        } else {
            match result.report.as_ref().map(|r| r.termination) {
                Some(NativeTermination::Exited) if result.effect == EffectOutcome::Completed => {
                    StageTerminal::Succeeded
                }
                Some(NativeTermination::Cancelled) => StageTerminal::Cancelled,
                _ => StageTerminal::Failed,
            }
        };
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = Some(result.clone());
        Ok(StageOutcome {
            stage_id: spec.stage_id.clone(),
            terminal,
            progress: JobProgress::default(),
            commit_boundaries: vec![],
            execution_ids: vec![],
            artifact_ids: vec![],
            effect_outcome: result.effect,
            error_code: None,
        })
    }
    fn durable_result(&self, outcome: &StageOutcome) -> Option<JobDomainResult> {
        let last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        let report = last.as_ref().and_then(|value| value.report.as_ref());
        Some(JobDomainResult {
            stage_id: outcome.stage_id.clone(),
            result_code: if report.is_some_and(|r| r.cleanup == NativeCleanup::Confirmed) {
                "nativeCleanupConfirmed"
            } else {
                "nativeNeedsVerification"
            }
            .into(),
            outcome_code: if outcome.effect_outcome == EffectOutcome::Completed {
                "completed"
            } else if outcome.effect_outcome == EffectOutcome::Unknown {
                "unknown"
            } else {
                "notStarted"
            }
            .into(),
            counters: report
                .map(|r| {
                    vec![
                        JobResultCounter {
                            code: "stdoutBytes".into(),
                            value: Counter::new(r.stdout_bytes),
                        },
                        JobResultCounter {
                            code: "stderrBytes".into(),
                            value: Counter::new(r.stderr_bytes),
                        },
                    ]
                })
                .unwrap_or_default(),
            items: vec![],
            artifact_ids: vec![],
        })
    }
    fn verify_recovery(&self, _: &NativeCheckpoint) -> RecoveryVerdict {
        RecoveryVerdict::RequireManualReview {
            reason: "native execution is never replayed after uncertain effects or restart".into(),
        }
    }
}

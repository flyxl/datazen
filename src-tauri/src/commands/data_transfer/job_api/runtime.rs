//! Transfer adapter for the shared durable desktop Job host.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{
    CommitBoundary, JobDefinition, JobDetails, JobProgress, JobRecoveryResult, JobRecoveryVerdict,
    JobState,
};
use datazen_platform_api::id::{
    ArtifactId, ClientInstanceId, IdempotencyKey, JobId, OrganizationId, PrincipalId, RequestId,
    Timestamp, WorkerId,
};
use datazen_platform_api::{PortError, RequestContext};
use datazen_runtime::job::{
    DesktopJobHost, EndpointRef, EndpointRole, JobHandler, RecoveryVerdict,
};

use super::super::plans;
use super::endpoint_identity::EndpointIdentity;
use crate::commands::AppState;
use crate::commands::error::CommandError;
use crate::data_transfer::job::{DataTransferHandler, derive_evidence};

pub(crate) const PREPARE_KIND: &str = "dataTransferPrepare";
pub(crate) const APPLY_KIND: &str = "dataTransferApply";
const LOCAL_CLIENT_INSTANCE: &str = "datazen-local-client";
pub(crate) const RECOVERY_RESUME_AFTER_VERIFY: &str = "resumeAfterVerify";
pub(crate) const RECOVERY_REJECT: &str = "reject";
pub(crate) const RECOVERY_REQUIRE_MANUAL_REVIEW: &str = "requireManualReview";
pub(crate) const RECOVERY_PENDING_VERIFICATION: &str = "pendingVerification";
pub(crate) const RECOVERY_NOT_EXECUTED: &str = "notExecuted";

pub(crate) type BoxedJobRun =
    Pin<Box<dyn Future<Output = Result<JobOutcome, CommandError>> + Send + 'static>>;

pub(crate) struct JobOutcome {
    pub(crate) job_id: String,
    pub(crate) kind: String,
    pub(crate) state: JobState,
    pub(crate) effect_outcome: EffectOutcome,
    pub(crate) progress: JobProgress,
    pub(crate) commit_boundaries: Vec<CommitBoundary>,
    pub(crate) artifact_ids: Vec<String>,
    pub(crate) error: Option<String>,
    pub(crate) recovery: RecoveryReport,
    pub(crate) replayed: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct RecoveryReport {
    pub(crate) verdict: String,
    pub(crate) resume_through: Option<usize>,
    /// Stable code only. Handler supplied free-form reason text is never persisted.
    pub(crate) reason_code: Option<String>,
}

pub(crate) struct JobRunRequest<'a> {
    pub(crate) kind: &'a str,
    pub(crate) payload: serde_json::Value,
    pub(crate) handler: Arc<DataTransferHandler>,
    pub(crate) endpoints: Vec<EndpointRef>,
    pub(crate) artifact_ids: Vec<String>,
    pub(crate) idempotency_key: &'a str,
    pub(crate) plan_id: Option<&'a str>,
}

pub(crate) struct AcceptedJob {
    pub(crate) job_id: JobId,
    pub(crate) kind: String,
    ctx: RequestContext,
}

pub(crate) enum Admission {
    Accepted(AcceptedJob),
    Replayed(JobOutcome),
}

pub(crate) struct DrivePlan {
    pub(crate) handler: Arc<DataTransferHandler>,
    pub(crate) endpoints: Vec<EndpointRef>,
    pub(crate) artifact_ids: Vec<String>,
    pub(crate) plan_id: Option<String>,
}

impl DrivePlan {
    pub(crate) fn from_request(request: &JobRunRequest<'_>) -> Self {
        Self {
            handler: request.handler.clone(),
            endpoints: request.endpoints.clone(),
            artifact_ids: request.artifact_ids.clone(),
            plan_id: request.plan_id.map(str::to_string),
        }
    }
}

pub(crate) fn now_timestamp() -> Timestamp {
    Timestamp::new(
        chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string(),
    )
}

/// Local desktop identity: no remote principal, token, or session handle is stored.
pub(super) fn request_context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("datazen-local"),
        PrincipalId::new("datazen-local-user"),
        None,
        ClientInstanceId::new(LOCAL_CLIENT_INSTANCE),
        RequestId::new(format!("transfer-{}", uuid::Uuid::new_v4())),
        None,
    )
}

pub(crate) fn endpoint_refs(
    source_objects: Vec<String>,
    target_objects: Vec<String>,
    source_identity: &EndpointIdentity,
    target_identity: Option<&EndpointIdentity>,
) -> Vec<EndpointRef> {
    let mut refs = vec![EndpointRef {
        connection_id: source_identity.connection_id.clone(),
        service_key: source_identity.service_key.clone(),
        objects: source_objects,
        role: EndpointRole::SourceReader,
    }];
    if let Some(target) = target_identity {
        refs.push(EndpointRef {
            connection_id: target.connection_id.clone(),
            service_key: target.service_key.clone(),
            objects: target_objects,
            role: EndpointRole::TargetWriter,
        });
    }
    refs
}

pub(crate) async fn receipt_for(
    state: &AppState,
    idempotency_key: &str,
) -> Result<Option<JobId>, CommandError> {
    let ctx = request_context();
    state
        .desktop_job_host
        .repository()
        .receipt_for(&ctx, &IdempotencyKey::new(idempotency_key.to_string()))
        .await
        .map_err(admit_error)
}

pub(crate) async fn replayed_outcome(
    state: &AppState,
    job_id: &JobId,
) -> Result<JobOutcome, CommandError> {
    let ctx = request_context();
    let details = state
        .desktop_job_host
        .details(&ctx, job_id.clone())
        .await
        .map_err(admit_error)?;
    Ok(outcome_from_details(details, true))
}

/// Accept one durable Job. A replay is detected by comparing the generated id
/// with the repository's atomic idempotency receipt result; only the winner may dispatch.
pub(crate) async fn admit(
    state: &AppState,
    request: &JobRunRequest<'_>,
) -> Result<Admission, CommandError> {
    let ctx = request_context();
    state
        .desktop_job_host
        .ensure_endpoint_services(&request.endpoints)
        .map_err(admit_error)?;
    let requested_job_id = JobId::new(format!(
        "transfer-{}-{}",
        request.kind,
        uuid::Uuid::new_v4()
    ));
    let definition = JobDefinition {
        job_id: requested_job_id.clone(),
        kind: request.kind.to_string(),
        owner: OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new(LOCAL_CLIENT_INSTANCE),
            purpose: request.kind.to_string(),
        },
        payload: request.payload.clone(),
        created_at: state.desktop_job_host.now(),
    };
    let record = state
        .desktop_job_host
        .accept(
            &ctx,
            definition,
            &IdempotencyKey::new(request.idempotency_key.to_string()),
        )
        .await
        .map_err(admit_error)?;
    if record.view.job_id != requested_job_id {
        let details = state
            .desktop_job_host
            .details(&ctx, record.view.job_id.clone())
            .await
            .map_err(admit_error)?;
        return Ok(Admission::Replayed(outcome_from_details(details, true)));
    }
    Ok(Admission::Accepted(AcceptedJob {
        job_id: record.view.job_id,
        kind: record.view.kind,
        ctx,
    }))
}

#[cfg(test)]
fn write_gates() -> &'static std::sync::Mutex<HashMap<String, Arc<WriteGate>>> {
    static GATES: std::sync::OnceLock<std::sync::Mutex<HashMap<String, Arc<WriteGate>>>> =
        std::sync::OnceLock::new();
    GATES.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

#[cfg(test)]
pub(crate) fn hold_write_closed(plan_id: &str) -> WriteGateGuard {
    let gate = Arc::new(WriteGate::default());
    write_gates()
        .lock()
        .expect("test write-gate lock is available")
        .insert(plan_id.to_string(), Arc::clone(&gate));
    WriteGateGuard {
        plan_id: plan_id.to_string(),
        gate,
    }
}

#[cfg(test)]
async fn park_on_write_gate(plan_id: Option<&str>) {
    let Some(plan_id) = plan_id else {
        return;
    };
    let Some(gate) = write_gates()
        .lock()
        .ok()
        .and_then(|gates| gates.get(plan_id).cloned())
    else {
        return;
    };
    gate.reached.notify_one();
    if gate.release.acquire().await.is_ok() {}
}

#[cfg(test)]
struct WriteGate {
    reached: tokio::sync::Notify,
    release: tokio::sync::Semaphore,
}

#[cfg(test)]
impl Default for WriteGate {
    fn default() -> Self {
        Self {
            reached: tokio::sync::Notify::new(),
            release: tokio::sync::Semaphore::new(0),
        }
    }
}

#[cfg(test)]
pub(crate) struct WriteGateGuard {
    plan_id: String,
    gate: Arc<WriteGate>,
}

#[cfg(test)]
impl WriteGateGuard {
    pub(crate) async fn wait_until_write_is_blocked(&self) {
        self.gate.reached.notified().await;
    }

    pub(crate) fn release(&self) {
        self.gate.release.add_permits(1);
    }
}

#[cfg(test)]
impl Drop for WriteGateGuard {
    fn drop(&mut self) {
        if let Ok(mut gates) = write_gates().lock() {
            gates.remove(&self.plan_id);
        }
        self.gate.release.add_permits(1);
    }
}

pub(crate) async fn drive(
    host: Arc<DesktopJobHost>,
    accepted: AcceptedJob,
    plan: DrivePlan,
) -> Result<JobOutcome, CommandError> {
    let AcceptedJob { job_id, kind, ctx } = accepted;
    #[cfg(test)]
    park_on_write_gate(plan.plan_id.as_deref()).await;

    let worker = WorkerId::new(format!("transfer-{kind}"));
    let dispatch = host
        .dispatch(
            &ctx,
            &job_id,
            &worker,
            &plan.endpoints,
            plan.handler.clone() as Arc<dyn JobHandler>,
        )
        .await;
    let repository = host.repository();
    match dispatch {
        Ok(result) => {
            if result.state == JobState::Succeeded {
                if let Some(plan_id) = plan.plan_id.as_deref() {
                    plans::mark_plan_consumed(plan_id).map_err(CommandError::from)?;
                }
            }
            let external_artifacts: Vec<ArtifactId> = plan
                .artifact_ids
                .iter()
                .cloned()
                .map(ArtifactId::new)
                .collect();
            if !external_artifacts.is_empty() {
                repository
                    .append_external_artifacts(&ctx, &job_id, &external_artifacts)
                    .await
                    .map_err(admit_error)?;
            }
            persist_recovery(&host, &ctx, &job_id, plan.handler.as_ref(), result.progress).await?;
        }
        Err(error) => {
            let current = host.get(&ctx, job_id.clone()).await.map_err(admit_error)?;
            match current.view.state {
                JobState::Queued => {
                    repository
                        .mark_failed_unstarted(&ctx, &job_id, dispatch_error_code(&error))
                        .await
                        .map_err(admit_error)?;
                }
                JobState::Running => {
                    repository
                        .mark_pending_verification(&ctx, &job_id, "dispatchNeedsVerification")
                        .await
                        .map_err(admit_error)?;
                    repository
                        .persist_recovery(
                            &ctx,
                            &job_id,
                            JobRecoveryResult {
                                verdict: JobRecoveryVerdict::PendingVerification,
                                resume_through: None,
                                reason_code: Some("dispatchNeedsVerification".into()),
                            },
                        )
                        .await
                        .map_err(admit_error)?;
                }
                JobState::Succeeded | JobState::Failed | JobState::Cancelled => {}
            }
        }
    }
    let details = host.details(&ctx, job_id).await.map_err(admit_error)?;
    Ok(outcome_from_details(details, false))
}

pub(crate) async fn run(
    state: &AppState,
    request: JobRunRequest<'_>,
) -> Result<JobOutcome, CommandError> {
    match admit(state, &request).await? {
        Admission::Replayed(outcome) => Ok(outcome),
        Admission::Accepted(accepted) => {
            drive(
                state.desktop_job_host.clone(),
                accepted,
                DrivePlan::from_request(&request),
            )
            .await
        }
    }
}

async fn persist_recovery(
    host: &DesktopJobHost,
    ctx: &RequestContext,
    job_id: &JobId,
    handler: &DataTransferHandler,
    progress: JobProgress,
) -> Result<(), CommandError> {
    let repository = host.repository();
    let mut boundaries = repository
        .committed_boundaries(ctx, job_id)
        .await
        .map_err(admit_error)?;
    let Some(mut checkpoint) = repository
        .latest_checkpoint(ctx, job_id)
        .await
        .map_err(admit_error)?
    else {
        let recovery = JobRecoveryResult {
            verdict: JobRecoveryVerdict::RequireManualReview,
            resume_through: None,
            reason_code: Some("missingCheckpoint".into()),
        };
        repository
            .persist_recovery(ctx, job_id, recovery)
            .await
            .map_err(admit_error)?;
        return Ok(());
    };
    boundaries.sort_by(|left, right| left.committed_at.as_str().cmp(right.committed_at.as_str()));
    checkpoint.verification_evidence = derive_evidence(
        handler.recovery_policy(),
        handler.snapshot_proven(),
        progress.unknown.get(),
        &boundaries,
    );
    let recovery = match handler.verify_recovery(&checkpoint) {
        RecoveryVerdict::ResumeAfterVerify { resume_through } => JobRecoveryResult {
            verdict: JobRecoveryVerdict::ResumeAfterVerify,
            resume_through: Some(u64::try_from(resume_through).unwrap_or(u64::MAX)),
            reason_code: None,
        },
        RecoveryVerdict::Reject { .. } => JobRecoveryResult {
            verdict: JobRecoveryVerdict::Reject,
            resume_through: None,
            reason_code: Some("recoveryRejected".into()),
        },
        RecoveryVerdict::RequireManualReview { .. } => JobRecoveryResult {
            verdict: JobRecoveryVerdict::RequireManualReview,
            resume_through: None,
            reason_code: Some("recoveryRequiresManualReview".into()),
        },
    };
    repository
        .persist_recovery(ctx, job_id, recovery)
        .await
        .map_err(admit_error)
}

fn outcome_from_details(details: JobDetails, replayed: bool) -> JobOutcome {
    let job = details.job;
    let recovery = details
        .recovery
        .map(recovery_report)
        .unwrap_or(RecoveryReport {
            verdict: String::new(),
            resume_through: None,
            reason_code: None,
        });
    JobOutcome {
        job_id: job.job_id.as_str().to_string(),
        kind: job.kind,
        state: job.state,
        effect_outcome: job.effect_outcome.unwrap_or(EffectOutcome::NotStarted),
        progress: job.progress,
        commit_boundaries: details.commit_boundaries,
        artifact_ids: job
            .artifact_ids
            .iter()
            .map(|artifact| artifact.as_str().to_string())
            .collect(),
        error: job.error,
        recovery,
        replayed,
    }
}

fn recovery_report(recovery: JobRecoveryResult) -> RecoveryReport {
    let verdict = match recovery.verdict {
        JobRecoveryVerdict::NotExecuted => RECOVERY_NOT_EXECUTED,
        JobRecoveryVerdict::PendingVerification => RECOVERY_PENDING_VERIFICATION,
        JobRecoveryVerdict::ResumeAfterVerify => RECOVERY_RESUME_AFTER_VERIFY,
        JobRecoveryVerdict::Reject => RECOVERY_REJECT,
        JobRecoveryVerdict::RequireManualReview => RECOVERY_REQUIRE_MANUAL_REVIEW,
    };
    RecoveryReport {
        verdict: verdict.to_string(),
        resume_through: recovery
            .resume_through
            .and_then(|index| usize::try_from(index).ok()),
        reason_code: recovery.reason_code,
    }
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

pub(super) fn admit_error(error: PortError) -> CommandError {
    let message = match error {
        PortError::PlanAlreadyConsumed(plan_id) => format!(
            "plan '{plan_id}' was already consumed by an earlier apply job; re-review the migration to mint a new plan"
        ),
        PortError::IdempotencyConflict => "this idempotency key was already used with a different payload; re-prepare the migration".to_string(),
        PortError::QuotaExceeded(reason) => {
            format!("data transfer job was denied by the local budget: {reason}")
        }
        PortError::UnsupportedVersion(reason) => {
            format!("data transfer job contract is not supported: {reason}")
        }
        PortError::StaleClaim => {
            "another worker holds this data transfer job claim; retry once it settles".to_string()
        }
        PortError::NotFound(reason) => format!("data transfer job was not found: {reason}"),
        PortError::CasConflict { entity, id } => {
            format!("data transfer job {entity} '{id}' changed concurrently; retry")
        }
        PortError::TokenInvalid => "data transfer job session is no longer valid".to_string(),
        PortError::BackendUnavailable(reason) => {
            format!("local data transfer job storage is unavailable: {reason}")
        }
        PortError::ProviderTimeout(reason) => {
            format!("data transfer provider timed out: {reason}")
        }
        other => format!("data transfer job failed: {other}"),
    };
    CommandError::Validation(message)
}

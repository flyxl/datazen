//! Job-based Data Transfer entry points.
//!
//! Two IPC commands replace the single "preview then execute" call with the
//! review contract a destructive cross-database migration needs:
//!
//! * `prepare_data_transfer_job` runs a terminal `dataTransferPrepare` Job: it
//!   inspects, plans, and freezes evidence, then releases every resource. Its
//!   output is a private plan plus the review payload — no connection handle, no
//!   snapshot, no worker claim and no budget are held for review.
//! * `apply_data_transfer_job` runs a **new** `dataTransferApply` Job whose input
//!   carries only `planId`, the plan digest, the selection revision, the reviewed
//!   selection and the destructive confirmation. The plan body is always re-read
//!   from the store: the client cannot smuggle a different plan body in.
//!
//! One `planId` is consumable by exactly one apply Job. A repeated idempotency
//! key replays the recorded Job instead of writing twice, and any refusal tells
//! the caller to prepare again rather than silently reusing a stale plan.
//!
//! Apply returns as soon as its Job is admitted, not when it finishes: the Job id
//! is the handle the UI polls and cancels with, so publishing it only after a
//! terminal run left a long migration both unobservable and structurally
//! uncancellable. `list_jobs` / `get_job` (in `queries`) are the read side of
//! that handle.
//!
//! Progress, stage artifacts, recovery verdicts and commit boundaries are written
//! through the shared durable runtime repository and remain queryable after reopen.

mod admission;
mod assembly;
mod cancel;
mod endpoint_identity;
mod queries;
mod runtime;
mod scope;

#[cfg(test)]
mod tests;

pub use queries::*;
pub use scope::TransferBackendScope;

pub(crate) use cancel::{cancel_data_transfer_job, job_cancel_requested};

use std::sync::Arc;

use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{CommitBoundary, JobProgress, JobState};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::State;

use super::plans::{self, PlanState, StoredTransferPlan};
use super::AppState;
use crate::commands::error::{CmdExt, CommandError};
use crate::data_transfer::job::{
    DataTransferHandler, TransferEndpoints, CHECKPOINT_VERSION, HANDLER_VERSION, PLAN_VERSION,
};
use crate::data_transfer::model::{TransferJob, TransferRunSelection};
use crate::data_transfer::TransferPreview;
use admission::{admit_apply_plan, ApplyPlanRequest, PlanAdmission, PlanAvailability};
use assembly::FreezeAssembly;
use runtime::JobRunRequest;

/// IPC input of `prepare_data_transfer_job`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferPrepareJobRequest {
    pub job: TransferJob,
    /// Both endpoints must declare the local desktop backend. There is no
    /// default, because "I forgot to declare a scope" is exactly the input the
    /// rule exists to reject.
    pub backend_scope: TransferBackendScope,
    /// Replaying the same key returns the recorded prepare Job.
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

/// IPC input of `apply_data_transfer_job`. Deliberately no plan body, no resume
/// token and no endpoint description: only what the user reviewed.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferApplyJobRequest {
    pub plan_id: String,
    pub plan_digest: String,
    pub selection_revision: u64,
    pub selection: TransferRunSelection,
    pub confirmed_destructive: bool,
    pub backend_scope: TransferBackendScope,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

/// What the review page needs to keep rendering while a plan ages.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferPrepareJobView {
    pub job_id: String,
    pub kind: String,
    pub state: JobState,
    pub effect_outcome: EffectOutcome,
    pub plan_id: String,
    pub plan_digest: String,
    pub plan_version: u64,
    pub handler_version: u64,
    pub checkpoint_version: u64,
    pub selection_revision: u64,
    pub expires_at: String,
    pub can_execute: bool,
    pub block_reason: Option<String>,
    pub review: TransferPreview,
}

/// Apply verdict. `commit_boundaries` are the durable write markers, so a caller
/// that lost its response can tell "unknown" from "never started" without
/// creating a second Job.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferApplyJobView {
    pub job_id: String,
    pub kind: String,
    pub state: JobState,
    pub effect_outcome: EffectOutcome,
    pub progress: JobProgress,
    pub plan_id: String,
    pub plan_digest: String,
    pub selection_revision: u64,
    pub commit_boundaries: Vec<CommitBoundary>,
    pub artifact_ids: Vec<String>,
    pub cancelled: bool,
    pub partial: bool,
    pub replayed: bool,
    pub error: Option<String>,
    /// Recovery verdict as decided by the handler over the recorded checkpoint:
    /// `resumeAfterVerify` | `reject` | `requireManualReview`.
    pub recovery_verdict: String,
    /// Position of the last confirmed boundary a resume may pass through.
    pub recovery_resume_through: Option<usize>,
    /// Why the verdict came out the way it did, when it was not a resume.
    pub recovery_reason: Option<String>,
}

/// Prepare: plan + freeze evidence, terminal, resources released afterwards.
#[tauri::command]
pub async fn prepare_data_transfer_job(
    state: State<'_, AppState>,
    request: TransferPrepareJobRequest,
) -> Result<TransferPrepareJobView, CommandError> {
    prepare_data_transfer_job_impl(&state, request).await
}

/// The prepare body, split from the Tauri wrapper so the admission contract can
/// be exercised on the real path instead of through a private helper.
pub(crate) async fn prepare_data_transfer_job_impl(
    state: &AppState,
    request: TransferPrepareJobRequest,
) -> Result<TransferPrepareJobView, CommandError> {
    scope::enforce_same_backend_scope(&request.job, Some(&request.backend_scope))?;
    let review = super::preview_data_transfer_impl(state, request.job)
        .await
        .cmd_err("prepare_data_transfer")?;
    // Re-read the plan from the store: the review is served from the authoritative
    // record, never from anything the caller kept on the client side.
    let plan = plans::peek_plan(&review.plan_id)?;
    let digest = plan_digest(&plan)?;
    let assembled = assembly::assemble(state, &plan, &TransferRunSelection::default(), false)
        .await
        .cmd_err("prepare_data_transfer")?;
    let handler = Arc::new(DataTransferHandler::prepare(
        assembled.freeze,
        assembled.inspected,
        assembled.source_schemas,
        assembled.endpoints,
    ));
    let key = request
        .idempotency_key
        .unwrap_or_else(|| fresh_key(&review.plan_id));
    let outcome = runtime::run(
        state,
        JobRunRequest {
            kind: runtime::PREPARE_KIND,
            payload: prepare_payload(&plan.id, &digest, plan.revision),
            handler,
            endpoints: runtime::endpoint_refs(
                assembled.source_objects,
                assembled.target_objects,
                &assembled.source_identity,
                assembled.target_identity.as_ref(),
            ),
            artifact_ids: Vec::new(),
            idempotency_key: &key,
            plan_id: None,
        },
    )
    .await
    .cmd_err("prepare_data_transfer")?;
    Ok(TransferPrepareJobView {
        job_id: outcome.job_id,
        kind: outcome.kind,
        state: outcome.state,
        effect_outcome: outcome.effect_outcome,
        plan_id: plan.id.clone(),
        plan_digest: digest,
        plan_version: PLAN_VERSION,
        handler_version: HANDLER_VERSION,
        checkpoint_version: CHECKPOINT_VERSION,
        selection_revision: plan.revision,
        expires_at: format_expiry(plan.expires_at_millis),
        can_execute: plan.can_execute,
        block_reason: review.block_reason.clone(),
        review,
    })
}

/// Apply: one planId, one Job, one idempotent write attempt.
///
/// The Job id comes back as soon as the Job is admitted — still `queued`, nothing
/// written — and the write runs on its own task. A cross-database migration can
/// take minutes; a caller that only received the id after that time had no handle
/// to poll, to watch progress with, or to cancel with. The Job's own state is the
/// observable outcome from here on, so a detached failure is logged rather than
/// returned: the caller has the id and can read what the Job recorded.
#[tauri::command]
pub async fn apply_data_transfer_job(
    state: State<'_, AppState>,
    request: TransferApplyJobRequest,
) -> Result<TransferApplyJobView, CommandError> {
    apply_detached(&state, request).await
}

/// The apply body, split from the Tauri wrapper so that detaching can itself be
/// exercised on the real path instead of only through the command wrapper.
pub(crate) async fn apply_detached(
    state: &AppState,
    request: TransferApplyJobRequest,
) -> Result<TransferApplyJobView, CommandError> {
    let admitted = admit_apply(state, request).await?;
    let Some(drive) = admitted.drive else {
        return Ok(admitted.view);
    };
    tauri::async_runtime::spawn(async move {
        let plan_id = drive.plan_id.clone();
        let job_id = drive.job_id.clone();
        if let Err(error) = drive.finish().await {
            tracing::warn!(
                cmd = "apply_data_transfer",
                plan_id = %plan_id,
                job_id = %job_id,
                error = %crate::log_redact::redact_secrets_for_log(&error.to_string()),
            );
        }
    });
    Ok(admitted.view)
}

/// An admitted apply Job: the view to return now, plus the write it still owes.
pub(crate) struct AdmittedApply {
    /// What the caller may return immediately. On the admitted path it describes
    /// a `queued` Job with no progress yet; on the replayed path it is the
    /// recorded outcome of the earlier run.
    pub(crate) view: TransferApplyJobView,
    /// `None` when nothing is left to run because this key already ran.
    pub(crate) drive: Option<ApplyDrive>,
}

/// The part of an apply that runs after its Job id is already published.
pub(crate) struct ApplyDrive {
    job_id: String,
    plan_id: String,
    plan_digest: String,
    selection_revision: u64,
    sql_file_destination: Option<std::path::PathBuf>,
    host: Arc<datazen_runtime::job::DesktopJobHost>,
    run: runtime::BoxedJobRun,
}

impl ApplyDrive {
    /// Run the write to a terminal state and report the finished verdict.
    pub(crate) async fn finish(self) -> Result<TransferApplyJobView, CommandError> {
        let outcome = (self.run).await.cmd_err("apply_data_transfer")?;
        // The Job view cannot surface stage artifacts, so the host hashes the
        // emitted SQL file itself and publishes the same content-addressed id.
        // The hash has to be computed by the host and not by the handler: the
        // host owns the content-addressing scheme these artifact ids are
        // expressed in, so an id minted anywhere else would not be comparable
        // with the ones prepare publishes.
        let host_artifacts = self
            .sql_file_destination
            .as_deref()
            .map(std::path::Path::new)
            .and_then(file_artifact_id)
            .into_iter()
            .collect::<Vec<String>>();
        if !host_artifacts.is_empty() {
            let artifacts = host_artifacts
                .iter()
                .cloned()
                .map(datazen_platform_api::id::ArtifactId::new)
                .collect::<Vec<_>>();
            self.host
                .repository()
                .append_external_artifacts(
                    &runtime::request_context(),
                    &datazen_platform_api::id::JobId::new(outcome.job_id.clone()),
                    &artifacts,
                )
                .await
                .map_err(runtime::admit_error)?;
        }
        Ok(apply_view(
            outcome,
            host_artifacts,
            self.plan_id,
            self.plan_digest,
            self.selection_revision,
        ))
    }
}

/// Admit one apply Job, stopping before the write attempt.
///
/// Order is load-bearing and is the whole of the recovery story. Everything up to
/// `assemble` is read-only and repeatable. The idempotency receipt is consulted
/// *next*, still before the plan is claimed: once a first attempt has consumed
/// the plan, any retry is refused as "already consumed", so a receipt check placed
/// after the claim could never recover the result of the attempt that consumed it
/// and the caller would be stuck holding a one-shot plan it can no longer read.
/// A receipt found here replays the recorded Job and claims nothing.
///
/// Only then does the claim happen, and *before* the write: the claim is the plan
/// registry's own atomic Available → Executing transition, so the legacy
/// `execute_data_transfer` path — which claims the very same record — is refused
/// from the moment this Job starts writing, including after a failed or unknown
/// outcome. Peeking alone would have left two managers free to execute one planId.
pub(crate) async fn admit_apply(
    state: &AppState,
    request: TransferApplyJobRequest,
) -> Result<AdmittedApply, CommandError> {
    // `peek_plan_any` also returns claimed/consumed plans, so the refusal can name
    // the real reason ("already consumed") instead of a misleading "unknown plan".
    let plan = plans::peek_plan_any(&request.plan_id)?;
    scope::enforce_same_backend_scope(&plan.job, Some(&request.backend_scope))?;
    let digest = plan_digest(&plan)?;
    let assembled: FreezeAssembly = assembly::assemble(state, &plan, &request.selection, true)
        .await
        .cmd_err("apply_data_transfer")?;
    let key = request
        .idempotency_key
        .unwrap_or_else(|| fresh_key(&request.plan_id));
    if let Some(job_id) = runtime::receipt_for(state, &key).await? {
        let outcome = runtime::replayed_outcome(state, &job_id)
            .await
            .cmd_err("apply_data_transfer")?;
        return Ok(AdmittedApply {
            view: apply_view(
                outcome,
                Vec::new(),
                request.plan_id.clone(),
                digest.clone(),
                plan.revision,
            ),
            drive: None,
        });
    }
    admit_apply_plan(
        &PlanAdmission {
            plan_id: plan.id.clone(),
            plan_digest: digest.clone(),
            selection_revision: plan.revision,
            expires_at_millis: plan.expires_at_millis,
            now_millis: plans::now_millis(),
            availability: availability_of(plan.state),
            can_execute: plan.can_execute,
            destructive: plan.job.write_mode.is_destructive(),
        },
        &ApplyPlanRequest {
            plan_id: request.plan_id.clone(),
            plan_digest: request.plan_digest.clone(),
            selection_revision: request.selection_revision,
            confirmed_destructive: request.confirmed_destructive,
        },
    )?;
    let sql_file_destination = sql_file_destination(&assembled.endpoints);
    let run_request = JobRunRequest {
        kind: runtime::APPLY_KIND,
        payload: apply_payload(
            &request.plan_id,
            &digest,
            plan.revision,
            &request.selection,
            request.confirmed_destructive,
        ),
        handler: Arc::new(DataTransferHandler::apply(
            assembled.freeze,
            assembled.inspected,
            assembled.source_schemas,
            assembled.target_schemas,
            assembled.endpoints,
            plan.database_structure.clone(),
        )),
        endpoints: runtime::endpoint_refs(
            assembled.source_objects,
            assembled.target_objects,
            &assembled.source_identity,
            assembled.target_identity.as_ref(),
        ),
        artifact_ids: Vec::new(),
        idempotency_key: &key,
        plan_id: Some(&request.plan_id),
    };
    let admission = runtime::admit(state, &run_request)
        .await
        .cmd_err("apply_data_transfer")?;
    match admission {
        runtime::Admission::Replayed(outcome) => Ok(AdmittedApply {
            view: apply_view(
                outcome,
                Vec::new(),
                request.plan_id.clone(),
                digest.clone(),
                plan.revision,
            ),
            drive: None,
        }),
        runtime::Admission::Accepted(accepted) => {
            if let Err(error) = plans::claim_plan(&request.plan_id) {
                state
                    .desktop_job_host
                    .repository()
                    .mark_failed_unstarted(
                        &runtime::request_context(),
                        &accepted.job_id,
                        "planClaimFailed",
                    )
                    .await
                    .map_err(runtime::admit_error)?;
                return Err(CommandError::from(error));
            }
            let view = queued_view(
                &accepted,
                request.plan_id.clone(),
                digest.clone(),
                plan.revision,
            );
            let drive_plan = runtime::DrivePlan::from_request(&run_request);
            let host = state.desktop_job_host.clone();
            let job_id = accepted.job_id.as_str().to_string();
            Ok(AdmittedApply {
                view,
                drive: Some(ApplyDrive {
                    job_id,
                    plan_id: request.plan_id.clone(),
                    plan_digest: digest,
                    selection_revision: plan.revision,
                    sql_file_destination,
                    host: host.clone(),
                    run: Box::pin(async move { runtime::drive(host, accepted, drive_plan).await }),
                }),
            })
        }
    }
}

/// Build an apply view from a finished run.
///
/// `extra_artifacts` is what the host computed outside the Job; it is appended
/// only when the Job did not already publish the same id.
fn apply_view(
    outcome: runtime::JobOutcome,
    extra_artifacts: Vec<String>,
    plan_id: String,
    plan_digest: String,
    selection_revision: u64,
) -> TransferApplyJobView {
    let mut artifact_ids = outcome.artifact_ids;
    for artifact in extra_artifacts {
        if !artifact_ids.contains(&artifact) {
            artifact_ids.push(artifact);
        }
    }
    TransferApplyJobView {
        job_id: outcome.job_id,
        kind: outcome.kind,
        state: outcome.state,
        effect_outcome: outcome.effect_outcome,
        progress: outcome.progress,
        plan_id,
        plan_digest,
        selection_revision,
        commit_boundaries: outcome.commit_boundaries,
        artifact_ids,
        cancelled: outcome.state == JobState::Cancelled,
        partial: matches!(
            outcome.effect_outcome,
            EffectOutcome::PartiallyApplied | EffectOutcome::Unknown
        ),
        replayed: outcome.replayed,
        error: outcome.error,
        recovery_verdict: outcome.recovery.verdict,
        recovery_resume_through: outcome.recovery.resume_through,
        recovery_reason: outcome.recovery.reason_code,
    }
}

/// The view of a Job that is admitted but not driven yet.
///
/// Nothing has been written, so every progress counter is zero and the effect
/// outcome is `notStarted`. `recovery_verdict` is empty rather than one of the
/// three real verdicts: the handler has judged nothing yet, and naming a verdict
/// here would report a decision that was never made.
pub(crate) fn queued_view(
    accepted: &runtime::AcceptedJob,
    plan_id: String,
    plan_digest: String,
    selection_revision: u64,
) -> TransferApplyJobView {
    TransferApplyJobView {
        job_id: accepted.job_id.as_str().to_string(),
        kind: accepted.kind.clone(),
        state: JobState::Queued,
        effect_outcome: EffectOutcome::NotStarted,
        progress: JobProgress::default(),
        plan_id,
        plan_digest,
        selection_revision,
        commit_boundaries: Vec::new(),
        artifact_ids: Vec::new(),
        cancelled: false,
        partial: false,
        replayed: false,
        error: None,
        recovery_verdict: String::new(),
        recovery_resume_through: None,
        recovery_reason: None,
    }
}

/// Prepare payload: contract versions only. No `consumedPlanId` — a prepare Job
/// consumes nothing, and `project_frozen_plan` refuses one that claims to.
pub(crate) fn prepare_payload(
    plan_id: &str,
    plan_digest: &str,
    selection_revision: u64,
) -> serde_json::Value {
    serde_json::json!({
        "kind": runtime::PREPARE_KIND,
        "planVersion": PLAN_VERSION,
        "handlerVersion": HANDLER_VERSION,
        "checkpointVersion": CHECKPOINT_VERSION,
        "planId": plan_id,
        "planDigest": plan_digest,
        "selectionRevision": selection_revision,
    })
}

/// Apply payload: the plan reference, never the plan body.
pub(crate) fn apply_payload(
    plan_id: &str,
    plan_digest: &str,
    selection_revision: u64,
    selection: &TransferRunSelection,
    confirmed_destructive: bool,
) -> serde_json::Value {
    serde_json::json!({
        "kind": runtime::APPLY_KIND,
        "planVersion": PLAN_VERSION,
        "handlerVersion": HANDLER_VERSION,
        "checkpointVersion": CHECKPOINT_VERSION,
        "consumedPlanId": plan_id,
        "planDigest": plan_digest,
        "selectionRevision": selection_revision,
        "sourceTables": selection.source_tables.clone().unwrap_or_default(),
        "confirmedDestructive": confirmed_destructive,
    })
}

/// Fingerprint of everything the user reviewed. The apply request echoes it, so
/// a plan mutated after review (re-issued target scope, new structure sequence)
/// is refused instead of executed.
pub(crate) fn plan_digest(plan: &StoredTransferPlan) -> Result<String, CommandError> {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "planId": plan.id,
        "job": plan.job,
        "canExecute": plan.can_execute,
        "sourceDriverType": plan.source_driver_type,
        "targetDriverType": plan.target_driver_type,
        "sourceDriverProtocol": plan.source_driver_protocol,
        "targetDriverProtocol": plan.target_driver_protocol,
        "sourceSchemaFingerprint": plan.source_schema_fingerprint,
        "targetSchemaFingerprint": plan.target_schema_fingerprint,
        "filter": plan.filter,
        "targetScopeFingerprint": plan.target_scope_fingerprint,
        "targetReadOnlyAtPreview": plan.target_read_only_at_preview,
        "sqlFileStructure": plan.sql_file_structure,
        "databaseStructure": plan.database_structure,
        "revision": plan.revision,
    }))
    .map_err(|error| {
        CommandError::Internal(format!("transfer plan cannot be fingerprinted: {error}"))
    })?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

/// Map stored plan state onto the admission vocabulary.
pub(crate) fn availability_of(state: PlanState) -> PlanAvailability {
    match state {
        PlanState::Available => PlanAvailability::Available,
        PlanState::Executing => PlanAvailability::Claimed,
        PlanState::Consumed => PlanAvailability::Consumed,
    }
}

/// Where a SQL-file run wrote, if anywhere: the only endpoint without a writer.
fn sql_file_destination(endpoints: &TransferEndpoints) -> Option<std::path::PathBuf> {
    match endpoints {
        TransferEndpoints::SqlFile { destination, .. } => Some(destination.clone()),
        TransferEndpoints::Database { .. } => None,
    }
}

/// Content-addressed id for an emitted SQL file, matching the id the handler
/// records on its own commit boundary.
fn file_artifact_id(path: &std::path::Path) -> Option<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(format!("transfer-sql-{:x}", hasher.finalize()))
}

/// Expiry is published as an ISO instant so the client never needs the host clock
/// to interpret it.
fn format_expiry(expires_at_millis: i64) -> String {
    chrono::DateTime::from_timestamp_millis(expires_at_millis)
        .map(|instant| instant.to_rfc3339())
        .unwrap_or_else(|| "1970-01-01T00:00:00+00:00".to_string())
}

/// Default idempotency key: unique per call, so a *deliberate* retry is a new Job
/// and only an explicitly reused key replays the recorded one.
fn fresh_key(plan_id: &str) -> String {
    format!("{plan_id}-{}", uuid::Uuid::new_v4())
}

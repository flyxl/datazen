//! Job-based Data Transfer entry points (P5 §2.1 / §6 / §7 / §8).
//!
//! Two IPC commands replace the single "preview then execute" call with the
//! review contract the platform docs require:
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

mod admission;
mod assembly;
mod runtime;
mod scope;

#[cfg(test)]
mod tests;

pub use scope::TransferBackendScope;

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
    /// §8: both endpoints must declare the local desktop backend. There is no
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
/// creating a second Job (§8).
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
}

/// Prepare: plan + freeze evidence, terminal, resources released afterwards.
#[tauri::command]
pub async fn prepare_data_transfer_job(
    state: State<'_, AppState>,
    request: TransferPrepareJobRequest,
) -> Result<TransferPrepareJobView, CommandError> {
    scope::enforce_same_backend_scope(&request.job, Some(&request.backend_scope))?;
    let review = super::preview_data_transfer_impl(&state, request.job)
        .await
        .cmd_err("prepare_data_transfer")?;
    // Re-read the plan from the store: the review is served from the authoritative
    // record, never from anything the caller kept on the client side.
    let plan = plans::peek_plan(&review.plan_id)?;
    let digest = plan_digest(&plan)?;
    let assembled = assembly::assemble(&state, &plan, &TransferRunSelection::default(), false)
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
    let outcome = runtime::run(JobRunRequest {
        kind: runtime::PREPARE_KIND,
        payload: prepare_payload(plan.revision),
        handler,
        endpoints: runtime::endpoint_refs(
            assembled.source_objects,
            assembled.target_objects,
            plan.job.sql_file_target.is_some(),
        ),
        artifact_ids: Vec::new(),
        idempotency_key: &key,
        plan_id: None,
    })
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
#[tauri::command]
pub async fn apply_data_transfer_job(
    state: State<'_, AppState>,
    request: TransferApplyJobRequest,
) -> Result<TransferApplyJobView, CommandError> {
    // `peek_plan_any` also returns claimed/consumed plans, so the refusal can name
    // the real reason ("already consumed") instead of a misleading "unknown plan".
    let plan = plans::peek_plan_any(&request.plan_id)?;
    scope::enforce_same_backend_scope(&plan.job, Some(&request.backend_scope))?;
    let digest = plan_digest(&plan)?;
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
    let assembled: FreezeAssembly = assembly::assemble(&state, &plan, &request.selection, true)
        .await
        .cmd_err("apply_data_transfer")?;
    let sql_file_destination = sql_file_destination(&assembled.endpoints);
    let handler = Arc::new(DataTransferHandler::apply(
        assembled.freeze,
        assembled.inspected,
        assembled.source_schemas,
        assembled.target_schemas,
        assembled.endpoints,
        plan.database_structure.clone(),
    ));
    let key = request
        .idempotency_key
        .unwrap_or_else(|| fresh_key(&request.plan_id));
    let outcome = runtime::run(JobRunRequest {
        kind: runtime::APPLY_KIND,
        payload: apply_payload(
            &request.plan_id,
            &digest,
            plan.revision,
            &request.selection,
            request.confirmed_destructive,
        ),
        handler,
        endpoints: runtime::endpoint_refs(
            assembled.source_objects,
            assembled.target_objects,
            plan.job.sql_file_target.is_some(),
        ),
        artifact_ids: Vec::new(),
        idempotency_key: &key,
        plan_id: Some(&request.plan_id),
    })
    .await
    .cmd_err("apply_data_transfer")?;
    // The Job view cannot surface stage artifacts, so the host hashes the emitted
    // SQL file itself and publishes the same content-addressed id (§ CM-49).
    let mut artifact_ids = outcome.artifact_ids;
    if let Some(path) = sql_file_destination.as_deref() {
        if let Some(artifact) = file_artifact_id(path) {
            if !artifact_ids.contains(&artifact) {
                artifact_ids.push(artifact);
            }
        }
    }
    Ok(TransferApplyJobView {
        job_id: outcome.job_id,
        kind: outcome.kind,
        state: outcome.state,
        effect_outcome: outcome.effect_outcome,
        progress: outcome.progress,
        plan_id: request.plan_id,
        plan_digest: digest,
        selection_revision: plan.revision,
        commit_boundaries: outcome.commit_boundaries,
        artifact_ids,
        cancelled: outcome.state == JobState::Cancelled,
        partial: matches!(
            outcome.effect_outcome,
            EffectOutcome::PartiallyApplied | EffectOutcome::Unknown
        ),
        replayed: outcome.replayed,
        error: outcome.error,
    })
}

/// Prepare payload: contract versions only. No `consumedPlanId` — a prepare Job
/// consumes nothing, and `project_frozen_plan` refuses one that claims to.
pub(crate) fn prepare_payload(selection_revision: u64) -> serde_json::Value {
    serde_json::json!({
        "kind": runtime::PREPARE_KIND,
        "planVersion": PLAN_VERSION,
        "handlerVersion": HANDLER_VERSION,
        "checkpointVersion": CHECKPOINT_VERSION,
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

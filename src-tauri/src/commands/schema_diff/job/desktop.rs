//! Durable desktop adapter for Schema Diff Job submission and readback.

use std::sync::Arc;

use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{
    JobDefinition, JobDetails, JobDomainResult, JobFilter, JobProgress, JobRecoveryRequest,
    JobRecoveryResult, JobRecoveryVerdict, JobRecoveryVerification, JobResultCounter,
    JobResultItem, JobState, JobView,
};
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{Counter, IdempotencyKey, JobId, StageId, WorkerId};
use datazen_runtime::job::{DesktopJobHost, EndpointRef, JobHandler, JobRecoveryVerifier};
use datazen_schema_diff::job::{ApplyRequest, PrepareRequest, SchemaDiffHandler};
use datazen_schema_diff::types::{DeployStatus, SchemaDiffDeployResult};
use serde::Serialize;
use tauri::State;

use super::{
    apply_payload, endpoints_from_session_pair, job_ctx, owner_connection_id, owner_ref,
    prepare_payload, recovery, AppStateBackend, SchemaDiffJobInfra, SchemaDiffPrepareEnvelope,
};
use crate::commands::error::{CmdExt, CommandError};
use crate::commands::AppState;

const PREPARE_KIND: &str = "schemaDiffPrepare";
const APPLY_KIND: &str = "schemaDiffApply";

/// Durable receipt returned before the background handler starts.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiffJobAccepted {
    pub job_id: String,
    pub kind: String,
    pub state: JobState,
    pub progress: JobProgress,
}

/// Domain readback combines the persistent host details with an optional
/// process-local plan body. SQL remains an in-memory review artifact only.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiffJobDetails {
    pub details: JobDetails,
    pub prepared: Option<SchemaDiffPrepareEnvelope>,
    pub plan_unavailable_after_restart: bool,
    pub deploy_result: Option<SchemaDiffDeployResult>,
}

#[tauri::command]
pub async fn get_schema_diff_job_details(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<SchemaDiffJobDetails, CommandError> {
    read_schema_diff_job_details(&state, &job_id)
        .await
        .cmd_err("get_schema_diff_job_details")
}

#[tauri::command]
pub async fn list_schema_diff_jobs(
    state: State<'_, AppState>,
    states: Option<Vec<JobState>>,
    limit: Option<u32>,
) -> Result<Vec<JobView>, CommandError> {
    read_schema_diff_jobs(&state, states, limit)
        .await
        .cmd_err("list_schema_diff_jobs")
}

#[tauri::command]
pub async fn verify_schema_diff_job_recovery(
    state: State<'_, AppState>,
    job_id: String,
    target_db_session_id: String,
) -> Result<SchemaDiffJobDetails, CommandError> {
    verify_schema_diff_recovery(&state, &job_id, &target_db_session_id)
        .await
        .cmd_err("verify_schema_diff_job_recovery")
}

async fn verify_schema_diff_recovery(
    state: &AppState,
    job_id: &str,
    target_db_session_id: &str,
) -> Result<SchemaDiffJobDetails, CommandError> {
    let context = job_ctx();
    let id = JobId::new(job_id.to_string());
    let details = state
        .desktop_job_host
        .details(&context, id.clone())
        .await
        .map_err(host_error)?;
    if details.job.kind != APPLY_KIND {
        return Err(CommandError::NotFound(format!(
            "schema diff apply job '{job_id}' was not found"
        )));
    }
    let connection_id = owner_connection_id(state, target_db_session_id).await?;
    if !details
        .recovery_targets
        .iter()
        .any(|target| target.connection_id == connection_id)
    {
        return Err(CommandError::Validation(
            "Reconnect the target connection used by this job before read-only verification".into(),
        ));
    }
    let verifier = Arc::new(SchemaDiffRecoveryVerifier {
        state: Arc::new(state.clone()),
        target_db_session_id: target_db_session_id.to_string(),
        connection_id,
    });
    state
        .desktop_job_host
        .verify_recovery(&context, id, verifier)
        .await
        .map_err(host_error)?;
    read_schema_diff_job_details(state, job_id).await
}

struct SchemaDiffRecoveryVerifier {
    state: Arc<AppState>,
    target_db_session_id: String,
    connection_id: String,
}

#[async_trait::async_trait]
impl JobRecoveryVerifier for SchemaDiffRecoveryVerifier {
    fn kind(&self) -> &str {
        APPLY_KIND
    }

    async fn verify(
        &self,
        _context: &RequestContext,
        request: &JobRecoveryRequest,
    ) -> Result<JobRecoveryVerification, PortError> {
        let targets = request
            .details
            .recovery_targets
            .iter()
            .find(|target| target.connection_id == self.connection_id);
        let Some(targets) = targets else {
            return Ok(manual_review("targetIdentityUnavailable", 0));
        };
        let decoded = targets
            .object_ids
            .iter()
            .map(|id| recovery::decode_target_id(id))
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
        let current = match recovery::fingerprint_targets(
            &self.state,
            &self.target_db_session_id,
            None,
            &decoded,
        )
        .await
        {
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
        domain_results: vec![JobDomainResult {
            stage_id: StageId::new("apply"),
            result_code: "schemaDiffRecovery".into(),
            outcome_code: outcome_code.into(),
            counters: vec![JobResultCounter {
                code: "targetsChecked".into(),
                value: Counter::new(checked.min(u64::MAX as usize) as u64),
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

pub async fn read_schema_diff_job_details(
    state: &AppState,
    job_id: &str,
) -> Result<SchemaDiffJobDetails, CommandError> {
    let context = job_ctx();
    let mut details = state
        .desktop_job_host
        .details(&context, JobId::new(job_id.to_string()))
        .await
        .map_err(host_error)?;
    if details.job.kind != PREPARE_KIND && details.job.kind != APPLY_KIND {
        return Err(CommandError::NotFound(format!(
            "schema diff job '{job_id}' was not found"
        )));
    }

    let plan_id = details
        .job
        .artifact_ids
        .iter()
        .find_map(|artifact| artifact.as_str().strip_prefix("artifact:plan:"));
    if details.plan_id.is_none() {
        details.plan_id = plan_id.map(str::to_owned);
    }
    let prepared = plan_id.and_then(|id| prepare_envelope(&state.schema_diff_jobs, id));
    let plan_unavailable_after_restart = details.job.kind == PREPARE_KIND
        && details.job.state == JobState::Succeeded
        && prepared.is_none();
    let deploy_result = (details.job.kind == APPLY_KIND && details.job.state.is_terminal())
        .then(|| deploy_result(&details));

    Ok(SchemaDiffJobDetails {
        details,
        prepared,
        plan_unavailable_after_restart,
        deploy_result,
    })
}

pub async fn read_schema_diff_jobs(
    state: &AppState,
    states: Option<Vec<JobState>>,
    limit: Option<u32>,
) -> Result<Vec<JobView>, CommandError> {
    let context = job_ctx();
    let records = state
        .desktop_job_host
        .list(
            &context,
            JobFilter {
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

pub async fn cancel_schema_diff_job(state: &AppState, job_id: &str) -> Result<bool, CommandError> {
    let context = job_ctx();
    let id = JobId::new(job_id.to_string());
    match state.desktop_job_host.get(&context, id.clone()).await {
        Err(PortError::NotFound(_)) => Ok(false),
        Err(error) => Err(host_error(error)),
        Ok(record) if record.view.kind != PREPARE_KIND && record.view.kind != APPLY_KIND => {
            Ok(false)
        }
        Ok(record) if record.view.state.is_terminal() => Ok(true),
        Ok(_) => state
            .desktop_job_host
            .cancel(&context, &id)
            .await
            .map(|_| true)
            .map_err(host_error),
    }
}

/// Admit the prepare record first, then detach the read-only plan generation.
pub async fn run_prepare_job(
    state: &AppState,
    request: PrepareRequest,
) -> Result<SchemaDiffJobAccepted, CommandError> {
    let (source_session, target_session, objects) = prepare_endpoints(&request);
    let endpoints =
        endpoints_from_session_pair(state, Some(&source_session), &target_session, &objects)
            .await?;
    let host = Arc::clone(&state.desktop_job_host);
    host.ensure_endpoint_services(&endpoints)
        .map_err(host_error)?;
    let context = job_ctx();
    let job_id = JobId::new(format!("schemaDiffPrepare-{}", uuid::Uuid::new_v4()));
    let definition = JobDefinition {
        job_id: job_id.clone(),
        kind: PREPARE_KIND.into(),
        owner: owner_ref(state),
        payload: prepare_payload(),
        created_at: host.now(),
    };
    let record = host
        .accept(
            &context,
            definition,
            &IdempotencyKey::new(format!("schema-diff-prepare-{job_id}")),
        )
        .await
        .map_err(host_error)?;
    if record.view.job_id == job_id {
        dispatch_prepare(
            Arc::clone(&host),
            Arc::new(state.clone()),
            Arc::clone(&state.schema_diff_jobs),
            context,
            job_id.clone(),
            endpoints,
            target_session,
            request,
        );
    }
    Ok(accepted(&record.view))
}

/// Admit a one-shot apply receipt and return it before any DDL executes.
pub async fn run_apply_job(
    state: &AppState,
    plan_id: &str,
    selection_revision: u64,
    mut apply_request: ApplyRequest,
) -> Result<SchemaDiffJobAccepted, CommandError> {
    let infra = Arc::clone(&state.schema_diff_jobs);
    let stored = infra.plans.get(plan_id).ok_or_else(|| {
        CommandError::NotFound(format!("plan '{plan_id}' is unavailable; compare again"))
    })?;
    if stored.meta.selection_revision != selection_revision {
        return Err(CommandError::Validation(
            "Schema Diff plan selection changed; compare again".into(),
        ));
    }

    let endpoints = endpoints_from_session_pair(
        state,
        None,
        &apply_request.target_db_session_id,
        &plan_objects(&stored.meta),
    )
    .await?;
    let host = Arc::clone(&state.desktop_job_host);
    host.ensure_endpoint_services(&endpoints)
        .map_err(host_error)?;
    let context = job_ctx();
    let job_id = JobId::new(format!("schemaDiffApply-{}", uuid::Uuid::new_v4()));
    apply_request.job_id = Some(job_id.as_str().to_string());
    let definition = JobDefinition {
        job_id: job_id.clone(),
        kind: APPLY_KIND.into(),
        owner: owner_ref(state),
        payload: apply_payload(&stored, &apply_request)?,
        created_at: host.now(),
    };
    let receipt_key = format!("schema-diff-apply-{plan_id}-{selection_revision}");
    let record = host
        .accept(&context, definition, &IdempotencyKey::new(receipt_key))
        .await
        .map_err(host_error)?;
    if record.view.job_id == job_id {
        dispatch_apply(
            Arc::clone(&host),
            Arc::new(state.clone()),
            infra,
            context,
            job_id.clone(),
            endpoints,
            apply_request.target_db_session_id.clone(),
            plan_id.to_string(),
            selection_revision,
            apply_request,
        );
    }
    Ok(accepted(&record.view))
}

fn dispatch_prepare(
    host: Arc<DesktopJobHost>,
    state: Arc<AppState>,
    infra: Arc<SchemaDiffJobInfra>,
    context: RequestContext,
    job_id: JobId,
    endpoints: Vec<EndpointRef>,
    target_session: String,
    request: PrepareRequest,
) {
    tauri::async_runtime::spawn(async move {
        let backend = Arc::new(AppStateBackend::new(
            Arc::clone(&state),
            Arc::clone(&infra.plans),
            Some(target_session),
        ));
        let handler = Arc::new(SchemaDiffHandler::for_prepare(
            backend,
            Arc::clone(&infra.plans),
            request,
        ));
        dispatch(host, context, job_id, endpoints, handler).await;
    });
}

fn dispatch_apply(
    host: Arc<DesktopJobHost>,
    state: Arc<AppState>,
    infra: Arc<SchemaDiffJobInfra>,
    context: RequestContext,
    job_id: JobId,
    endpoints: Vec<EndpointRef>,
    target_session: String,
    plan_id: String,
    selection_revision: u64,
    request: ApplyRequest,
) {
    tauri::async_runtime::spawn(async move {
        let backend = Arc::new(AppStateBackend::new(
            Arc::clone(&state),
            Arc::clone(&infra.plans),
            Some(target_session),
        ));
        let handler = Arc::new(SchemaDiffHandler::for_apply(
            backend,
            Arc::clone(&infra.plans),
            plan_id,
            selection_revision,
            request,
        ));
        dispatch(host, context, job_id, endpoints, handler).await;
    });
}

async fn dispatch(
    host: Arc<DesktopJobHost>,
    context: RequestContext,
    job_id: JobId,
    endpoints: Vec<EndpointRef>,
    handler: Arc<dyn JobHandler>,
) {
    let result = host
        .dispatch(
            &context,
            &job_id,
            &WorkerId::new("schema-diff-host"),
            &endpoints,
            handler,
        )
        .await;
    let Err(error) = result else { return };
    let reason_code = dispatch_error_code(&error);
    let repository = host.repository();
    match host.get(&context, job_id.clone()).await {
        Ok(record) if record.view.state == JobState::Queued => {
            let _ = repository
                .mark_failed_unstarted(&context, &job_id, reason_code)
                .await;
        }
        Ok(record) if record.view.state == JobState::Running => {
            if repository
                .mark_pending_verification(&context, &job_id, "dispatchNeedsVerification")
                .await
                .is_ok()
            {
                let _ = repository
                    .persist_recovery(
                        &context,
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
    tracing::error!(job_id = %job_id.as_str(), reason_code, "schema diff Job dispatch failed");
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

fn plan_objects(meta: &datazen_schema_diff::job::SchemaDiffFrozenPlan) -> Vec<String> {
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

fn accepted(view: &JobView) -> SchemaDiffJobAccepted {
    SchemaDiffJobAccepted {
        job_id: view.job_id.as_str().to_owned(),
        kind: view.kind.clone(),
        state: view.state,
        progress: view.progress,
    }
}

fn prepare_envelope(
    infra: &SchemaDiffJobInfra,
    plan_id: &str,
) -> Option<SchemaDiffPrepareEnvelope> {
    let stored = infra.plans.get(plan_id)?;
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

fn deploy_result(details: &JobDetails) -> SchemaDiffDeployResult {
    let effect = details
        .job
        .effect_outcome
        .unwrap_or(EffectOutcome::NotStarted);
    let status = match (details.job.state, effect) {
        (_, EffectOutcome::Completed) => DeployStatus::Committed,
        (_, EffectOutcome::Unknown) => DeployStatus::Unknown,
        (JobState::Failed, EffectOutcome::PartiallyApplied) => DeployStatus::Mixed,
        (JobState::Cancelled, EffectOutcome::PartiallyApplied) => DeployStatus::Mixed,
        (JobState::Failed, EffectOutcome::RolledBack) => DeployStatus::RolledBack,
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

fn host_error(error: PortError) -> CommandError {
    match error {
        PortError::NotFound(id) => CommandError::NotFound(id),
        PortError::IdempotencyConflict => CommandError::Validation(
            "Schema Diff job receipt conflicts with an earlier request".into(),
        ),
        PortError::BackendUnavailable(_) => {
            CommandError::Internal("Schema Diff job storage is unavailable".into())
        }
        other => CommandError::Validation(format!("Schema Diff job failed: {other}")),
    }
}

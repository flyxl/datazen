//! Durable Data Sync IPC reads, cancellation, and runtime-only session bindings.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use datazen_platform_api::dto::job::{
    JobDetails, JobRecoveryRequest, JobRecoveryResult, JobRecoveryTarget, JobRecoveryVerdict,
    JobRecoveryVerification, JobView,
};
use datazen_platform_api::{PortError, RequestContext};
use datazen_runtime::job::{EndpointRef, EndpointRole, JobRecoveryVerifier};
use futures_util::FutureExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::State;

use super::super::{AppState, CmdExt, CommandError};
use super::jobs;

/// Runtime-only dedicated sessions. Their ids are never part of a durable plan.
pub(crate) struct OwnedSessions {
    pub(crate) source_id: String,
    pub(crate) target_id: String,
    pub(crate) source_endpoint: EndpointRef,
    pub(crate) target_endpoint: EndpointRef,
    pub(crate) source_target: JobRecoveryTarget,
    pub(crate) target_target: JobRecoveryTarget,
}

impl OwnedSessions {
    pub(crate) fn endpoints(&self) -> Vec<EndpointRef> {
        vec![self.source_endpoint.clone(), self.target_endpoint.clone()]
    }

    pub(crate) fn recovery_targets(&self) -> Vec<JobRecoveryTarget> {
        vec![self.source_target.clone(), self.target_target.clone()]
    }

    pub(crate) fn ids(&self) -> Vec<String> {
        vec![self.source_id.clone(), self.target_id.clone()]
    }
}

/// Open dedicated job-owned sessions and prove they resolve to the same
/// physical endpoint identity as the user-authorized sessions.
pub(crate) async fn own_sessions(
    state: &AppState,
    source_session_id: &str,
    target_session_id: &str,
    source_database: &str,
    target_database: &str,
    source_schema: Option<&str>,
    target_schema: Option<&str>,
) -> Result<OwnedSessions, CommandError> {
    let manager = &state.connection_manager;
    let source_identity = crate::services::migration_endpoint::session_identity(
        manager,
        source_session_id,
        Some((source_database, source_schema)),
    )
    .await?;
    let target_identity = crate::services::migration_endpoint::session_identity(
        manager,
        target_session_id,
        Some((target_database, target_schema)),
    )
    .await?;

    let source_id = manager
        .connect_dedicated(
            source_identity.connection_id.as_str(),
            Some(source_database),
        )
        .await
        .map_err(|_| {
            CommandError::Validation("could not open a dedicated Data Sync source session".into())
        })?;
    let target_id = match manager
        .connect_dedicated(
            target_identity.connection_id.as_str(),
            Some(target_database),
        )
        .await
    {
        Ok(id) => id,
        Err(_) => {
            let _ = manager.release(&source_id).await;
            return Err(CommandError::Validation(
                "could not open a dedicated Data Sync target session".into(),
            ));
        }
    };

    let pair = async {
        let source_check = crate::services::migration_endpoint::session_identity(
            manager,
            &source_id,
            Some((source_database, source_schema)),
        )
        .await?;
        let target_check = crate::services::migration_endpoint::session_identity(
            manager,
            &target_id,
            Some((target_database, target_schema)),
        )
        .await?;
        if source_check != source_identity || target_check != target_identity {
            return Err(CommandError::Validation(
                "dedicated Data Sync session does not match the reviewed physical endpoint".into(),
            ));
        }
        Ok((source_check, target_check))
    }
    .await;
    let (source_identity, target_identity) = match pair {
        Ok(pair) => pair,
        Err(error) => {
            let _ = manager.release(&source_id).await;
            let _ = manager.release(&target_id).await;
            return Err(error);
        }
    };

    Ok(OwnedSessions {
        source_id,
        target_id,
        source_endpoint: endpoint_ref(
            &source_identity,
            source_database,
            EndpointRole::SourceReader,
        ),
        target_endpoint: endpoint_ref(
            &target_identity,
            target_database,
            EndpointRole::TargetWriter,
        ),
        source_target: recovery_target(&source_identity, source_database, source_schema),
        target_target: recovery_target(&target_identity, target_database, target_schema),
    })
}

fn endpoint_ref(
    identity: &crate::services::migration_endpoint::EndpointIdentity,
    database: &str,
    role: EndpointRole,
) -> EndpointRef {
    EndpointRef {
        connection_id: identity.connection_id.clone(),
        service_key: identity.service_key.clone(),
        objects: vec![database.to_string()],
        role,
    }
}

fn recovery_target(
    identity: &crate::services::migration_endpoint::EndpointIdentity,
    database: &str,
    schema: Option<&str>,
) -> JobRecoveryTarget {
    let scope = serde_json::json!({ "database": database, "schema": schema });
    let digest = Sha256::digest(serde_json::to_vec(&scope).unwrap_or_default());
    JobRecoveryTarget {
        connection_id: identity.connection_id.as_str().to_string(),
        object_ids: vec![format!("scope:{digest:x}")],
    }
}

pub(crate) async fn release_sessions(state: &AppState, sessions: &[String]) {
    for session in sessions {
        if state.connection_manager.release(session).await.is_err() {
            tracing::warn!("Data Sync job-owned session cleanup was not confirmed");
        }
    }
}

pub(crate) async fn preview_for_durable_job(
    state: &AppState,
    job_id: &str,
) -> Result<super::plans::SyncComparisonPreview, CommandError> {
    let details = jobs::details_durable_job(state, job_id).await?;
    if details.job.kind != super::jobs::PREPARE_KIND {
        return Err(CommandError::Validation(
            "job is not a Data Sync prepare job".into(),
        ));
    }
    let plan_id = details.plan_id.ok_or_else(|| {
        CommandError::Validation("Data Sync prepare job has no durable plan receipt".into())
    })?;
    // The durable comparison can finish before its row-bearing review store
    // is materialized. Wait for that producer, rather than publishing a stale
    // plan error after an arbitrary ten-second retry window.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    while super::host::state::preview_pending(job_id) {
        if tokio::time::Instant::now() >= deadline {
            return Err(CommandError::Validation(
                "Data Sync review projection is still being prepared; retry loading the preview"
                    .into(),
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    super::plans::preview_for_plan(&plan_id).map_err(CommandError::Validation)
}

#[tauri::command]
pub async fn get_data_sync_job(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<JobView, CommandError> {
    jobs::read_durable_job(&state, &job_id)
        .await
        .cmd_err("get_data_sync_job")
}

#[tauri::command]
pub async fn list_data_sync_jobs(
    state: State<'_, AppState>,
) -> Result<Vec<JobDetails>, CommandError> {
    let summaries = jobs::list_durable_jobs(&state)
        .await
        .cmd_err("list_data_sync_jobs")?;
    let mut details = Vec::with_capacity(summaries.len());
    for job in summaries {
        details.push(
            jobs::details_durable_job(&state, job.job_id.as_str())
                .await
                .cmd_err("list_data_sync_jobs")?,
        );
    }
    Ok(details)
}

#[tauri::command]
pub async fn get_data_sync_job_details(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<JobDetails, CommandError> {
    jobs::details_durable_job(&state, &job_id)
        .await
        .cmd_err("get_data_sync_job_details")
}

#[tauri::command]
pub async fn get_data_sync_job_preview(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<super::plans::SyncComparisonPreview, CommandError> {
    preview_for_durable_job(&state, &job_id)
        .await
        .cmd_err("get_data_sync_job_preview")
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataSyncRecoveryRequest {
    pub job_id: String,
    pub source_db_session_id: String,
    pub target_db_session_id: String,
    pub source_database: String,
    pub target_database: String,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
}

/// Recovery is explicit and read-only. A successful endpoint authorization
/// records evidence but never resumes/re-dispatches the interrupted handler.
#[tauri::command]
pub async fn verify_data_sync_recovery(
    state: State<'_, AppState>,
    request: DataSyncRecoveryRequest,
) -> Result<JobDetails, CommandError> {
    let initial_details = jobs::details_durable_job(&state, &request.job_id).await?;
    if initial_details.job.kind != super::jobs::APPLY_KIND
        && initial_details.job.kind != super::jobs::PREPARE_KIND
    {
        return Err(CommandError::NotFound(
            "Data Sync recovery candidate was not found".into(),
        ));
    }
    if !initial_details
        .recovery
        .as_ref()
        .is_some_and(|recovery| recovery.verdict == JobRecoveryVerdict::PendingVerification)
    {
        return Err(CommandError::Validation(
            "Data Sync job is not awaiting recovery verification".into(),
        ));
    }
    let sessions = own_sessions(
        &state,
        &request.source_db_session_id,
        &request.target_db_session_id,
        &request.source_database,
        &request.target_database,
        request.source_schema.as_deref(),
        request.target_schema.as_deref(),
    )
    .await?;
    let verifier = Arc::new(DataSyncRecoveryVerifier {
        state: state.inner().clone(),
        sessions: sessions.ids(),
        targets: sessions.recovery_targets(),
        kind: initial_details.job.kind,
    });
    let result = AssertUnwindSafe(jobs::verify_durable_recovery(
        &state,
        &request.job_id,
        verifier,
    ))
    .catch_unwind()
    .await;
    release_sessions(&state, &sessions.ids()).await;
    match result {
        Ok(result) => result.cmd_err("verify_data_sync_recovery"),
        Err(_) => Err(CommandError::Internal(
            "Data Sync recovery verification failed safely".into(),
        )),
    }
}

struct DataSyncRecoveryVerifier {
    state: AppState,
    sessions: Vec<String>,
    targets: Vec<JobRecoveryTarget>,
    kind: String,
}

#[async_trait::async_trait]
impl JobRecoveryVerifier for DataSyncRecoveryVerifier {
    fn kind(&self) -> &str {
        &self.kind
    }

    async fn verify(
        &self,
        _ctx: &RequestContext,
        request: &JobRecoveryRequest,
    ) -> Result<JobRecoveryVerification, PortError> {
        if request.details.recovery_targets != self.targets || self.sessions.len() != 2 {
            return Ok(manual_review("reauthorizedEndpointScopeMismatch"));
        }
        let live = futures_util::future::try_join_all(
            self.sessions
                .iter()
                .map(|id| self.state.connection_manager.get_session(id)),
        )
        .await;
        if live.is_err() {
            return Ok(manual_review("reauthorizedSessionUnavailable"));
        }
        // The durable Sync receipt intentionally omits row values and SQL, so
        // an endpoint match cannot prove the row effects. Record the explicit
        // live-session evidence and require a human compare before any write.
        Ok(manual_review("syncEffectsRequireFreshComparison"))
    }
}

fn manual_review(reason_code: &str) -> JobRecoveryVerification {
    JobRecoveryVerification {
        result: JobRecoveryResult {
            verdict: JobRecoveryVerdict::RequireManualReview,
            resume_through: None,
            reason_code: Some(reason_code.to_string()),
        },
        confirmed_boundaries: Vec::new(),
        domain_results: Vec::new(),
    }
}

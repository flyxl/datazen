//! Desktop identity, durable job admission and native delivery for portable file jobs.
use super::{AppState, CommandError};
use datazen_feature_core::file_job::{FileArtifactJob, FileJobOperation};
use datazen_platform_api::{
    context::OwnerRef,
    dto::{
        artifact::ArtifactLifecycle,
        job::{JobDefinition, JobState},
    },
    id::{IdempotencyKey, JobId, LeaseId, WorkerId},
    ports::artifact::{ArtifactRepository, ExportSink},
};
use datazen_runtime::job::{EndpointRef, EndpointRole, JobHandler};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};
use tauri::{AppHandle, Emitter};

pub(crate) async fn run_output_job(
    state: &AppState,
    app: &AppHandle,
    session: &str,
    database: Option<&str>,
    kind: &str,
    operation: Arc<dyn FileJobOperation>,
    destination: PathBuf,
) -> Result<u64, CommandError> {
    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let context = adapter.begin_call().context().clone();
    let artifacts = adapter
        .artifact_store()
        .map_err(crate::platform::into_command_error)?;
    let identity = file_job_identity(state, session, database).await?;
    let endpoints = vec![EndpointRef {
        connection_id: identity.connection_id,
        service_key: identity.service_key,
        objects: vec![],
        role: EndpointRole::SourceReader,
    }];
    let host = &state.desktop_job_host;
    host.ensure_endpoint_services(&endpoints)
        .map_err(unavailable)?;
    let job = JobId::new(format!("{kind}-{}", uuid::Uuid::new_v4()));
    let record = host.accept(&context, JobDefinition {
        job_id: job.clone(), kind: kind.into(),
        organization_id: context.organization_id.clone(),
        owner: OwnerRef::ClientSession { client_instance_id: context.client_instance_id.clone(), purpose: kind.into() },
        payload: serde_json::json!({"planVersion":1,"handlerVersion":1,"checkpointVersion":1,"fileExtension":destination.extension().and_then(|value| value.to_str()).unwrap_or("bin").to_ascii_lowercase()}),
        created_at: host.now(),
    }, &IdempotencyKey::new(uuid::Uuid::new_v4().to_string())).await.map_err(unavailable)?;
    if record.view.job_id != job {
        return Err(CommandError::Internal("unexpected file job replay".into()));
    }
    let _ = app.emit(
        "file-job-accepted",
        serde_json::json!({"jobId":job,"kind":kind}),
    );
    let handler = Arc::new(FileArtifactJob::new(
        kind,
        context.clone(),
        artifacts.clone(),
        operation,
    ));
    let dispatched = host
        .dispatch(
            &context,
            &job,
            &WorkerId::new("desktop-file-worker"),
            &endpoints,
            handler.clone() as Arc<dyn JobHandler>,
        )
        .await;
    if let Err(error) = dispatched {
        // Admission is durable even if no worker can start. Never leave a
        // rejected dispatch looking like a job that may still execute later.
        let current = host.get(&context, job.clone()).await.map_err(unavailable)?;
        if current.view.state == JobState::Queued {
            host.repository()
                .mark_failed_unstarted(&context, &job, "fileJobDispatchRejected")
                .await
                .map_err(unavailable)?;
        }
        return Err(unavailable(error));
    }
    let details = host
        .details(&context, job.clone())
        .await
        .map_err(unavailable)?;
    let _ = app.emit("file-job-finished", &details.job);
    if details.job.state != JobState::Succeeded {
        return Err(CommandError::Validation(
            "file job did not complete; inspect its durable status".into(),
        ));
    }
    let (artifact, rows) = handler
        .output()
        .ok_or_else(|| CommandError::Internal("file artifact unavailable".into()))?;
    let metadata = artifacts
        .describe(&context, artifact.clone())
        .await
        .map_err(unavailable)?;
    if metadata.lifecycle != ArtifactLifecycle::Complete {
        return Err(CommandError::Validation(
            "file artifact is incomplete".into(),
        ));
    }
    let handle = LeaseId::new(uuid::Uuid::new_v4().to_string());
    artifacts
        .authorize_dialog_export(
            &context,
            artifact.clone(),
            handle.clone(),
            destination,
            false,
            Arc::new(AtomicBool::new(false)),
        )
        .map_err(unavailable)?;
    artifacts
        .export(&context, artifact, ExportSink::HostHandle(handle))
        .await
        .map_err(unavailable)?;
    Ok(rows)
}

async fn file_job_identity(
    state: &AppState,
    session: &str,
    database: Option<&str>,
) -> Result<datazen_feature_core::services::migration_endpoint::EndpointIdentity, CommandError> {
    use datazen_feature_core::services::migration_endpoint::{checked_identify, normalized};

    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let runtime = adapter
        .runtime()
        .map_err(crate::platform::into_command_error)?;
    let owner = runtime
        .connection_id_for_session(&datazen_runtime::connection::DbSessionId::new(session))
        .ok_or_else(|| CommandError::Validation("File job session unavailable".into()))?;
    let (mut config, metadata) = state
        .store
        .platform_profile(owner.as_str())
        .await
        .filter(|(_, metadata)| metadata.enabled)
        .ok_or_else(|| CommandError::Validation("File job profile unavailable".into()))?;
    let driver = state
        .driver_registry
        .get(&config.database_type)
        .await
        .ok_or_else(|| CommandError::Validation("File job driver unavailable".into()))?;
    if let Some(database) = database {
        config.database = normalized(Some(database))
            .or_else(|| normalized(config.database.as_deref()))
            .map(str::to_owned);
        config.schema = datazen_feature_core::services::schema_scope::metadata_schema(
            driver.as_ref(),
            None,
            None,
            config.schema.as_deref(),
        );
    }
    config.id = owner.as_str().to_owned();
    let identity = checked_identify(&config, driver.as_ref())?;
    let _ = metadata;
    Ok(identity)
}
/// Keep the port's own reason.
///
/// Collapsing every port failure into one constant string made every file job
/// error read "storage or dispatch unavailable" — the user (and the log) could
/// not tell a missing file-job host from a rejected path or a backend that is
/// genuinely down, and the E2E suite could not either. The port error is
/// already sanitized (`PortError` carries no credentials), so pass it through
/// with the file-job prefix instead of discarding it.
fn unavailable(error: datazen_platform_api::PortError) -> CommandError {
    tracing::warn!(%error, "file job port failure");
    CommandError::Internal(format!("file job storage or dispatch unavailable: {error}"))
}

/// Job IDs remain opaque; organization/principal identity comes from the IPC adapter.
#[tauri::command]
pub async fn get_file_job(
    state: tauri::State<'_, AppState>,
    job_id: JobId,
) -> Result<datazen_platform_api::dto::job::JobDetails, CommandError> {
    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let context = adapter.begin_call().context().clone();
    let details = state
        .desktop_job_host
        .details(&context, job_id)
        .await
        .map_err(unavailable)?;
    if !matches!(
        details.job.kind.as_str(),
        "tableExport" | "databaseBackup" | "databaseRestoreApply"
    ) {
        return Err(CommandError::Validation("not a file job".into()));
    }
    Ok(details)
}
#[tauri::command]
pub async fn cancel_file_job(
    state: tauri::State<'_, AppState>,
    job_id: JobId,
) -> Result<datazen_platform_api::dto::job::JobView, CommandError> {
    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let context = adapter.begin_call().context().clone();
    let record = state
        .desktop_job_host
        .get(&context, job_id.clone())
        .await
        .map_err(unavailable)?;
    if !matches!(
        record.view.kind.as_str(),
        "tableExport" | "databaseBackup" | "databaseRestoreApply"
    ) {
        return Err(CommandError::Validation("not a file job".into()));
    }
    Ok(state
        .desktop_job_host
        .cancel(&context, &job_id)
        .await
        .map_err(unavailable)?
        .view)
}

/// Retry delivery of an immutable result without running its source query again.
#[tauri::command]
pub async fn export_file_job(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    job_id: JobId,
) -> Result<bool, CommandError> {
    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let context = adapter.begin_call().context().clone();
    let record = state
        .desktop_job_host
        .get(&context, job_id)
        .await
        .map_err(unavailable)?;
    if !matches!(record.view.kind.as_str(), "tableExport" | "databaseBackup")
        || record.view.state != JobState::Succeeded
    {
        return Err(CommandError::Validation(
            "file job has no complete output".into(),
        ));
    }
    let extension = record
        .definition
        .payload
        .get("fileExtension")
        .and_then(serde_json::Value::as_str)
        .filter(|extension| {
            matches!(
                *extension,
                "csv" | "json" | "sql" | "zip" | "xlsx" | "gz" | "dump"
            )
        })
        .ok_or_else(|| CommandError::Validation("file job output format unavailable".into()))?;
    let artifact = record
        .view
        .artifact_ids
        .first()
        .cloned()
        .ok_or_else(|| CommandError::Validation("file job output unavailable".into()))?;
    let Some(path) = super::dialog::save_file(
        &app,
        ("Export".into(), vec![extension.into()]),
        format!("export.{extension}"),
    )
    .await?
    else {
        return Ok(false);
    };
    super::file::validate_extension(&path, &[extension])?;
    let artifacts = adapter
        .artifact_store()
        .map_err(crate::platform::into_command_error)?;
    let handle = LeaseId::new(uuid::Uuid::new_v4().to_string());
    artifacts
        .authorize_dialog_export(
            &context,
            artifact.clone(),
            handle.clone(),
            path,
            false,
            Arc::new(AtomicBool::new(false)),
        )
        .map_err(unavailable)?;
    artifacts
        .export(&context, artifact, ExportSink::HostHandle(handle))
        .await
        .map_err(unavailable)?;
    Ok(true)
}

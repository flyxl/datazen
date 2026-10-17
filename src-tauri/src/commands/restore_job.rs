//! Native file upload and durable admission for restore apply Jobs.
use super::{AppState, CommandError};
use datazen_feature_core::restore_job::{
    RestoreApplyJob, RestorePlan, UnsupportedRestoreGateway, RESTORE_APPLY_KIND,
};
use datazen_platform_api::{
    context::OwnerRef,
    dto::{
        artifact::AbortReason,
        job::{JobDefinition, JobState},
    },
    id::{ConnectionId, Counter, ExecutionId, IdempotencyKey, JobId, WorkerId},
    ports::artifact::{ArtifactSpec, ArtifactStore, ChunkPayload},
    target::{ExecutionTarget, NamespaceTarget},
};
use datazen_runtime::job::{EndpointRef, EndpointRole, JobHandler};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::Arc};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncReadExt;

/// This path is supplied only by the native dialog (webdriver override is gated
/// by its existing adapter). It is never accepted by the shared Job/Artifact API.
pub(crate) async fn restore_from_native_file(
    state: &AppState,
    app: &AppHandle,
    session: &str,
    database: Option<String>,
    path: PathBuf,
    options: Option<Vec<String>>,
) -> Result<(), CommandError> {
    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let runtime = adapter
        .runtime()
        .map_err(crate::platform::into_command_error)?;
    let owner = runtime
        .connection_id_for_session(&datazen_runtime::connection::DbSessionId::new(session))
        .map(|id| id.as_str().to_owned())
        .ok_or_else(|| {
            CommandError::Validation("Restore session has no owning connection".into())
        })?;
    let (mut config, metadata) = state
        .store
        .platform_profile(&owner)
        .await
        .filter(|(_, metadata)| metadata.enabled)
        .ok_or_else(|| CommandError::Validation("Restore profile unavailable".into()))?;
    if config.read_only || state.store.execution_settings().await.safe_mode {
        return Err(CommandError::Validation(
            "Restore is blocked by read-only or safe-mode policy".into(),
        ));
    }
    let database = database
        .filter(|database| !database.trim().is_empty())
        .or(config.database.clone());
    let driver = state
        .driver_registry
        .get(&config.database_type)
        .await
        .ok_or_else(|| CommandError::Validation("Restore driver unavailable".into()))?;
    if driver.has_multi_database()
        && database
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err(CommandError::Validation(
            "Restore requires an explicit database".into(),
        ));
    }
    config.database = database;
    config.schema =
        crate::services::metadata_schema(driver.as_ref(), None, None, config.schema.as_deref());
    let identity = datazen_feature_core::services::migration_endpoint::checked_identify(
        &config,
        driver.as_ref(),
    )?;
    let namespace =
        NamespaceTarget::new(config.database.clone(), None, config.schema.clone(), vec![]);
    let target = ExecutionTarget::new(ConnectionId::new(owner), namespace);
    let options = options.unwrap_or_default();
    if options
        .iter()
        .any(|value| !matches!(value.as_str(), "overwrite" | "single-transaction"))
    {
        return Err(CommandError::Validation(
            "Unsupported restore option".into(),
        ));
    }
    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let context = adapter.begin_call().context().clone();
    let artifacts = adapter
        .artifact_store()
        .map_err(crate::platform::into_command_error)?;
    // Copy into a private, host-owned staging directory. The subsequent reader
    // has no link back to the selected path or a mutable client file.
    let staging = tempfile::tempdir()?;
    let staged = staging.path().join("restore-input.sql");
    tokio::fs::copy(path, &staged).await?;
    let mut file = tokio::fs::File::open(staged).await?;
    let (artifact, mut writer) = artifacts
        .create(
            &context,
            ArtifactSpec::new(
                context.organization_id.clone(),
                context.principal_id.clone(),
                ExecutionId::new(uuid::Uuid::new_v4().to_string()),
                RestorePlan::target_fingerprint(&target),
            ),
        )
        .await
        .map_err(storage_error)?;
    let mut hash = Sha256::new();
    let upload = async {
        let mut buffer = vec![0; 64 * 1024];
        let mut bytes = 0u64;
        loop {
            let count = file.read(&mut buffer).await?;
            if count == 0 {
                break;
            }
            bytes += count as u64;
            hash.update(&buffer[..count]);
            artifacts
                .append_chunk(&mut writer, ChunkPayload::new(buffer[..count].to_vec()))
                .await
                .map_err(storage_error)?;
        }
        if bytes == 0 {
            return Err(CommandError::Validation("Restore input is empty".into()));
        }
        Ok::<u64, CommandError>(bytes)
    }
    .await;
    let bytes = match upload {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = artifacts.abort(writer, AbortReason::ExecutionFailed).await;
            return Err(error);
        }
    };
    artifacts.finalize(writer).await.map_err(storage_error)?;
    let parsed = datazen_feature_core::backup::parse_restore_options(&options);
    let plan = RestorePlan {
        plan_id: uuid::Uuid::new_v4().to_string(),
        target,
        config_revision: Counter::new(metadata.config_revision),
        artifact_id: artifact,
        input_sha256: format!("{:x}", hash.finalize()),
        input_bytes: Counter::new(bytes),
        single_transaction: parsed.single_transaction,
        overwrite: parsed.overwrite,
    };
    let job = JobId::new(format!("restore-{}", uuid::Uuid::new_v4()));
    let host = &state.desktop_job_host;
    let endpoints = vec![EndpointRef {
        connection_id: plan.target.connection_id.clone(),
        service_key: identity.service_key,
        objects: vec!["*".into()],
        role: EndpointRole::TargetWriter,
    }];
    host.ensure_endpoint_services(&endpoints)
        .map_err(storage_error)?;
    let definition = JobDefinition {
        job_id: job.clone(),
        kind: RESTORE_APPLY_KIND.into(),
        owner: OwnerRef::ClientSession {
            client_instance_id: context.client_instance_id.clone(),
            purpose: RESTORE_APPLY_KIND.into(),
        },
        payload: plan.payload(),
        created_at: host.now(),
    };
    let accepted = host
        .accept(
            &context,
            definition,
            &IdempotencyKey::new(uuid::Uuid::new_v4().to_string()),
        )
        .await
        .map_err(storage_error)?;
    if accepted.view.job_id != job {
        return Err(CommandError::Validation(
            "Unexpected restore admission replay".into(),
        ));
    }
    let _ = app.emit("file-job-accepted", &accepted.view);
    // Only providers with the explicit retained SQL restore contract opt in.
    // Other drivers remain Unsupported/NotStarted until P10.
    let gateway: Arc<dyn datazen_feature_core::restore_job::RestoreGateway> = match state
        .driver_registry
        .resource_provider(&config.database_type)
        .await
        .ok()
        .and_then(|provider| {
            provider
                .capabilities()
                .snapshot
                .confirmed
                .get(datazen_driver_api::restore_sql::CAPABILITY_KEY)
                .cloned()
        }) {
        Some(profile) => Arc::new(super::restore_gateway::DesktopRestoreGateway {
            adapter: adapter.clone(),
            store: state.store.clone(),
            driver,
            profile,
        }),
        None => Arc::new(UnsupportedRestoreGateway),
    };
    let handler = Arc::new(RestoreApplyJob::new(
        context.clone(),
        plan.clone(),
        artifacts,
        gateway,
    ));
    if let Err(error) = host
        .dispatch(
            &context,
            &job,
            &WorkerId::new("desktop-restore-worker"),
            &endpoints,
            handler as Arc<dyn JobHandler>,
        )
        .await
    {
        if host
            .get(&context, job.clone())
            .await
            .map_err(storage_error)?
            .view
            .state
            == JobState::Queued
        {
            host.repository()
                .mark_failed_unstarted(&context, &job, "restoreDispatchRejected")
                .await
                .map_err(storage_error)?;
        }
        let details = host.details(&context, job).await.map_err(storage_error)?;
        let _ = app.emit("file-job-finished", &details.job);
        let _ = error; // The durable outcome is authoritative, not dispatch transport text.
        return Err(CommandError::RestoreOutcome(Box::new(details)));
    }
    let details = host.details(&context, job).await.map_err(storage_error)?;
    let _ = app.emit("file-job-finished", &details.job);
    if details.job.state != JobState::Succeeded {
        return Err(CommandError::RestoreOutcome(Box::new(details)));
    }
    Ok(())
}
fn storage_error(_: datazen_platform_api::PortError) -> CommandError {
    CommandError::Internal(
        "Restore artifact or Job storage unavailable; inspect Job status before retrying".into(),
    )
}

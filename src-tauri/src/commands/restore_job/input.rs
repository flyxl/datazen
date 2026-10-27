//! Native-selected file staging and owner-bound restore Artifact upload.
use super::{storage_error, CommandError};
use datazen_feature_core::restore_job::RestorePlan;
use datazen_modules::{
    execution::domain::{ArtifactRepository, ArtifactSpec, ChunkPayload},
    execution::target::ExecutionTarget,
    execution::AbortReason,
    shared::id::{Counter, ExecutionId},
    RequestContext,
};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Arc};
use tokio::io::AsyncReadExt;

pub(super) async fn upload_native_input(
    context: &RequestContext,
    artifacts: Arc<dyn ArtifactRepository>,
    target: ExecutionTarget,
    config_revision: u64,
    path: &Path,
    options: &[String],
) -> Result<RestorePlan, CommandError> {
    // Copy into a private, host-owned staging directory. The subsequent reader
    // has no link back to the selected path or a mutable client file.
    let staging = tempfile::tempdir()?;
    let staged = staging.path().join("restore-input.sql");
    tokio::fs::copy(path, &staged).await?;
    let mut file = tokio::fs::File::open(staged).await?;
    let (artifact, mut writer) = artifacts
        .create(
            context,
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
    let parsed = datazen_feature_core::backup::parse_restore_options(options);
    Ok(RestorePlan {
        plan_id: uuid::Uuid::new_v4().to_string(),
        target,
        config_revision: Counter::new(config_revision),
        artifact_id: artifact,
        input_sha256: format!("{:x}", hash.finalize()),
        input_bytes: Counter::new(bytes),
        single_transaction: parsed.single_transaction,
        overwrite: parsed.overwrite,
    })
}

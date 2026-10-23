//! Production database backups through a Runtime-owned session and a single
//! staging writer. The FileArtifactJob publishes the sole delivered artifact.
use crate::{backup::BackupEvents, error::FeatureError};
use datazen_driver_api::{sql_dump::stream::BACKUP_COMMAND, BackupDumpOptions, DumpProgress};
use datazen_platform_api::{dto::requests::*, use_cases::ConnectionUseCases};
use datazen_platform_api::{
    dto::{execution::ExecutionState, idempotency::IdempotentOperation},
    id::ExecutionId,
    target::ExecutionTarget,
    OwnerRef, RequestContext,
};
use datazen_runtime::{application::RuntimeConnectionUseCases, job::CancelToken};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

struct LimitedFile {
    file: std::fs::File,
    bytes: u64,
}
impl Write for LimitedFile {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.bytes.saturating_add(bytes.len() as u64)
            > datazen_runtime::artifact::ARTIFACT_BYTE_QUOTA
        {
            return Err(std::io::Error::other("backup artifact quota exceeded"));
        }
        let written = self.file.write(bytes)?;
        self.bytes += written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
enum Output {
    Plain(LimitedFile),
    Gzip(flate2::write::GzEncoder<LimitedFile>),
}
impl Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        match self {
            Self::Plain(f) => f.write_all(bytes),
            Self::Gzip(f) => f.write_all(bytes),
        }
    }
    fn finish(self) -> std::io::Result<()> {
        match self {
            Self::Plain(mut f) => f.flush(),
            Self::Gzip(f) => f.finish()?.flush(),
        }
    }
}
struct PartialFile {
    path: PathBuf,
    finished: bool,
}
impl Drop for PartialFile {
    fn drop(&mut self) {
        if !self.finished {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
fn unavailable() -> FeatureError {
    FeatureError::Validation("runtimeBackupFailed".into())
}

/// `target` is the canonical, frozen database target. No legacy driver connect
/// or String dump fallback exists here. The returned execution id is evidence
/// only; no session handle is persisted with the output.
#[allow(clippy::too_many_arguments)]
pub async fn backup_to_path(
    runtime: Arc<RuntimeConnectionUseCases>,
    ctx: &RequestContext,
    target: ExecutionTarget,
    opts: BackupDumpOptions,
    path: &Path,
    compress: bool,
    events: Option<Arc<dyn BackupEvents>>,
    cancel: &CancelToken,
) -> Result<ExecutionId, FeatureError> {
    if cancel.is_cancelled() {
        return Err(unavailable());
    }
    let database = target
        .namespace
        .database
        .clone()
        .filter(|s| !s.is_empty())
        .ok_or_else(unavailable)?;
    let token = runtime
        .issue_submission_token(ctx, IdempotentOperation::OpenSession, None)
        .await
        .map_err(|_| unavailable())?;
    let opened = runtime
        .open_session(
            ctx,
            OpenSessionRequest::new(
                target.clone(),
                OwnerRef::ClientSession {
                    client_instance_id: ctx.client_instance_id.clone(),
                    purpose: "databaseBackup".into(),
                },
                token.idempotency_key,
            ),
        )
        .await
        .map_err(|_| unavailable())?;
    let handle = opened.session.handle;
    let result = async {
        let file = LimitedFile {
            file: std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?,
            bytes: 0,
        };
        let mut partial = PartialFile {
            path: path.into(),
            finished: false,
        };
        let output = Arc::new(Mutex::new(Some(if compress {
            Output::Gzip(flate2::write::GzEncoder::new(
                file,
                flate2::Compression::default(),
            ))
        } else {
            Output::Plain(file)
        })));
        let writer = output.clone();
        let cancelled = cancel.clone();
        let callback = Arc::new(move |value: serde_json::Value| {
            use datazen_platform_api::error::{ApiError, ApiErrorCode};
            let failure = || {
                ApiError::new(
                    ApiErrorCode::ServiceUnavailable,
                    "backup staging write rejected",
                )
            };
            if cancelled.is_cancelled() {
                return Err(failure());
            }
            match value["kind"].as_str() {
                Some("backupChunk") => {
                    let mut writer = writer.lock().map_err(|_| failure())?;
                    writer
                        .as_mut()
                        .ok_or_else(failure)?
                        .write(value["text"].as_str().ok_or_else(failure)?.as_bytes())
                        .map_err(|_| failure())?;
                }
                Some("backupProgress") => {
                    let progress: DumpProgress =
                        serde_json::from_value(value["progress"].clone()).map_err(|_| failure())?;
                    if let Some(events) = &events {
                        events.backup_progress(progress);
                    }
                }
                _ => return Err(failure()),
            }
            Ok(())
        });
        let binding = runtime
            .bind_backup_output(ctx, &handle, &target, callback)
            .await
            .map_err(|_| unavailable())?;
        let token = runtime
            .issue_submission_token(ctx, IdempotentOperation::ExecuteInSession, Some(&handle))
            .await
            .map_err(|_| unavailable())?;
        let receipt = runtime
            .execute_in_session(
                ctx,
                ExecuteInSessionRequest::new(
                    handle.clone(),
                    CommandCall::new(
                        BACKUP_COMMAND,
                        serde_json::json!({"database":database,"options":opts}),
                    ),
                    token.idempotency_key,
                ),
            )
            .await
            .map_err(|_| unavailable())?;
        let mut cancel_sent = false;
        let success = loop {
            let view = runtime
                .get_execution(ctx, receipt.execution_id.clone())
                .await
                .map_err(|_| unavailable())?;
            if view.state.is_terminal() {
                break view.state == ExecutionState::Succeeded;
            }
            if cancel.is_cancelled() && !cancel_sent {
                let _ = runtime
                    .cancel_execution(ctx, receipt.execution_id.clone())
                    .await;
                cancel_sent = true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        };
        drop(binding); // Wait for in-flight callbacks before closing/removing staging.
        let writer = output
            .lock()
            .map_err(|_| unavailable())?
            .take()
            .ok_or_else(unavailable)?;
        if !success || cancel.is_cancelled() {
            drop(writer);
            return Err(unavailable());
        }
        writer.finish()?;
        partial.finished = true;
        Ok(receipt.execution_id)
    }
    .await;
    let closed = runtime
        .close_session(
            ctx,
            CloseSessionRequest::new(handle, CloseMode::RollbackAndClose),
        )
        .await;
    if !matches!(closed,Ok(ref receipt) if receipt.resource_release==datazen_platform_api::dto::session::ResourceRelease::Confirmed)
    {
        if result.is_ok() {
            let _ = std::fs::remove_file(path);
        }
        return Err(unavailable());
    }
    result
}

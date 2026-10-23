//! Host-only output binding for one backup session. No file paths, sink tokens
//! or callbacks are exposed through a transport DTO or persisted artifact.
use super::*;

pub use datazen_platform_api::dto::artifact::BackupOutput;
pub(super) struct Binding {
    ctx: RequestContext,
    target: ExecutionTarget,
    callback: BackupOutput,
    // Serializes revocation with a writer already delivering a chunk.
    active: Mutex<bool>,
}
impl Binding {
    pub(super) fn write(
        &self,
        entry: &results::ExecutionEntry,
        output: serde_json::Value,
    ) -> Result<(), ApiError> {
        let active = lock(&self.active);
        if !*active
            || !entry.visible_to(&self.ctx)
            || entry.view.provenance.as_ref().map(|p| &p.requested_target) != Some(&self.target)
        {
            return Err(ApiError::new(
                ApiErrorCode::PermissionDenied,
                "backup output binding unavailable",
            ));
        }
        match output.get("kind").and_then(serde_json::Value::as_str) {
            Some("backupChunk")
                if output
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|t| {
                        t.len() <= datazen_driver_api::sql_dump::stream::BACKUP_CHUNK_BYTES
                    }) => {}
            Some("backupProgress") => {
                let progress: datazen_driver_api::DumpProgress =
                    serde_json::from_value(output.get("progress").cloned().unwrap_or_default())
                        .map_err(|_| ApiError::invalid_argument("invalid backup progress"))?;
                if progress.object_name.len() > 4096 || progress.current > progress.total {
                    return Err(ApiError::invalid_argument("backup progress exceeds bounds"));
                }
            }
            _ => return Err(ApiError::invalid_argument("invalid backup output")),
        }
        (self.callback)(output)
    }
}
/// Dropping the binding waits for an in-flight synchronous write, then rejects
/// all late output. An admitted execution retains only a revoked capability.
pub struct BackupOutputBinding {
    sink: RuntimeResultSink,
    handle: SessionHandle,
    binding: Arc<Binding>,
}
impl Drop for BackupOutputBinding {
    fn drop(&mut self) {
        *lock(&self.binding.active) = false;
        let mut bindings = lock(&self.sink.2);
        if bindings
            .get(&self.handle)
            .is_some_and(|b| Arc::ptr_eq(b, &self.binding))
        {
            bindings.remove(&self.handle);
        }
    }
}
impl RuntimeConnectionUseCases {
    /// Trusted host seam; requires an owned dedicated backup session. It cannot
    /// convert an arbitrary query execution into a backup/file-writing command.
    pub async fn bind_backup_output(
        &self,
        ctx: &RequestContext,
        handle: &SessionHandle,
        target: &ExecutionTarget,
        callback: BackupOutput,
    ) -> Result<BackupOutputBinding, ApiError> {
        self.authorize_target(
            ctx,
            AuthorizationAction::Query,
            target,
            "backup_dump",
            &serde_json::Value::Null,
        )
        .await?;
        let entry = self.session_entry(ctx, handle)?;
        if entry.closed.is_some()
            || entry.initial != *target
            || entry.ctx.authentication_session_id != ctx.authentication_session_id
            || entry.ctx.delegation_id != ctx.delegation_id
            || !matches!(&entry.owner,OwnerRef::ClientSession{purpose,..} if purpose=="databaseBackup")
        {
            return Err(ApiError::new(
                ApiErrorCode::PermissionDenied,
                "dedicated backup session required",
            ));
        }
        let binding = Arc::new(Binding {
            ctx: ctx.clone(),
            target: target.clone(),
            callback,
            active: Mutex::new(true),
        });
        let mut bindings = lock(&self.inner.sink.2);
        if bindings.contains_key(handle) {
            return Err(ApiError::new(
                ApiErrorCode::ContextConflict,
                "backup output already bound",
            ));
        }
        bindings.insert(handle.clone(), binding.clone());
        Ok(BackupOutputBinding {
            sink: self.inner.sink.clone(),
            handle: handle.clone(),
            binding,
        })
    }
}

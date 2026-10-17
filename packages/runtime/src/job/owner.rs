//! A stage capability is minted only after JobRuntime has acquired a fenced claim.
//! It is not serializable and has no public constructor. Caller-supplied OwnerRef
//! still follows the ordinary, default-deny session admission path.
use super::JobRuntimeRepository;
use datazen_application::error::{ApiError, ApiErrorCode};
use datazen_platform_api::{
    context::{OwnerRef, RequestContext},
    dto::{job::JobClaim, session::OpenSessionReceipt},
    id::{BlockId, RunId, StageId},
    target::ExecutionTarget,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct StageOwnerPermit {
    repository: Arc<dyn JobRuntimeRepository>,
    context: RequestContext,
    claim: JobClaim,
    stage: StageId,
}
impl StageOwnerPermit {
    pub(crate) fn new(
        repository: Arc<dyn JobRuntimeRepository>,
        context: RequestContext,
        claim: JobClaim,
        stage: StageId,
    ) -> Self {
        Self {
            repository,
            context,
            claim,
            stage,
        }
    }
    pub fn context(&self) -> &RequestContext {
        &self.context
    }
    pub fn job_id(&self) -> &datazen_platform_api::id::JobId {
        &self.claim.job_id
    }
    pub async fn verify_active(&self) -> Result<(), ApiError> {
        // Renew is a fenced repository operation, not a check against a caller's
        // Job DTO. It rejects replaced workers, expired leases and terminal jobs.
        self.repository
            .renew(&self.claim)
            .await
            .map_err(|_| ApiError::permission_denied("job claim is no longer authoritative"))?;
        let job = self
            .repository
            .get(&self.context, self.claim.job_id.clone())
            .await
            .map_err(|_| ApiError::permission_denied("job unavailable"))?;
        if job.view.cancel_requested
            || job.view.state.is_terminal()
            || !job.stages.iter().any(|s| {
                s.stage_id == self.stage
                    && s.claimed_by.as_ref() == Some(&self.claim.worker_id)
                    && s.finished_at.is_none()
            })
        {
            return Err(ApiError::permission_denied("job stage is no longer active"));
        }
        Ok(())
    }
    /// Acquire under a genuine WorkflowBlock owner. No owner or identity comes
    /// from YAML/IPC. An abandoned acquisition is closed by the acquisition task.
    pub async fn open_workflow_block(
        &self,
        runtime: Arc<crate::application::RuntimeConnectionUseCases>,
        block: BlockId,
        target: ExecutionTarget,
        deadline: tokio::time::Instant,
    ) -> Result<JobSessionLease, ApiError> {
        self.open_owned(runtime, Some(block), target, deadline)
            .await
    }
    pub async fn open_job_session(
        &self,
        runtime: Arc<crate::application::RuntimeConnectionUseCases>,
        target: ExecutionTarget,
        deadline: tokio::time::Instant,
    ) -> Result<JobSessionLease, ApiError> {
        self.open_owned(runtime, None, target, deadline).await
    }
    async fn open_owned(
        &self,
        runtime: Arc<crate::application::RuntimeConnectionUseCases>,
        block: Option<BlockId>,
        target: ExecutionTarget,
        deadline: tokio::time::Instant,
    ) -> Result<JobSessionLease, ApiError> {
        self.verify_active().await?;
        if tokio::time::Instant::now() >= deadline {
            return Err(ApiError::new(
                ApiErrorCode::OutcomeUnknown,
                "block acquisition deadline expired",
            ));
        }
        runtime
            .authorize_target(
                &self.context,
                datazen_platform_api::ports::policy::AuthorizationAction::Connect,
                &target,
                "open_session",
                &serde_json::Value::Null,
            )
            .await?;
        let (owner, internal) = match block {
            Some(block_id) => (
                OwnerRef::WorkflowBlock {
                    job_id: self.claim.job_id.clone(),
                    block_id: block_id.clone(),
                },
                crate::connection::OwnerRef::WorkflowBlock {
                    run_id: RunId::new(self.claim.job_id.as_str()),
                    block_id,
                },
            ),
            None => (
                OwnerRef::Job {
                    job_id: self.claim.job_id.clone(),
                    stage_id: self.stage.clone(),
                },
                crate::connection::OwnerRef::Job {
                    organization_id: self.context.organization_id.clone(),
                    job_id: self.claim.job_id.clone(),
                    stage_id: self.stage.as_str().to_owned(),
                },
            ),
        };
        let ctx = self.context.clone();
        let permit = self.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let result = async {
                permit.verify_active().await?;
                let receipt = runtime
                    .create_owned_session(&ctx, target, owner, internal, "")
                    .await?;
                let lease = JobSessionLease {
                    receipt: Some(receipt),
                    runtime: runtime.clone(),
                    context: ctx.clone(),
                    released: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                };
                // A hard block lifetime is independent of GUI attachment and
                // of the deployment's optional idle reclaimer.
                let expiry_runtime = runtime.clone();
                let expiry_context = ctx.clone();
                let expiry_handle = lease.handle().cloned();
                let released = lease.released.clone();
                tokio::spawn(async move {
                    tokio::time::sleep_until(deadline).await;
                    if !released.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        if let Some(handle) = expiry_handle {
                            close_owned(expiry_runtime, expiry_context, handle).await;
                        }
                    }
                });
                permit.verify_active().await?;
                if let Some(handle) = lease.handle() {
                    runtime.bind_job_permit(handle, permit.clone())?;
                }
                Ok(lease)
            }
            .await;
            // Failed delivery drops the lease and triggers RollbackAndClose.
            let _ = send.send(result);
        });
        tokio::time::timeout_at(deadline, receive)
            .await
            .map_err(|_| {
                ApiError::new(
                    ApiErrorCode::OutcomeUnknown,
                    "block acquisition timed out; cleanup must be verified",
                )
            })?
            .map_err(|_| {
                ApiError::new(
                    ApiErrorCode::OutcomeUnknown,
                    "block acquisition receipt unavailable",
                )
            })?
    }
}

/// Owns an acquired session even while its admission receipt is in transit.
/// Consumers must keep this guard until their own deterministic cleanup ends.
pub struct JobSessionLease {
    receipt: Option<OpenSessionReceipt>,
    runtime: Arc<crate::application::RuntimeConnectionUseCases>,
    context: RequestContext,
    released: Arc<std::sync::atomic::AtomicBool>,
}
impl JobSessionLease {
    pub fn handle(&self) -> Option<&datazen_platform_api::dto::session::SessionHandle> {
        self.receipt.as_ref().map(|r| &r.session.handle)
    }
    /// Call only after a confirmed close; an unconfirmed release keeps the guard.
    pub fn confirm_closed(&mut self) {
        self.released
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.receipt = None;
    }
}
impl Drop for JobSessionLease {
    fn drop(&mut self) {
        let Some(receipt) = self.receipt.take() else {
            return;
        };
        if self
            .released
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        let runtime = self.runtime.clone();
        let ctx = self.context.clone();
        if let Ok(executor) = tokio::runtime::Handle::try_current() {
            executor.spawn(close_owned(runtime, ctx, receipt.session.handle));
        }
    }
}
async fn close_owned(
    runtime: Arc<crate::application::RuntimeConnectionUseCases>,
    ctx: RequestContext,
    handle: datazen_platform_api::dto::session::SessionHandle,
) {
    use datazen_application::sessions::ConnectionUseCases;
    match runtime
        .close_session(
            &ctx,
            datazen_application::dto::requests::CloseSessionRequest::new(
                handle,
                datazen_application::dto::requests::CloseMode::RollbackAndClose,
            ),
        )
        .await
    {
        Ok(r)
            if r.resource_release
                == datazen_platform_api::dto::session::ResourceRelease::Confirmed => {}
        _ => tracing::warn!("job block resource release unconfirmed; verification required"),
    }
}

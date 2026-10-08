//! Runtime-side operations that extend the transport-neutral JobRepository port.
//!
//! These methods are deliberately separate from `platform-api::JobRepository`: they are
//! needed while a handler is running (especially by the cancel watcher), or to persist
//! the runtime's result projection. Persistent identifiers and DTOs only cross this API.

use async_trait::async_trait;
use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{
    Checkpoint, CommitBoundary, JobDetails, JobProgress, JobRecord, JobRecoveryResult, JobState,
};
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{IdempotencyKey, JobId, JobStateVersion};

use crate::job::repository::CancelPollSnapshot;
use datazen_platform_api::dto::job::JobClaim;
use datazen_platform_api::id::ArtifactId;
use datazen_platform_api::ports::job::JobRepository;

/// Operations consumed by `JobRuntime` and the desktop Job host.
#[async_trait]
pub trait JobRuntimeRepository: JobRepository {
    async fn cancel_poll(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<CancelPollSnapshot, PortError>;

    async fn request_cancel(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<JobRecord, PortError>;

    async fn confirm_cancelled_not_started(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        outcome: EffectOutcome,
    ) -> Result<JobRecord, PortError>;

    async fn mark_failed_unstarted(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        reason_code: &str,
    ) -> Result<JobRecord, PortError>;

    async fn mark_pending_verification(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        reason_code: &str,
    ) -> Result<JobRecord, PortError>;

    async fn record_progress(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        progress: JobProgress,
    ) -> Result<(), PortError>;

    async fn record_artifacts(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        artifacts: &[ArtifactId],
    ) -> Result<(), PortError>;

    async fn append_external_artifacts(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        artifacts: &[ArtifactId],
    ) -> Result<(), PortError>;

    async fn finish(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        expected: JobStateVersion,
        state: JobState,
        outcome: EffectOutcome,
        progress: JobProgress,
        error_code: Option<String>,
        pending_verification_reason: Option<String>,
    ) -> Result<JobRecord, PortError>;

    async fn persist_recovery(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        recovery: JobRecoveryResult,
    ) -> Result<(), PortError>;

    async fn receipt_for(
        &self,
        ctx: &RequestContext,
        key: &IdempotencyKey,
    ) -> Result<Option<JobId>, PortError>;

    async fn committed_boundaries(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<Vec<CommitBoundary>, PortError>;

    async fn latest_checkpoint(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<Option<Checkpoint>, PortError>;

    async fn get_details(
        &self,
        ctx: &RequestContext,
        job_id: JobId,
    ) -> Result<JobDetails, PortError>;
}

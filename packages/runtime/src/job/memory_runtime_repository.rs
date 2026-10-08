//! Runtime extension implementation for the in-memory contract repository.

use async_trait::async_trait;
use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{
    Checkpoint, CommitBoundary, JobClaim, JobDetails, JobProgress, JobRecord, JobRecoveryResult,
    JobState,
};
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{ArtifactId, IdempotencyKey, JobId, JobStateVersion};

use crate::job::repository::{CancelPollSnapshot, InMemoryJobRepository};
use crate::job::runtime_repository::JobRuntimeRepository;

#[async_trait]
impl JobRuntimeRepository for InMemoryJobRepository {
    async fn cancel_poll(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<CancelPollSnapshot, PortError> {
        InMemoryJobRepository::cancel_poll(self, ctx, job_id)
    }

    async fn request_cancel(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<JobRecord, PortError> {
        InMemoryJobRepository::request_cancel(self, ctx, job_id)
    }

    async fn confirm_cancelled_not_started(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        outcome: EffectOutcome,
    ) -> Result<JobRecord, PortError> {
        InMemoryJobRepository::confirm_cancelled_not_started(self, ctx, job_id, outcome)
    }

    async fn mark_failed_unstarted(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        reason_code: &str,
    ) -> Result<JobRecord, PortError> {
        InMemoryJobRepository::mark_failed_unstarted(self, ctx, job_id, reason_code).await
    }

    async fn mark_pending_verification(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        reason_code: &str,
    ) -> Result<JobRecord, PortError> {
        InMemoryJobRepository::mark_pending_verification(self, ctx, job_id, reason_code)
    }

    async fn record_progress(
        &self,
        _ctx: &RequestContext,
        claim: &JobClaim,
        progress: JobProgress,
    ) -> Result<(), PortError> {
        InMemoryJobRepository::record_progress(self, claim, progress)
    }

    async fn record_artifacts(
        &self,
        _ctx: &RequestContext,
        claim: &JobClaim,
        artifacts: &[ArtifactId],
    ) -> Result<(), PortError> {
        InMemoryJobRepository::record_artifacts(self, claim, artifacts)
    }

    async fn append_external_artifacts(
        &self,
        _ctx: &RequestContext,
        job_id: &JobId,
        artifacts: &[ArtifactId],
    ) -> Result<(), PortError> {
        InMemoryJobRepository::append_external_artifacts(self, job_id, artifacts)
    }

    async fn finish(
        &self,
        _ctx: &RequestContext,
        claim: &JobClaim,
        expected: JobStateVersion,
        state: JobState,
        outcome: EffectOutcome,
        progress: JobProgress,
        error_code: Option<String>,
        pending_verification_reason: Option<String>,
    ) -> Result<JobRecord, PortError> {
        InMemoryJobRepository::finish(
            self,
            claim,
            expected,
            state,
            outcome,
            progress,
            error_code,
            pending_verification_reason,
        )
    }

    async fn persist_recovery(
        &self,
        _ctx: &RequestContext,
        job_id: &JobId,
        recovery: JobRecoveryResult,
    ) -> Result<(), PortError> {
        InMemoryJobRepository::persist_recovery(self, job_id, recovery)
    }

    async fn receipt_for(
        &self,
        _ctx: &RequestContext,
        key: &IdempotencyKey,
    ) -> Result<Option<JobId>, PortError> {
        InMemoryJobRepository::receipt_for(self, key)
    }

    async fn committed_boundaries(
        &self,
        _ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<Vec<CommitBoundary>, PortError> {
        Ok(InMemoryJobRepository::committed_boundaries(self, job_id))
    }

    async fn latest_checkpoint(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<Option<Checkpoint>, PortError> {
        Ok(InMemoryJobRepository::latest_checkpoint(self, ctx, job_id))
    }

    async fn get_details(
        &self,
        _ctx: &RequestContext,
        job_id: JobId,
    ) -> Result<JobDetails, PortError> {
        InMemoryJobRepository::get_details(self, job_id)
    }
}

//! Runtime-side operations that extend the transport-neutral JobRepository port.
//!
//! These methods are deliberately separate from `platform-api::JobRepository`: they are
//! needed while a handler is running (especially by the cancel watcher), or to persist
//! the runtime's result projection. Persistent identifiers and DTOs only cross this API.

use async_trait::async_trait;
use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{
    Checkpoint, CommitBoundary, JobDetails, JobDomainResult, JobProgress, JobRecord,
    JobRecoveryResult, JobRecoveryVerification, JobState,
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

    /// Persist one bounded handler result under the active claim fence.
    async fn record_domain_result(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        result: JobDomainResult,
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

    /// Atomically apply an explicit verifier's safe verdict and newly confirmed facts.
    async fn persist_recovery_verification(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        expected: JobStateVersion,
        verification: JobRecoveryVerification,
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

/// Validate the field-level safety and resource bounds shared by all repository implementations.
pub fn validate_job_domain_result(result: &JobDomainResult) -> Result<(), PortError> {
    if !safe_result_marker(result.stage_id.as_str())
        || !safe_result_marker(&result.result_code)
        || !safe_result_marker(&result.outcome_code)
        || result.counters.len() > 32
        || result.items.len() > 4096
        || result.artifact_ids.len() > 4096
    {
        return Err(PortError::BackendUnavailable(
            "job domain result is outside the safe format".into(),
        ));
    }
    let mut counter_codes = std::collections::HashSet::new();
    if result
        .counters
        .iter()
        .any(|counter| !safe_result_marker(&counter.code) || !counter_codes.insert(&counter.code))
    {
        return Err(PortError::BackendUnavailable(
            "job result counters are invalid".into(),
        ));
    }
    let mut item_ids = std::collections::HashSet::new();
    if result.items.iter().any(|item| {
        !safe_result_marker(&item.item_id)
            || !safe_result_marker(&item.outcome_code)
            || item
                .reason_code
                .as_deref()
                .is_some_and(|code| !safe_result_marker(code))
            || !item_ids.insert(&item.item_id)
    }) || result
        .artifact_ids
        .iter()
        .any(|artifact| !safe_result_marker(artifact.as_str()))
    {
        return Err(PortError::BackendUnavailable(
            "job result items are invalid".into(),
        ));
    }
    Ok(())
}

/// Result codes and opaque IDs use a narrow alphabet and reject secret/session/SQL labels.
pub fn safe_result_marker(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    !value.is_empty()
        && value.len() <= 512
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
        && ![
            "password",
            "passwd",
            "credential",
            "secret",
            "token",
            "session",
            "sql",
        ]
        .iter()
        .any(|label| lower.contains(label))
}

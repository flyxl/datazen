//! Public desktop facade over the durable job repository and runtime.
//!
//! Accepting a job never schedules it. Callers may dispatch only after the
//! repository has committed the job and its idempotency receipt. On process
//! reopen, [`DesktopJobHost::mark_restart_candidates_for_verification`] marks
//! old running jobs for inspection and never invokes a handler.

use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};

use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{JobClaim, JobDomainResult};
use datazen_platform_api::dto::job::{
    JobDefinition, JobDetails, JobFilter, JobRecord, JobRecoveryRequest, JobRecoveryResult,
    JobRecoveryVerdict, JobRecoveryVerification, JobState, RecoveryFilter,
};
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{IdempotencyKey, JobId, Timestamp, WorkerId};
use futures_util::FutureExt;

use crate::budget::ledger::BudgetLedger;
use crate::job::budget::EndpointRef;
use crate::job::handler::{HandlerRegistry, JobHandler};
use crate::job::recovery::JobRecoveryVerifier;
use crate::job::runtime::{JobResult, JobRuntime};
use crate::job::runtime_repository::JobRuntimeRepository;
use crate::job::time::JobClock;

/// Shared, transport-neutral API for desktop job adapters.
pub struct DesktopJobHost {
    repository: Arc<dyn JobRuntimeRepository>,
    ledger: Arc<Mutex<BudgetLedger>>,
    clock: Arc<dyn JobClock>,
}

impl DesktopJobHost {
    pub fn new(
        repository: Arc<dyn JobRuntimeRepository>,
        ledger: Arc<Mutex<BudgetLedger>>,
        clock: Arc<dyn JobClock>,
    ) -> Self {
        Self {
            repository,
            ledger,
            clock,
        }
    }

    /// Return only after the repository has durably accepted the job and receipt.
    pub async fn accept(
        &self,
        ctx: &RequestContext,
        definition: JobDefinition,
        idempotency_key: &IdempotencyKey,
    ) -> Result<JobRecord, PortError> {
        self.repository
            .accept(ctx, definition, idempotency_key)
            .await
    }

    pub async fn get(&self, ctx: &RequestContext, job_id: JobId) -> Result<JobRecord, PortError> {
        self.repository.get(ctx, job_id).await
    }

    pub async fn list(
        &self,
        ctx: &RequestContext,
        filter: JobFilter,
    ) -> Result<Vec<JobRecord>, PortError> {
        self.repository.list(ctx, filter).await
    }

    /// Read one job together with its persisted plan summary, commit boundaries,
    /// artifact references and recovery verdict.
    pub async fn details(
        &self,
        ctx: &RequestContext,
        job_id: JobId,
    ) -> Result<JobDetails, PortError> {
        self.repository.get_details(ctx, job_id).await
    }

    /// Record cancel intent. A queued job is terminalized as NotStarted if it
    /// has not raced with a worker claim; running jobs observe intent at runtime
    /// stage boundaries and through the cancellation watcher.
    pub async fn cancel(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<JobRecord, PortError> {
        let requested = self.repository.request_cancel(ctx, job_id).await?;
        if requested.view.state != JobState::Queued {
            return Ok(requested);
        }
        match self
            .repository
            .confirm_cancelled_not_started(ctx, job_id, EffectOutcome::NotStarted)
            .await
        {
            Ok(record) => Ok(record),
            Err(PortError::CasConflict { .. } | PortError::StaleClaim) => {
                self.repository.get(ctx, job_id.clone()).await
            }
            Err(error) => Err(error),
        }
    }

    /// Persist a bounded, handler-owned read model under the active worker claim.
    pub async fn record_domain_result(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        result: JobDomainResult,
    ) -> Result<(), PortError> {
        self.repository
            .record_domain_result(ctx, claim, result)
            .await
    }

    /// Enumerate restart candidates. This method is read-only and never dispatches.
    pub async fn recovery_candidates(
        &self,
        ctx: &RequestContext,
        filter: RecoveryFilter,
    ) -> Result<Vec<JobRecord>, PortError> {
        self.repository.list_recoverable(ctx, filter).await
    }

    /// Convert interrupted running jobs into durable pending-verification
    /// candidates. No worker, handler, session, or lease is restored or started.
    pub async fn mark_restart_candidates_for_verification(
        &self,
        ctx: &RequestContext,
        filter: RecoveryFilter,
    ) -> Result<Vec<JobRecord>, PortError> {
        let candidates = self.repository.list_recoverable(ctx, filter).await?;
        let mut marked = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            if candidate.view.state == JobState::Running {
                let job_id = &candidate.view.job_id;
                let mut record = self
                    .repository
                    .mark_pending_verification(ctx, job_id, "restartNeedsVerification")
                    .await?;
                self.repository
                    .persist_recovery(
                        ctx,
                        job_id,
                        JobRecoveryResult {
                            verdict: JobRecoveryVerdict::PendingVerification,
                            resume_through: None,
                            reason_code: Some("restartNeedsVerification".into()),
                        },
                    )
                    .await?;
                record.view.error = Some("restartNeedsVerification".into());
                marked.push(record);
            } else if candidate.view.state == JobState::Queued {
                let record = self
                    .repository
                    .mark_failed_unstarted(ctx, &candidate.view.job_id, "notDispatchedAfterRestart")
                    .await?;
                marked.push(record);
            } else {
                marked.push(candidate);
            }
        }
        Ok(marked)
    }

    /// Explicitly verify an interrupted job using a verifier that owns newly authorized,
    /// read-only resources. The result is persisted; this method never dispatches a Job.
    pub async fn verify_recovery(
        &self,
        ctx: &RequestContext,
        job_id: JobId,
        verifier: Arc<dyn JobRecoveryVerifier>,
    ) -> Result<JobDetails, PortError> {
        let details = self.repository.get_details(ctx, job_id.clone()).await?;
        let pending = details
            .recovery
            .as_ref()
            .is_some_and(|result| result.verdict == JobRecoveryVerdict::PendingVerification);
        if details.job.state != JobState::Failed
            || details.job.pending_verification_reason.is_none()
            || !pending
            || verifier.kind() != details.job.kind
        {
            return Err(PortError::CasConflict {
                entity: "job_recovery",
                id: job_id.as_str().into(),
            });
        }
        let checkpoint = self.repository.latest_checkpoint(ctx, &job_id).await?;
        let request = JobRecoveryRequest {
            details,
            checkpoint,
        };
        let verification = match AssertUnwindSafe(verifier.verify(ctx, &request))
            .catch_unwind()
            .await
        {
            Ok(Ok(value)) => value,
            Ok(Err(_)) | Err(_) => JobRecoveryVerification {
                result: JobRecoveryResult {
                    verdict: JobRecoveryVerdict::RequireManualReview,
                    resume_through: None,
                    reason_code: Some("verificationUnavailable".into()),
                },
                confirmed_boundaries: Vec::new(),
                domain_results: Vec::new(),
            },
        };
        self.repository
            .persist_recovery_verification(
                ctx,
                &job_id,
                request.details.state_version,
                verification,
            )
            .await?;
        self.repository.get_details(ctx, job_id).await
    }

    /// Explicit caller-driven dispatch. The handler is supplied from live
    /// session state and is never serialized or reconstructed after a restart.
    pub async fn dispatch(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        worker: &WorkerId,
        endpoints: &[EndpointRef],
        handler: Arc<dyn JobHandler>,
    ) -> Result<JobResult, PortError> {
        let mut registry = HandlerRegistry::new();
        registry.register(handler);
        let runtime = JobRuntime::new(
            self.repository.clone(),
            Arc::new(registry),
            self.ledger.clone(),
            self.clock.clone(),
            self.clock_millis(),
        );
        runtime.run(ctx, job_id, worker, endpoints).await
    }

    pub fn repository(&self) -> Arc<dyn JobRuntimeRepository> {
        self.repository.clone()
    }

    /// Register each connection used by a local Job with the budget ledger.
    /// This stores connection ids only; endpoint sessions remain live handler state.
    pub fn ensure_endpoint_services(&self, endpoints: &[EndpointRef]) -> Result<(), PortError> {
        let mut ledger = self.ledger.lock().map_err(|_| {
            PortError::BackendUnavailable("local job budget ledger is unavailable".into())
        })?;
        for endpoint in endpoints {
            ledger.ensure_service(&endpoint.connection_id);
        }
        Ok(())
    }

    pub fn now(&self) -> Timestamp {
        self.clock.now()
    }

    fn clock_millis(&self) -> u64 {
        chrono::DateTime::parse_from_rfc3339(self.clock.now().as_str())
            .ok()
            .and_then(|time| u64::try_from(time.timestamp_millis()).ok())
            .unwrap_or_default()
    }
}

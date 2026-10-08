use super::{lock_inner, InMemoryJobRepository};
use crate::job::runtime_repository::{safe_result_marker, validate_job_domain_result};
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{
    JobDetails, JobDomainResult, JobProgress, JobRecoveryResult, JobRecoveryTarget,
    JobRecoveryVerdict, JobRecoveryVerification, JobState,
};
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{ArtifactId, IdempotencyKey, JobId, JobStateVersion};

impl InMemoryJobRepository {
    pub fn record_progress(
        &self,
        claim: &datazen_platform_api::dto::job::JobClaim,
        progress: JobProgress,
    ) -> Result<(), PortError> {
        let mut inner = lock_inner(&self.inner)?;
        Self::check_claim(&inner, claim, &self.clock.now()).map_err(PortError::from)?;
        let row = inner
            .jobs
            .get_mut(&claim.job_id)
            .ok_or_else(|| PortError::NotFound(claim.job_id.as_str().into()))?;
        row.record.view.progress = progress;
        row.record.view.updated_at = self.clock.now();
        Ok(())
    }

    pub fn record_artifacts(
        &self,
        claim: &datazen_platform_api::dto::job::JobClaim,
        artifacts: &[ArtifactId],
    ) -> Result<(), PortError> {
        let mut inner = lock_inner(&self.inner)?;
        Self::check_claim(&inner, claim, &self.clock.now()).map_err(PortError::from)?;
        let row = inner
            .jobs
            .get_mut(&claim.job_id)
            .ok_or_else(|| PortError::NotFound(claim.job_id.as_str().into()))?;
        append_artifacts(&mut row.record.view.artifact_ids, artifacts);
        row.record.view.updated_at = self.clock.now();
        Ok(())
    }

    pub fn append_external_artifacts(
        &self,
        job_id: &JobId,
        artifacts: &[ArtifactId],
    ) -> Result<(), PortError> {
        let mut inner = lock_inner(&self.inner)?;
        let row = inner
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        if !row.record.view.state.is_terminal() {
            return Err(PortError::CasConflict {
                entity: "job_artifact",
                id: job_id.as_str().into(),
            });
        }
        append_artifacts(&mut row.record.view.artifact_ids, artifacts);
        row.record.view.updated_at = self.clock.now();
        Ok(())
    }

    pub fn finish(
        &self,
        claim: &datazen_platform_api::dto::job::JobClaim,
        expected: JobStateVersion,
        state: JobState,
        outcome: EffectOutcome,
        progress: JobProgress,
        error_code: Option<String>,
        pending_reason: Option<String>,
    ) -> Result<datazen_platform_api::dto::job::JobRecord, PortError> {
        if !state.is_terminal() {
            return Err(PortError::BackendUnavailable(
                "job finish requires a terminal state".into(),
            ));
        }
        let mut inner = lock_inner(&self.inner)?;
        Self::check_claim(&inner, claim, &self.clock.now()).map_err(PortError::from)?;
        let record = {
            let row = inner
                .jobs
                .get_mut(&claim.job_id)
                .ok_or_else(|| PortError::NotFound(claim.job_id.as_str().into()))?;
            if row.record.state_version != expected {
                return Err(PortError::CasConflict {
                    entity: "job",
                    id: claim.job_id.as_str().into(),
                });
            }
            row.record.view.state = state;
            row.record.view.effect_outcome = Some(outcome);
            row.record.view.progress = progress;
            row.record.view.error = error_code;
            row.record.view.pending_verification_reason = pending_reason.clone();
            row.record.view.updated_at = self.clock.now();
            row.record.state_version = JobStateVersion::new(expected.get().saturating_add(1));
            row.claim = None;
            row.record.clone()
        };
        if let Some(reason_code) = pending_reason {
            inner.recovery.insert(
                claim.job_id.clone(),
                JobRecoveryResult {
                    verdict: JobRecoveryVerdict::PendingVerification,
                    resume_through: None,
                    reason_code: Some(reason_code),
                },
            );
        }
        Ok(record)
    }

    pub fn persist_recovery(
        &self,
        job_id: &JobId,
        recovery: JobRecoveryResult,
    ) -> Result<(), PortError> {
        let mut inner = lock_inner(&self.inner)?;
        let row = inner
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        row.record.view.updated_at = self.clock.now();
        inner.recovery.insert(job_id.clone(), recovery);
        Ok(())
    }

    pub fn record_domain_result(
        &self,
        claim: &datazen_platform_api::dto::job::JobClaim,
        result: JobDomainResult,
    ) -> Result<(), PortError> {
        validate_job_domain_result(&result)?;
        let mut inner = lock_inner(&self.inner)?;
        Self::check_claim(&inner, claim, &self.clock.now()).map_err(PortError::from)?;
        inner
            .domain_results
            .insert((claim.job_id.clone(), result.stage_id.clone()), result);
        Ok(())
    }

    pub fn persist_recovery_verification(
        &self,
        job_id: &JobId,
        expected: JobStateVersion,
        verification: JobRecoveryVerification,
    ) -> Result<(), PortError> {
        if verification
            .result
            .reason_code
            .as_deref()
            .is_some_and(|code| !safe_result_marker(code))
            || verification.domain_results.len() > 64
        {
            return Err(PortError::BackendUnavailable(
                "recovery result is outside the safe format".into(),
            ));
        }
        for result in &verification.domain_results {
            validate_job_domain_result(result)?;
        }
        if !verification.confirmed_boundaries.is_empty()
            && verification.result.verdict != JobRecoveryVerdict::ResumeAfterVerify
        {
            return Err(PortError::BackendUnavailable(
                "unverified commit boundaries cannot be recorded".into(),
            ));
        }

        let mut inner = lock_inner(&self.inner)?;
        let current = inner
            .jobs
            .get(job_id)
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        if current.record.state_version != expected
            || current.record.view.state != JobState::Failed
            || current.record.view.pending_verification_reason.is_none()
            || inner.recovery.get(job_id).map_or(true, |value| {
                value.verdict != JobRecoveryVerdict::PendingVerification
            })
        {
            return Err(PortError::CasConflict {
                entity: "job_recovery",
                id: job_id.as_str().into(),
            });
        }
        let current_boundary_count = inner.boundaries.get(job_id).map_or(0, Vec::len);
        let total_boundaries =
            current_boundary_count.saturating_add(verification.confirmed_boundaries.len());
        if verification
            .result
            .resume_through
            .is_some_and(|index| usize::try_from(index).unwrap_or(usize::MAX) > total_boundaries)
        {
            return Err(PortError::BackendUnavailable(
                "recovery boundary index is invalid".into(),
            ));
        }
        let now = self.clock.now();
        let reason = verification.result.reason_code.clone();
        let row = inner
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        row.record.state_version = JobStateVersion::new(expected.get().saturating_add(1));
        row.record.view.updated_at = now;
        let pending =
            (verification.result.verdict != JobRecoveryVerdict::ResumeAfterVerify).then(|| {
                reason
                    .clone()
                    .unwrap_or_else(|| "recoveryNeedsReview".into())
            });
        row.record.view.pending_verification_reason = pending.clone();
        row.record.view.error = pending;

        inner.recovery.insert(job_id.clone(), verification.result);
        inner
            .boundaries
            .entry(job_id.clone())
            .or_default()
            .extend(verification.confirmed_boundaries);
        for result in verification.domain_results {
            inner
                .domain_results
                .insert((job_id.clone(), result.stage_id.clone()), result);
        }
        Ok(())
    }

    pub fn receipt_for(&self, key: &IdempotencyKey) -> Result<Option<JobId>, PortError> {
        let inner = lock_inner(&self.inner)?;
        Ok(inner
            .idempotency
            .get(key.as_str())
            .map(|receipt| receipt.job_id.clone()))
    }

    pub fn get_details(&self, job_id: JobId) -> Result<JobDetails, PortError> {
        let inner = lock_inner(&self.inner)?;
        let row = inner
            .jobs
            .get(&job_id)
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        let payload = &row.record.definition.payload;
        let plan_id = payload
            .get("consumedPlanId")
            .or_else(|| payload.get("planId"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let plan_digest = payload
            .get("planDigest")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let selection_revision = payload
            .get("selectionRevision")
            .and_then(serde_json::Value::as_u64);
        let recovery_targets = payload
            .get("recoveryTargets")
            .cloned()
            .and_then(|value| serde_json::from_value::<Vec<JobRecoveryTarget>>(value).ok())
            .unwrap_or_default();
        let target_before_fingerprint = payload
            .get("targetBeforeFingerprint")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let recovery_policy = payload
            .get("recoveryPolicy")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let mut domain_results: Vec<_> = inner
            .domain_results
            .iter()
            .filter_map(|((stored_job, _), result)| {
                (stored_job == &job_id).then_some(result.clone())
            })
            .collect();
        domain_results.sort_by(|left, right| left.stage_id.cmp(&right.stage_id));
        Ok(JobDetails {
            job: row.record.view.clone(),
            state_version: row.record.state_version,
            plan_id,
            plan_digest,
            selection_revision,
            commit_boundaries: inner.boundaries.get(&job_id).cloned().unwrap_or_default(),
            recovery: inner.recovery.get(&job_id).cloned(),
            domain_results,
            recovery_targets,
            target_before_fingerprint,
            recovery_policy,
        })
    }
}

fn append_artifacts(target: &mut Vec<ArtifactId>, artifacts: &[ArtifactId]) {
    for artifact in artifacts {
        if !target.contains(artifact) {
            target.push(artifact.clone());
        }
    }
}

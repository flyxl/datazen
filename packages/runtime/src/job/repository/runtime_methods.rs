use super::{lock_inner, InMemoryJobRepository};
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{JobDetails, JobProgress, JobRecoveryResult, JobState};
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
        row.record.view.pending_verification_reason = pending_reason;
        row.record.view.updated_at = self.clock.now();
        row.record.state_version = JobStateVersion::new(expected.get().saturating_add(1));
        row.claim = None;
        Ok(row.record.clone())
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
        Ok(JobDetails {
            job: row.record.view.clone(),
            plan_id,
            plan_digest,
            selection_revision,
            commit_boundaries: inner.boundaries.get(&job_id).cloned().unwrap_or_default(),
            recovery: inner.recovery.get(&job_id).cloned(),
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

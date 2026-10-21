//! In-memory equivalent of persisted owner admission, used by the runtime contract.
use super::*;
impl InMemoryJobRepository {
    fn current_owner_row<'a>(
        &self,
        inner: &'a Inner,
        ctx: &RequestContext,
        claim: &JobClaim,
        stage: &StageId,
    ) -> Result<&'a JobRow, PortError> {
        let row = inner.jobs.get(&claim.job_id).ok_or(PortError::StaleClaim)?;
        let bound = row.claim_context.as_ref().ok_or(PortError::StaleClaim)?;
        let valid = bound.organization_id == ctx.organization_id
            && bound.principal_id == ctx.principal_id
            && bound.client_instance_id == ctx.client_instance_id
            && row.claim.as_ref() == Some(claim)
            && self.clock.now().as_str() < claim.expires_at.as_str()
            && row.record.view.state == JobState::Running
            && !row.record.view.cancel_requested
            && row.record.view.stage.as_deref() == Some(stage.as_str())
            && row.record.stages.iter().any(|s| {
                &s.stage_id == stage
                    && s.claimed_by.as_ref() == Some(&claim.worker_id)
                    && s.started_at.is_some()
                    && s.finished_at.is_none()
            });
        if valid {
            Ok(row)
        } else {
            Err(PortError::StaleClaim)
        }
    }
    pub(in crate::job) fn validate_owner_claim(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        stage: &StageId,
    ) -> Result<(), PortError> {
        let inner = lock_inner(&self.inner)?;
        self.current_owner_row(&inner, ctx, claim, stage)
            .map(|_| ())
    }
}

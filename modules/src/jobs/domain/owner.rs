//! Runtime-issued authority for Job handlers. This capability is minted
//! only after JobRuntime acquires a fenced claim and is never serialized.
use super::job::JobClaim;
use super::runtime_repository::JobRuntimeRepository;
use crate::shared::context::RequestContext;
use crate::shared::error::ApiError;
use crate::shared::id::{JobId, StageId};
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

    pub fn stage_id(&self) -> &StageId {
        &self.stage
    }

    pub fn job_id(&self) -> &JobId {
        &self.claim.job_id
    }

    pub async fn verify_active(&self) -> Result<(), ApiError> {
        self.repository
            .validate_stage_owner(&self.context, &self.claim, &self.stage)
            .await
            .map_err(|_| ApiError::permission_denied("job stage claim is no longer authoritative"))
    }
}

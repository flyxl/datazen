//! Runtime-issued authority for Job handlers. This capability is minted
//! only after JobRuntime acquires a fenced claim and is never serialized.
use super::JobRuntimeRepository;
use datazen_platform_api::error::ApiError;
use datazen_platform_api::{
    context::RequestContext,
    dto::job::JobClaim,
    id::{JobId, StageId},
    ports::native_tool::NativeToolSelection,
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

    pub(crate) async fn verify_native_selection(
        &self,
        selection: &NativeToolSelection,
    ) -> Result<(), ApiError> {
        self.verify_active().await?;
        let record = self
            .repository
            .get(&self.context, self.claim.job_id.clone())
            .await
            .map_err(|_| ApiError::permission_denied("native job selection unavailable"))?;
        let expected = serde_json::to_value(selection)
            .map_err(|_| ApiError::permission_denied("native tool selection invalid"))?;
        if record.definition.kind != "nativeToolProbe"
            || record.definition.payload.get("nativeToolSelection") != Some(&expected)
        {
            return Err(ApiError::permission_denied(
                "native tool differs from the frozen job selection",
            ));
        }
        Ok(())
    }
}

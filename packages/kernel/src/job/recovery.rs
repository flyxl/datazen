//! Explicit, asynchronous recovery verification with freshly authorized resources.

use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::job::{JobRecoveryRequest, JobRecoveryVerification};
use datazen_platform_api::error::PortError;

/// Read-only verifier supplied by a domain adapter after it has established fresh access.
/// The host persists its result but never dispatches the interrupted side-effecting job.
#[async_trait::async_trait]
pub trait JobRecoveryVerifier: Send + Sync {
    fn kind(&self) -> &str;

    async fn verify(
        &self,
        ctx: &RequestContext,
        request: &JobRecoveryRequest,
    ) -> Result<JobRecoveryVerification, PortError>;
}

//! Explicit, asynchronous recovery verification with freshly authorized resources.

use crate::jobs::domain::job::{JobRecoveryRequest, JobRecoveryVerification};
use crate::shared::context::RequestContext;
use crate::shared::error::PortError;

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

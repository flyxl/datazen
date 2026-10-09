//! Job 领域错误。端口实现按错误类别映射到 [`PortError`]。

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum JobError {
    #[error("job not found: {0}")]
    NotFound(String),
    #[error("idempotency conflict: same key, different request payload")]
    IdempotencyConflict,
    #[error("plan already consumed by an apply job: {0}")]
    PlanAlreadyConsumed(String),
    #[error("plan projection invalid: {0}")]
    PlanProjectionInvalid(String),
    #[error("unsupported plan/checkpoint version major {got}, supported {supported}")]
    VersionIncompatible { got: u64, supported: u64 },
    #[error("stale claim: generation/worker mismatch")]
    StaleClaim,
    #[error("claim expired: worker must re-claim before writing")]
    ClaimExpired,
    #[error("state version conflict on {0}")]
    VersionConflict(String),
    #[error("job already in terminal state")]
    Terminal,
    #[error("duplicate checkpoint version: {0}")]
    CheckpointConflict(String),
    #[error("endpoint overlap: {0}")]
    EndpointOverlap(String),
    #[error("budget admission denied: {0}")]
    BudgetDenied(String),
    #[error("handler not registered for kind={kind} version={version}")]
    HandlerNotRegistered { kind: String, version: u64 },
    #[error("capability unsupported: {0}")]
    CapabilityUnsupported(String),
    #[error("recovery decision: {0}")]
    RecoveryRejected(String),
}

impl From<JobError> for datazen_platform_api::error::PortError {
    fn from(e: JobError) -> Self {
        use datazen_platform_api::error::PortError;
        match e {
            JobError::NotFound(s) => PortError::NotFound(s),
            JobError::IdempotencyConflict => PortError::IdempotencyConflict,
            JobError::PlanAlreadyConsumed(s) => PortError::PlanAlreadyConsumed(s),
            JobError::VersionIncompatible { got, supported } => {
                PortError::UnsupportedVersion(format!("major {got}, supported {supported}"))
            }
            JobError::StaleClaim | JobError::ClaimExpired => PortError::StaleClaim,
            JobError::VersionConflict(id) => PortError::CasConflict { entity: "job", id },
            JobError::Terminal => PortError::CasConflict {
                entity: "job",
                id: "terminal".into(),
            },
            JobError::CheckpointConflict(id) => PortError::CasConflict {
                entity: "job_checkpoint",
                id,
            },
            JobError::PlanProjectionInvalid(s)
            | JobError::EndpointOverlap(s)
            | JobError::BudgetDenied(s)
            | JobError::CapabilityUnsupported(s)
            | JobError::RecoveryRejected(s) => PortError::BackendUnavailable(s),
            JobError::HandlerNotRegistered { kind, version } => {
                PortError::BackendUnavailable(format!("handler {kind}@{version} not registered"))
            }
        }
    }
}

//! 表编辑（写链路）的应用错误（D4-d，随 data_browsing 自 modules 迁入）。
//!
//! 变体与 `Display` 字符串与旧 `FeatureError` 对应变体**逐字一致**；
//! modules 侧 `From<TableEditingError> for FeatureError` 保证留守消费者
//! （result_edit / workflow / 宿主命令）零改动（§5.5 单向委托）。

#[derive(Debug, thiserror::Error)]
pub enum TableEditingError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    NotConfigured(String),
    #[error("{0}")]
    Internal(String),
    #[error(transparent)]
    Driver(#[from] datazen_driver_api::DriverError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl From<String> for TableEditingError {
    fn from(message: String) -> Self {
        Self::Internal(message)
    }
}

impl From<crate::shared::error::ApiError> for TableEditingError {
    fn from(error: crate::shared::error::ApiError) -> Self {
        use crate::shared::error::ApiErrorCode;

        let detail = match error.request_id.as_ref() {
            Some(request_id) => format!("（requestId={request_id}）"),
            None => String::new(),
        };
        let message = format!("{}{detail}", error.message);
        // 映射与旧 FeatureError 的 From<ApiError> 逐臂一致（错误兼容）。
        match error.code {
            ApiErrorCode::SessionNotFound
            | ApiErrorCode::SessionLost
            | ApiErrorCode::NotFound
            | ApiErrorCode::SourceChanged => Self::NotFound(message),
            ApiErrorCode::InvalidArgument
            | ApiErrorCode::PayloadTooLarge
            | ApiErrorCode::TargetRequired
            | ApiErrorCode::TargetConflict
            | ApiErrorCode::TargetConflictRows
            | ApiErrorCode::ConfigRevisionMismatch
            | ApiErrorCode::TransactionResolutionRequired
            | ApiErrorCode::IdempotencyConflict
            | ApiErrorCode::IdempotencyExpired => Self::Validation(message),
            ApiErrorCode::PermissionDenied
            | ApiErrorCode::Unauthenticated
            | ApiErrorCode::CapabilityUnsupported
            | ApiErrorCode::UnsupportedPlan
            | ApiErrorCode::ServiceUnavailable => Self::NotConfigured(message),
            ApiErrorCode::ResourceBusy
            | ApiErrorCode::QueueFull
            | ApiErrorCode::SessionQuotaExceeded
            | ApiErrorCode::RateLimited
            | ApiErrorCode::QuotaExceeded
            | ApiErrorCode::EndpointOverlap
            | ApiErrorCode::RuntimeEpochMismatch
            | ApiErrorCode::ContextConflict
            | ApiErrorCode::OutcomeUnknown
            | ApiErrorCode::RollbackFailed
            | ApiErrorCode::CleanupFailed
            | ApiErrorCode::PlanStale
            | ApiErrorCode::TargetUnsupported => Self::Internal(message),
        }
    }
}

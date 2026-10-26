//! Errors from portable feature services. Hosts map these to their transport envelope.
#[derive(Debug, thiserror::Error)]
pub enum FeatureError {
    #[error(transparent)]
    Ai(#[from] datazen_modules::ai::AiError),
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
impl From<String> for FeatureError {
    fn from(value: String) -> Self {
        Self::Internal(value)
    }
}

/// Map a shared application error onto the feature error shape.
///
/// Mirrors the Desktop adapter's mapping buckets so both transports report the
/// same message and the same class of failure for the same API error.
impl From<datazen_platform_api::error::ApiError> for FeatureError {
    fn from(error: datazen_platform_api::error::ApiError) -> Self {
        use datazen_platform_api::error::ApiErrorCode;

        let detail = match error.request_id.as_ref() {
            Some(request_id) => format!("（requestId={request_id}）"),
            None => String::new(),
        };
        let message = format!("{}{detail}", error.message);
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

// D4-d：写链路错误自 table_editing 迁入后，留守消费者仍以 FeatureError 汇聚。
impl From<datazen_modules::table_editing::TableEditingError> for FeatureError {
    fn from(error: datazen_modules::table_editing::TableEditingError) -> Self {
        use datazen_modules::table_editing::TableEditingError;
        match error {
            TableEditingError::Validation(message) => Self::Validation(message),
            TableEditingError::NotFound(message) => Self::NotFound(message),
            TableEditingError::NotConfigured(message) => Self::NotConfigured(message),
            TableEditingError::Internal(message) => Self::Internal(message),
            TableEditingError::Driver(error) => Self::Driver(error),
            TableEditingError::Io(error) => Self::Io(error),
            TableEditingError::Json(error) => Self::Json(error),
        }
    }
}

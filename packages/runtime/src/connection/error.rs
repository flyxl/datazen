//! 连接运行时的两套**互不交织**的错误命名空间。
//!
//! connection-management.md §13（L769–773）把错误分成三层，任何一层都不得用另一层的码表达：
//!
//! 1. [`ApiErrorCode`] —— `ApiError.code`。只表达「请求被拒绝」，即**还没有 execution 记录**。
//! 2. [`ExecutionErrorCode`] —— 派发之后的执行终态错误码，载体是 `ExecutionState='failed'` + 事件。
//!    它与 `ApiError.code` 是不同命名空间。
//! 3. [`ProviderError`] —— provider（fake 或真实驱动）在 §5.1 端口上返回的错误，
//!    由端口适配层翻译成上面两层之一。
//!
//! 记录已存在之后的宿主二次校验失败只能用 `ExecutionErrorCode::HostRejected`，
//! 绝不能用 `ApiError`（§13 L777）。

use serde::{Deserialize, Serialize};

/// 请求拒绝错误码。命名空间来源：connection-management.md §13 错误表。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApiErrorCode {
    InvalidArgument,
    TargetRequired,
    TargetConflict,
    TargetUnsupported,
    SessionNotFound,
    SessionLost,
    RuntimeEpochMismatch,
    ContextConflict,
    PermissionDenied,
    ResourceBusy,
    QueueFull,
    TransactionResolutionRequired,
    CapabilityUnsupported,
    UnsupportedPlan,
    EndpointOverlap,
    SessionQuotaExceeded,
    IdempotencyExpired,
    RollbackFailed,
    CleanupFailed,
    OutcomeUnknown,
    PlanStale,
    SourceChanged,
    TargetConflictRows,
    IdempotencyConflict,
    Unauthenticated,
    NotFound,
    PayloadTooLarge,
    QuotaExceeded,
    RateLimited,
    ServiceUnavailable,
    ConfigRevisionMismatch,
}

impl ApiErrorCode {
    /// 稳定的线上字面量。协议字面量只在本处声明一次，其余位置一律引用本枚举。
    pub const fn as_str(self) -> &'static str {
        match self {
            ApiErrorCode::InvalidArgument => "InvalidArgument",
            ApiErrorCode::TargetRequired => "TargetRequired",
            ApiErrorCode::TargetConflict => "TargetConflict",
            ApiErrorCode::TargetUnsupported => "TargetUnsupported",
            ApiErrorCode::SessionNotFound => "SessionNotFound",
            ApiErrorCode::SessionLost => "SessionLost",
            ApiErrorCode::RuntimeEpochMismatch => "RuntimeEpochMismatch",
            ApiErrorCode::ContextConflict => "ContextConflict",
            ApiErrorCode::PermissionDenied => "PermissionDenied",
            ApiErrorCode::ResourceBusy => "ResourceBusy",
            ApiErrorCode::QueueFull => "QueueFull",
            ApiErrorCode::TransactionResolutionRequired => "TransactionResolutionRequired",
            ApiErrorCode::CapabilityUnsupported => "CapabilityUnsupported",
            ApiErrorCode::UnsupportedPlan => "UnsupportedPlan",
            ApiErrorCode::EndpointOverlap => "EndpointOverlap",
            ApiErrorCode::SessionQuotaExceeded => "SessionQuotaExceeded",
            ApiErrorCode::IdempotencyExpired => "IdempotencyExpired",
            ApiErrorCode::RollbackFailed => "RollbackFailed",
            ApiErrorCode::CleanupFailed => "CleanupFailed",
            ApiErrorCode::OutcomeUnknown => "OutcomeUnknown",
            ApiErrorCode::PlanStale => "PlanStale",
            ApiErrorCode::SourceChanged => "SourceChanged",
            ApiErrorCode::TargetConflictRows => "TargetConflictRows",
            ApiErrorCode::IdempotencyConflict => "IdempotencyConflict",
            ApiErrorCode::Unauthenticated => "Unauthenticated",
            ApiErrorCode::NotFound => "NotFound",
            ApiErrorCode::PayloadTooLarge => "PayloadTooLarge",
            ApiErrorCode::QuotaExceeded => "QuotaExceeded",
            ApiErrorCode::RateLimited => "RateLimited",
            ApiErrorCode::ServiceUnavailable => "ServiceUnavailable",
            ApiErrorCode::ConfigRevisionMismatch => "ConfigRevisionMismatch",
        }
    }

    /// 该码是否表示「请求在派发前被拒绝」。
    ///
    /// 这是 §13 L769 的判据：`true` 意味着**不产生 execution 记录**，
    /// 该路径上不存在 `ExecutionState` / `errorCode` / `effectOutcome`。
    pub const fn is_pre_dispatch_rejection(self) -> bool {
        !matches!(
            self,
            ApiErrorCode::RollbackFailed
                | ApiErrorCode::CleanupFailed
                | ApiErrorCode::OutcomeUnknown
                | ApiErrorCode::TargetConflictRows
                | ApiErrorCode::SourceChanged
                | ApiErrorCode::PlanStale
        )
    }
}

/// 请求拒绝错误。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: ApiErrorCode,
    pub message: String,
}

impl ApiError {
    pub fn new(code: ApiErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for ApiError {}

/// provider 在 §5.1 端口上返回的错误。
///
/// 这一层是**私有**的：端口适配层负责把它翻译为 `ApiError` 或 `ExecutionErrorCode`，
/// 夹具之外的代码不应直接持有它。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    /// 能力缺失或计划不支持 → 派发前拒绝（不产生 execution 记录）。
    #[error("capability unsupported")]
    CapabilityUnsupported,
    #[error("unsupported plan")]
    UnsupportedPlan,
    #[error("target unsupported: {0}")]
    TargetUnsupported(String),
    #[error("target required: {0}")]
    TargetRequired(String),
    #[error("target conflict: {0}")]
    TargetConflict(String),
    /// 预算不足 / 队列已满，可重试。
    #[error("resource busy: {0}")]
    ResourceBusy(&'static str),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("session lost: {0}")]
    SessionLost(String),
    #[error("runtime epoch mismatch: {0}")]
    RuntimeEpochMismatch(String),
    #[error("context conflict: {0}")]
    ContextConflict(String),
    #[error("rollback failed: {0}")]
    RollbackFailed(String),
    #[error("cleanup failed: {0}")]
    CleanupFailed(String),
    /// 结果不确定；`effectOutcome` 必须为 `unknown`。
    #[error("outcome unknown: {0}")]
    OutcomeUnknown(&'static str),
    #[error("protocol error: {0}")]
    ProtocolError(String),
    #[error("sql error: {0}")]
    SqlError(String),
    #[error("timed out: {0}")]
    Timeout(&'static str),
    #[error("resource lost: {0}")]
    ResourceLost(String),
    /// 记录已存在之后的宿主二次校验失败（§13 L776）。
    #[error("host rejected: {0}")]
    HostRejected(String),
}

impl ProviderError {
    /// 派发前拒绝 → `ApiError.code`；派发后的执行期错误 → `ExecutionErrorCode`。
    pub fn api_code(&self) -> Option<ApiErrorCode> {
        Some(match self {
            ProviderError::CapabilityUnsupported => ApiErrorCode::CapabilityUnsupported,
            ProviderError::UnsupportedPlan => ApiErrorCode::UnsupportedPlan,
            ProviderError::TargetUnsupported(_) => ApiErrorCode::TargetUnsupported,
            ProviderError::TargetRequired(_) => ApiErrorCode::TargetRequired,
            ProviderError::TargetConflict(_) => ApiErrorCode::TargetConflict,
            ProviderError::ResourceBusy(_) => ApiErrorCode::ResourceBusy,
            ProviderError::SessionNotFound(_) => ApiErrorCode::SessionNotFound,
            ProviderError::SessionLost(_) => ApiErrorCode::SessionLost,
            ProviderError::RuntimeEpochMismatch(_) => ApiErrorCode::RuntimeEpochMismatch,
            ProviderError::ContextConflict(_) => ApiErrorCode::ContextConflict,
            ProviderError::RollbackFailed(_) => ApiErrorCode::RollbackFailed,
            ProviderError::CleanupFailed(_) => ApiErrorCode::CleanupFailed,
            ProviderError::OutcomeUnknown(_) => ApiErrorCode::OutcomeUnknown,
            ProviderError::HostRejected(_) => ApiErrorCode::InvalidArgument,
            ProviderError::ProtocolError(_)
            | ProviderError::SqlError(_)
            | ProviderError::Timeout(_)
            | ProviderError::ResourceLost(_) => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_error_code_literals_are_distinct() {
        // §13 的错误表是一个平铺集合：任何两个码的线上字面量不得相同，
        // 否则宿主无法区分「请求拒绝」的具体原因。
        let all = [
            ApiErrorCode::InvalidArgument,
            ApiErrorCode::TargetRequired,
            ApiErrorCode::TargetConflict,
            ApiErrorCode::TargetUnsupported,
            ApiErrorCode::SessionNotFound,
            ApiErrorCode::SessionLost,
            ApiErrorCode::RuntimeEpochMismatch,
            ApiErrorCode::ContextConflict,
            ApiErrorCode::PermissionDenied,
            ApiErrorCode::ResourceBusy,
            ApiErrorCode::QueueFull,
            ApiErrorCode::TransactionResolutionRequired,
            ApiErrorCode::CapabilityUnsupported,
            ApiErrorCode::UnsupportedPlan,
            ApiErrorCode::EndpointOverlap,
            ApiErrorCode::SessionQuotaExceeded,
            ApiErrorCode::IdempotencyExpired,
            ApiErrorCode::RollbackFailed,
            ApiErrorCode::CleanupFailed,
            ApiErrorCode::OutcomeUnknown,
            ApiErrorCode::PlanStale,
            ApiErrorCode::SourceChanged,
            ApiErrorCode::TargetConflictRows,
            ApiErrorCode::IdempotencyConflict,
            ApiErrorCode::Unauthenticated,
            ApiErrorCode::NotFound,
            ApiErrorCode::PayloadTooLarge,
            ApiErrorCode::QuotaExceeded,
            ApiErrorCode::RateLimited,
            ApiErrorCode::ServiceUnavailable,
            ApiErrorCode::ConfigRevisionMismatch,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for code in all {
            assert!(
                seen.insert(code.as_str()),
                "重复的错误码字面量: {}",
                code.as_str()
            );
        }
    }

    #[test]
    fn pre_dispatch_codes_never_appear_on_execution_records() {
        // §13 L769：`ApiError.code` 表示请求被拒绝，因此这些码走 ApiError 而非 execution 终态。
        for code in [
            ApiErrorCode::TargetRequired,
            ApiErrorCode::UnsupportedPlan,
            ApiErrorCode::SessionNotFound,
            ApiErrorCode::ResourceBusy,
        ] {
            assert!(
                code.is_pre_dispatch_rejection(),
                "{} 必须是派发前拒绝",
                code.as_str()
            );
        }
    }

    #[test]
    fn provider_errors_never_alias_into_execution_namespace() {
        // `SqlError` / `ProtocolError` / `Timeout` / `ResourceLost` 只能变成 execution 终态码，
        // 绝不能降级成 ApiError —— 否则 §13 L769 的「没有 execution 记录」会与实际执行矛盾。
        for err in [
            ProviderError::SqlError("boom".into()),
            ProviderError::ProtocolError("boom".into()),
            ProviderError::Timeout("acquire"),
            ProviderError::ResourceLost("r1".into()),
        ] {
            assert_eq!(err.api_code(), None, "{err:?} 不得映射到 ApiError 命名空间");
        }
        assert_eq!(
            ProviderError::UnsupportedPlan.api_code(),
            Some(ApiErrorCode::UnsupportedPlan)
        );
    }
}

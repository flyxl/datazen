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
//!
//! ## 第 1 层为什么是再导出
//!
//! `ApiErrorCode` 的唯一定义处在 `platform-api`（§4:234），本模块 `pub use` 它。
//! 此前本 crate 与 `application` 各定义了一份：31 个变体逐字相同**却是两个 Rust 类型**，
//! 既无法互传，也没有任何机制能发现此后二者各自漂移。端口报事实、用层把事实判定成
//! 业务拒绝，两侧共用一份词汇表才是 §13 的要求。
//!
//! `is_pre_dispatch_rejection()` 随类型同住（Rust orphan 规则 E0117：`impl` 必须与被
//! `impl` 的类型同 crate），经再导出后调用形态不变。`ApiError` 与 `ProviderError`
//! 仍定义在本模块——它们是本层独有的载体，不是共享词汇。

use serde::{Deserialize, Serialize};

// 第 1 层的 code 枚举是跨边界共享词汇，唯一定义处在 platform-api（§4:234），
// 这里只做再导出。`is_pre_dispatch_rejection()` 属于类型自身的固有成员，
// 按 Rust orphan 规则（E0117）与类型同住于 platform-api，经此再导出后调用不变。
pub use datazen_platform_api::error::ApiErrorCode;

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

    /// 编译期证明（不是自报）：`runtime::ApiErrorCode` **就是** platform-api 那一个类型，
    /// 而不是本 crate 另有一份同名副本。
    ///
    /// 双向赋值只有在两侧同类型时才编译得过——若本 crate 保留了本地枚举定义，
    /// 下面第二行的类型标注就会立刻 `mismatched types` 编译失败。
    /// application 侧有对称的一条 `packages/application/src/error.rs`，两者都指向
    /// 同一个第三方类型 `datazen_platform_api::error::ApiErrorCode`，因此彼此同类型。
    #[test]
    fn api_error_code_is_the_platform_api_type_not_a_local_copy() {
        let from_reexport: ApiErrorCode = ApiErrorCode::RollbackFailed;
        let from_home: datazen_platform_api::error::ApiErrorCode = from_reexport;
        let round_tripped: ApiErrorCode = from_home;
        assert_eq!(round_tripped, ApiErrorCode::RollbackFailed);
        // 能力随类型同住，经再导出后调用形态不变：执行记录已存在的终态码
        // 不得被误判成派发前拒绝。
        assert!(!ApiErrorCode::RollbackFailed.is_pre_dispatch_rejection());
        assert!(ApiErrorCode::NotFound.is_pre_dispatch_rejection());
    }

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

    #[test]
    fn wire_literal_matches_as_str() {
        // §13 的 `code` 列是 camelCase。这里不手写 31 个期望值，而是把 `as_str()`
        // 与 serde 实际派生的线上字面量逐字比对：任何一侧漂移都会让本测试失败
        // （历史上 `as_str()` 曾返回 PascalCase，与 `#[serde(rename_all)]` 派生的
        // 结果不一致，日志里的码与线上 JSON 的 `code` 会对不上）。
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
        assert_eq!(all.len(), 31, "§13 的 ApiError.code 表是平铺的 31 个码");
        for code in all {
            let wire = serde_json::to_value(code).expect("ApiErrorCode 必须可序列化");
            assert_eq!(
                wire.as_str(),
                Some(code.as_str()),
                "{code:?} 的 Display 字面量与线上字面量不一致"
            );
        }

        // 绝对锚点：上面对比能发现「一侧相对另一侧漂移」，但发现不了两者一起漂移
        // （例如有人把 `rename_all` 改掉）。因此逐字钉住 §13 表中的代表性字面量。
        assert_eq!(ApiErrorCode::InvalidArgument.as_str(), "invalidArgument");
        assert_eq!(
            ApiErrorCode::TransactionResolutionRequired.as_str(),
            "transactionResolutionRequired"
        );
        assert_eq!(
            ApiErrorCode::TargetConflictRows.as_str(),
            "targetConflictRows"
        );
        assert_eq!(
            ApiErrorCode::IdempotencyExpired.as_str(),
            "idempotencyExpired"
        );
        assert_eq!(
            ApiErrorCode::ServiceUnavailable.as_str(),
            "serviceUnavailable"
        );
        assert_eq!(
            ApiErrorCode::ConfigRevisionMismatch.as_str(),
            "configRevisionMismatch"
        );
    }
}

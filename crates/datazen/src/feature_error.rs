//! Errors from portable feature services. Hosts map these to their transport envelope.
//!
//! WR-A3 P3 反向依赖反转：`FeatureError` 只保留基础变体（Validation /
//! NotFound / NotConfigured / Internal / Driver / Io / Json，外加
//! `From<String>` / `From<ApiError>`），不再携带域错误变体——
//! `Ai(#[from] crate::ai::AiError)` 已删除，域错误 → 信封的 `From` 边
//! 全部下沉到各域文件尾部。依赖方向自「信封拉域」（feature_error ⇒
//! ai + table-editing + export）反转为「域指信封」；裸 `feature_error`
//! 编译闭包 = shared（常驻）+ 本模块 + datazen-driver-api + serde_json，
//! 零域符号（任何域类型在本文件无引用）。
//!
//! 域错误 → FeatureError 映射落点（均在域侧文件尾部，`#[cfg(feature =
//! "feature_error")]` 门控；同一 crate 内实现，无孤儿规则问题）：
//! - `crate::table_editing::TableEditingError` → `crates/datazen/src/table_editing/error.rs` 尾部
//! - `crate::ai::application::AiUseCaseError`  → `crates/datazen/src/ai/application/error.rs` 尾部
//! - `crate::export::ExportError`              → `crates/datazen/src/export/error.rs` 尾部
//!
//! 与旧映射的差异清单：仅 `AiUseCaseError::Ai(AiError)` 一臂——旧投
//! `FeatureError::Ai`（`#[error(transparent)]`，文案即 `AiError` 的
//! Display）；反转后信封无 `Ai` 变体，改投 `Internal(e.to_string())`：
//! **类别 Ai→Internal 变化，消息逐字不变**。TableEditingError（7 臂）与
//! ExportError（7 臂）同桶同消息逐字搬移，零类别变化。
//!
//! 主线验证清单（本刀只做 modules 侧反转本体，未跑构建）：
//! 1. `cargo check -p datazen --no-default-features --features feature_error`
//!    （裸信封闭包，确认无域符号残留）；
//! 2. `cargo check -p datazen --no-default-features --features services`
//!    （services/migration_endpoint 走 feature_error）；
//! 3. `cargo check -p datazen --no-default-features --features feature_error,ai,export,table-editing`
//!    （域侧反向 impl 全量入编，无重复定义）；
//! 4. 桌面宿主全量（packages/desktop：feature_error + ai + export +
//!    table-editing 同开；commands/error.rs 的 `From<FeatureError>` 已由
//!    主线对齐为 7 基础变体，AiError 经宿主自有 `From<AiError>` 直达）。
#[derive(Debug, thiserror::Error)]
pub enum FeatureError {
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
impl From<crate::shared::error::ApiError> for FeatureError {
    fn from(error: crate::shared::error::ApiError) -> Self {
        use crate::shared::error::ApiErrorCode;

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

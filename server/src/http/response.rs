//! 跨边界请求拒绝的 HTTP 落地：`ApiError` → 状态码 + JSON 体。
//!
//! wire 类型是 [`datazen::shared::error::ApiError`]——modules 里的唯一定义，
//! IPC 与 HTTP 两绑定共用（同用例、两绑定原则；`serde` camelCase 序列化出
//! 契约的四字段体 `{code, message, requestId, retryDisposition}`，与
//! `@datazen/backend-client` 的 `ApiErrorPayload` 逐字段对齐）。
//!
//! # 状态码表
//!
//! [`status_for`] 落地 team-server §9.2 的映射 + http-api-contract §7 的
//! 两处裁决：
//!
//! - `CapabilityUnsupported` → **405**（`capability` 类：此形态不服务）或
//!   **501**（`not-implemented` 类：尚未实现）。两类的区分是服务端装配
//!   知识，wire 体不携带（backend-client 的 `unsupportedKind` 是客户端
//!   侧区分），所以需要 501 的调用点用 [`ApiErrorResponse::with_status`]
//!   显式覆盖。
//! - §9.2 未列行的 code（`UnsupportedPlan` / `EndpointOverlap` /
//!   `RollbackFailed` / `CleanupFailed` / `OutcomeUnknown` / `PlanStale` /
//!   `SourceChanged` / `TargetConflictRows`）暂按 409 状态冲突类处理——
//!   这是 D1 的**临时分组**，逐行确认是 WR-D4 错误映射的交付内容，
//!   确认后本函数是唯一改动点。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use datazen::shared::error::{ApiError, ApiErrorCode};

/// `ApiError` 的 HTTP 载体。
///
/// 存在的原因是 orphan 规则：`IntoResponse` 是 axum 的 trait、`ApiError`
/// 是 modules 的类型，server 只能用自己的 newtype 实现两者的桥接。
#[derive(Debug, Clone)]
pub struct ApiErrorResponse {
    pub error: ApiError,
    status_override: Option<StatusCode>,
}

impl ApiErrorResponse {
    /// 包一个错误，状态码由 [`status_for`] 从 code 推导。
    pub fn new(error: ApiError) -> Self {
        Self {
            error,
            status_override: None,
        }
    }

    /// 显式覆盖状态码。用于 `CapabilityUnsupported` 的 501 类（见模块文档）
    /// 与 host 层有意降级的场景（如 `ArtifactExpired` → 404，team-server §11）。
    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status_override = Some(status);
        self
    }
}

impl From<ApiError> for ApiErrorResponse {
    fn from(error: ApiError) -> Self {
        Self::new(error)
    }
}

impl IntoResponse for ApiErrorResponse {
    fn into_response(self) -> Response {
        let status = self.status_override.unwrap_or_else(|| status_for(self.error.code));
        (status, Json(self.error)).into_response()
    }
}

/// `ApiErrorCode` → HTTP 状态码（team-server §9.2 + http-api-contract §7）。
pub fn status_for(code: ApiErrorCode) -> StatusCode {
    use ApiErrorCode as C;
    use StatusCode as S;
    match code {
        // 400 参数类
        C::InvalidArgument | C::TargetRequired | C::TargetConflict | C::TargetUnsupported => S::BAD_REQUEST,
        // 401 认证
        C::Unauthenticated => S::UNAUTHORIZED,
        // 403 授权 / CSRF / 出站策略
        C::PermissionDenied => S::FORBIDDEN,
        // 404 不可见 / 不存在 / 二者不可区分（防 ID 枚举）
        C::NotFound => S::NOT_FOUND,
        // 413 物理上限
        C::PayloadTooLarge => S::PAYLOAD_TOO_LARGE,
        // 409 状态冲突类（§9.2 的 409 行）
        C::SessionNotFound
        | C::SessionLost
        | C::RuntimeEpochMismatch
        | C::ContextConflict
        | C::TransactionResolutionRequired
        | C::IdempotencyExpired
        | C::IdempotencyConflict
        | C::ConfigRevisionMismatch => S::CONFLICT,
        // 429 额度 / 速率 / 队列
        C::QuotaExceeded | C::RateLimited | C::ResourceBusy | C::QueueFull | C::SessionQuotaExceeded => {
            S::TOO_MANY_REQUESTS
        }
        // 503 暂不可用（退避重发复用同一 idempotencyKey）
        C::ServiceUnavailable => S::SERVICE_UNAVAILABLE,
        // 405/501（http-api-contract §7）：默认 405；501 类由调用点 with_status
        // 显式覆盖——wire 体不携带 kind，状态码是唯一的传输层区分。
        C::CapabilityUnsupported => S::METHOD_NOT_ALLOWED,
        // §9.2 未列行的 code：D1 临时按 409 状态冲突类，WR-D4 逐行确认。
        C::UnsupportedPlan
        | C::EndpointOverlap
        | C::RollbackFailed
        | C::CleanupFailed
        | C::OutcomeUnknown
        | C::PlanStale
        | C::SourceChanged
        | C::TargetConflictRows => S::CONFLICT,
    }
}

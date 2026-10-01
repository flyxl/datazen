//! 用例层的机器可读失败类型：`ApiError` + `ApiErrorCode` + `RetryDisposition`。
//!
//! 依据：[概要 §6.3](../../../docs/architecture/platform/system-overview.md)「`ApiError` 包含
//! `code`、脱敏 `message`、`requestId`、`retryDisposition`」与
//! [连接 §13](../../../docs/architecture/platform/connection-management.md) 的 code 表。
//!
//! ## 两条必须保持的边界
//!
//! 1. **本类型只表示「请求被拒绝」**。派发后的执行终态失败由
//!    `ExecutionState = failed` + `ExecutionErrorCode` 表达（连接 §13 末段），**不是** `ApiError`。
//!    因此本枚举里没有 `hostRejected`——它是 `ExecutionErrorCode` 的取值，由 platform-api 定义。
//! 2. **`ApiError` 与 `ExecutionErrorCode` 是两个命名空间**，不得互相塞入对方取值。
//! 3. **本模块不做 `PortError` → HTTP 状态码的映射**：端口失败到 HTTP 的映射是 server host
//!    的职责（同 §4.7「`ArtifactExpired` 映射成 404 而不是 410 由 host 决定」）。端口失败到
//!    **用例** 失败的业务判定由各用例自己做，因为 `CasConflict { entity }` 是否等于
//!    `ConfigRevisionMismatch` 取决于 entity，见 [`ApiError::config_revision_mismatch`] 的说明。
//!
//! ## `message` 必须是脱敏文本
//!
//! `message` 是给调用者看的脱敏说明：driver 原始错误、SQL 片段、绝对路径、凭据**不得**进入
//! 这里（连接 §13 末段：driver 内部错误由网关脱敏映射）。构造时只写入本模块或用例自己拼装的
//! 固定文案；把 `&dyn Display` 形式的外部错误直接塞进 `message` 是本模块明确禁止的用法，
//! 需要归因时用 `tracing` 记录，错误体只给 code + 脱敏文案。

use datazen_platform_api::id::RequestId;
use serde::{Deserialize, Serialize};

/// 请求被拒绝的机器可读 code，取值逐字来自连接 §13 的错误表。
///
/// **枚举是封闭的**：新增取值等同协议版本升级，必须同时更新 driver、宿主与前端
/// （连接 §13：「新增取值需要提升枚举版本并同时更新全部消费者，宿主、前端与 driver 不得各自扩展」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApiErrorCode {
    /// DTO/Command schema 不合法。修正请求，不调用 driver。
    InvalidArgument,
    /// 目标缺失。不默认切库。
    TargetRequired,
    /// 目标重复/冲突（含 namespace 内重复表达同一层级不一致）。
    TargetConflict,
    /// 无法定位目标，不回退默认库。
    TargetUnsupported,
    /// 会话不存在。
    SessionNotFound,
    /// 物理会话已失效；禁止透明重建，显式新建会话必须使用新 ID。
    SessionLost,
    /// 旧运行时句柄：runtimeEpoch/dbSessionId 不匹配。
    RuntimeEpochMismatch,
    /// 上下文版本冲突，重新读取后由用户决定。
    ContextConflict,
    /// 当前授权拒绝；敏感资源由 host 映射为 404。
    PermissionDenied,
    /// 预算/队列超限。
    ResourceBusy,
    /// 队列超限。
    QueueFull,
    /// 事务阻止切换/关闭，需用户选择处理事务。
    TransactionResolutionRequired,
    /// 缺少所需能力。
    CapabilityUnsupported,
    /// 无法证明事务边界。
    UnsupportedPlan,
    /// 源目标对象危险重叠或无法排除自覆盖。
    EndpointOverlap,
    /// 逻辑会话/编辑器额度超限。
    SessionQuotaExceeded,
    /// 幂等键已过提交有效期：查询原执行并核验，不自动用新键重投。
    IdempotencyExpired,
    /// 无法证明清理（回滚失败）。
    RollbackFailed,
    /// 无法证明清理。
    CleanupFailed,
    /// 写入/提交结果未知：核验执行，不自动重试。
    OutcomeUnknown,
    /// 计划已过期。
    PlanStale,
    /// 源数据已变化。
    SourceChanged,
    /// 目标行冲突。
    TargetConflictRows,
    /// 相同幂等键不同输入：修正请求键，不能覆盖记录。
    IdempotencyConflict,
    /// 未认证，不自动重放原请求。
    Unauthenticated,
    /// 资源不可见或不存在，或二者不可区分。
    NotFound,
    /// 请求体超过物理上限。
    PayloadTooLarge,
    /// 组织/用户/数据库额度超限。
    QuotaExceeded,
    /// 令牌桶限流拒绝。
    RateLimited,
    /// 暂无可用 worker、drain 中、管理库不可达或迁移未完成。
    ServiceUnavailable,
    /// `ProfileRepository::compare_and_set` 的 expectedRevision CAS 失败。
    ///
    /// 与 `TargetConflict` 是两件事：CAS 失败说明**命名空间目标本身合法**（连接 §13 末行），
    /// 调用方要重读最新 configRevision 后由用户决定，不得自动覆盖。
    ConfigRevisionMismatch,
}

impl ApiErrorCode {
    /// 协议字面值，序列化后与连接 §13 的 code 列逐字一致。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidArgument => "invalidArgument",
            Self::TargetRequired => "targetRequired",
            Self::TargetConflict => "targetConflict",
            Self::TargetUnsupported => "targetUnsupported",
            Self::SessionNotFound => "sessionNotFound",
            Self::SessionLost => "sessionLost",
            Self::RuntimeEpochMismatch => "runtimeEpochMismatch",
            Self::ContextConflict => "contextConflict",
            Self::PermissionDenied => "permissionDenied",
            Self::ResourceBusy => "resourceBusy",
            Self::QueueFull => "queueFull",
            Self::TransactionResolutionRequired => "transactionResolutionRequired",
            Self::CapabilityUnsupported => "capabilityUnsupported",
            Self::UnsupportedPlan => "unsupportedPlan",
            Self::EndpointOverlap => "endpointOverlap",
            Self::SessionQuotaExceeded => "sessionQuotaExceeded",
            Self::IdempotencyExpired => "idempotencyExpired",
            Self::RollbackFailed => "rollbackFailed",
            Self::CleanupFailed => "cleanupFailed",
            Self::OutcomeUnknown => "outcomeUnknown",
            Self::PlanStale => "planStale",
            Self::SourceChanged => "sourceChanged",
            Self::TargetConflictRows => "targetConflictRows",
            Self::IdempotencyConflict => "idempotencyConflict",
            Self::Unauthenticated => "unauthenticated",
            Self::NotFound => "notFound",
            Self::PayloadTooLarge => "payloadTooLarge",
            Self::QuotaExceeded => "quotaExceeded",
            Self::RateLimited => "rateLimited",
            Self::ServiceUnavailable => "serviceUnavailable",
            Self::ConfigRevisionMismatch => "configRevisionMismatch",
        }
    }

    /// 全部取值的清单，host/前端做反查与审计时使用。
    pub const ALL: [ApiErrorCode; 31] = [
        Self::InvalidArgument,
        Self::TargetRequired,
        Self::TargetConflict,
        Self::TargetUnsupported,
        Self::SessionNotFound,
        Self::SessionLost,
        Self::RuntimeEpochMismatch,
        Self::ContextConflict,
        Self::PermissionDenied,
        Self::ResourceBusy,
        Self::QueueFull,
        Self::TransactionResolutionRequired,
        Self::CapabilityUnsupported,
        Self::UnsupportedPlan,
        Self::EndpointOverlap,
        Self::SessionQuotaExceeded,
        Self::IdempotencyExpired,
        Self::RollbackFailed,
        Self::CleanupFailed,
        Self::OutcomeUnknown,
        Self::PlanStale,
        Self::SourceChanged,
        Self::TargetConflictRows,
        Self::IdempotencyConflict,
        Self::Unauthenticated,
        Self::NotFound,
        Self::PayloadTooLarge,
        Self::QuotaExceeded,
        Self::RateLimited,
        Self::ServiceUnavailable,
        Self::ConfigRevisionMismatch,
    ];

    /// 机器可读的重试政策（概要 §6.3）。
    ///
    /// * [`RetryDisposition::Never`]：权限、参数、能力、上下文/版本冲突、预算与速率超限。
    /// * [`RetryDisposition::SafeRead`]：确认无副作用的读取结果。
    /// * [`RetryDisposition::CheckExecution`]：已接受但结果未知的提交，先核验执行。
    ///
    /// **已知规格缺口（P1 不发明取值）**：连接 §13 对 `ResourceBusy` / `QueueFull` /
    /// `RateLimited` / `ServiceUnavailable` / `QuotaExceeded` / `SessionQuotaExceeded`
    /// 给的动作是「等待或按 Retry-After 退避后重发」，语义上既不是 `never`，
    /// 也不是「确认无副作用的读取」，但概要 §6.3 只定义了三个取值。P1 对这六个 code
    /// 保守取 `Never`（宁可不自动重试，也不要让调用方把退避重发误当作安全读），
    /// 是否补第四个取值（例如带退避的 `retryAfterBackoff`）需要概要先给出枚举。
    pub const fn retry_disposition(self) -> RetryDisposition {
        match self {
            // 「查询原执行并核验，不自动用新键重投」（idempotencyExpired）
            // 与「核验执行，不自动重试」（outcomeUnknown）同属一类。
            Self::OutcomeUnknown | Self::IdempotencyExpired => RetryDisposition::CheckExecution,
            Self::NotFound | Self::SessionNotFound => RetryDisposition::SafeRead,
            Self::InvalidArgument
            | Self::TargetRequired
            | Self::TargetConflict
            | Self::TargetUnsupported
            | Self::SessionLost
            | Self::RuntimeEpochMismatch
            | Self::ContextConflict
            | Self::PermissionDenied
            | Self::ResourceBusy
            | Self::QueueFull
            | Self::TransactionResolutionRequired
            | Self::CapabilityUnsupported
            | Self::UnsupportedPlan
            | Self::EndpointOverlap
            | Self::SessionQuotaExceeded
            | Self::RollbackFailed
            | Self::CleanupFailed
            | Self::PlanStale
            | Self::SourceChanged
            | Self::TargetConflictRows
            | Self::IdempotencyConflict
            | Self::Unauthenticated
            | Self::PayloadTooLarge
            | Self::QuotaExceeded
            | Self::RateLimited
            | Self::ServiceUnavailable
            | Self::ConfigRevisionMismatch => RetryDisposition::Never,
        }
    }
}

impl std::fmt::Display for ApiErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 机器可读的重试政策，取值与 wire 字面值一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RetryDisposition {
    /// 不重试：先修复请求或刷新状态。
    Never,
    /// 只允许重试确认无副作用的读取。
    SafeRead,
    /// 先核验已接受的执行/提交，不盲目重发。
    CheckExecution,
}

/// 用例层的失败类型。跨边界传输时 `code` / `requestId` / `retryDisposition` 全部必填，
/// `message` 是**脱敏**后的给调用者文案。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    /// 机器可读 code（连接 §13）。
    pub code: ApiErrorCode,
    /// 脱敏文案：不得包含 driver 原始错误、SQL 片段、绝对路径或凭据。
    pub message: String,
    /// 日志关联 id，来自 adapter 构造的 `RequestContext`；便于调用方报障时对齐服务端日志。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<RequestId>,
    /// 机器可读重试政策。
    pub retry_disposition: RetryDisposition,
}

impl ApiError {
    /// 构造一个不带 requestId 的错误。
    pub fn new(code: ApiErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            request_id: None,
            retry_disposition: code.retry_disposition(),
        }
    }

    /// 挂上 `RequestContext.request_id`，供 host 写回响应头 / 事件。
    pub fn with_request_id(mut self, request_id: Option<RequestId>) -> Self {
        self.request_id = request_id;
        self
    }

    /// 构造 DTO/Command schema 不合法。
    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::new(ApiErrorCode::InvalidArgument, message)
    }

    /// 构造目标缺失：无法确认时必须返回本错误，不得猜测 schema（例如把未指定的 schema
    /// 猜成 public，连接 §4.3）。
    pub fn target_required(message: impl Into<String>) -> Self {
        Self::new(ApiErrorCode::TargetRequired, message)
    }

    /// 构造目标冲突：同一层级被重复表达且不一致，或 namespace 内别名规范化后不相等。
    pub fn target_conflict(message: impl Into<String>) -> Self {
        Self::new(ApiErrorCode::TargetConflict, message)
    }

    /// 构造授权拒绝。host 必须把「不可见资源」的 `PermissionDenied` 映射为 404，避免 ID 枚举
    /// （概要 §6.3），本模块不做这层映射。
    pub fn permission_denied(message: impl Into<String>) -> Self {
        Self::new(ApiErrorCode::PermissionDenied, message)
    }

    /// 构造 CAS 失败（profile configRevision）。
    pub fn config_revision_mismatch(message: impl Into<String>) -> Self {
        Self::new(ApiErrorCode::ConfigRevisionMismatch, message)
    }

    /// 是否为「不可见 / 不存在不可区分」的读取失败。
    pub fn is_not_found(&self) -> bool {
        matches!(
            self.code,
            ApiErrorCode::NotFound | ApiErrorCode::SessionNotFound
        )
    }

    /// 是否允许调用方在不核验执行的前提下重试同一请求。
    pub fn may_retry_without_checking_execution(&self) -> bool {
        self.retry_disposition == RetryDisposition::SafeRead
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn code_only(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn wire_literals_match_the_spec_table() {
        assert_eq!(
            ApiErrorCode::ConfigRevisionMismatch.as_str(),
            "configRevisionMismatch"
        );
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
    }

    #[test]
    fn all_is_complete_and_unique() {
        assert_eq!(ApiErrorCode::ALL.len(), 31);
        let mut literals: Vec<&str> = ApiErrorCode::ALL.iter().map(|c| c.as_str()).collect();
        literals.sort_unstable();
        let before = literals.len();
        literals.dedup();
        assert_eq!(before, literals.len(), "code 字面值必须互不相同");
    }

    #[test]
    fn retry_disposition_follows_the_spec_actions() {
        // 概要 §6.3：safeRead 仅用于确认无副作用的读取。
        assert_eq!(
            ApiErrorCode::NotFound.retry_disposition(),
            RetryDisposition::SafeRead
        );
        assert_eq!(
            ApiErrorCode::SessionNotFound.retry_disposition(),
            RetryDisposition::SafeRead
        );
        // 连接 §13：outcomeUnknown/idempotencyExpired 都是「核验执行，不自动重发」。
        assert_eq!(
            ApiErrorCode::OutcomeUnknown.retry_disposition(),
            RetryDisposition::CheckExecution
        );
        assert_eq!(
            ApiErrorCode::IdempotencyExpired.retry_disposition(),
            RetryDisposition::CheckExecution
        );
        // 权限/参数/能力/上下文冲突 → 先修复或刷新。
        for code in [
            ApiErrorCode::PermissionDenied,
            ApiErrorCode::InvalidArgument,
            ApiErrorCode::CapabilityUnsupported,
            ApiErrorCode::ContextConflict,
            ApiErrorCode::ConfigRevisionMismatch,
        ] {
            assert_eq!(code.retry_disposition(), RetryDisposition::Never, "{code}");
        }
    }

    #[test]
    fn every_code_has_a_disposition() {
        for code in ApiErrorCode::ALL {
            let _ = code.retry_disposition();
        }
        assert_eq!(ApiErrorCode::ALL.len(), 31);
    }

    #[test]
    fn serialized_shape_has_camel_case_keys() {
        let error = ApiError::new(
            ApiErrorCode::ConfigRevisionMismatch,
            "config revision mismatch",
        )
        .with_request_id(Some(RequestId::new("req-1")));
        let json = serde_json::to_value(&error).expect("serialize");
        assert_eq!(json["code"], "configRevisionMismatch");
        assert_eq!(json["message"], "config revision mismatch");
        assert_eq!(json["requestId"], "req-1");
        assert_eq!(json["retryDisposition"], "never");
    }

    #[test]
    fn api_error_round_trips_through_serde() {
        for code in ApiErrorCode::ALL {
            let error = ApiError::new(code, "sanitized")
                .with_request_id(Some(RequestId::new("req-round-trip")));
            let encoded = serde_json::to_string(&error).expect("serialize");
            let decoded: ApiError = serde_json::from_str(&encoded).expect("deserialize");
            assert_eq!(decoded, error, "code {}", code.as_str());
            assert_eq!(decoded.retry_disposition, code.retry_disposition());
        }
    }

    #[test]
    fn request_id_is_optional_on_the_wire() {
        let error = ApiError::new(ApiErrorCode::Unauthenticated, "no session");
        let json = serde_json::to_value(&error).expect("serialize");
        assert!(json.get("requestId").is_none());
        let decoded: ApiError = serde_json::from_value(json).expect("deserialize");
        assert_eq!(decoded, error);
    }

    #[test]
    fn execution_error_codes_do_not_leak_into_this_enum() {
        // 连接 §13：执行终态 errorCode 与 ApiError.code 是两个命名空间；
        // hostRejected 是 ExecutionErrorCode 的取值，绝不能出现在 ApiErrorCode。
        let code = code_only(
            include_str!("error.rs")
                .split("#[cfg(test)]")
                .next()
                .unwrap_or_default(),
        );
        assert!(
            !code.contains("HostRejected"),
            "ApiErrorCode 不得包含 hostRejected"
        );
        assert!(!code.contains("SqlError"));
        assert!(
            !code.contains("Cancelled"),
            "cancelled 属于 ExecutionErrorCode"
        );
    }

    #[test]
    fn not_found_predicate_never_reports_write_rejections() {
        assert!(ApiError::new(ApiErrorCode::NotFound, "x").is_not_found());
        assert!(ApiError::new(ApiErrorCode::SessionNotFound, "x").is_not_found());
        assert!(!ApiError::new(ApiErrorCode::PermissionDenied, "x").is_not_found());
        assert!(!ApiError::new(ApiErrorCode::TargetConflict, "x").is_not_found());
    }

    #[test]
    fn retry_helpers_agree_with_the_disposition() {
        let unknown = ApiError::new(ApiErrorCode::OutcomeUnknown, "unknown");
        assert!(!unknown.may_retry_without_checking_execution());
        let missing = ApiError::new(ApiErrorCode::NotFound, "missing");
        assert!(missing.may_retry_without_checking_execution());
    }

    #[test]
    fn display_is_code_then_message() {
        let error = ApiError::new(ApiErrorCode::SessionLost, "physical session lost");
        assert_eq!(error.to_string(), "sessionLost: physical session lost");
    }
}

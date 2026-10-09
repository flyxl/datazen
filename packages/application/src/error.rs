//! 用例层的机器可读失败类型：`ApiError`，以及从 platform-api 再导出的
//! [`ApiErrorCode`] 与 [`RetryDisposition`]。
//!
//! 依据：[概要 §6.3](../../../docs/architecture/platform/system-overview.md)「`ApiError` 包含
//! `code`、脱敏 `message`、`requestId`、`retryDisposition`」与
//! [连接 §13](../../../docs/architecture/platform/connection-management.md) 的 code 表。
//!
//! ## 为什么 code 与 disposition 不在本模块定义
//!
//! `ApiErrorCode`（31 个 code）与 `RetryDisposition` 是**跨边界的 wire 词汇**：
//! `runtime` 的 `ApiError` 用的是同一份 code 枚举。此前两边各定义一份同名同形的枚举，
//! 变体、顺序与字面值逐字相同**却是两个 Rust 类型**——既无法互传，也没有任何机制能
//! 发现二者此后各自漂移。§4:234 定的方向是「唯一定义处放在 `platform-api`，
//! `application` 与 `runtime` 用 `pub use` 再导出，不重复定义」；反向依赖不可行，
//! 因为 platform-api 受 F-04 约束不得依赖本包。
//!
//! 随类型一同住在 platform-api 的还有它**自身的能力**：`ApiErrorCode::ALL`、
//! `retry_disposition()`、`as_str()` 与 `Display`。这不是能力搬迁——经再导出后本 crate
//! 内逐字照旧可用（`ApiErrorCode::ALL`、`code.retry_disposition()`、`{code}` 格式化、
//! 本模块 `ApiError` 的 serde 形态全部不变）。这是 Rust orphan 规则（E0117）决定的唯一
//! 合法形态：`impl` 必须与被 impl 的类型同 crate，因此这些成员无法留在本模块。
//!
//! ## 两条必须保持的边界
//!
//! 1. **本类型只表示「请求被拒绝」**。派发后的执行终态失败由
//!    `ExecutionState = failed` + `ExecutionErrorCode` 表达（连接 §13 末段），**不是** `ApiError`。
//!    因此 `ApiErrorCode` 里没有 `hostRejected`——它是 `ExecutionErrorCode` 的取值，
//!    由 platform-api 定义并在其内部以 `execution_error_codes_do_not_leak_into_this_enum` 钉住。
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

// 错误码枚举与重试政策是跨边界的 wire 词汇，唯一定义处在 platform-api（§4:234）：
// 端口报事实、用层把事实判定成业务拒绝，两侧共用同一份词汇表而不是各写一份。
// 这里只做再导出，因此本 crate 的调用方逐字照旧使用同一批名字与同样的能力
// （`ApiErrorCode::ALL`、`code.retry_disposition()`、`as_str()`、`Display`，以及本模块
// `ApiError` 的 serde 形态，全部不变）。
//
// 这些成员「随类型同住」是由 Rust orphan 规则（E0117）决定的，不是搬迁偏好：
// `impl` 必须与被 impl 的类型同 crate，因此无法留在本模块。理由见模块 doc。
pub use datazen_platform_api::error::{ApiErrorCode, RetryDisposition};

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

    /// 编译期证明（不是自报）：`application::ApiErrorCode` **就是** platform-api 那一个类型，
    /// 而不是本 crate 另有一份同名副本。
    ///
    /// 双向赋值只有在两侧同类型时才编译得过——若本 crate 保留了本地枚举定义，
    /// 下面第二行的类型标注就会立刻 `mismatched types` 编译失败。
    /// runtime 侧有对称的一条 `runtime/src/connection/error.rs`，两者都指向同一个
    /// 第三方类型 `datazen_platform_api::error::ApiErrorCode`，因此彼此同类型。
    #[test]
    fn api_error_code_is_the_platform_api_type_not_a_local_copy() {
        let from_reexport: ApiErrorCode = ApiErrorCode::NotFound;
        let from_home: datazen_platform_api::error::ApiErrorCode = from_reexport;
        let round_tripped: ApiErrorCode = from_home;
        assert_eq!(round_tripped, ApiErrorCode::NotFound);
        // 能力也必须随之可见：`ALL` 与 `retry_disposition()` 是这个类型的固有成员，
        // 经再导出调用，证明调用方不需要改写任何一行。
        assert_eq!(ApiErrorCode::ALL.len(), 31);
        assert_eq!(
            ApiErrorCode::NotFound.retry_disposition(),
            RetryDisposition::SafeRead
        );
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

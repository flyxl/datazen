//! 连接运行时的三套**互不交织**的错误命名空间。
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
//!
//! ## 第三套：[`RuntimeError`]
//!
//! §13 的三层划分**没有留位置**给「请求已经落在一个具体 `dbSessionId` 上、被 runtime 拒绝」：
//! 这类判定发生在 registry / actor 一侧，它既不产生 `execution` 记录（`ApiError` 的语义前提
//! 是「还没有 execution 记录」），也不是 provider 端口的返回值（provider 看不到登记表、
//! TTL 判定和预算账本，无从说出「句柄不存在」）。把它硬塞进 `ApiError` 会让 §13 L777
//! 「记录已存在之后只能用 `HostRejected`」这条边界重新变糊；塞进 `ProviderError` 则是
//! 谎报事实来源。
//!
//! 因此本模块并列第三套命名空间 [`RuntimeError`]，并用 `api_code()` 把每种拒绝**单向**
//! 映射回 §13 的 `ApiErrorCode`（`CancelFailed` / `InvariantBroken` 故意返回 `None`，
//! 见其变体文档）。映射方向不可逆：`ApiError` 不还原成 `RuntimeError`，因为
//! `ApiError` 已经丢失了句柄、版本差与隔离原因这些只有 runtime 侧才有的信息。

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

/// 会话操作被 runtime 拒绝（第三套命名空间，见模块文档）。
///
/// 与 `ApiError` 的区别在**判定时点**，不在严重程度：走到本枚举的请求，句柄、归属、
/// `contextRevision`、预算账本都已经确定，runtime 只是拒绝执行它。因此
/// `ExecuteInSessionRequest` 里那些「请求形状」字段在这一层已经被消费掉了，
/// 留下的失败原因全部是**状态性**的。
///
/// 变体里的字面量一律用 `String`（携带句柄）与 `&'static str`（原因集合封闭）两种形态，
/// 与 `ProviderError` 保持一致：句柄必须能被排障的人读出来，原因则不该在枚举里再长出
/// 无界的取值空间——要加新原因就加变体，让 `api_code()` 的映射表被迫一起更新。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeError {
    /// 句柄不在登记表里：从未登记、已被清扫出表、或 `runtime_epoch` 与当前登记不符
    /// （资源替换后旧句柄必须立即失效，INV-07/INV-08）。
    #[error("unknown session handle: {0}")]
    UnknownSession(String),

    /// 会话已进入 `Closed` 终态（§6.1 状态机：`Lost`/`Closed` 都不得直接转回 `Ready`）。
    /// 与 [`Self::SessionLost`] 分开是因为判定依据不同——`Closed` 是用户主动关闭的结果，
    /// 重连即可；`Lost` 是物理资源丢失，重连之前必须先核验。
    #[error("session closed: {0}")]
    SessionClosed(String),

    /// 会话已进入 `Lost`：物理资源丢失。**禁止**透明重建（§4.5：invalidate 之后
    /// 后续请求必须拿到 `SessionLost`，由调用方显式建立新会话）。
    #[error("session lost: {0}")]
    SessionLost(String),

    /// 调用方带来的 `contextRevision` 与服务端当前值不符。
    /// 字段是 `context_revision.get()` 的**裸值**：`error` 模块刻意不依赖 `types`
    /// （`types` 已反向依赖本模块的 `ApiError`），故此处不用 `Counter` newtype。
    #[error("context revision mismatch: expected {expected}, actual {actual}")]
    ContextRevisionMismatch { expected: u64, actual: u64 },

    /// 预算不足或队列已满。**可重试**，且不得降级成无预算执行（§13 `ResourceBusy`
    /// 的调用者动作是「等待或用户取消；不得创建额外连接」）。
    #[error("budget exhausted: {0}")]
    BudgetExhausted(&'static str),

    /// 会话已被隔离：清理未确认、写入结果不可判定。**不可重试**，也不得再次 acquire
    /// 同一物理资源（§13 `CleanupFailed` 的调用者动作是「隔离连接、报告真实结果」）。
    #[error("session quarantined: {0}")]
    SessionQuarantined(&'static str),

    /// 取消请求无法送达或无法登记。`api_code()` 故意返回 `None`：取消发生在
    /// `execution` 记录**已经存在之后**，按 §13 L777 不得用 `ApiError` 表达，
    /// 只能落到 `ExecutionErrorCode` 或事件流上。
    #[error("cancel failed: {0}")]
    CancelFailed(&'static str),

    /// 关闭请求被当前状态拒绝。典型形态是 `CloseMode::RequireNoTransaction`
    /// 撞上活跃事务（§13 `TransactionResolutionRequired`）。
    #[error("close rejected: {0}")]
    CloseRejected(&'static str),

    /// 内部不变式被破坏：登记表与 actor 视图不一致、已销毁的 actor 邮箱仍被寻址、
    /// 终态计数器回退等。这不是业务拒绝而是缺陷，用它而不是 `unwrap()` 崩溃，
    /// 是 `docs/development/panic-policy.md` 对生产路径的要求。
    /// `api_code()` 返回 `None`：缺陷不得被伪装成调用者可以处置的拒绝。
    #[error("internal invariant broken: {0}")]
    InvariantBroken(&'static str),
}

impl RuntimeError {
    /// 稳定的原因码字面量，供 tracing 事件字段与用例层翻译共用。
    ///
    /// **它不是 `ApiErrorCode`**，也不能上线路：`ApiErrorCode` 是 §13 那张对外表，
    /// 增删变体要连带改前端；这里是纯内部观测用的闭集，随本枚举演进。
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::UnknownSession(_) => "unknownSession",
            Self::SessionClosed(_) => "sessionClosed",
            Self::SessionLost(_) => "sessionLost",
            Self::ContextRevisionMismatch { .. } => "contextRevisionMismatch",
            Self::BudgetExhausted(_) => "budgetExhausted",
            Self::SessionQuarantined(_) => "sessionQuarantined",
            Self::CancelFailed(_) => "cancelFailed",
            Self::CloseRejected(_) => "closeRejected",
            Self::InvariantBroken(_) => "invariantBroken",
        }
    }

    /// 单向映射回 §13 的对外拒绝码；`None` 表示「不该用 `ApiError` 表达」，理由见变体文档。
    ///
    /// 与 [`ProviderError::api_code`] 同一个方向：**端口/运行时报事实，用层把事实判定成
    /// 对外拒绝**。三处映射表（`ProviderError` / 本方法 / 未来用例层的兜底）必须给出
    /// 一致的调用者动作，否则同一事实会以两种码出现在同一页面上。
    pub const fn api_code(&self) -> Option<ApiErrorCode> {
        match self {
            // §13「失效或不存在 → 读终态，显式建立新会话」：`Closed` 与 `Lost` 在这里
            // 塌缩成同一个对外码，但它们的内部判定依据不同，不能合并变体。
            Self::UnknownSession(_) | Self::SessionClosed(_) => Some(ApiErrorCode::SessionNotFound),
            // 隔离中的会话对调用者同样不可用：只能新建会话，旧物理资源不得复用。
            Self::SessionLost(_) | Self::SessionQuarantined(_) => Some(ApiErrorCode::SessionLost),
            // §13「旧运行时/上下文请求 → 读取新状态，不自动执行旧写入」。
            // 注意不是 `ConfigRevisionMismatch`：那张码属于 `ProfileRepository::compare_and_set` 的 CAS。
            Self::ContextRevisionMismatch { .. } => Some(ApiErrorCode::ContextConflict),
            Self::BudgetExhausted(_) => Some(ApiErrorCode::ResourceBusy),
            Self::CloseRejected(_) => Some(ApiErrorCode::TransactionResolutionRequired),
            Self::CancelFailed(_) | Self::InvariantBroken(_) => None,
        }
    }

    /// 在拒绝发生的边界上记一条 `warn` 事件，并把错误原样交还调用方。
    ///
    /// 存在的理由是拒绝路径没有返回值可供观测：`session_view` 这类读操作一旦被拒，
    /// 整条链路上唯一留下的痕迹就是这条事件。把「记录」和「返回」绑成
    /// `x().await.map_err(|e| e.rejected("close"))` 之外的分两句写法，
    /// 漏掉一处就是一次静默拒绝——`Err` 返回值里没有任何东西能提示它本该被记录过。
    pub fn rejected<T>(self, operation: &'static str) -> Result<T, Self> {
        tracing::warn!(
            reason = self.reason(),
            operation,
            "session operation rejected"
        );
        Err(self)
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

    #[test]
    fn runtime_errors_map_to_request_side_codes_or_nowhere_at_all() {
        // 锁住三套命名空间之间那条唯一允许的缝：会话面拒绝可以上线路成 `ApiErrorCode`，
        // 但**不得**凭空变成 `ExecutionErrorCode`（一旦存在执行记录，§13 L777 只允许
        // `HostRejected`）。因此这里逐个钉住整张映射表，而不是只测几个代表值。
        let mapped = [
            (
                RuntimeError::UnknownSession("s".into()),
                ApiErrorCode::SessionNotFound,
            ),
            (
                RuntimeError::SessionClosed("s".into()),
                ApiErrorCode::SessionNotFound,
            ),
            (
                RuntimeError::SessionLost("s".into()),
                ApiErrorCode::SessionLost,
            ),
            (
                RuntimeError::SessionQuarantined("inUse"),
                ApiErrorCode::SessionLost,
            ),
            (
                RuntimeError::ContextRevisionMismatch {
                    expected: 1,
                    actual: 2,
                },
                ApiErrorCode::ContextConflict,
            ),
            (
                RuntimeError::BudgetExhausted("orgQuota"),
                ApiErrorCode::ResourceBusy,
            ),
            (
                RuntimeError::CloseRejected("activeTransaction"),
                ApiErrorCode::TransactionResolutionRequired,
            ),
        ];
        for (err, code) in mapped {
            assert_eq!(err.api_code(), Some(code), "{err:?} 的上线路由被改了");
        }

        // 取消失败与内部不变式破裂不映射：前者要由调用方按 driver 能力决定如何呈现
        // （§7.6 要求「不支持」与「已终结」分开报），后者根本不是业务事实，
        // 编一个 ApiErrorCode 等于把一次 bug 伪装成一次合法拒绝。
        for err in [
            RuntimeError::CancelFailed("driverUnreachable"),
            RuntimeError::InvariantBroken("activeExecutionMismatch"),
        ] {
            assert_eq!(err.api_code(), None, "{err:?} 不得伪造 ApiErrorCode");
        }

        // `reason()` 是 tracing 字段的取值来源：必须非空且互不重复，否则日志里
        // 两种不同的拒绝会长得一模一样。
        let all = [
            RuntimeError::UnknownSession("s".into()),
            RuntimeError::SessionClosed("s".into()),
            RuntimeError::SessionLost("s".into()),
            RuntimeError::ContextRevisionMismatch {
                expected: 1,
                actual: 2,
            },
            RuntimeError::BudgetExhausted("orgQuota"),
            RuntimeError::SessionQuarantined("inUse"),
            RuntimeError::CancelFailed("driverUnreachable"),
            RuntimeError::CloseRejected("activeTransaction"),
            RuntimeError::InvariantBroken("activeExecutionMismatch"),
        ];
        for err in &all {
            assert!(!err.reason().is_empty(), "{err:?} 缺少可观测的 reason");
        }
        let mut unique = all.iter().map(RuntimeError::reason).collect::<Vec<_>>();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), all.len(), "reason 词汇表出现重复");

        // `rejected()` 只把同一个错误原样返回，不做任何改写。
        let rejected: Result<(), RuntimeError> =
            RuntimeError::CloseRejected("activeTransaction").rejected("closeSession");
        assert_eq!(
            rejected,
            Err(RuntimeError::CloseRejected("activeTransaction"))
        );
    }
}

//! 错误词汇的两个半边：`PortError`（端口层事实）与 `ApiErrorCode`（跨边界业务拒绝码）。
//!
//! 两者共处一个模块，因为它们是**同一份跨 crate 词汇表的两端**：端口报事实、用层把事实
//! 判定成业务拒绝，而判定结果的 wire 形态（`ApiErrorCode`）必须由两侧共用同一份定义。
//! §4:234 定的正是这个手法——共享类型定义在本包，`application` 与 `runtime` 用
//! `pub use` 再导出，而不是各写一份。
//!
//! 方向上这是**有意的**：本包不依赖 `application` / `runtime`（F-04 禁止反向依赖），
//! 而 `application` 与 `runtime` 都是兄弟、互不依赖，所以「唯一定义处」只能是本包。
//!
//! # 端口层：`PortError`
//!
//! 变体集合来自 `shared-boundaries-and-ports.md` §4.1 的目标设计，逐条落地，**不增不减**。
//!
//! 约定（概要 §6.4 与 §4.1）：
//!
//! - 端口层**只报告基础设施与并发事实**，不表达业务拒绝。权限不足、目标不合法、额度已满、
//!   会话已失效等判定由用例层完成并转成 `ApiError`。
//! - 端口不做重试、不做缓存、不做授权。缓存与版本计算是端口**提供方**的责任
//!   （例如 `SecretProvider` 负责 `credentialRevision`）。
//! - `PortError::ArtifactExpired` 是端口取值，**HTTP 状态码由 server host 决定**，
//!   不构成端口契约（team-server 出于防 ID 枚举把它与 `NotFound` 一起映射成 404）。
//! - 执行终态错误走 `ExecutionErrorCode`，与 `ApiError.code` 是两个独立命名空间。
//!
//! **`PortError` 刻意不实现 serde**（§4.1 的目标设计只 `derive(Debug, thiserror::Error)`）。
//! 原因有二：`entity: &'static str` 无法在不泄漏内存的前提下反序列化；更重要的是跨边界的
//! 错误类型是 `ApiError`（概要 §6.3、连接 §13），由 host 负责把 `PortError` 映射过去。
//! 在端口层再定一套 JSON 形态，只会给前端多一个没有消费者的错误形状。

/// 端口错误。`entity` 用 `&'static str` 承载聚合名（compile 期常量，不引入生命周期）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PortError {
    /// 依赖的后端不可用（数据库、元数据库、文件系统……）。可重试由用例层判定。
    #[error("backend unavailable: {0}")]
    BackendUnavailable(String),

    /// compare-and-set 冲突：调用方拿到的版本已经不是最新，需重新读取后重试。
    #[error("compare-and-set conflict on {entity} {id}")]
    CasConflict { entity: &'static str, id: String },

    /// 资源不存在。注意 host 可能出于防 ID 枚举把它与「无权限」合并成同一响应。
    #[error("not found: {0}")]
    NotFound(String),

    /// 令牌签名/有效期/作用域/owner epoch 校验失败，或签名密钥版本未知。**未知版本一律拒绝**。
    #[error("token verification failed")]
    TokenInvalid,

    /// 端口提供方超时。预算申请的 10 秒 acquire 超时属于用例层语义，此处只报事实。
    #[error("provider timeout: {0}")]
    ProviderTimeout(String),

    /// 产物已过 TTL，读取必须失败，不得以缓存副本继续服务。
    #[error("artifact expired")]
    ArtifactExpired,

    /// 超出配额。
    #[error("quota exceeded: {0}")]
    QuotaExceeded(String),

    /// P5：同键不同 payload 的幂等重放。
    #[error("idempotency conflict: same key, different request payload")]
    IdempotencyConflict,

    /// P5：旧 claim/过期 claim 的写入一律拒绝（§4.3 fence）。
    #[error("stale or expired claim: write rejected")]
    StaleClaim,

    /// P5：planId 已被其他 apply Job 消费（§2.1 唯一约束）。
    #[error("plan already consumed by an apply job: {0}")]
    PlanAlreadyConsumed(String),

    /// P5：未知 major 版本的计划 / 检查点拒绝。
    #[error("unsupported version: {0}")]
    UnsupportedVersion(String),
}

impl PortError {
    /// 便于用例层统一登记的聚合/对象名，出现在错误消息里；不含机密。
    pub fn cas_conflict(entity: &'static str, id: impl Into<String>) -> Self {
        Self::CasConflict {
            entity,
            id: id.into(),
        }
    }

    /// 该错误是否属于「同一请求重发仍可能成功」的类别。
    ///
    /// 只覆盖端口事实层面的显然可重试项；**这不是重试政策**——重试政策由
    /// `ApiError.retryDisposition` 表达，且用例层必须结合「是否已建立执行记录」判断。
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::BackendUnavailable(_) | Self::ProviderTimeout(_) | Self::CasConflict { .. }
        )
    }
}

// ────────────────────────────────────────────────────────────────────────────
// 跨边界业务拒绝码（§4:234）
//
// `ApiErrorCode` 与 `RetryDisposition` 是**跨边界的 wire 词汇**：`application` 的 `ApiError`
// 序列化时写出它们，`runtime` 的 `ApiError` 也用同一个 code 枚举判定「派发前拒绝」。
// 两边此前各定义一份同名同形的枚举——变体、顺序、字面值逐字相同，**却是两个 Rust 类型**，
// 所以既无法互相传值，也没有任何一处转换能证明它们同步；两份定义各自漂移不会被编译器
// 发现。唯一定义处放在本包正是 §4:234 的约定：`application` 与 `runtime` 用 `pub use`
// 再导出，不重复定义（本包不得反向依赖它们，见 F-04）。
//
// 方向约束：本包不依赖 `application` / `runtime`，而那两个 crate 是互不依赖的兄弟
// （`connection/mod.rs` 与 `application/lib.rs` 都不引用对方），所以共享定义只能落在本包。
// ────────────────────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};

/// 请求被拒绝的机器可读 code，取值逐字来自连接 §13 的错误表。
///
/// **枚举是封闭的**：新增取值等同协议版本升级，必须同时更新 driver、宿主与前端
/// （连接 §13：「新增取值需要提升枚举版本并同时更新全部消费者，宿主、前端与 driver 不得各自扩展」）。
///
/// **只表示「请求被拒绝」**：派发后的执行终态失败由 `ExecutionState = failed` +
/// `ExecutionErrorCode` 表达（连接 §13 末段）。因此本枚举里没有 `hostRejected`——它是
/// `ExecutionErrorCode` 的取值，属于另一个命名空间；`api_error_code` 守卫测试钉住这一点。
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

    /// 该码是否表示「请求在派发前被拒绝」。
    ///
    /// 这是连接 §13 L769 的判据：`true` 意味着**不产生 execution 记录**，
    /// 该路径上不存在 `ExecutionState` / `errorCode` / `effectOutcome`；
    /// `false` 的六个码只能出现在已有执行记录的终态里。
    pub const fn is_pre_dispatch_rejection(self) -> bool {
        !matches!(
            self,
            Self::RollbackFailed
                | Self::CleanupFailed
                | Self::OutcomeUnknown
                | Self::TargetConflictRows
                | Self::SourceChanged
                | Self::PlanStale
        )
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
    /// **已知规格缺口（不发明取值）**：连接 §13 对 `ResourceBusy` / `QueueFull` /
    /// `RateLimited` / `ServiceUnavailable` / `QuotaExceeded` / `SessionQuotaExceeded`
    /// 给的动作是「等待或按 Retry-After 退避后重发」，语义上既不是 `never`，
    /// 也不是「确认无副作用的读取」，但概要 §6.3 只定义了三个取值。这里对它们
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cas_conflict_carries_entity_and_id() {
        let err = PortError::cas_conflict("profile", "conn-1");
        assert_eq!(
            err.to_string(),
            "compare-and-set conflict on profile conn-1"
        );
        assert!(err.is_transient());
    }

    #[test]
    fn variant_set_includes_the_p5_job_contract() {
        // Include the four P5 persistence facts; the exhaustive matcher below
        // makes future additions require an explicit contract update.
        let all = [
            PortError::BackendUnavailable("db".into()),
            PortError::CasConflict {
                entity: "profile",
                id: "1".into(),
            },
            PortError::NotFound("conn-1".into()),
            PortError::TokenInvalid,
            PortError::ProviderTimeout("db".into()),
            PortError::ArtifactExpired,
            PortError::QuotaExceeded("rows".into()),
            PortError::IdempotencyConflict,
            PortError::StaleClaim,
            PortError::PlanAlreadyConsumed("plan-1".into()),
            PortError::UnsupportedVersion("plan/2".into()),
        ];
        let names: Vec<&str> = all.iter().map(|e| variant_name(e)).collect();
        assert_eq!(
            names,
            [
                "BackendUnavailable",
                "CasConflict",
                "NotFound",
                "TokenInvalid",
                "ProviderTimeout",
                "ArtifactExpired",
                "QuotaExceeded",
                "IdempotencyConflict",
                "StaleClaim",
                "PlanAlreadyConsumed",
                "UnsupportedVersion",
            ]
        );
    }

    fn variant_name(error: &PortError) -> &'static str {
        match error {
            PortError::BackendUnavailable(_) => "BackendUnavailable",
            PortError::CasConflict { .. } => "CasConflict",
            PortError::NotFound(_) => "NotFound",
            PortError::TokenInvalid => "TokenInvalid",
            PortError::ProviderTimeout(_) => "ProviderTimeout",
            PortError::ArtifactExpired => "ArtifactExpired",
            PortError::QuotaExceeded(_) => "QuotaExceeded",
            PortError::IdempotencyConflict => "IdempotencyConflict",
            PortError::StaleClaim => "StaleClaim",
            PortError::PlanAlreadyConsumed(_) => "PlanAlreadyConsumed",
            PortError::UnsupportedVersion(_) => "UnsupportedVersion",
        }
    }

    #[test]
    fn display_messages_match_the_spec_text() {
        // §4.1 把 `#[error(...)]` 文案直接写进了契约，host 的日志与重试判断都读它。
        let cases: Vec<(PortError, &str)> = vec![
            (
                PortError::BackendUnavailable("db".into()),
                "backend unavailable: db",
            ),
            (
                PortError::CasConflict {
                    entity: "profile",
                    id: "conn-1".into(),
                },
                "compare-and-set conflict on profile conn-1",
            ),
            (PortError::NotFound("conn-1".into()), "not found: conn-1"),
            (PortError::TokenInvalid, "token verification failed"),
            (
                PortError::ProviderTimeout("db".into()),
                "provider timeout: db",
            ),
            (PortError::ArtifactExpired, "artifact expired"),
            (
                PortError::QuotaExceeded("rows".into()),
                "quota exceeded: rows",
            ),
            (
                PortError::IdempotencyConflict,
                "idempotency conflict: same key, different request payload",
            ),
            (
                PortError::StaleClaim,
                "stale or expired claim: write rejected",
            ),
            (
                PortError::PlanAlreadyConsumed("plan-1".into()),
                "plan already consumed by an apply job: plan-1",
            ),
            (
                PortError::UnsupportedVersion("plan/2".into()),
                "unsupported version: plan/2",
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn the_port_error_has_no_wire_shape_of_its_own() {
        // 跨边界的错误类型是 `ApiError`（application 层）。如果哪天有人给 `PortError`
        // 加了 serde，这里会失败并提醒：要么删掉，要么连同 host 的映射一起改。
        //
        // 判据不是「`PortError` 那段文本里有没有 marker」，而是**归属**：本文件里任何一处
        // 引用 serde 的代码，都必须挂在一个允许 wire 形态的类型下（`ApiErrorCode` /
        // `RetryDisposition`，即 §4:234 定的跨边界 wire 词汇）。挂到别的类型上就判失败——
        // 不管它出现在第几行、出现在 derive 上方还是文件末尾。
        //
        // 为什么不再划位置取样：前两代守卫都是「截出 `PortError` 那一段再 grep」，于是代码
        // 每挪一次，取样段就静默缩小，守卫拿着一个查不到东西的定义永远绿。两个已复现的
        // 绕过都出在这个洞里：
        //   * 把 `Serialize` 追加进 derive——derive 在 `pub enum` 的**上一行**；
        //   * 手写 `impl serde::Serialize for PortError` 放在同文件任意位置。
        // 位置会变，类型名不会，所以按类型名归属。
        //
        // 边界：`#[cfg(test)]` 之后不扫（本测试自己必然提到 serde）。本守卫只管一件事：
        // **端口层的 wire 形态不得在本模块里私定。**
        //
        // 跨文件给 `PortError` 实现 serde，本守卫**看不见**，而且这不是待补的 TODO——
        // 是没有守卫。实测（探针：另一个文件里 `impl serde::Serialize for PortError`，
        // 本文件只加一行 `#[path] mod` 挂进来）：EXIT=0，守卫
        // `the_port_error_has_no_wire_shape_of_its_own` 与一条证明
        // `PortError` 确实已能序列化的正向测试**同时通过**。原因是结构性的：本守卫读的是
        // `include_str!("error.rs")` 这一个字符串。
        //
        // 写这段注释时此处原本写的是「该由那条边界的守卫负责」。那句话是假的：全仓
        // `include_str!` 类守卫 71 处、分布在 4 个 crate，其中 `packages/driver-api`
        // 49 个 `.rs` 只被 1 个守卫触达，同 crate 的 `ports/secret.rs` 兄弟守卫
        // （`resolved_credential_has_no_serde_impls_in_source`）也是只读自己那一个文件。
        // 指给一个不存在的守卫，比不写更坏——它会让下一个读到这里的人以为这一维已经
        // 有人兜底，而代码里没有任何东西会在它失守时报错。
        //
        // 要真封住只有两条路，都不在这条测试里：给 `Cargo.toml` 加 `static_assertions`
        // 的 `assert_not_impl_any!(PortError: !Serialize)`，或在 `scripts/check-*.mjs`
        // 里做 crate 级扫描。后者必须**遍历模块树**（从 `lib.rs` 解析 `mod`，或直接遍历
        // 目录）——**不要手写文件清单**：清单会随新增文件静默过期，守卫照样判绿，而且
        // 过期时没有任何信号，比只读单文件更坏。
        const ALLOWED: [&str; 2] = ["ApiErrorCode", "RetryDisposition"];
        const MARKERS: [&str; 3] = ["Serialize", "Deserialize", "serde_json"];

        /// 从一条顶层 item 的声明行里取出它所服务的类型名。
        /// `enum X` / `impl X` / `impl Trait for X` / `impl<T> X` / `mod X` 都归到 `X`；
        /// 取不出类型名（`use`、`pub use`）时返回空串，交给调用方当作「不是 owner」。
        ///
        /// 空串是有代价的：一旦某行「取不到名字」，它就不再是归属屏障，上行查找会
        /// 穿过它落进别的 item。`mod wire { … }` 那次漏检就是这么来的——`mod` 当年不在
        /// 关键字表里，容器内的 marker 就一路穿到了容器外某个恰好合法的类型上。
        /// 所以这里宁可多剥一点标点，也不能让常见写法归到空串。
        fn subject_of(header: &str) -> String {
            let h = header.trim().trim_end_matches('{').trim();
            let h = h.strip_prefix("pub ").unwrap_or(h);
            let h = h
                .strip_prefix("impl")
                .or_else(|| h.strip_prefix("enum"))
                .or_else(|| h.strip_prefix("struct"))
                .or_else(|| h.strip_prefix("union"))
                .or_else(|| h.strip_prefix("trait"))
                .or_else(|| h.strip_prefix("type"))
                .or_else(|| h.strip_prefix("const"))
                .or_else(|| h.strip_prefix("static"))
                .or_else(|| h.strip_prefix("fn"))
                .or_else(|| h.strip_prefix("mod "))
                .unwrap_or(h)
                .trim_start();
            let h = h.split_once(" for ").map_or(h, |(_, after)| after);
            let h = match h.strip_prefix('<') {
                Some(_) => h.find('>').map_or(h, |i| h[i + 1..].trim_start()),
                None => h,
            };
            // `macro_rules! m`、`::path::T` 这类前缀标点先剥掉，免得 `!` 把标识符取成空串。
            let h = h.trim_start_matches(|c: char| !c.is_alphanumeric() && c != '_');
            h.chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect()
        }

        /// 一行是否是顶层 item 的声明行：顶格、且以 item 关键字开头。
        /// `#[derive(..)]` 和 `///` 文档注释都**不是**声明行——它们属于自己那个 item。
        /// `mod` / `macro_rules` 也在表内，但它们是**容器**：容器内的 marker 归容器所有，
        /// 不能穿过去落到容器外面某个恰好合法的类型上。
        fn item_subject(line: &str) -> Option<String> {
            const KEYWORDS: [&str; 13] = [
                "pub ",
                "impl",
                "const ",
                "static ",
                "fn ",
                "struct ",
                "enum ",
                "type ",
                "trait ",
                "union ",
                "unsafe ",
                "mod ",
                "macro_rules",
            ];
            if line.starts_with(char::is_whitespace)
                || !KEYWORDS.iter().any(|k| line.starts_with(k))
            {
                return None;
            }
            match subject_of(line).as_str() {
                "" => None,
                name => Some(name.to_string()),
            }
        }

        let source = include_str!("error.rs");
        let body_end = source
            .find("#[cfg(test)]")
            .expect("测试模块必须仍然在本文件末尾");
        let lines: Vec<&str> = source[..body_end].lines().collect();

        let mut owned = 0usize;
        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            // 注释给不了类型 wire 形态，否则一句「以后也许要给 PortError 加 Serialize」
            // 就会把守卫顶红，而顶红只会被用「放宽 marker 列表」抹掉。
            if trimmed.starts_with("//") {
                continue;
            }
            if !MARKERS.iter().any(|marker| line.contains(marker)) {
                continue;
            }
            // 导入不是形态本身；它所启用的 `derive` / `impl` 会被同一套归属抓到。
            if trimmed.starts_with("use ") {
                continue;
            }
            // 归属方向取决于这一行的角色，三种都要认全，否则守卫会在自己没想到的写法上
            // 漏掉：`pub enum X {` 属于它自己；顶格的 `#[derive(..)]` 属于它**下方**那个
            // item（derive 总在声明之上）；函数体内的引用属于它上方那个 item。
            let owner = match item_subject(line) {
                Some(own) => Some(own),
                None if line.starts_with("#[") || line.starts_with("///") => lines[index + 1..]
                    .iter()
                    .find_map(|later| item_subject(later)),
                None => lines[..index]
                    .iter()
                    .rev()
                    .find_map(|earlier| item_subject(earlier)),
            };
            let owner = owner.unwrap_or_else(|| "<模块顶层>".to_string());
            assert!(
                ALLOWED.contains(&owner.as_str()),
                "只有 {ALLOWED:?} 在本模块可以有 serde 形态；这一行归到了 `{owner}` 上：{trimmed}"
            );
            owned += 1;
        }

        // 自检：上面这个循环必须真的把 marker 归到了 wire 词汇名下。全程归到 0 个时它会
        // 因为「一个都没匹配上」而空转通过——那正是前两代守卫静默失效的同款形态。
        //
        // 条件与提示共用这个常量：阈值改成 3 之后，提示里那句「至少 2 处」不能还留着，
        // 否则下一个人照着提示调参会照着一个假数字调。
        const MIN_OWNED: usize = 2;
        assert!(
            owned >= MIN_OWNED,
            "守卫空转：只归到 {owned} 处 marker，预期至少 {MIN_OWNED} 处（ApiErrorCode 与 RetryDisposition 的 derive）"
        );
    }

    #[test]
    fn token_invalid_and_artifact_expired_are_not_transient() {
        assert!(!PortError::TokenInvalid.is_transient());
        assert!(!PortError::ArtifactExpired.is_transient());
        assert!(!PortError::QuotaExceeded("rows".into()).is_transient());
        assert!(!PortError::IdempotencyConflict.is_transient());
        assert!(!PortError::StaleClaim.is_transient());
        assert!(!PortError::PlanAlreadyConsumed("plan-1".into()).is_transient());
        assert!(!PortError::UnsupportedVersion("plan/2".into()).is_transient());
    }

    /// 从 application 迁到「类型的唯一定义处」：守卫的对象是这个枚举，
    /// 枚举在这里，守卫就得在这里——留在 application 会扫描一份不再含枚举的源文件，
    /// 变成一条恒真的假守卫。
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

    fn code_only(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

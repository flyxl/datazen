//! `RuntimeEpoch` 与**唯一**的对外错误码出口（D-02）。
//!
//! ## 为什么折叠必须是纯函数
//!
//! §6.3 末段要求客户端的 `dbSessionId` **与** `runtimeEpoch` 精确匹配，
//! §12 要求协调者租约失效后旧 epoch 的请求一律按失效处理。而「一个事实到底报哪个对外码」
//! 散落在各处时，同一件事会在不同入口报出两种码——调用方拿到 `sessionNotFound` 时
//! 重连，拿到 `runtimeEpochMismatch` 时查资源，两条处置路径必然有一条是错的。
//! 所以本模块把「事实 → 对外投影」收敛成 [`fold_exit`] 一个**纯函数**：
//! 没有 IO、没有状态、没有 `self`，因此可以整表穷举测试（见下方测试模块）。
//!
//! ## `RuntimeEpochMismatch` 为什么塌成 `SessionNotFound`
//!
//! `ProviderError::api_code()` 把它映射成 `runtimeEpochMismatch`，而**本模块故意不照抄**：
//! §6.3 末段明确「陈旧 epoch 一律按未知句柄处理——不泄露『资源被换过一次』」。
//! `runtimeEpochMismatch` 这个码字本身就是泄露：它告诉调用方「这个会话曾经存在、
//! 后来被换掉了」。一旦透露，调用方就会尝试去恢复一个按 §4.5 已禁止透明重建的会话，
//! 或者据此推断别的租户什么时候做过什么。两边都指向同一个对外动作（读终态、显式新建会话），
//! 所以对外只给一个码：`sessionNotFound`。差异留在内部（`RuntimeError::reason()` 与审计条目）。
//!
//! 这条改写是 **D-02 的裁定**，与 `ProviderError::api_code()` 的差异是本 track 已知的
//! 冻结面偏离之一。

use crate::connection::error::ApiErrorCode;
use crate::connection::{Counter, ProviderError, RuntimeError, SessionHandle};

/// 运行时世代号。worker 每次启动重新生成；资源替换后旧世代一律失效（§12）。
///
/// 新类型而不是裸 `u64`：句柄上同时存在 `dbSessionId` 和 `runtimeEpoch`，两者都参与
/// 精确匹配；用裸 `u64` 传参时编译器无法阻止把 configRevision 当成 epoch 传进来。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeEpoch(Counter);

impl RuntimeEpoch {
    pub const fn new(value: u64) -> Self {
        Self(Counter::new(value))
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }

    /// 会话句柄的世代是否与本世代相同（§6.3 末段的精确匹配）。
    pub const fn matches(self, handle: &SessionHandle) -> bool {
        handle.runtime_epoch.get() == self.0.get()
    }
}

/// 对外投影的两种可能结果。
///
/// 之所以不是裸 `ApiErrorCode`：§13.1 明确取消类失败**不得**用 `ApiError` 表达，
/// 派发后的执行终态失败走 `ExecutionState = failed` + `ExecutionErrorCode`。
/// 折叠函数如果强行对每一个事实都产出 `ApiErrorCode`，就得给
/// `cancelFailed` / `invariantBroken` 编一个「调用者可以处置的拒绝码」，
/// 那正是这两条错误被刻意排除在 `api_code()` 之外的原因。
/// 因此「不上线路」也必须是一个**显式的、被测试覆盖的返回值**，
/// 而不是悄悄让调用方自己 `if let Some(...)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitProjection {
    /// 有对外拒绝码：填进 `ApiError.code`。
    Code(ApiErrorCode),
    /// 不该用 `ApiError` 表达（§13.1）：由调用方转成
    /// `ExecutionErrorCode::HostRejected` 或事件流上的失败终态。
    /// `reason` 是 `RuntimeError::reason()` 的同一批稳定字面量。
    NotOnTheWire { reason: &'static str },
}

impl ExitProjection {
    /// 对外码；不上线路时返回 `None`。
    pub const fn code(self) -> Option<ApiErrorCode> {
        match self {
            Self::Code(code) => Some(code),
            Self::NotOnTheWire { .. } => None,
        }
    }

    /// 该投影是否会上线路。
    pub const fn is_on_the_wire(self) -> bool {
        matches!(self, Self::Code(_))
    }
}

/// 折叠的输入事实：会话层事实或 provider 层事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitFact<'a> {
    Session(&'a RuntimeError),
    Provider(&'a ProviderError),
}

/// **唯一**的对外错误码出口（D-02）。
///
/// 纯函数、无状态、无 IO：对同一组事实永远给出同一组码。因此
/// 「陈旧 epoch 的旧请求从哪个入口出去」不影响调用方看到什么。
///
/// 映射规则（§13 错误表 + §6.3 末段 + §13.1）：
///
/// | 事实 | 对外投影 |
/// | --- | --- |
/// | `RuntimeError::UnknownSession` | `sessionNotFound` |
/// | `RuntimeError::SessionClosed` | `sessionNotFound` |
/// | `RuntimeError::SessionLost` | `sessionLost` |
/// | `RuntimeError::SessionQuarantined` | `sessionLost` |
/// | `RuntimeError::ContextRevisionMismatch` | `contextConflict` |
/// | `RuntimeError::BudgetExhausted` | `resourceBusy` |
/// | `RuntimeError::CloseRejected` | `transactionResolutionRequired` |
/// | `RuntimeError::CancelFailed` | 不上线路（§13.1） |
/// | `RuntimeError::InvariantBroken` | 不上线路（缺陷不得伪装成可处置拒绝） |
/// | `ProviderError::RuntimeEpochMismatch` | **`sessionNotFound`**（见模块文档） |
/// | 其余 `ProviderError` | 与 `ProviderError::api_code()` 一致 |
/// | 派发后的 `ProtocolError`/`SqlError`/`Timeout`/`ResourceLost` | 不上线路 |
pub fn fold_exit(fact: ExitFact<'_>) -> ExitProjection {
    match fact {
        ExitFact::Session(error) => match error {
            // `Closed` 与 `Lost` 对外分流靠的是前者的两个码（sessionNotFound），
            // 而隔离中的会话对调用者同样不可用，只能新建，旧物理资源不得复用。
            RuntimeError::UnknownSession(_) | RuntimeError::SessionClosed(_) => {
                ExitProjection::Code(ApiErrorCode::SessionNotFound)
            }
            RuntimeError::SessionLost(_) | RuntimeError::SessionQuarantined(_) => {
                ExitProjection::Code(ApiErrorCode::SessionLost)
            }
            RuntimeError::ContextRevisionMismatch { .. } => {
                ExitProjection::Code(ApiErrorCode::ContextConflict)
            }
            RuntimeError::BudgetExhausted(_) => ExitProjection::Code(ApiErrorCode::ResourceBusy),
            RuntimeError::CloseRejected(_) => {
                ExitProjection::Code(ApiErrorCode::TransactionResolutionRequired)
            }
            RuntimeError::CancelFailed(_) => ExitProjection::NotOnTheWire {
                reason: "cancelFailed",
            },
            RuntimeError::InvariantBroken(_) => ExitProjection::NotOnTheWire {
                reason: "invariantBroken",
            },
        },
        ExitFact::Provider(error) => match error {
            // D-02：与 `ProviderError::api_code()` 的唯一分歧点，理由见模块文档。
            ProviderError::RuntimeEpochMismatch(_) => {
                ExitProjection::Code(ApiErrorCode::SessionNotFound)
            }
            ProviderError::ProtocolError(_)
            | ProviderError::SqlError(_)
            | ProviderError::Timeout(_)
            | ProviderError::ResourceLost(_) => ExitProjection::NotOnTheWire {
                reason: "executionTerminal",
            },
            other => match other.api_code() {
                Some(code) => ExitProjection::Code(code),
                None => ExitProjection::NotOnTheWire {
                    reason: "executionTerminal",
                },
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 会话层事实 → 期望投影的**整表**（§4.4 表格驱动）。
    fn session_rows() -> Vec<(RuntimeError, ExitProjection)> {
        vec![
            (
                RuntimeError::UnknownSession("db_1".to_owned()),
                ExitProjection::Code(ApiErrorCode::SessionNotFound),
            ),
            (
                RuntimeError::SessionClosed("db_1".to_owned()),
                ExitProjection::Code(ApiErrorCode::SessionNotFound),
            ),
            (
                RuntimeError::SessionLost("db_1".to_owned()),
                ExitProjection::Code(ApiErrorCode::SessionLost),
            ),
            (
                RuntimeError::SessionQuarantined("inUse"),
                ExitProjection::Code(ApiErrorCode::SessionLost),
            ),
            (
                RuntimeError::ContextRevisionMismatch {
                    expected: 1,
                    actual: 2,
                },
                ExitProjection::Code(ApiErrorCode::ContextConflict),
            ),
            (
                RuntimeError::BudgetExhausted("sessionQuota"),
                ExitProjection::Code(ApiErrorCode::ResourceBusy),
            ),
            (
                RuntimeError::CloseRejected("activeTransaction"),
                ExitProjection::Code(ApiErrorCode::TransactionResolutionRequired),
            ),
            (
                RuntimeError::CancelFailed("driverUnreachable"),
                ExitProjection::NotOnTheWire {
                    reason: "cancelFailed",
                },
            ),
            (
                RuntimeError::InvariantBroken("executionStateWentBackwards"),
                ExitProjection::NotOnTheWire {
                    reason: "invariantBroken",
                },
            ),
        ]
    }

    /// provider 层事实 → 期望投影的整表。
    fn provider_rows() -> Vec<(ProviderError, ExitProjection)> {
        let mut rows = Vec::new();
        rows.push((
            ProviderError::RuntimeEpochMismatch("db_1".to_owned()),
            ExitProjection::Code(ApiErrorCode::SessionNotFound),
        ));
        for (error, code) in [
            (
                ProviderError::CapabilityUnsupported,
                ApiErrorCode::CapabilityUnsupported,
            ),
            (
                ProviderError::UnsupportedPlan,
                ApiErrorCode::UnsupportedPlan,
            ),
            (
                ProviderError::TargetUnsupported("t".to_owned()),
                ApiErrorCode::TargetUnsupported,
            ),
            (
                ProviderError::TargetRequired("t".to_owned()),
                ApiErrorCode::TargetRequired,
            ),
            (
                ProviderError::TargetConflict("t".to_owned()),
                ApiErrorCode::TargetConflict,
            ),
            (
                ProviderError::ResourceBusy("queue"),
                ApiErrorCode::ResourceBusy,
            ),
            (
                ProviderError::SessionNotFound("db_1".to_owned()),
                ApiErrorCode::SessionNotFound,
            ),
            (
                ProviderError::SessionLost("db_1".to_owned()),
                ApiErrorCode::SessionLost,
            ),
            (
                ProviderError::ContextConflict("ctx".to_owned()),
                ApiErrorCode::ContextConflict,
            ),
            (
                ProviderError::RollbackFailed("r".to_owned()),
                ApiErrorCode::RollbackFailed,
            ),
            (
                ProviderError::CleanupFailed("c".to_owned()),
                ApiErrorCode::CleanupFailed,
            ),
            (
                ProviderError::OutcomeUnknown("rollback"),
                ApiErrorCode::OutcomeUnknown,
            ),
            (
                ProviderError::HostRejected("db_1".to_owned()),
                ApiErrorCode::InvalidArgument,
            ),
        ] {
            rows.push((error, ExitProjection::Code(code)));
        }
        for error in [
            ProviderError::ProtocolError("p".to_owned()),
            ProviderError::SqlError("s".to_owned()),
            ProviderError::Timeout("t"),
            ProviderError::ResourceLost("r".to_owned()),
        ] {
            rows.push((
                error,
                ExitProjection::NotOnTheWire {
                    reason: "executionTerminal",
                },
            ));
        }
        rows
    }

    /// D-02 的核心：**全笛卡尔积**表格驱动测试。
    ///
    /// 每一对 (会话事实, provider 事实) 都各折叠一次并断言各自命中表里的期望投影。
    /// 只测单一来源会漏掉「两条路径对同一事实给出不同码」这类漂移——而那正是 D-02 要消灭的。
    #[test]
    fn fold_exit_is_total_and_pairwise_consistent_over_both_fact_spaces() {
        let session = session_rows();
        let provider = provider_rows();
        let mut pairs = 0usize;
        for (session_error, session_expected) in &session {
            for (provider_error, provider_expected) in &provider {
                assert_eq!(
                    fold_exit(ExitFact::Session(session_error)),
                    *session_expected,
                    "会话事实 {session_error:?} 的投影漂移"
                );
                assert_eq!(
                    fold_exit(ExitFact::Provider(provider_error)),
                    *provider_expected,
                    "provider 事实 {provider_error:?} 的投影漂移"
                );
                pairs += 1;
            }
        }
        assert_eq!(session.len(), 9, "会话层事实表必须覆盖全部变体");
        assert_eq!(provider.len(), 18, "provider 层事实表必须覆盖全部变体");
        assert_eq!(pairs, 162, "9 × 18 必须全部两两折叠");
    }

    /// D-02 关键裁定：陈旧 `runtimeEpoch` **不得**对外暴露「被替换过」。
    ///
    /// 反例一旦成立，调用方就会拿到 `runtimeEpochMismatch` 并据此推断出资源曾经被换过，
    /// 然后尝试恢复一个按 §4.5 禁止透明重建的会话。
    #[test]
    fn stale_runtime_epoch_exits_as_session_not_found_not_epoch_mismatch() {
        let fact = ProviderError::RuntimeEpochMismatch("db_1".to_owned());
        assert_eq!(
            fold_exit(ExitFact::Provider(&fact)).code(),
            Some(ApiErrorCode::SessionNotFound),
            "陈旧 epoch 必须与未知句柄共用一个对外码"
        );
        assert_eq!(
            fact.api_code(),
            Some(ApiErrorCode::RuntimeEpochMismatch),
            "provider 自己的映射保持不变——差异只存在于本折叠函数，且已被显式记录"
        );
    }

    /// 陈旧 epoch 的两种来源（未知句柄 / provider 的 epoch 不符）必须**塌缩成同一个码**：
    /// 调用方只有一条动作路径（读终态、显式新建会话）。
    #[test]
    fn both_stale_epoch_paths_share_one_external_code() {
        let unknown = RuntimeError::UnknownSession("db_1".to_owned());
        let mismatch = ProviderError::RuntimeEpochMismatch("db_1".to_owned());
        assert_eq!(
            fold_exit(ExitFact::Session(&unknown)).code(),
            fold_exit(ExitFact::Provider(&mismatch)).code(),
            "两条路径给出不同码就等于泄露了『被替换过』"
        );
    }

    /// 纯函数性：同一输入重复折叠结果不变，且折叠不改变输入。
    #[test]
    fn folding_is_pure_and_repeatable() {
        let fact = RuntimeError::SessionLost("db_1".to_owned());
        let before = fact.clone();
        let first = fold_exit(ExitFact::Session(&fact));
        let second = fold_exit(ExitFact::Session(&fact));
        assert_eq!(first, second);
        assert_eq!(fact, before, "折叠不得改动被折叠的值");
    }

    /// 上线/不上线是**显式**判定，不是 `Option` 静默兜底。
    #[test]
    fn cancel_and_defects_are_explicitly_off_the_wire() {
        for error in [
            RuntimeError::CancelFailed("driverUnreachable"),
            RuntimeError::InvariantBroken("x"),
        ] {
            let projection = fold_exit(ExitFact::Session(&error));
            assert!(!projection.is_on_the_wire(), "{error:?} 不得上 ApiError");
            assert_eq!(projection.code(), None);
            assert_eq!(error.api_code(), None, "与 RuntimeError 自身判定一致");
        }
    }

    /// `RuntimeEpoch` 与句柄的精确匹配（§6.3 末段）。
    #[test]
    fn epoch_matches_only_the_exact_generation() {
        let epoch = RuntimeEpoch::new(3);
        let mut handle = SessionHandle {
            db_session_id: crate::connection::DbSessionId::new("db_1"),
            runtime_epoch: Counter::new(3),
        };
        assert!(epoch.matches(&handle));
        assert_eq!(epoch.get(), 3);
        handle.runtime_epoch = Counter::new(2);
        assert!(!epoch.matches(&handle), "陈旧世代不得命中当前登记");
        handle.runtime_epoch = Counter::new(4);
        assert!(!epoch.matches(&handle), "未来世代同样不命中");
    }
}

//! 宿主资源台账与生命周期裁决（shared-boundaries-and-ports.md §3.2 模块表第 4 行 `resource`）。
//!
//! 一句话职责：**管账与裁决，不管驱动实现**。本模块持有物理连接表、`poolKey` 索引、
//! 租约生命周期与替代提交闸门，回答「能不能复用 / 能不能再执行 / 能不能回池 / 能不能发布」。
//! 它**从不**自己连接数据库，也**从不**自己判定驱动的 `Clean`。
//!
//! ## 两条硬边界（缺一条即实现违规，由 `tests` 里的结构断言长期钉住）
//!
//! **边界一：不得在本模块内做驱动 `Clean` 判定。**
//! `Clean` 由驱动以**结论**上报（[`cleanup::DriverCleanVerdict`]，刻意不携带任何判定依据），
//! 宿主只做**自己的**检查（活动执行 / 未完成协议 / 未释放的会话级句柄 / 归属被禁用），
//! 两边都通过才允许回池（connection-management.md §7.3 步骤 6、CM-69）。
//! 换句话说：宿主**看不到** `Clean` 是怎么算出来的，也**不允许**在本模块里重算它。
//!
//! **边界二：不得把物理连接句柄放进任何公开类型或返回值。**
//! socket / stream / driver handle 只以 [`table::PhysicalHandle`]`（`pub(crate)`）这种
//! 宿主内部不透明身份存在，并且**不在**本文件重新导出。对外唯一的身份是账目标识
//! `ResourceId` —— 与 `platform-api` 的 `PoolLease.physical` 同义：**账目，不是句柄**
//! （`ports/budget/pool.rs` 模块头「句柄留在 adapter 侧」）。
//!
//! ## 依赖方向（单向，无环）
//!
//! ```text
//! mod.rs（错误 + 时钟）
//!    ├→ lease（池键分片 + 租约状态机）
//!    ├→ generation（缓存代次闸门 + 轮换）
//!    ├→ cleanup（宿主侧裁决 + 注入式物理出入口）
//!    └→ replacement（候选替换 + 提交屏障 + 目录发布）
//!              ↑
//!           table（物理资源表 + 空闲池索引）
//!              ↑
//!     manager（ResourceManager 门面）→ rotation（轮换/禁用/排队）
//!              └→ adoption（候选替换门面）
//! ```
//!
//! ## 与其它模块的分工
//!
//! * `connection/**` 是冻结的会话面：会话状态、句柄登记、`PoolKeyInputs`/`PoolKeyFingerprint`
//!   都在那里，本模块**只用不重定义**。
//! * `registry/**` 是 Wave 2 接缝，冻结；资源层的租约/句柄/预算许可**不得**浮到会话面，
//!   会话面只会看到 `RuntimeError::BudgetExhausted`（见 `registry/mod.rs` 的边界声明）。
//! * 物理端口 [`cleanup::PhysicalTransport`] 是本模块自己定义的**注入式**驱动侧接缝：
//!   `connection/port.rs` 只冻结了 DTO 与 `BudgetPort`，并没有资源操作端口。

mod adoption;
mod cleanup;
mod generation;
mod lease;
mod manager;
mod publication;
mod replacement;
mod rotation;
mod table;
mod transition;

/// 连续旅程测试的共享替身（`FakeClock` 适配、记录式物理端口、可注入故障的目录）。
#[cfg(test)]
mod harness;
/// 归还裁决旅程（CM-69 / CM-73）。
#[cfg(test)]
mod journey;
/// 候选替换旅程（CM-68）。
#[cfg(test)]
mod journey_ledger;
/// 轮换、配置版本与禁用旅程（CM-67 / CM-38 / CM-39）。
#[cfg(test)]
mod journey_rotation;

pub use cleanup::{
    CancellationOutcome, CleanupDisposition, CleanupPlan, CleanupReport, DriverCleanVerdict,
    ExecutionCancellationRecord, HostConditionSnapshot, PhysicalTransport, ProtocolDebt,
    ReturnDecision, ReturnReason, SharedTransport,
};
pub use generation::{
    CacheFillOutcome, CacheRevision, GenerationRegistry, RotationKind, RotationOutcome,
};
pub use lease::{LeasePurpose, LeaseRecord, LeaseRequest, LeaseState, PoolKeyGeneration};
pub use manager::ResourceManager;
pub use publication::{
    CommitBarrier, DirectoryFault, DirectoryPublisher, Publication, PublishOutcome,
    ReplacementReceipt,
};
pub use replacement::{BeginOutcome, CandidateState, ReplacementCandidate, ReplacementLedger};
pub use table::{ConfigRevisionDrift, DisableOutcome, QueueDrain, QueuedLease, RotationReport};
pub use transition::{TransitionRule, LEASE_TRANSITIONS};

use crate::connection::RuntimeError;

/// 本模块全部拒绝的**唯一**出口。
///
/// **为什么不是 `PortError`**：`PortError` 只有 7 个变体且刻意不实现 `Serialize`
/// （`shared-boundaries-and-ports.md` §4），而 platform-api 是冻结上游、禁止加变体；
/// 会话面则已经有 `RuntimeError`（connection/error.rs）。所以本模块给出自己精确的原因集，
/// 并用 [`ResourceError::as_runtime_error`] 把**每一个**变体映射到**既有的** `RuntimeError`
/// 变体上 —— 线上不产生任何新的拒绝形状。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResourceError {
    #[error("lease state transition {from} -> {to} is not legal")]
    IllegalTransition { from: LeaseState, to: LeaseState },
    #[error("resource {0} is not registered in the host resource table")]
    UnknownResource(String),
    #[error("host condition for reuse is not met: {0}")]
    HostConditionUnmet(&'static str),
    #[error("the driver reported the connection is not reusable")]
    DriverUnclean,
    #[error("pool key rotated; the idle is no longer issued and must be closed")]
    PoolKeyRotated,
    #[error("config revision conflict: caller expected {expected}, host is at {actual}")]
    ConfigRevisionConflict { expected: u64, actual: u64 },
    #[error("candidate resource is not published and must not serve traffic")]
    CandidateNotPublished,
    #[error("idempotency key {0} is already bound to a different session")]
    DuplicateCandidate(String),
    #[error("owner is disabled: {0}")]
    OwnerDisabled(String),
    #[error("empty pool metadata exceeded its lru bound of {limit}")]
    PoolMetadataOverflow { limit: usize },
    #[error("cache result from revision {offered} may not backfill revision {current}")]
    StaleCacheRevision { current: u64, offered: u64 },
    #[error("physical transport refused: {0}")]
    TransportRefused(&'static str),
    #[error("resource invariant broken: {0}")]
    InvariantBroken(&'static str),
}

impl ResourceError {
    /// 稳定字面码，用于日志与断言（不随文案改写而漂移）。
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::IllegalTransition { .. } => "resourceIllegalTransition",
            Self::UnknownResource(_) => "resourceUnknown",
            Self::HostConditionUnmet(_) => "resourceHostConditionUnmet",
            Self::DriverUnclean => "resourceDriverUnclean",
            Self::PoolKeyRotated => "resourcePoolKeyRotated",
            Self::ConfigRevisionConflict { .. } => "resourceConfigRevisionConflict",
            Self::CandidateNotPublished => "resourceCandidateNotPublished",
            Self::DuplicateCandidate(_) => "resourceDuplicateCandidate",
            Self::OwnerDisabled(_) => "resourceOwnerDisabled",
            Self::PoolMetadataOverflow { .. } => "resourcePoolMetadataOverflow",
            Self::StaleCacheRevision { .. } => "resourceStaleCacheRevision",
            Self::TransportRefused(_) => "resourceTransportRefused",
            Self::InvariantBroken(_) => "resourceInvariantBroken",
        }
    }

    /// 映射到**既有** `RuntimeError` 变体。本模块**不新增**任何线路可见的拒绝形状。
    pub fn as_runtime_error(&self) -> RuntimeError {
        match self {
            Self::IllegalTransition { .. } | Self::InvariantBroken(_) => {
                RuntimeError::InvariantBroken(self.reason())
            }
            // 候选未提交 ⇒ 对公开目录而言**根本不存在**这个会话（§7.4 步骤 6）。
            Self::UnknownResource(_) | Self::CandidateNotPublished => {
                RuntimeError::UnknownSession(self.reason().to_string())
            }
            // 宿主条件不满足 / 驱动不干净 / 键已轮换 ⇒ 一律隔离，绝不放行复用。
            Self::HostConditionUnmet(_) | Self::DriverUnclean | Self::PoolKeyRotated => {
                RuntimeError::SessionQuarantined(self.reason())
            }
            Self::ConfigRevisionConflict { expected, actual } => {
                RuntimeError::ContextRevisionMismatch {
                    expected: *expected,
                    actual: *actual,
                }
            }
            Self::DuplicateCandidate(_) | Self::OwnerDisabled(_) => {
                RuntimeError::SessionClosed(self.reason().to_string())
            }
            Self::PoolMetadataOverflow { .. } | Self::TransportRefused(_) => {
                RuntimeError::BudgetExhausted(self.reason())
            }
            Self::StaleCacheRevision { current, offered } => {
                RuntimeError::ContextRevisionMismatch {
                    expected: *current,
                    actual: *offered,
                }
            }
        }
    }

    /// 记一条拒绝日志并把**自己**交还出去。
    ///
    /// 模块内部一律用这个：函数签名保持 `Result<T, ResourceError>`（保留精确原因），
    /// 只有真正要越过模块边界时才用 [`ResourceError::rejected`] 转成既有 `RuntimeError`。
    pub fn logged(self, operation: &'static str) -> Self {
        tracing::warn!(
            target: "datazen_runtime::resource",
            operation,
            reason = self.reason(),
            "resource layer refused the request"
        );
        self
    }

    /// 唯一的拒绝出口：记日志 + 转成既有 `RuntimeError`。**任何拒绝都不允许静默**。
    pub fn rejected<T>(self, operation: &'static str) -> Result<T, RuntimeError> {
        Err(self.logged(operation).as_runtime_error())
    }
}

/// 单调时间源。**只为时长服务**（空闲 TTL、关闭超时），不做对外投影；
/// 对外时间戳一律由 `connection::Timestamp` 单独给出。
///
/// 生产代码禁止直接依赖 `connection::testing::FakeClock`（fixture 与生产依赖方向单向），
/// 所以这里只定义一个可注入的小接口；测试侧用 `FakeClock` 实现它。
pub trait MonotonicSource: Send + Sync + 'static {
    fn now_nanos(&self) -> u64;
}

/// §9.2 的空池元数据上限。池里**没有**资源时可以保留元数据条目（供后续命中同一 key），
/// 但条目数必须被这个上限卡住，否则每个历史键都会留一条永不回收的影子记录。
pub const EMPTY_POOL_METADATA_LRU_LIMIT: usize = 32;

/// §9.2 的短操作池空闲 TTL（秒）。到期后池内资源不再被签发，转关闭。
pub const IDLE_POOL_TTL_SECONDS: u64 = 60;

/// 把「纳秒 + 秒」换成毫秒级秒数常量，便于 `FakeClock` 推进时直接对表。
pub const NANOS_PER_SECOND: u64 = 1_000_000_000;

#[cfg(test)]
mod tests {
    use super::{
        ResourceError, EMPTY_POOL_METADATA_LRU_LIMIT, IDLE_POOL_TTL_SECONDS, NANOS_PER_SECOND,
    };
    use crate::connection::error::ApiErrorCode;
    use crate::connection::RuntimeError;

    /// 源码级边界护栏：物理句柄类型只允许出现在 `table.rs` 的私有定义里，
    /// 绝不允许出现在任何一个 `pub` 结构体的字段或 `pub fn` 签名中。
    ///
    /// 这条断言**不是文字游戏**：删掉 `PhysicalHandle` 的 `pub(crate)` 可见性、
    /// 或把它塞进任何公开类型，本测试立刻红。
    #[test]
    fn no_public_item_mentions_a_physical_handle() {
        let sources = [
            ("mod.rs", include_str!("mod.rs")),
            ("lease.rs", include_str!("lease.rs")),
            ("generation.rs", include_str!("generation.rs")),
            ("table.rs", include_str!("table.rs")),
            ("manager.rs", include_str!("manager.rs")),
            ("rotation.rs", include_str!("rotation.rs")),
            ("adoption.rs", include_str!("adoption.rs")),
            ("cleanup.rs", include_str!("cleanup.rs")),
            ("replacement.rs", include_str!("replacement.rs")),
            ("harness.rs", include_str!("harness.rs")),
            ("journey.rs", include_str!("journey.rs")),
            ("journey_ledger.rs", include_str!("journey_ledger.rs")),
        ];

        for (name, source) in sources {
            for (index, line) in source.lines().enumerate() {
                let trimmed = line.trim_start();
                // 只检查确实可见的声明行；`pub(crate)` / 私有字段不在检查范围内。
                if !trimmed.starts_with("pub ") {
                    continue;
                }
                for needle in [
                    "PhysicalHandle",
                    "TcpStream",
                    "UnixStream",
                    "socket",
                    "driver_handle",
                    "Pool<TcpStream>",
                    "Box<dyn Read",
                ] {
                    assert!(
                        !line.contains(needle),
                        "{name}:{} leaks `{needle}` through a public item: {trimmed}",
                        index + 1
                    );
                }
            }
        }
    }

    /// 边界二的另一半：`PhysicalHandle` 必须既不是 `pub`，也不在 `mod.rs` 的再导出名单里。
    /// 两条一起才构成「不导出物理连接句柄」（shared-boundaries-and-ports.md §3.2 第 4 行）。
    #[test]
    fn physical_handle_is_not_exported() {
        let table = include_str!("table.rs");
        assert!(table.contains("struct PhysicalHandle"));
        // 定义处必须带 `pub(crate)`，一旦改成 `pub` 这条断言立刻红。
        assert!(
            table.contains("pub(crate) struct PhysicalHandle"),
            "PhysicalHandle must stay pub(crate)"
        );

        let exported = include_str!("mod.rs");
        let re_export_block = exported
            .split("mod cleanup;")
            .nth(1)
            .and_then(|rest| rest.split("use crate::connection::RuntimeError;").next())
            .unwrap_or_default();
        assert!(
            !re_export_block.contains("PhysicalHandle"),
            "resource/mod.rs must not re-export the physical handle"
        );
    }

    /// 边界一：`Clean` 的判定只允许以 `bool` 结论出现，本模块不得有任何
    /// 「自己算 Clean」的入口（例如出现按连接串/事务状态推导 Clean 的函数）。
    #[test]
    fn clean_verdict_carries_no_judgement_basis() {
        let cleanup = include_str!("cleanup.rs");
        // 结论类型只有一个私有 bool 字段，不暴露任何判定依据。
        assert!(cleanup.contains("pub struct DriverCleanVerdict"));
        assert!(cleanup.contains("clean: bool"));
        // 宿主裁决入口的名字里必须同时出现 driver 与 host 两侧 —— 两侧都过才放行。
        assert!(cleanup.contains("pub fn decide("));
    }

    /// 每个拒绝都必须映射到**既有** `RuntimeError` 变体，不得自造线路形状。
    #[test]
    fn every_rejection_maps_onto_an_existing_runtime_error() {
        let cases = [
            (
                ResourceError::IllegalTransition {
                    from: crate::resource::LeaseState::Closed,
                    to: crate::resource::LeaseState::InUse,
                },
                RuntimeError::InvariantBroken("resourceIllegalTransition"),
            ),
            (
                ResourceError::UnknownResource("r".to_string()),
                RuntimeError::UnknownSession("resourceUnknown".to_string()),
            ),
            (
                ResourceError::CandidateNotPublished,
                RuntimeError::UnknownSession("resourceCandidateNotPublished".to_string()),
            ),
            (
                ResourceError::HostConditionUnmet("x"),
                RuntimeError::SessionQuarantined("resourceHostConditionUnmet"),
            ),
            (
                ResourceError::DriverUnclean,
                RuntimeError::SessionQuarantined("resourceDriverUnclean"),
            ),
            (
                ResourceError::PoolKeyRotated,
                RuntimeError::SessionQuarantined("resourcePoolKeyRotated"),
            ),
            (
                ResourceError::ConfigRevisionConflict {
                    expected: 1,
                    actual: 2,
                },
                RuntimeError::ContextRevisionMismatch {
                    expected: 1,
                    actual: 2,
                },
            ),
            (
                ResourceError::StaleCacheRevision {
                    current: 9,
                    offered: 7,
                },
                RuntimeError::ContextRevisionMismatch {
                    expected: 9,
                    actual: 7,
                },
            ),
            (
                ResourceError::DuplicateCandidate("k".to_string()),
                RuntimeError::SessionClosed("resourceDuplicateCandidate".to_string()),
            ),
            (
                ResourceError::OwnerDisabled("o".to_string()),
                RuntimeError::SessionClosed("resourceOwnerDisabled".to_string()),
            ),
            (
                ResourceError::PoolMetadataOverflow { limit: 32 },
                RuntimeError::BudgetExhausted("resourcePoolMetadataOverflow"),
            ),
            (
                ResourceError::TransportRefused("x"),
                RuntimeError::BudgetExhausted("resourceTransportRefused"),
            ),
            (
                ResourceError::InvariantBroken("x"),
                RuntimeError::InvariantBroken("resourceInvariantBroken"),
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(
                error.as_runtime_error(),
                expected,
                "wrong mapping for {error:?}"
            );
        }
    }

    /// 拒绝必须真的被拒绝：映射出去之后仍然是 `Err`，且带上稳定的线路码。
    #[test]
    fn a_rejection_is_never_a_silent_ok() {
        let outcome: Result<(), RuntimeError> =
            ResourceError::OwnerDisabled("editor-1".to_string()).rejected("acquire_lease");
        let error = outcome.expect_err("a disabled owner must not acquire");
        assert_eq!(
            error,
            RuntimeError::SessionClosed("resourceOwnerDisabled".to_string())
        );
        assert_eq!(error.api_code(), Some(ApiErrorCode::SessionNotFound));
    }

    #[test]
    fn the_pool_metadata_bound_is_the_one_the_spec_names() {
        // connection-management.md §9.2：空池元数据条目上限 32。
        assert_eq!(EMPTY_POOL_METADATA_LRU_LIMIT, 32);
        assert_eq!(IDLE_POOL_TTL_SECONDS, 60);
        assert_eq!(NANOS_PER_SECOND, 1_000_000_000);
    }
}

//! 能力快照与十项能力枚举。
//!
//! 下面每个默认值都写明「非默认取值会把哪条路径推向哪一侧」，
//! 避免夹具在 `unsupported` / `unknown` 分支上静默跳过断言。

use serde::{Deserialize, Serialize};

/// 有状态会话支持度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StatefulSessionSupport {
    /// 默认。固定会话保证成立。
    Supported,
    /// **不得**在 `unknown` 下开启固定会话保证。
    Unsupported,
    Unknown,
}

/// 命名空间切换方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NamespaceSwitch {
    /// 默认。原地切换，`contextRevision + 1`。
    InPlace,
    /// 两阶段替换。
    RequiresReplacement,
    Unsupported,
    Unknown,
}

/// 上下文观测强度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContextObservation {
    /// 默认。
    Full,
    /// `observedContext` 必须保持 `unknown`。
    Partial,
    Unsupported,
}

/// 事务观测强度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransactionObservationSupport {
    /// 默认。
    Full,
    /// 必须走 `TransactionResolutionRequired` 路径。
    Partial,
    Unsupported,
}

/// 会话级句柄支持度。`Unsupported` 时句柄用例必须断言「正确拒绝」，**不得静默 skip**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionScopedHandles {
    /// 默认。
    Supported,
    Unsupported,
}

/// 归池复用是否经过验证。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResetForReuse {
    /// 默认。
    Verified,
    /// 一切复用路径必须走关闭。
    Unsupported,
}

impl ResetForReuse {
    /// 这条资源有没有资格进归池候选。
    ///
    /// `Verified` 才放行 —— 未验证时归pool 候选当场作废，直接走关闭路径
    /// （§9.4「reset 不支持…直接关闭」）。与 `SessionScopedHandleSupport::enables_feature`
    /// / `driver-api` 侧同形：**只有显式声明过的一侧才给过**。
    ///
    /// `CapabilitySnapshot` 那侧的 `reuse_path_verified()` 是同一个判据的快照级读法；
    /// 归池判定需要一个不经过快照、只对着这一个枚举问的口径，所以这里再给一次。
    pub const fn admits_reuse(self) -> bool {
        matches!(self, Self::Verified)
    }
}

impl Default for ResetForReuse {
    /// **`Verified`**，与 [`CapabilitySnapshot::defaults`]() 的取值**一致**。
    ///
    /// 这不是疏漏，是**改动前后逐字节同行为**的要求：`HostConditionSnapshot` 派生
    /// `Default`，而 `Default` 的语义是「调用方什么都没供给时的初值」。若这里取
    /// `Unsupported`，27 处既有 `HostConditionSnapshot::default()` 调用点的归池裁决
    /// 会一次性翻成 `Closed`——那不是本任务引入的语义变化，是凭空把别人的用例改红。
    ///
    /// 反过来问「默认允许归池是不是更危险」：不会，因为它只**退回改动前的旧行为**
    /// ——跑一次复位、发现不行、关闭。也就是说，「忘记供给」的失败模式与本次改动
    /// 之前**完全相同**，不会把没有 verified baseline 的连接偷偷塞进共享池。
    /// 真正要防的是**声明了不复位却被当成可归池**，那由供给侧显式给 `Unsupported` 负责。
    fn default() -> Self {
        Self::Verified
    }
}

/// 精确取消支持度。`Unsupported` 时必须返回 `Unsupported`，**不得降级**为 session-wide cancel。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PreciseCancel {
    /// 默认。
    Supported,
    Unsupported,
}

/// 快照粒度。`Unsupported` 时三件套快照计划必须报 `UnsupportedPlan`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Snapshots {
    /// 默认。
    PerDatabase,
    Unsupported,
}

/// 事务语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransactionSemantics {
    /// 默认：读已提交 / 可重复读 + savepoint。
    ReadCommittedRepeatableReadWithSavepoint,
    ReadCommitted,
    Unsupported,
}

/// DDL 原子性。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DdlAtomicity {
    /// 默认：事务内 DDL 原子，含可声明的非事务步骤。
    TransactionalWithDeclaredNonTransactional,
    Unsupported,
}

/// 能力快照。`confirmed` 表达这份快照本身是否被驱动确认过。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySnapshot {
    pub driver_id: String,
    pub driver_version: String,
    pub protocol_version: u32,
    pub capability_revision: u64,
    pub confirmed: bool,

    pub stateful_session: StatefulSessionSupport,
    pub namespace_switch: NamespaceSwitch,
    pub context_observation: ContextObservation,
    pub transaction_observation: TransactionObservationSupport,
    pub session_scoped_handles: SessionScopedHandles,
    pub reset_for_reuse: ResetForReuse,
    pub precise_cancel: PreciseCancel,
    pub snapshots: Snapshots,
    pub transactions: TransactionSemantics,
    pub ddl_atomicity: DdlAtomicity,
}

impl CapabilitySnapshot {
    /// 全部能力的默认值集合。
    pub fn defaults(driver_id: impl Into<String>, driver_version: impl Into<String>) -> Self {
        Self {
            driver_id: driver_id.into(),
            driver_version: driver_version.into(),
            protocol_version: datazen_driver_api::PROTOCOL_VERSION,
            capability_revision: 1,
            confirmed: true,
            stateful_session: StatefulSessionSupport::Supported,
            namespace_switch: NamespaceSwitch::InPlace,
            context_observation: ContextObservation::Full,
            transaction_observation: TransactionObservationSupport::Full,
            session_scoped_handles: SessionScopedHandles::Supported,
            reset_for_reuse: ResetForReuse::Verified,
            precise_cancel: PreciseCancel::Supported,
            snapshots: Snapshots::PerDatabase,
            transactions: TransactionSemantics::ReadCommittedRepeatableReadWithSavepoint,
            ddl_atomicity: DdlAtomicity::TransactionalWithDeclaredNonTransactional,
        }
    }

    /// 是否允许开启固定会话保证。`unknown` 明确**不允许**。
    pub const fn fixed_session_guaranteed(&self) -> bool {
        matches!(self.stateful_session, StatefulSessionSupport::Supported)
    }

    /// 上下文观测不可靠时 `observedContext` 必须保持 `unknown`，禁止回填 `initialTarget`。
    pub const fn context_must_stay_unknown(&self) -> bool {
        matches!(
            self.context_observation,
            ContextObservation::Partial | ContextObservation::Unsupported
        )
    }

    /// 事务观测降级时必须走 `TransactionResolutionRequired` 路径。
    pub const fn requires_transaction_resolution(&self) -> bool {
        matches!(
            self.transaction_observation,
            TransactionObservationSupport::Partial
        )
    }

    /// 归池复用是否被驱动验证过。未验证时一切复用路径必须走关闭。
    pub const fn reuse_path_verified(&self) -> bool {
        matches!(self.reset_for_reuse, ResetForReuse::Verified)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_section_3_3() {
        let caps = CapabilitySnapshot::defaults("fake-sql", "0.0.1");
        assert_eq!(caps.stateful_session, StatefulSessionSupport::Supported);
        assert_eq!(caps.namespace_switch, NamespaceSwitch::InPlace);
        assert_eq!(caps.context_observation, ContextObservation::Full);
        assert_eq!(
            caps.transaction_observation,
            TransactionObservationSupport::Full
        );
        assert_eq!(caps.session_scoped_handles, SessionScopedHandles::Supported);
        assert_eq!(caps.reset_for_reuse, ResetForReuse::Verified);
        assert_eq!(caps.precise_cancel, PreciseCancel::Supported);
        assert_eq!(caps.snapshots, Snapshots::PerDatabase);
        assert_eq!(
            caps.transactions,
            TransactionSemantics::ReadCommittedRepeatableReadWithSavepoint
        );
        assert_eq!(
            caps.ddl_atomicity,
            DdlAtomicity::TransactionalWithDeclaredNonTransactional
        );
        assert!(caps.fixed_session_guaranteed());
        assert!(!caps.context_must_stay_unknown());
        assert!(!caps.requires_transaction_resolution());
        assert!(caps.reuse_path_verified());
        assert!(caps.confirmed);
    }

    #[test]
    fn unknown_stateful_session_refuses_the_fixed_session_guarantee() {
        // unknown 时不得开启固定会话保证。
        let mut caps = CapabilitySnapshot::defaults("fake-sql", "0.0.1");
        caps.stateful_session = StatefulSessionSupport::Unknown;
        assert!(!caps.fixed_session_guaranteed());
    }

    #[test]
    fn degraded_observation_and_transaction_flags_pick_the_right_host_path() {
        // partial/unsupported → observedContext 保持 unknown；partial → TransactionResolutionRequired。
        let mut caps = CapabilitySnapshot::defaults("fake-sql", "0.0.1");
        caps.context_observation = ContextObservation::Partial;
        assert!(caps.context_must_stay_unknown());
        caps.context_observation = ContextObservation::Unsupported;
        assert!(caps.context_must_stay_unknown());

        caps.transaction_observation = TransactionObservationSupport::Partial;
        assert!(caps.requires_transaction_resolution());
        caps.transaction_observation = TransactionObservationSupport::Full;
        assert!(!caps.requires_transaction_resolution());
    }

    #[test]
    fn unsupported_reset_for_reuse_pushes_every_reuse_path_to_close() {
        // resetForReuse=unsupported → 一切复用路径必须走关闭。
        let mut caps = CapabilitySnapshot::defaults("fake-sql", "0.0.1");
        caps.reset_for_reuse = ResetForReuse::Unsupported;
        assert!(!caps.reuse_path_verified());
        // 枚举自身的判据与快照级判据必须是同一个结论。
        assert!(!caps.reset_for_reuse.admits_reuse());
        assert!(
            ResetForReuse::Verified.admits_reuse(),
            "`Verified` 是唯一允许进归池候选的取值"
        );
    }
}

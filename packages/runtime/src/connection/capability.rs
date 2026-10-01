//! 能力快照与十项能力枚举（fake-runtime-fixtures.md §3.3 / connection-management.md §5.2）。
//!
//! 默认值逐条对齐 §3.3，并且每一项都写明「非默认取值会把哪条路径推向哪一侧」，
//! 避免夹具在 `unsupported` / `unknown` 分支上静默跳过断言（§10.4 L534）。

use serde::{Deserialize, Serialize};

/// 有状态会话支持度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StatefulSessionSupport {
    /// 默认。固定会话保证成立。
    Supported,
    /// **不得**在 `unknown` 下开启固定会话保证（§3.3）。
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
    /// §3.3 的默认值集合。
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

    /// 是否允许开启固定会话保证。`unknown` 明确**不允许**（§3.3）。
    pub const fn fixed_session_guaranteed(&self) -> bool {
        matches!(self.stateful_session, StatefulSessionSupport::Supported)
    }

    /// 上下文观测不可靠时 `observedContext` 必须保持 `unknown`，禁止回填 `initialTarget`。
    pub const fn context_must_stay_unknown(&self) -> bool {
        matches!(self.context_observation, ContextObservation::Partial | ContextObservation::Unsupported)
    }

    /// 事务观测降级时必须走 `TransactionResolutionRequired` 路径。
    pub const fn requires_transaction_resolution(&self) -> bool {
        matches!(self.transaction_observation, TransactionObservationSupport::Partial)
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
        assert_eq!(caps.transaction_observation, TransactionObservationSupport::Full);
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
        // §3.3：unknown 时不得开启固定会话保证。
        let mut caps = CapabilitySnapshot::defaults("fake-sql", "0.0.1");
        caps.stateful_session = StatefulSessionSupport::Unknown;
        assert!(!caps.fixed_session_guaranteed());
    }

    #[test]
    fn degraded_observation_and_transaction_flags_pick_the_right_host_path() {
        // §3.3：partial/unsupported → observedContext 保持 unknown；partial → TransactionResolutionRequired。
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
        // §3.3：resetForReuse=unsupported → 一切复用路径必须走关闭。
        let mut caps = CapabilitySnapshot::defaults("fake-sql", "0.0.1");
        caps.reset_for_reuse = ResetForReuse::Unsupported;
        assert!(!caps.reuse_path_verified());
    }
}
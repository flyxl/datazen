//! Shared physical identity for migration admission; profile IDs only own
//! budget accounting.
//!
//! `Data Sync`, `Data Transfer` and `Schema Diff` plans all bind one
//! `EndpointIdentity` per endpoint so the admission rules (overlap detection)
//! and the atomic budget ledger agree on a single type. The struct is
//! intentionally data-only: deriving a location key from a [`ConnectionInput`]
//! style config is feature logic and stays in `datazen-feature-core`
//! (`services::migration_endpoint::identify`).
//!
//! [`ConnectionInput`]: datazen_driver_api::ConnectionConfig

use crate::shared::id::ConnectionId;

/// 一个端点交给 runtime 的身份。
///
/// 两个字段各司其职，缺一不可：缺 `connection_id` 就没有配额记账的归属，缺
/// `service_key` 就没有重叠检测的物理依据。两者都不是装饰性的常量，但**只有
/// `service_key` 参与重叠比较**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointIdentity {
    /// 真实持久化连接配置 id —— runtime 的原子预算预留按它注册。**不**参与重叠检测。
    pub connection_id: ConnectionId,
    /// 端点位置的稳定摘要。位置无法指认（既无 host 也无 database）时留空，
    /// 交给 runtime 的 fail-closed 分支，而不是编造一个不证明任何事的摘要。
    pub service_key: String,
}

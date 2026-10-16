//! runtime 的只读观测快照。
//!
//! CM-33 的 F 半要在真实 UI 上断言「表格并发刷新时编辑器资源/上下文不变、并发资源
//! 不超过上限」，而外部计数无法归因：同一个库上同时存在宿主旧 `ConnectionManager`
//! 的会话与 runtime 的会话，`pg_stat_activity` 分不开两者。因此需要一个**由 runtime
//! 自己回答**的只读口：每个池键当前在借 / 空闲 / 关闭中 / 隔离的数量，以及预算上限
//! 与在用量。
//!
//! 本模块只做投影、不持有状态：所有数字在读取时从台账现算，池策略从 `PoolManager`
//! 现取——与 `resource/table.rs`「台账是唯一事实」的规矩一致，**没有第二本计数**。
//!
//! 池键在载荷里以摘要出现：`PoolKeyFingerprint` 本身就是 64 位 FNV 摘要
//! （`connection/types.rs` 的 `derive`），不含数据库名、账号或凭据；再补上凭据代与
//! 路由代，让「换材料 = 换键」在快照里可见。

use serde::Serialize;

use super::{PoolKeyGeneration, ResourceModel};

/// 单个池键的只读视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolObservabilityEntry {
    /// `指纹:c<凭据代>:n<路由代>`；三段都不含原始标识符。
    pub pool_key_digest: String,
    pub model: &'static str,
    pub max_idle: u64,
    /// 已签发、尚未归还（含刚打开还没挂执行的那一瞬）。
    pub in_use: u64,
    pub idle: u64,
    pub closing: u64,
    pub quarantined: u64,
    /// 该池占用的物理资源数（在借 + 空闲 + 关闭中 + 隔离）。
    pub issued: u64,
}

/// runtime 的只读观测快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeObservabilityReport {
    /// 预算的物理连接上限（`BudgetConfig::service_physical_connections`）。
    ///
    /// 台账不知道预算是谁配的，所以这个数字由组装层补（见 `with_budget`）。
    pub physical_limit: u64,
    /// 台账里的物理资源总数。
    pub physical_total: u64,
    pub in_use_total: u64,
    pub idle_total: u64,
    pub closing_total: u64,
    pub quarantined_total: u64,
    /// 预算台账视角：当前在用量与累计签发量。
    pub budget_in_use: u64,
    pub budget_granted_total: u64,
    pub pools: Vec<PoolObservabilityEntry>,
}

impl RuntimeObservabilityReport {
    pub(crate) fn from_ledger(
        pools: Vec<PoolObservabilityEntry>,
        physical_total: u64,
        in_use_total: u64,
        idle_total: u64,
        closing_total: u64,
        quarantined_total: u64,
    ) -> Self {
        Self {
            physical_limit: 0,
            physical_total,
            in_use_total,
            idle_total,
            closing_total,
            quarantined_total,
            budget_in_use: 0,
            budget_granted_total: 0,
            pools,
        }
    }

    /// 补上预算视角的三个数字（上限 / 在用量 / 累计签发量）。
    pub fn with_budget(mut self, physical_limit: u64, in_use: u64, granted_total: u64) -> Self {
        self.physical_limit = physical_limit;
        self.budget_in_use = in_use;
        self.budget_granted_total = granted_total;
        self
    }
}

/// 池键的对外摘要。指纹已是摘要，这里只再带上两代，使轮换在快照里可分辨。
pub(crate) fn pool_key_digest(key: &PoolKeyGeneration) -> String {
    format!(
        "{}:c{}:n{}",
        key.fingerprint.as_str(),
        key.credential_revision(),
        key.network_route_revision()
    )
}

pub(crate) const fn model_str(model: ResourceModel) -> &'static str {
    match model {
        ResourceModel::Stateful => "stateful",
        ResourceModel::Stateless => "stateless",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::{
        capability::ResetForReuse, ConfigRevision, ConnectionId, NamespaceTarget,
        PoolKeyFingerprint, PoolKeyInputs, ResourceId,
    };
    use crate::resource::{
        DriverCleanVerdict, HostConditionSnapshot, LeasePurpose, PoolPolicy, ProductionLeaseManager,
    };

    fn inputs() -> PoolKeyInputs {
        PoolKeyInputs {
            connection_id: ConnectionId::new("conn-secret-name"),
            config_revision: ConfigRevision::new(3),
            driver_id: "postgres".into(),
            namespace: NamespaceTarget {
                database: "secret_db".into(),
                catalog: String::new(),
                schema: "public".into(),
                path: String::new(),
            },
            execution_identity_key: "principal=u1;scope=local".into(),
            policy_isolation_key: "policy=p1".into(),
        }
    }

    fn generation(credential_revision: u64, network_route_revision: u64) -> PoolKeyGeneration {
        PoolKeyGeneration {
            fingerprint: PoolKeyFingerprint::derive(&inputs()),
            credential_revision,
            network_route_revision,
        }
    }

    #[test]
    fn the_digest_carries_no_original_identifier() {
        let digest = pool_key_digest(&generation(1, 2));
        // 指纹是 FNV 摘要：库名/连接 id 不进载荷。
        assert!(!digest.contains("secret_db"), "{digest}");
        assert!(!digest.contains("conn-secret-name"), "{digest}");
        assert!(digest.contains(":c1:n2"), "{digest}");
    }

    #[test]
    fn rotation_changes_the_digest_while_the_fingerprint_stays() {
        let before = pool_key_digest(&generation(1, 2));
        let after_credential = pool_key_digest(&generation(2, 2));
        let after_route = pool_key_digest(&generation(1, 3));
        assert_ne!(before, after_credential);
        assert_ne!(before, after_route);
        assert_eq!(
            generation(1, 2).fingerprint,
            generation(2, 2).fingerprint,
            "换材料只动代，随身指纹不变——两代正是为此单独存在"
        );
    }

    #[test]
    fn model_names_are_the_two_documented_models() {
        assert_eq!(model_str(ResourceModel::Stateful), "stateful");
        assert_eq!(model_str(ResourceModel::Stateless), "stateless");
    }

    fn host_conditions() -> HostConditionSnapshot {
        HostConditionSnapshot {
            active_executions: Vec::new(),
            outstanding_protocol: Vec::new(),
            unreleased_handles: Vec::new(),
            owner_disabled: None,
            fixed_session: false,
            reset_for_reuse: ResetForReuse::Unsupported,
        }
    }

    /// CM-33 的计数口径：打开即占槽位、进池与否只看驱动结论与宿主条件、确认关闭后
    /// 池元数据一起摘除。数字全部来自台账，没有第二本计数可以对不上。
    #[test]
    fn the_snapshot_tracks_the_ledger_by_pool_key() {
        let manager = ProductionLeaseManager::default();
        let key = generation(1, 1);
        let id = ResourceId::new("res-1");
        manager
            .opened(
                id.clone(),
                key,
                ConnectionId::new("conn-1"),
                LeasePurpose::FixedSession,
                PoolPolicy::stateful(2),
            )
            .expect("first open lands in the ledger");

        let after_open = manager.snapshot().expect("ledger readable");
        assert_eq!(after_open.physical_total, 1);
        assert_eq!(
            after_open.in_use_total, 1,
            "打开即占物理槽位，此时还没有执行挂上去"
        );
        assert_eq!(after_open.idle_total, 0);
        assert_eq!(after_open.pools.len(), 1);
        assert_eq!(after_open.pools[0].max_idle, 2, "池策略从 PoolManager 现取");
        assert_eq!(after_open.pools[0].model, "stateful");
        assert_eq!(after_open.pools[0].issued, 1);

        manager.executing(&id).expect("execution attaches");
        assert_eq!(manager.snapshot().unwrap().in_use_total, 1);

        // 驱动报 Clean、宿主条件也全清，但复位能力未声明 ⇒ 不进池、转关闭。
        let parked = manager
            .returning(
                &id,
                DriverCleanVerdict::reported_clean(),
                &host_conditions(),
            )
            .expect("return decision");
        assert!(!parked, "ResetForReuse::Unsupported 的资源绝不进空闲池");
        let closing = manager.snapshot().unwrap();
        assert_eq!(closing.in_use_total, 0);
        assert_eq!(closing.closing_total, 1);

        manager.closed(&id, true).expect("confirmed close");
        let empty = manager.snapshot().unwrap();
        assert_eq!(empty.physical_total, 0);
        assert!(empty.pools.is_empty(), "确认关闭后池元数据一起摘除");
    }
}

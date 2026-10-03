//! 代际闸门：凭据 / 路由 / ACL 轮换后的「哪些东西还能用」判定。
//!
//! [`PoolKeyFingerprint`] 只覆盖 **database + policy**（CM-67 的分片键），
//! 凭据与路由代不在它的字段里。因此这里显式维护三层代际：
//!
//! ```text
//! PoolKeyGeneration = 指纹(database+policy) × credential_revision × network_route_revision
//! CacheRevision     = PoolKeyGeneration × generation × config_revision
//! ```
//!
//! 轮换把「当前代」推高一代；**任何**在旧代上算出来的结论（空闲租约、缓存结果、
//! 排队中的请求）都因此过期，且**只能被丢弃，不能被降级改写**。

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::connection::{ConfigRevision, ConnectionId};

use crate::resource::lease::PoolKeyGeneration;

/// 一条缓存结果所属的代。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheRevision {
    pub generation: u64,
    pub pool_key: PoolKeyGeneration,
    pub config_revision: ConfigRevision,
}

impl CacheRevision {
    pub fn new(
        generation: u64,
        pool_key: PoolKeyGeneration,
        config_revision: ConfigRevision,
    ) -> Self {
        Self {
            generation,
            pool_key,
            config_revision,
        }
    }
}

/// 缓存回填的裁决结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheFillOutcome {
    /// 代次匹配，可以写入。
    Stored,
    /// **慢结果回填**：这条结果是在旧代上算出来的，写进去就是跨代污染。
    ///
    /// CM-37 / CM-67 的核心断言：轮换之后，晚到的旧代结果**必须**被丢在门外，
    /// 而不是「反正内容差不多，先存着」。
    RejectedStale { current: u64, offered: u64 },
}

impl CacheFillOutcome {
    pub const fn stored(&self) -> bool {
        matches!(self, Self::Stored)
    }

    pub const fn reason_code(&self) -> &'static str {
        match self {
            Self::Stored => "cacheFillStored",
            Self::RejectedStale { .. } => "cacheFillRejectedStale",
        }
    }
}

/// 轮换的种类。三种都只是「换掉 PoolKeyGeneration 的某一层」，副作用一致：
/// 旧代的东西停止签发并关闭，慢结果不得回填。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RotationKind {
    /// 凭据轮换（改密码 / 换用户）。
    Credentials,
    /// 网络路由轮换（隧道、端口转发、代理变更）。
    NetworkRoute,
    /// 权限轮换：连带推进 `configRevision`，旧会话**不得**被静默改配。
    AccessControl,
}

impl RotationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Credentials => "credentials",
            Self::NetworkRoute => "networkRoute",
            Self::AccessControl => "accessControl",
        }
    }

    /// 本次轮换是否推进 `configRevision`。
    pub const fn advances_config_revision(self) -> bool {
        matches!(self, Self::AccessControl)
    }
}

/// 轮换的记账。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RotationOutcome {
    pub kind: RotationKind,
    pub from: PoolKeyGeneration,
    pub to: PoolKeyGeneration,
    /// 轮换后新的缓存代。
    pub cache_generation: u64,
}

/// 每个连接的当前代 + 被淘汰的代。
pub struct GenerationRegistry {
    current: IndexMap<ConnectionId, CacheRevision>,
    retired: Vec<(ConnectionId, PoolKeyGeneration)>,
    next_generation: u64,
}

impl GenerationRegistry {
    pub fn new() -> Self {
        Self {
            current: IndexMap::new(),
            retired: Vec::new(),
            next_generation: 1,
        }
    }

    pub fn current(&self, connection_id: &ConnectionId) -> Option<&CacheRevision> {
        self.current.get(connection_id)
    }

    /// 取当前代；没有登记过就按传入材料登记第一代。
    pub fn register(
        &mut self,
        connection_id: &ConnectionId,
        pool_key: PoolKeyGeneration,
        config_revision: ConfigRevision,
    ) -> CacheRevision {
        let generation = self.next_generation;
        self.next_generation += 1;
        let revision = CacheRevision::new(generation, pool_key, config_revision);
        self.current.insert(connection_id.clone(), revision.clone());
        revision
    }

    /// 轮换到新代。旧代记入 `retired` —— 它不再「当前」，但**仍然存在物理连接**，
    /// 必须由调用方关掉；本模块只负责声明它们过期。
    pub fn rotate(
        &mut self,
        connection_id: &ConnectionId,
        pool_key: PoolKeyGeneration,
        config_revision: ConfigRevision,
    ) -> CacheRevision {
        if let Some(previous) = self.current.shift_remove(connection_id) {
            self.retired
                .push((connection_id.clone(), previous.pool_key.clone()));
        }
        let generation = self.next_generation;
        self.next_generation += 1;
        let revision = CacheRevision::new(generation, pool_key, config_revision);
        self.current.insert(connection_id.clone(), revision.clone());
        revision
    }

    /// 缓存回填闸门。
    ///
    /// 只接受**当前代**（`generation` 完全相等且池键相等）的结果。
    /// 代次落后 ⇒ [`CacheFillOutcome::RejectedStale`]，没有任何「就地升级」的旁路。
    pub fn admit_fill(
        &self,
        connection_id: &ConnectionId,
        offered: &CacheRevision,
    ) -> CacheFillOutcome {
        match self.current.get(connection_id) {
            None => CacheFillOutcome::RejectedStale {
                current: 0,
                offered: offered.generation,
            },
            Some(current) => {
                if offered.generation == current.generation && offered.pool_key == current.pool_key
                {
                    CacheFillOutcome::Stored
                } else {
                    CacheFillOutcome::RejectedStale {
                        current: current.generation,
                        offered: offered.generation,
                    }
                }
            }
        }
    }

    /// 该池键是否仍是当前代。**只有**返回 `true` 的池键可以签发空闲租约。
    pub fn is_current(&self, connection_id: &ConnectionId, pool_key: &PoolKeyGeneration) -> bool {
        self.current
            .get(connection_id)
            .is_some_and(|current| &current.pool_key == pool_key)
    }

    pub fn current_config_revision(&self, connection_id: &ConnectionId) -> Option<ConfigRevision> {
        self.current.get(connection_id).map(|r| r.config_revision)
    }

    /// 某连接被淘汰过的代（供台账关闭这些代上的空闲资源）。
    pub fn retired_for(&self, connection_id: &ConnectionId) -> Vec<&PoolKeyGeneration> {
        self.retired
            .iter()
            .filter(|(id, _)| id == connection_id)
            .map(|(_, key)| key)
            .collect()
    }

    pub fn is_retired(&self, connection_id: &ConnectionId, pool_key: &PoolKeyGeneration) -> bool {
        self.retired
            .iter()
            .any(|(id, key)| id == connection_id && key == pool_key)
    }
}

impl Default for GenerationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::types::NamespaceTarget;
    use crate::connection::{PoolKeyFingerprint, PoolKeyInputs};

    fn pool_key_inputs(config_revision: u64) -> PoolKeyInputs {
        PoolKeyInputs {
            connection_id: ConnectionId::new("conn-a"),
            config_revision: ConfigRevision::new(config_revision),
            driver_id: "postgres".to_owned(),
            namespace: NamespaceTarget {
                database: "app".to_owned(),
                catalog: "public".to_owned(),
                schema: "public".to_owned(),
                path: "public.t".to_owned(),
            },
            execution_identity_key: "exec-key".to_owned(),
            policy_isolation_key: "policy-a".to_owned(),
        }
    }

    fn key(config_revision: u64, credential_revision: u64) -> PoolKeyGeneration {
        let inputs = pool_key_inputs(config_revision);
        PoolKeyGeneration {
            fingerprint: PoolKeyFingerprint::derive(&inputs),
            credential_revision,
            network_route_revision: 1,
        }
    }

    #[test]
    fn a_slow_result_from_the_previous_generation_never_backfills() {
        let connection = ConnectionId::new("conn-a");
        let mut registry = GenerationRegistry::new();
        let old_key = key(1, 1);
        let slow = registry.register(&connection, old_key.clone(), ConfigRevision::new(1));
        // 查询在路上跑着：凭据轮换发生。
        let new_key = key(1, 2);
        let fresh = registry.rotate(&connection, new_key.clone(), ConfigRevision::new(1));
        assert_ne!(slow.generation, fresh.generation);

        let outcome = registry.admit_fill(&connection, &slow);
        assert_eq!(
            outcome,
            CacheFillOutcome::RejectedStale {
                current: fresh.generation,
                offered: slow.generation
            },
            "CM-67：慢缓存结果不得回填新一代"
        );
        assert_eq!(
            registry.admit_fill(&connection, &fresh),
            CacheFillOutcome::Stored
        );
    }

    #[test]
    fn a_same_generation_result_from_a_different_pool_key_is_also_rejected() {
        let connection = ConnectionId::new("conn-a");
        let mut registry = GenerationRegistry::new();
        let current_key = key(1, 1);
        let current = registry.register(&connection, current_key.clone(), ConfigRevision::new(1));
        let foreign = CacheRevision::new(current.generation, key(1, 2), ConfigRevision::new(1));
        assert_eq!(
            registry.admit_fill(&connection, &foreign),
            CacheFillOutcome::RejectedStale {
                current: current.generation,
                offered: current.generation
            }
        );
    }

    #[test]
    fn rotation_retires_the_previous_generation_and_only_the_new_one_issues() {
        let connection = ConnectionId::new("conn-a");
        let mut registry = GenerationRegistry::new();
        let old_key = key(1, 1);
        registry.register(&connection, old_key.clone(), ConfigRevision::new(1));
        assert!(registry.is_current(&connection, &old_key));

        let new_key = key(1, 2);
        registry.rotate(&connection, new_key.clone(), ConfigRevision::new(1));
        assert!(!registry.is_current(&connection, &old_key), "旧代不再签发");
        assert!(registry.is_current(&connection, &new_key));
        assert!(registry.is_retired(&connection, &old_key));
    }
}

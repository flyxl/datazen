//! §9 连接池：能力声明与归还裁决，**不持有任何连接**。
//!
//! ## 为什么池里没有连接
//!
//! 物理连接只在 [`ResourceTable`](super::table::ResourceTable) 里，`PoolManager` 一个都不存。
//! 它只回答一个问题：**这条已经裁决为「可回池」的连接，这个池还要不要**。所以池的占用量
//! 一律由调用方**现算**后传进来（`ResourceTable::idle_len` / `all_leases`），本文件**不自建
//! 任何计数器**——自建计数器会让两处事实不一致，而 §9.3 的「归还到 idle pool 不释放预算、
//! 只有实际 close 才释放」正要求这两处**必须**是同一份事实。
//!
//! ## 无状态能力的驱动不得伪装成固定资源池（CM-19 / CM-25）
//!
//! sqlite 这类进程内单连接驱动**共享全局状态**：把它的连接留在池里复用，等于把
//! 「一个进程只有一条连接」这条驱动契约悄悄改成「一个池可以有多条空闲连接」。
//! 所以无状态能力是**每个 PoolKey 的显式声明**（[`PoolManager::declare`]），且
//! [`PoolPolicy::max_idle`] 对无状态池**结构上**返回 0：声明是它的唯一构造方式，
//! 没有「声明了但仍可配置条数」的旁路。
//!
//! ## 池策略不与预算重复
//!
//! §9.2 说「每池 max=2，但**仍受 service 总预算约束**」。本文件只管前者；后者由
//! [`InProcessBudgetCoordinator`](crate::budget::coordinator::InProcessBudgetCoordinator) 的
//! `admit` 在开新连接前裁决（见 `lease_manager`）。两处规则不同名、不同层，互不覆盖。

use std::collections::HashMap;

use super::lease::{LeasePurpose, PoolKeyGeneration};
use super::ResourceError;

/// §9.2 短操作池默认上限：每个 PoolKey 最多留 2 条空闲连接。
pub const DEFAULT_MAX_IDLE_PER_POOL: usize = 2;

/// 驱动的连接能力模型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceModel {
    /// 连接自带会话状态，可被后续租约复用（默认）。
    Stateful,
    /// 连接**不**自带可复用状态；进程内单连接驱动属于此类。
    Stateless,
}

/// 一个池键的容量与能力策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolPolicy {
    model: ResourceModel,
    max_idle: usize,
}

impl Default for PoolPolicy {
    fn default() -> Self {
        Self::stateful(DEFAULT_MAX_IDLE_PER_POOL)
    }
}

impl PoolPolicy {
    /// 有状态池：最多留 `max_idle` 条空闲连接。
    pub const fn stateful(max_idle: usize) -> Self {
        Self {
            model: ResourceModel::Stateful,
            max_idle,
        }
    }

    /// 无状态池：容量**恒为 0**，构造时无法表达别的值。
    pub const fn stateless() -> Self {
        Self {
            model: ResourceModel::Stateless,
            max_idle: 0,
        }
    }

    pub const fn model(&self) -> ResourceModel {
        self.model
    }

    /// 生效的空闲上限。无状态池**忽略**配置值、恒为 0。
    pub const fn max_idle(&self) -> usize {
        match self.model {
            ResourceModel::Stateless => 0,
            ResourceModel::Stateful => self.max_idle,
        }
    }

    /// 这个池是否可能留下任何一条空闲连接。
    pub const fn parkable(&self) -> bool {
        self.max_idle() > 0
    }
}

/// 拒绝留下的原因。**驱动结论**与**宿主条件**的拒绝已经由 `CleanupPlan` 裁决完，
/// 这里只解释「策略层为什么不要它」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolRefusal {
    /// 无状态能力驱动：复用会破坏驱动的单连接契约（CM-19 / CM-25）。
    Stateless,
    /// 固定会话租约：会话与物理连接同生共死，不得进任何池（INV-02）。
    FixedSession,
    /// 池已达空闲上限。
    PoolFull { idle: usize, max_idle: usize },
    /// 本次归还压根没进空闲候选（已被关闭或隔离）——池策略无从表态。
    ///
    /// 和另外三个变体的区别：那三个是「**它来了但池子不要**」，这个是「**它根本没来**」。
    /// 不区分就会让调用方把一条已经关掉的连接读成「已停放」，以为还能复用。
    NotOffered,
}

impl PoolRefusal {
    pub const fn as_str(self) -> &'static str {
        match self {
            PoolRefusal::Stateless => "poolRefusedStateless",
            PoolRefusal::FixedSession => "poolRefusedFixedSession",
            PoolRefusal::PoolFull { .. } => "poolRefusedPoolFull",
            PoolRefusal::NotOffered => "poolNotOffered",
        }
    }
}

/// 归还裁决结果。`park = false` 时调用方**必须**立刻关闭这条连接（§9.3：不释放预算到
/// close 被确认为止），不能只把它留在表里。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolDecision {
    pub park: bool,
    pub refusal: Option<PoolRefusal>,
    /// 采纳这次归还之后的空闲条数。
    pub idle_after: usize,
}

impl PoolDecision {
    const fn park(idle_after: usize) -> Self {
        Self {
            park: true,
            refusal: None,
            idle_after,
        }
    }

    const fn refuse(refusal: PoolRefusal, idle_now: usize) -> Self {
        Self {
            park: false,
            refusal: Some(refusal),
            idle_after: idle_now,
        }
    }

    /// 这次归还没有产生任何空闲候选（已关闭 / 已隔离）。
    ///
    /// `idle_after` 取**当前**空闲数：既没加一，也绝不能虚报一条不存在的停放。
    pub const fn not_offered(idle_now: usize) -> Self {
        Self {
            park: false,
            refusal: Some(PoolRefusal::NotOffered),
            idle_after: idle_now,
        }
    }
}

/// 一个池键的策略。只存**策略**，不存连接。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionPool {
    key: PoolKeyGeneration,
    policy: PoolPolicy,
}

impl ConnectionPool {
    pub fn new(key: PoolKeyGeneration, policy: PoolPolicy) -> Self {
        Self { key, policy }
    }

    pub fn key(&self) -> &PoolKeyGeneration {
        &self.key
    }

    pub fn policy(&self) -> PoolPolicy {
        self.policy
    }

    /// 归还裁决。`idle_now` 是**这次归还之前**该池的空闲条数（由台账现算）。
    pub fn decide(&self, purpose: LeasePurpose, idle_now: usize) -> PoolDecision {
        if purpose == LeasePurpose::FixedSession {
            return PoolDecision::refuse(PoolRefusal::FixedSession, idle_now);
        }
        if self.policy.model() == ResourceModel::Stateless {
            return PoolDecision::refuse(PoolRefusal::Stateless, idle_now);
        }
        if idle_now >= self.policy.max_idle() {
            return PoolDecision::refuse(
                PoolRefusal::PoolFull {
                    idle: idle_now,
                    max_idle: self.policy.max_idle(),
                },
                idle_now,
            );
        }
        PoolDecision::park(idle_now + 1)
    }
}

/// 池的**派生**视图。三个数字全部由台账现算后传入，本结构自己什么都不记。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolSnapshot {
    pub key: PoolKeyGeneration,
    pub model: ResourceModel,
    pub max_idle: usize,
    /// 该池当前的空闲租约数（台账 `idle_len`）。
    pub idle_leases: usize,
    /// 该池当前占用的物理资源数（含 Opening/InUse/Cleaning/Quarantined，§9.3 占用集）。
    pub issued_leases: usize,
    /// 全节点物理并发占用（台账 `occupied_slots`），不是池内计数。
    pub occupied_slots: usize,
}

/// 全部池的策略登记处。
///
/// 未登记的池键取默认策略（有状态、每池 2 条空闲）——**登记**是声明无状态能力或
/// 非默认容量的唯一方式，所以不登记等价于「按 §9.2 默认值走」。
#[derive(Debug, Default)]
pub struct PoolManager {
    pools: HashMap<PoolKeyGeneration, ConnectionPool>,
}

impl PoolManager {
    pub fn new() -> Self {
        Self {
            pools: HashMap::new(),
        }
    }

    /// 声明某个池键的能力与容量。同一策略重复声明是幂等的；**冲突**声明直接拒绝，
    /// 因为一个池键同时有状态又无状态意味着有人把驱动契约说错了。
    pub fn declare(
        &mut self,
        key: PoolKeyGeneration,
        policy: PoolPolicy,
    ) -> Result<(), ResourceError> {
        match self.pools.get(&key) {
            Some(existing) if existing.policy != policy => Err(ResourceError::PoolPolicyConflict {
                key,
                declared: existing.policy.model().name(),
                requested: policy.model().name(),
            }),
            Some(_) => Ok(()),
            None => {
                self.pools
                    .insert(key.clone(), ConnectionPool::new(key, policy));
                Ok(())
            }
        }
    }

    /// 声明「这个池键的驱动没有可复用状态」。等价于 `declare(key, PoolPolicy::stateless())`。
    pub fn declare_stateless(&mut self, key: PoolKeyGeneration) -> Result<(), ResourceError> {
        self.declare(key, PoolPolicy::stateless())
    }

    pub fn pool(&self, key: &PoolKeyGeneration) -> Option<&ConnectionPool> {
        self.pools.get(key)
    }

    /// 取策略；未登记时按 §9.2 默认登记一条（并留在登记处，便于事后审计）。
    pub fn pool_or_default(&mut self, key: &PoolKeyGeneration) -> &ConnectionPool {
        self.pools
            .entry(key.clone())
            .or_insert_with(|| ConnectionPool::new(key.clone(), PoolPolicy::default()))
    }

    /// 归还裁决。未登记的池键先按默认策略登记。
    pub fn decide(
        &mut self,
        key: &PoolKeyGeneration,
        purpose: LeasePurpose,
        idle_now: usize,
    ) -> PoolDecision {
        self.pool_or_default(key).decide(purpose, idle_now)
    }

    /// 派生视图。三个计数由台账现算传入。
    pub fn snapshot(
        &self,
        key: PoolKeyGeneration,
        idle_leases: usize,
        issued_leases: usize,
        occupied_slots: usize,
    ) -> PoolSnapshot {
        let policy = self
            .pools
            .get(&key)
            .map_or_else(PoolPolicy::default, ConnectionPool::policy);
        PoolSnapshot {
            key,
            model: policy.model(),
            max_idle: policy.max_idle(),
            idle_leases,
            issued_leases,
            occupied_slots,
        }
    }

    pub fn declared(&self) -> usize {
        self.pools.len()
    }
}

impl ResourceModel {
    /// 稳定名（错误理由与测试断言用）。
    pub const fn name(self) -> &'static str {
        match self {
            ResourceModel::Stateful => "stateful",
            ResourceModel::Stateless => "stateless",
        }
    }
}

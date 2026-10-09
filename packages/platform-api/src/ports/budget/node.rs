//! 集群节点预算：**最小必要**契约。
//!
//! 依据 `connection-management.md` §9.3：
//!
//! > 多 worker 使用节点额度或全局 permit。失去协调器续约停止新建连接；旧额度在确认
//! > worker 隔离/连接关闭之前不重新分配，以免网络分区超额。
//!
//! ## 为什么只有三个方法
//!
//! 协调租约的**续期**不在这里：它已经是 P1 [`BudgetCoordinator::renew_node_lease`]
//! （[`NodeLease`](crate::ports::budget::NodeLease)），重复声明只会造出两个续约真相。
//! 本模块只补上 P1 没有的那一半——**节点能持有多少物理连接**、**现在还能不能新建**。
//!
//! ## 一条最容易被写错的规则
//!
//! worker 失联时，**不能**把它的额度立刻发给别人。网络分区期间两边都以为对方死了，
//! 立刻重分配就会在同一批资源上开出两份连接（§9.3 的「以免网络分区超额」）。
//! 所以额度有第三种状态：被扣住、尚未确认释放——见 [`NodeQuota::held_pending_isolation`]。

use async_trait::async_trait;

use crate::error::PortError;
use crate::id::{Counter, OrganizationId, Timestamp, WorkerId};
use crate::ports::budget::dimension::{BudgetDimension, QuotaVerdict, ResourceClass};

/// 节点额度与全局 permit 的二选一（§9.3 原文「多 worker 使用节点额度或全局 permit」）。
///
/// 这不是运行时开关，而是**部署形态**的取舍：单 worker 形态下 `PerNodeQuota` 退化成
/// 一个恒等于全局额度的格子，`GlobalPermit` 则完全没有节点这一层。两种形态下
/// 「一条物理连接必须同时拿到节点额度与全局额度」都不变。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeBudgetMode {
    /// 节点额度：每个 worker 自己有一份上限。
    PerNodeQuota,
    /// 全局 permit：不按节点切分，只有全局上限。
    GlobalPermit,
}

/// 节点当前是否还能新建连接。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeBudgetState {
    /// 协调租约在有效期内，可以新建。
    Available,
    /// drain 中，不再分派新连接（§13 的 `ServiceUnavailable` 场景之一）。
    Draining,
    /// 失去协调器续约，**停止新建连接**（§9.3）。
    ///
    /// 已有的连接不作废——它们还占着额度；只是不再新建。
    RenewalLost,
}

impl NodeBudgetState {
    /// 是否允许新建物理连接。`RenewalLost` 与 `Draining` 都不允许。
    pub const fn allows_new_connections(&self) -> bool {
        matches!(self, Self::Available)
    }
}

/// 一个 worker 节点的额度。
///
/// 物理与逻辑分开，与 [`BudgetUsage`](crate::ports::budget::BudgetUsage) 同一个理由：
/// §9.2 的「每用户/每组织逻辑 session」与「总目标连接」是两套额度。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeQuota {
    pub worker_id: WorkerId,
    /// 本节点可持有的物理连接上限。
    pub physical_connections: u32,
    /// 本节点可持有的逻辑 session 上限。
    pub logical_sessions: u32,
    /// 本节点已占用的物理连接数。
    pub in_use: u32,
    /// 失联后被扣住、**尚未确认释放**的条数。
    ///
    /// §9.3：「旧额度在确认 worker 隔离/连接关闭之前不重新分配」。把它单列成一个字段，
    /// 是为了让「扣住」在账上可见——否则「已释放」和「暂时不敢发」长得一模一样，
    /// 实现方会在分区测试里超发。
    pub held_pending_isolation: u32,
}

impl NodeQuota {
    pub const fn new(worker_id: WorkerId, physical_connections: u32) -> Self {
        Self {
            worker_id,
            physical_connections,
            logical_sessions: 0,
            in_use: 0,
            held_pending_isolation: 0,
        }
    }

    /// 还能再开几条物理连接。扣住的部分**不计入**可用。
    pub const fn remaining_physical(&self) -> u32 {
        self.physical_connections
            .saturating_sub(self.in_use)
            .saturating_sub(self.held_pending_isolation)
    }

    pub const fn can_open(&self) -> bool {
        self.remaining_physical() > 0
    }
}

/// 在某个 worker 上一笔扣账。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeCharge {
    pub worker_id: WorkerId,
    pub organization_id: OrganizationId,
    /// 主体维度。组织层由 [`BudgetDimension::capped_by`] 自动补上。
    pub dimension: BudgetDimension,
    pub class: ResourceClass,
    /// 本笔占用的物理连接条数。
    pub physical: u32,
    /// 本笔占用的逻辑 session 数。
    pub logical: u32,
    pub at: Timestamp,
}

impl NodeCharge {
    /// 本笔要扣账的全部额度格，由外到内。
    pub fn quota_keys(&self) -> Vec<crate::ports::budget::QuotaKey> {
        self.dimension
            .capped_by()
            .iter()
            .map(|dimension| crate::ports::budget::QuotaKey::new(dimension.clone(), self.class))
            .collect()
    }
}

/// 集群节点预算端口。
///
/// 业务拒绝（节点额度用尽、状态不允许新建）以 [`QuotaVerdict::Denied`] **返回**，
/// 不抛 [`PortError`]（§4.1）。
#[async_trait]
pub trait ClusterNodeBudgetPort: Send + Sync + 'static {
    /// 节点用哪一种额度形态（§9.3 的二选一）。
    async fn node_budget_mode(&self) -> Result<NodeBudgetMode, PortError>;

    /// 查节点额度。
    async fn node_quota(&self, worker_id: &WorkerId) -> Result<NodeQuota, PortError>;

    /// 查节点状态。`RenewalLost` 时实现方**必须**拒绝新建，调用方据此提前失败而不是先连再断。
    async fn node_budget_state(&self, worker_id: &WorkerId) -> Result<NodeBudgetState, PortError>;

    /// 在节点上记一笔物理扣账。额度不足时返回
    /// [`QuotaDenial::Insufficient`](crate::ports::budget::QuotaDenial::Insufficient)；
    /// 准许时返回扣账后的额度快照。
    async fn charge_node_physical(
        &self,
        charge: &NodeCharge,
    ) -> Result<QuotaVerdict<NodeQuota>, PortError>;

    /// 归还一笔扣账。worker 失联时**不要**调用：额度要等确认隔离后才可重分配（§9.3）。
    async fn release_node_physical(&self, charge: &NodeCharge) -> Result<Counter, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::PrincipalId;

    fn charge() -> NodeCharge {
        NodeCharge {
            worker_id: WorkerId::new("w-1"),
            organization_id: OrganizationId::new("org-1"),
            dimension: BudgetDimension::Principal {
                organization_id: OrganizationId::new("org-1"),
                principal_id: PrincipalId::new("u-7"),
            },
            class: ResourceClass::Interactive,
            physical: 1,
            logical: 1,
            at: Timestamp::new("2026-01-01T00:00:00Z"),
        }
    }

    #[test]
    fn a_lost_renewal_stops_new_connections() {
        // §9.3：失去协调器续约停止新建连接。已有连接不作废。
        assert!(NodeBudgetState::Available.allows_new_connections());
        assert!(!NodeBudgetState::RenewalLost.allows_new_connections());
        assert!(!NodeBudgetState::Draining.allows_new_connections());
    }

    #[test]
    fn held_quota_is_not_available_for_reallocation() {
        // §9.3：旧额度在确认 worker 隔离/连接关闭之前不重新分配，以免网络分区超额。
        let mut quota = NodeQuota::new(WorkerId::new("w-1"), 4);
        assert_eq!(quota.remaining_physical(), 4);
        assert!(quota.can_open());

        quota.in_use = 1;
        assert_eq!(quota.remaining_physical(), 3);

        // worker 失联：1 条在用 + 1 条被扣住。
        quota.held_pending_isolation = 1;
        assert_eq!(
            quota.remaining_physical(),
            2,
            "被扣住的部分不算可用，否则分区期间会超发"
        );

        // 确认隔离并关闭后才释放那条。
        quota.held_pending_isolation = 0;
        assert_eq!(quota.remaining_physical(), 3);
    }

    #[test]
    fn quota_never_goes_negative() {
        // 账实不符（实现 bug 或上报滞后）时不得回绕成巨大的正数。
        let quota = NodeQuota {
            worker_id: WorkerId::new("w-1"),
            physical_connections: 2,
            logical_sessions: 0,
            in_use: 5,
            held_pending_isolation: 4,
        };
        assert_eq!(quota.remaining_physical(), 0);
        assert!(!quota.can_open());
    }

    #[test]
    fn node_and_global_permit_are_a_deployment_choice() {
        // §9.3 的「二选一」是部署形态，不是运行时可切的状态。
        assert_ne!(NodeBudgetMode::PerNodeQuota, NodeBudgetMode::GlobalPermit);
    }

    #[test]
    fn a_node_charge_is_booked_on_every_capped_layer() {
        let keys = charge().quota_keys();
        assert_eq!(
            keys,
            vec![
                crate::ports::budget::QuotaKey::new(
                    BudgetDimension::Organization {
                        organization_id: OrganizationId::new("org-1")
                    },
                    ResourceClass::Interactive,
                ),
                crate::ports::budget::QuotaKey::new(
                    BudgetDimension::Principal {
                        organization_id: OrganizationId::new("org-1"),
                        principal_id: PrincipalId::new("u-7"),
                    },
                    ResourceClass::Interactive,
                ),
            ],
            "节点上的连接同样要扣组织额度，最外层封顶不能被 worker 层绕过"
        );
    }

    #[test]
    fn the_node_port_is_object_safe() {
        #[allow(dead_code)]
        fn round_trip(
            port: std::sync::Arc<dyn ClusterNodeBudgetPort>,
        ) -> std::sync::Arc<dyn ClusterNodeBudgetPort> {
            port
        }
    }
}

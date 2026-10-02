//! 控制资源预算：取消与健康恢复专用连接。
//!
//! 依据 `connection-management.md` §9.3（control sockets 属于占用集合）与 §9.5
//! （「control 只能用于取消/健康恢复，不用于普通 SQL」「control 保留 2」）。
//!
//! ## 为什么控制连接要单独立一个端口
//!
//! 因为它有一条普通连接没有的性质：**它的额度必须是保留额度，且不能被共享**。
//! 如果控制连接走普通池，它就可能在满池时抢走普通 SQL 的名额；反过来，普通 SQL 挂了它
//! 也可能因为没有独立余量而开不出来——而此时恰恰是最需要它的时候（取消一个卡死的语句、
//! 恢复一个连不上的连接）。所以它单独记账。
//!
//! 结构性保证：[`ControlResourceLease`] **不存 `class` 字段**，它的
//! [`quota_key`](ControlResourceLease::quota_key) 恒返回 `ResourceClass::Control`。
//! 因此「把取消连接记到共享额度上」这件事在类型层就构造不出来。
//!
//! §9.5 顺带说明了 control 保留的用途边界：只服务取消与健康恢复。端口把这两件事
//! 收成 [`ControlResourceKind`] 的两个取值，没有 `Other`——没有出口，就不会有第三个用途。

use async_trait::async_trait;

use crate::error::PortError;
use crate::id::{
    ConnectionId, Counter, ExecutionId, LeaseId, OrganizationId, ResourceId, Timestamp,
};
use crate::ports::budget::dimension::{
    BudgetDimension, BudgetUsage, QuotaKey, QuotaVerdict, ResourceClass,
};

/// 控制资源的用途。**封闭集合**：只有取消与健康恢复（§9.5）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlResourceKind {
    /// 取消一条正在执行的语句。§9.5：「control 只能用于取消/健康恢复，不用于普通 SQL」。
    Cancel { execution_id: ExecutionId },
    /// 健康检查 / 恢复探测。
    HealthProbe,
}

/// 申请一条控制连接。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlResourceRequest {
    pub kind: ControlResourceKind,
    pub organization_id: OrganizationId,
    pub connection_id: ConnectionId,
    /// 主体维度（主体 / 数据库服务 / worker / job）。组织层由
    /// [`BudgetDimension::capped_by`] 自动补上。
    pub dimension: BudgetDimension,
    /// 等待上限（毫秒）。`None` 走 §9.2 的 10 秒默认。
    pub acquire_timeout_ms: Option<u64>,
}

/// 控制连接租约。
///
/// 刻意没有 `class` 字段——类别由类型固定为 [`ResourceClass::Control`]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlResourceLease {
    pub lease_id: LeaseId,
    pub kind: ControlResourceKind,
    pub organization_id: OrganizationId,
    pub connection_id: ConnectionId,
    pub dimension: BudgetDimension,
    /// 物理连接的账目标识，不是句柄。
    pub physical: ResourceId,
    pub acquired_at: Timestamp,
}

impl ControlResourceLease {
    /// 恒为 control 类。**不依赖调用方**——这是本类型唯一的记账入口。
    pub fn quota_key(&self) -> QuotaKey {
        QuotaKey::new(
            BudgetDimension::DatabaseService {
                organization_id: self.organization_id.clone(),
                connection_id: self.connection_id.clone(),
            },
            ResourceClass::Control,
        )
    }

    /// 扣账链：组织 → 数据库服务，类别恒为 control。
    pub fn quota_keys(&self) -> Vec<QuotaKey> {
        self.dimension
            .capped_by()
            .iter()
            .map(|dimension| QuotaKey::new(dimension.clone(), ResourceClass::Control))
            .collect()
    }
}

/// 控制类余量。
///
/// §9.3 把 control sockets 算进占用集合，所以「控制类还有没有余量」= 保留额度 - 已占用的
/// control socket 数。桌面形态 §9.2 的起始配置是保留 1。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlBudgetHeadroom {
    /// 本服务为 control 预留的额度。
    pub reserved: u32,
    /// 已被控制连接占用的条数。
    pub in_use: u32,
}

impl ControlBudgetHeadroom {
    pub const fn new(reserved: u32, in_use: u32) -> Self {
        Self { reserved, in_use }
    }

    /// 还能再开几条。超发时返回 0，不回绕。
    pub const fn available(&self) -> u32 {
        self.reserved.saturating_sub(self.in_use)
    }

    /// 是否还能再开一条控制连接。
    pub const fn can_open(&self) -> bool {
        self.available() > 0
    }
}

/// 控制资源端口。
///
/// 业务拒绝（保留额度用尽、等待超时、节点失联）以 [`QuotaVerdict::Denied`] **返回**，
/// 不抛 [`PortError`]（§4.1）。`Err` 只留给预算后端自身故障。
#[async_trait]
pub trait ControlResourcePort: Send + Sync + 'static {
    /// 申请一条控制连接。额度不足时返回
    /// [`QuotaDenial::Insufficient`](crate::ports::budget::QuotaDenial::Insufficient)，
    /// 而不是借用共享额度。
    async fn acquire_control(
        &self,
        request: &ControlResourceRequest,
    ) -> Result<QuotaVerdict<ControlResourceLease>, PortError>;

    /// 控制连接 close 之后释放保留额度。
    async fn release_control(&self, lease: &ControlResourceLease) -> Result<Counter, PortError>;

    /// 查控制类余量。用例层据此在普通 SQL 被拒时给出更准确的提示
    /// （「控制类也已用尽」比「额度不足」有用）。
    async fn control_headroom(
        &self,
        organization_id: &OrganizationId,
        connection_id: &ConnectionId,
    ) -> Result<ControlBudgetHeadroom, PortError>;

    /// 查某个额度格上的用量。
    async fn usage(&self, key: &QuotaKey) -> Result<BudgetUsage, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{PrincipalId, WorkerId};

    fn lease() -> ControlResourceLease {
        ControlResourceLease {
            lease_id: LeaseId::new("ctl-1"),
            kind: ControlResourceKind::Cancel {
                execution_id: ExecutionId::new("exec-9"),
            },
            organization_id: OrganizationId::new("org-1"),
            connection_id: ConnectionId::new("conn-1"),
            dimension: BudgetDimension::Principal {
                organization_id: OrganizationId::new("org-1"),
                principal_id: PrincipalId::new("u-7"),
            },
            physical: ResourceId::new("res-c1"),
            acquired_at: Timestamp::new("2026-01-01T00:00:00Z"),
        }
    }

    #[test]
    fn a_control_lease_is_always_charged_to_the_control_class() {
        // 不接受调用方指定类别：取消连接永远不能记到共享额度上。
        assert_eq!(
            lease().quota_key().class,
            ResourceClass::Control,
            "控制租约的类别必须恒为 Control"
        );
        for key in lease().quota_keys() {
            assert_eq!(
                key.class,
                ResourceClass::Control,
                "扣账链每一层都必须是 Control"
            );
        }
    }

    #[test]
    fn the_control_lease_has_no_class_field() {
        // 结构性保证：字段表里不得出现 class，否则就有了「自报类别」的口子。
        let source = include_str!("control.rs");
        let struct_block = source
            .split("pub struct ControlResourceLease {")
            .nth(1)
            .and_then(|rest| rest.split("}").next())
            .unwrap_or_default();
        assert!(!struct_block.is_empty(), "必须能定位到控制租约的字段块");
        assert!(
            !struct_block.contains("class"),
            "ControlResourceLease 不得持有 class 字段，类别由类型固定"
        );
    }

    #[test]
    fn the_control_class_cannot_join_the_shared_pool() {
        // §9.5：共享额度按 interactive:metadata:job = 4:2:1 轮转，control 不在其中。
        // 团队形态保留 2、桌面形态保留 1，control 的容量就等于它的保留值。
        assert!(!ResourceClass::Control.may_join_shared());
        assert_eq!(ResourceClass::Control.shared_weight(), None);
    }

    #[test]
    fn control_headroom_counts_down_to_zero() {
        // §9.3：control sockets 属于占用集合，所以余量 = 保留 - 已占。
        let free = ControlBudgetHeadroom::new(2, 0);
        assert_eq!(free.available(), 2);
        assert!(free.can_open());

        let one_used = ControlBudgetHeadroom::new(2, 1);
        assert_eq!(one_used.available(), 1);
        assert!(one_used.can_open(), "还剩 1 条就必须还能开");

        let exhausted = ControlBudgetHeadroom::new(2, 2);
        assert_eq!(exhausted.available(), 0);
        assert!(!exhausted.can_open(), "保留 2 用满后不得再开控制连接");

        // 超发不回绕，避免 `available()` 变成一个巨大的正数。
        let overspent = ControlBudgetHeadroom::new(2, 5);
        assert_eq!(overspent.available(), 0);
        assert!(!overspent.can_open());
    }

    #[test]
    fn the_desktop_control_reserve_of_one_is_honoured() {
        // §9.2：桌面形态 control 保留 1。开满一条之后立刻不可再开。
        let desktop = ControlBudgetHeadroom::new(1, 1);
        assert!(!desktop.can_open());
        assert_eq!(ControlBudgetHeadroom::new(1, 0).available(), 1);
    }

    #[test]
    fn the_control_kind_set_is_closed_to_cancel_and_health_only() {
        // §9.5：control 只能用于取消/健康恢复。没有 Other、没有 Query，
        // 第三个用途就没有入口。
        let kinds = [
            ControlResourceKind::Cancel {
                execution_id: ExecutionId::new("exec-1"),
            },
            ControlResourceKind::HealthProbe,
        ];
        assert_eq!(kinds.len(), 2, "控制资源只有取消与健康恢复两种用途");
        assert_ne!(kinds[0], kinds[1], "取消与健康恢复必须可区分");
    }

    #[test]
    fn a_node_lost_denial_is_available_for_control_requests() {
        // §9.3：多 worker 协调权丢失即停止发放新资源，这一条对控制连接同样成立。
        let denial = QuotaVerdict::<ControlResourceLease>::Denied(
            crate::ports::budget::QuotaDenial::NodeLost {
                worker_id: WorkerId::new("w-3"),
            },
        );
        assert!(denial.granted().is_none());
        assert!(!denial.denied().expect("denied").is_retryable());
    }

    #[test]
    fn the_control_port_is_object_safe() {
        #[allow(dead_code)]
        fn round_trip(
            port: std::sync::Arc<dyn ControlResourcePort>,
        ) -> std::sync::Arc<dyn ControlResourcePort> {
            port
        }
    }
}

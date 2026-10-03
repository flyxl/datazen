//! 资源类别与每类桶（connection-management.md §9.5「四类资源与调度」）。
//!
//! 这一层只做**记账形状**的职责划分：谁在占、占的是保留还是共享、钉住没有。
//! 放不放行是 [`crate::budget::ledger`] 的决定，轮转顺序是 [`crate::budget::queues`] 的决定。
//!
//! 两条 §9.5 硬规则落在本文件的数据形状上而不是注释里：
//!
//! 1. **类别由服务端决定，调用方不能自报优先级**——`BudgetClaim` 只能由用例层构造。
//!    端口的 `BudgetRequest` 结构里根本没有 `class` 字段，调用方能表达的只有 `purpose`；
//!    `class` 的取值由 `crate::budget::coordinator::classify` 从 `purpose` 推导。
//! 2. **首版保留不可被其他类借用**——名额被拆成 `Reserved` / `Shared` 两种槽位，
//!    每张 permit 记下自己落在哪一槽；核销时必须退回**同一**槽。
//!    如果核销永远退回共享，保留额度的空位就会凭空漂移到别人头上，这条会被 CM-65 抓到。

use datazen_platform_api::id::{ConnectionId, OrganizationId, PrincipalId, WorkerId};
use datazen_platform_api::ports::budget::{ResourceClass, ServiceQuota};

/// 一次准入申请的完整上下文。
///
/// 只由用例层构造：`organization_id` / `principal` 来自服务端上下文（认证主体、连接归属），
/// `class` 由 `purpose` 推导，**任何情况下都不来自 IPC 参数**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetClaim {
    pub organization_id: OrganizationId,
    pub connection_id: ConnectionId,
    /// 排队与「同类内主体轮转」的归属主体（§9.5）。
    pub principal: PrincipalId,
    pub class: ResourceClass,
    /// 获批后即被钉住：已建立的连接、活跃事务、游标不得被 drain 或回收抢占（§9.5）。
    pub pinned: bool,
    /// 本次申请需要几个名额（`reserve_many` 的多端点申请会 > 1）。
    pub slots: u32,
    /// 发放资源的 worker。多 worker 部署下由节点额度租约归属（§9.3），
    /// 单 worker 部署为 `None`。`DrainScope::Node` 按它算在手 permit。
    pub worker: Option<WorkerId>,
}

impl BudgetClaim {
    /// 单名额申请。
    pub fn single(
        organization_id: OrganizationId,
        connection_id: ConnectionId,
        principal: PrincipalId,
        class: ResourceClass,
    ) -> Self {
        Self {
            organization_id,
            connection_id,
            principal,
            class,
            pinned: false,
            slots: 1,
            worker: None,
        }
    }

    /// 标记为不可抢占（活跃事务 / 游标 / 已建立的连接）。
    pub fn pinned(mut self) -> Self {
        self.pinned = true;
        self
    }

    /// 设置名额数。
    pub fn with_slots(mut self, slots: u32) -> Self {
        self.slots = slots;
        self
    }

    /// 绑定发放节点。
    pub fn on_worker(mut self, worker_id: WorkerId) -> Self {
        self.worker = Some(worker_id);
        self
    }
}

/// 名额落在哪一槽。核销必须退回同一槽，否则保留额度的空位会漂移。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    /// 本类**自己**的保留额（§9.5 首版不可借用：别的类拿不到）。
    Reserved,
    /// 共享池里按 `4:2:1` 轮转分到的那一个。
    Shared,
}

/// 一个 (服务, 类别) 的桶。保留与共享分开记账，保留是这一类专有的。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClassBucket {
    reserved_in_use: u32,
    shared_in_use: u32,
    served: u64,
}

impl ClassBucket {
    /// 保留额已用。
    pub fn reserved_in_use(&self) -> u32 {
        self.reserved_in_use
    }

    /// 共享额已用（仅统计本类从共享池拿到的部分；池水位在账本里单独维护）。
    pub fn shared_in_use(&self) -> u32 {
        self.shared_in_use
    }

    /// 本类被放行的总次数（观测用，不参与额度判定）。
    pub fn served(&self) -> u64 {
        self.served
    }

    /// 本类保留额还剩多少。
    pub fn reserved_headroom(&self, quota: &ServiceQuota, class: ResourceClass) -> u32 {
        quota
            .reserved_of(class)
            .saturating_sub(self.reserved_in_use)
    }

    /// 占一个槽。`Reserved` 槽必须先由调用方确认过 `reserved_headroom > 0`；
    /// `Shared` 槽由账本确认共享池水位。返回是否真的占上了。
    pub fn occupy(&mut self, slot: SlotKind) -> bool {
        let counter = match slot {
            SlotKind::Reserved => &mut self.reserved_in_use,
            SlotKind::Shared => &mut self.shared_in_use,
        };
        *counter = counter.saturating_add(1);
        self.served = self.served.saturating_add(1);
        true
    }

    /// 退回一个槽。**只对已经占上的槽调用**，账本凭 permit 上记录的 `SlotKind` 决定退哪一边。
    pub fn release(&mut self, slot: SlotKind) {
        let counter = match slot {
            SlotKind::Reserved => &mut self.reserved_in_use,
            SlotKind::Shared => &mut self.shared_in_use,
        };
        *counter = counter.saturating_sub(1);
    }
}

/// 一个服务的四个桶，按 [`ResourceClass::index`] 定位。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServiceClasses {
    buckets: [ClassBucket; 4],
}

impl ServiceClasses {
    /// 取某一类的桶。
    pub fn bucket(&self, class: ResourceClass) -> &ClassBucket {
        &self.buckets[class.index()]
    }

    /// 可变地取某一类的桶。
    pub fn bucket_mut(&mut self, class: ResourceClass) -> &mut ClassBucket {
        &mut self.buckets[class.index()]
    }

    /// 该类能否占一个共享槽。
    ///
    /// §9.5：control 只用于取消与健康恢复，**不参与共享轮转**——
    /// 否则一次全服务的排空就能把控制通道饿死。`ResourceClass::may_join_shared` 是这条规则的
    /// 唯一事实源，这里只做转发，不重复实现一份。
    pub fn may_join_shared(class: ResourceClass) -> bool {
        class.may_join_shared()
    }
}

/// 该类是否属于共享池的轮转成员。
///
/// 端口把 `Control` 排除在外（`may_join_shared() == false`），所以轮转只发生在
/// interactive / metadata / job 三类之间，权重 `4:2:1`。
pub fn shared_members() -> [ResourceClass; 3] {
    [
        ResourceClass::Interactive,
        ResourceClass::Metadata,
        ResourceClass::Job,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_platform_api::ports::budget::ServiceQuota;

    fn quota() -> ServiceQuota {
        ServiceQuota::new(8, [1, 1, 1, 0]).expect("夹具额度合法")
    }

    #[test]
    fn reserved_and_shared_slots_are_accounted_separately() {
        let mut bucket = ClassBucket::default();
        bucket.occupy(SlotKind::Reserved);
        bucket.occupy(SlotKind::Shared);
        assert_eq!(bucket.reserved_in_use(), 1);
        assert_eq!(bucket.shared_in_use(), 1);
        bucket.release(SlotKind::Reserved);
        assert_eq!(bucket.reserved_in_use(), 0);
        assert_eq!(bucket.shared_in_use(), 1, "退回保留槽不得影响共享占用");
    }

    #[test]
    fn control_never_joins_the_shared_pool() {
        assert!(!ServiceClasses::may_join_shared(ResourceClass::Control));
        assert!(ServiceClasses::may_join_shared(ResourceClass::Interactive));
        assert!(ServiceClasses::may_join_shared(ResourceClass::Metadata));
        assert!(ServiceClasses::may_join_shared(ResourceClass::Job));
    }

    #[test]
    fn reserved_headroom_is_class_own_and_never_shared() {
        let quota = quota();
        let mut classes = ServiceClasses::default();
        classes
            .bucket_mut(ResourceClass::Interactive)
            .occupy(SlotKind::Shared);
        // interactive 借了共享一个槽，自己的保留额一个没动。
        assert_eq!(
            classes
                .bucket(ResourceClass::Interactive)
                .reserved_headroom(&quota, ResourceClass::Interactive),
            1
        );
        // control 的保留额也不因为别人借共享而减少。
        assert_eq!(
            classes
                .bucket(ResourceClass::Control)
                .reserved_headroom(&quota, ResourceClass::Control),
            1
        );
    }
}

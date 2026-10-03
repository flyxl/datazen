//! 轮转引擎：把排队中的等待者按 §9.5 的规则放行成 permit。
//!
//! [`BudgetLedger::pump`] 是唯一入口，顺序固定：
//!
//! 1. **摘超时**——`deadline_ms <= now_ms` 的等待者离队。它们一个槽都没占，
//!    所以这里**不做任何回收**，只把 id 交回给调用方去报 `ResourceBusy`。
//! 2. **保留额**——本类专属、不参与轮转。`ResourceClass::ALL` 的下标顺序是
//!    Control → Interactive → Metadata → Job，所以排空时 control 天然先拿。
//! 3. **共享池**——加权轮转 `interactive:metadata:job = 4:2:1`，跳过空队列。
//!
//! 保留额先于共享池是可证明的不抢占：只要某类还有自己的保留空位，它就不会去挤共享池；
//! 反过来共享池排空也不会把保留额借走（`may_borrow_reserved()` 恒 `false`）。

use datazen_platform_api::ports::budget::ServiceQuota;

use crate::budget::classes::{BudgetClaim, SlotKind};
use crate::budget::ledger::BudgetLedger;
use crate::budget::queues::Waiter;
use crate::budget::quotas::BudgetConfig;
use crate::budget::records::{PumpReport, ServiceState};

impl BudgetLedger {
    /// 推进调度：先摘超时，再按本类保留额放行，最后共享池 `4:2:1` 轮转。
    pub fn pump(&mut self, now_ms: u64) -> PumpReport {
        let config = self.config.clone();
        let mut report = PumpReport {
            granted: Vec::new(),
            timed_out: Vec::new(),
        };
        for connection_id in self.services.keys().cloned().collect::<Vec<_>>() {
            // 先把服务取出来：调度过程中要写 permit 表，两者不能同时可变借用。
            let Some(mut service) = self.services.remove(&connection_id) else {
                continue;
            };
            for class in datazen_platform_api::ports::budget::ResourceClass::ALL {
                report.timed_out.extend(
                    service.queues[class.index()]
                        .expire(now_ms)
                        .into_iter()
                        .map(|waiter| waiter.id),
                );
            }
            if !service.draining {
                dispatch_service(self, &mut service, &config, now_ms, &mut report);
            }
            self.services.insert(connection_id, service);
        }
        report
    }
}

/// 先走保留额（本类专属，不参与轮转），再走共享池 `4:2:1`。
fn dispatch_service(
    ledger: &mut BudgetLedger,
    service: &mut ServiceState,
    config: &BudgetConfig,
    now_ms: u64,
    report: &mut PumpReport,
) {
    use datazen_platform_api::ports::budget::ResourceClass;
    // 保留额按类专属：哪类有空位就放行哪类，不参与轮转。`ResourceClass::index()` 顺序是
    // Control → Interactive → Metadata → Job，所以排空时 control 天然先拿。
    loop {
        let mut granted_any = false;
        for class in ResourceClass::ALL {
            let headroom = service
                .classes
                .bucket(class)
                .reserved_headroom(&config.service_quota, class);
            if headroom == 0 || service.queues[class.index()].is_empty() {
                continue;
            }
            let Some(waiter) = service.queues[class.index()].pop() else {
                continue;
            };
            let waiter_id = waiter.id;
            let claim = claim_of(waiter);
            service.classes.bucket_mut(class).occupy(SlotKind::Reserved);
            report.granted.push(ledger.commit_grant(
                &claim,
                SlotKind::Reserved,
                Some(waiter_id),
                now_ms,
            ));
            granted_any = true;
        }
        if !granted_any {
            break;
        }
    }
    // 共享池：加权轮转，跳过空队列（§9.5）。
    while service.shared_used < config.service_quota.shared {
        let Some(class) = service.rr.pick(&service.queues) else {
            break;
        };
        let Some(waiter) = service.queues[class.index()].pop() else {
            continue;
        };
        let waiter_id = waiter.id;
        let claim = claim_of(waiter);
        service.classes.bucket_mut(class).occupy(SlotKind::Shared);
        service.shared_used = service.shared_used.saturating_add(1);
        report
            .granted
            .push(ledger.commit_grant(&claim, SlotKind::Shared, Some(waiter_id), now_ms));
    }
}

/// 等待者 → 获批时的 claim（放行不改变它的归属、类别与名额数）。
fn claim_of(waiter: Waiter) -> BudgetClaim {
    BudgetClaim {
        organization_id: waiter.organization_id,
        connection_id: waiter.connection_id,
        principal: waiter.principal,
        class: waiter.class,
        pinned: false,
        slots: waiter.slots,
        worker: None,
    }
}

/// 挑一个可用的槽：先本类保留额，再共享池。control 不进共享，保留额不可借用。
pub(crate) fn pick_slot(
    service: &ServiceState,
    quota: &ServiceQuota,
    class: datazen_platform_api::ports::budget::ResourceClass,
) -> Option<SlotKind> {
    if service
        .classes
        .bucket(class)
        .reserved_headroom(quota, class)
        > 0
    {
        return Some(SlotKind::Reserved);
    }
    if class.may_join_shared() && service.shared_used < quota.shared {
        return Some(SlotKind::Shared);
    }
    None
}

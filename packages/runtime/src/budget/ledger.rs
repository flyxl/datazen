//! 预算账本（核心）：permit 记账、类别准入、排队、幂等核销。
//!
//! **本文件不含任何时间源**。每个入口都接受 `now_ms: u64`（单调毫秒），由调用方注入。
//! 于是整条调度链可以在假时钟上**同步**跑完：没有 `sleep`，没有真实等待，没有定时器竞态。
//!
//! 轮转/放行在 [`crate::budget::dispatch`]，逻辑与物理会话在 [`crate::budget::sessions`]，
//! 值类型在 [`crate::budget::records`]。本文件只留「额度准入 + 记账 + 核销」这一条主干。
//!
//! 四条账本级硬规则：
//!
//! 1. **名额按槽退回**（首版保留额不可借用）：permit 记下自己占的是本类保留槽还是共享池槽，
//!    核销时退回**同一槽**。永远退回共享会让保留额度的空位凭空漂移到别的类头上。
//! 2. **取消与超时都不漏**：等待者带 `WaiterId`，取消/超时按 id 精确摘除。等待者
//!    本来就一个槽都没占，所以取消之后共享池水位必须与取消前**逐位相同**。
//! 3. **保留额不可借用、control 不进共享**：`ResourceClass::may_borrow_reserved()` 恒为
//!    `false`，`may_join_shared()` 对 `Control` 恒为 `false`。
//! 4. **同批全有或全无**：`try_admit_many` 失败时整体回滚，一个名额都不占。

use std::collections::{BTreeMap, BTreeSet};

use datazen_platform_api::id::{
    ConnectionId, DbSessionId, LeaseId, OrganizationId, PrincipalId, WorkerId,
};
use datazen_platform_api::ports::budget::BudgetPermit;

use crate::budget::classes::{BudgetClaim, SlotKind};
use crate::budget::dispatch::pick_slot;
use crate::budget::queues::{Waiter, WaiterId};
use crate::budget::quotas::BudgetConfig;

// 值类型搬到了 `records.rs`，但**原路径必须继续可用**——本模块是它们对外的唯一门面。
pub use crate::budget::records::{
    AdmitOutcome, DenialReason, DrainReport, PermitRecord, PumpReport, QueueScope, ReleaseResult,
    Retirement, ServiceState, SessionRecord, SessionScope,
};

/// 预算账本：同步、无 I/O、无时间源。
#[derive(Debug)]
pub struct BudgetLedger {
    pub(crate) config: BudgetConfig,
    pub(crate) services: BTreeMap<ConnectionId, ServiceState>,
    pub(crate) permits: BTreeMap<LeaseId, PermitRecord>,
    pub(crate) retired: BTreeMap<LeaseId, Retirement>,
    pub(crate) sessions: BTreeMap<DbSessionId, SessionRecord>,
    pub(crate) user_logical: BTreeMap<PrincipalId, u32>,
    pub(crate) org_logical: BTreeMap<OrganizationId, u32>,
    pub(crate) draining_nodes: BTreeSet<WorkerId>,
    pub(crate) draining_orgs: BTreeSet<OrganizationId>,
    pub(crate) node_leases: BTreeMap<WorkerId, u64>,
    next_waiter: u64,
    pub(crate) next_permit: u64,
    pub(crate) next_session: u64,
    pub(crate) granted_total: u64,
    pub(crate) consumed_total: u64,
    pub(crate) returned_total: u64,
}

impl BudgetLedger {
    /// 以一份**已校验**的配置构造：所有集合都为空，所有计数器归零。
    /// 账本不做配置校验——那是 [`BudgetConfig::validate`] 的职责，
    /// 所以这里刻意没有 `Result`，避免每个调用点重复校验同一份配置。
    pub fn new(config: BudgetConfig) -> Self {
        Self {
            config,
            services: BTreeMap::new(),
            permits: BTreeMap::new(),
            retired: BTreeMap::new(),
            sessions: BTreeMap::new(),
            user_logical: BTreeMap::new(),
            org_logical: BTreeMap::new(),
            draining_nodes: BTreeSet::new(),
            draining_orgs: BTreeSet::new(),
            node_leases: BTreeMap::new(),
            next_waiter: 0,
            next_permit: 0,
            next_session: 0,
            granted_total: 0,
            consumed_total: 0,
            returned_total: 0,
        }
    }

    /// 只读配置。所有上限都从这里读，逻辑里没有任何写死的数字。
    pub fn config(&self) -> &BudgetConfig {
        &self.config
    }

    // ------------------------------------------------------------ 服务

    /// 按需登记一个 DB 服务。
    pub fn ensure_service(&mut self, connection_id: &ConnectionId) {
        self.services.entry(connection_id.clone()).or_default();
    }

    /// 服务是否在排空。
    pub fn is_draining(&self, connection_id: &ConnectionId) -> bool {
        self.services
            .get(connection_id)
            .is_some_and(|service| service.draining)
    }

    /// 共享池水位。保留额不计入。
    pub fn shared_used(&self, connection_id: &ConnectionId) -> u32 {
        self.services
            .get(connection_id)
            .map_or(0, |service| service.shared_used)
    }

    /// 共享池剩余。
    pub fn shared_headroom(&self, connection_id: &ConnectionId) -> u32 {
        self.config
            .service_quota
            .shared
            .saturating_sub(self.shared_used(connection_id))
    }

    /// 某类保留额剩余。首版不可借用，所以它就是本类的空位。
    pub fn reserved_headroom(
        &self,
        connection_id: &ConnectionId,
        class: datazen_platform_api::ports::budget::ResourceClass,
    ) -> u32 {
        self.services.get(connection_id).map_or_else(
            || self.config.service_quota.reserved_of(class),
            |service| {
                service
                    .classes
                    .bucket(class)
                    .reserved_headroom(&self.config.service_quota, class)
            },
        )
    }

    /// 某类队列深度。
    pub fn queue_depth(
        &self,
        connection_id: &ConnectionId,
        class: datazen_platform_api::ports::budget::ResourceClass,
    ) -> usize {
        self.services
            .get(connection_id)
            .map_or(0, |service| service.queues[class.index()].depth())
    }

    /// 某用户在某服务里的等待总数。
    pub fn user_queue_depth(&self, connection_id: &ConnectionId, principal: &PrincipalId) -> usize {
        self.services
            .get(connection_id)
            .map_or(0, |service| service.user_queue_depth(principal))
    }

    /// 某类累计放行次数。
    pub fn served(
        &self,
        connection_id: &ConnectionId,
        class: datazen_platform_api::ports::budget::ResourceClass,
    ) -> u64 {
        self.services
            .get(connection_id)
            .map_or(0, |service| service.classes.bucket(class).served())
    }

    /// 全部在手 permit。
    pub fn permits(&self) -> impl Iterator<Item = &PermitRecord> {
        self.permits.values()
    }

    /// 某用户在手的 permit 数。
    pub fn permits_of(&self, principal: &PrincipalId) -> usize {
        self.permits
            .values()
            .filter(|record| &record.principal == principal)
            .count()
    }

    /// 累计放行次数（计数器，跨 JS 边界安全）。
    pub fn granted_total(&self) -> u64 {
        self.granted_total
    }

    /// 累计「消费后核销」次数。重复核销**不**计入。
    pub fn consumed_total(&self) -> u64 {
        self.consumed_total
    }

    /// 累计「未使用直接退回」次数。重复核销**不**计入。
    pub fn returned_total(&self) -> u64 {
        self.returned_total
    }

    // ------------------------------------------------------------ 准入

    /// 立即准入**一个**名额；额度不够时返回 [`AdmitOutcome::Busy`] 而不是排队。
    ///
    /// 多名额申请走 [`BudgetLedger::try_admit_many`]：那条路径才做全有或全无的回滚。
    pub fn try_admit(&mut self, claim: &BudgetClaim, now_ms: u64) -> AdmitOutcome {
        if claim.slots != 1 {
            return AdmitOutcome::Denied(DenialReason::BatchNotQueueable {
                requested: claim.slots,
            });
        }
        if let Some(worker_id) = &claim.worker {
            if self.draining_nodes.contains(worker_id) {
                return AdmitOutcome::Denied(DenialReason::NodeLost {
                    worker_id: worker_id.clone(),
                });
            }
        }
        let quota = self.config.service_quota;
        let capacity = self.config.capacity_of(claim.class);
        let Some(mut service) = self.services.remove(&claim.connection_id) else {
            return AdmitOutcome::Denied(DenialReason::UnknownConnection {
                connection_id: claim.connection_id.clone(),
            });
        };
        let denied = service.draining || self.draining_orgs.contains(&claim.organization_id);
        // 同类里已经有人在等，新来的不插队：主体轮转只在队列内部发生。
        let queued = !service.queues[claim.class.index()].is_empty();
        if denied {
            self.services.insert(claim.connection_id.clone(), service);
            return AdmitOutcome::Denied(DenialReason::ServiceDraining {
                connection_id: claim.connection_id.clone(),
            });
        }
        if queued {
            self.services.insert(claim.connection_id.clone(), service);
            return AdmitOutcome::Busy(DenialReason::Exhausted {
                connection_id: claim.connection_id.clone(),
                class: claim.class,
                capacity,
            });
        }
        let Some(slot) = pick_slot(&service, &quota, claim.class) else {
            self.services.insert(claim.connection_id.clone(), service);
            return AdmitOutcome::Busy(DenialReason::Exhausted {
                connection_id: claim.connection_id.clone(),
                class: claim.class,
                capacity,
            });
        };
        service.classes.bucket_mut(claim.class).occupy(slot);
        if slot == SlotKind::Shared {
            service.shared_used = service.shared_used.saturating_add(1);
        }
        self.services.insert(claim.connection_id.clone(), service);
        AdmitOutcome::Granted(self.commit_grant(claim, slot, None, now_ms))
    }

    /// 全有或全无的多名额预留：要么一次性预留全部 permit，要么失败释放全部。
    ///
    /// 失败路径必须**一个名额都不占**：先备份被触及的服务状态与 permit 表，失败时整体还原。
    pub fn try_admit_many(
        &mut self,
        claims: &[BudgetClaim],
        now_ms: u64,
    ) -> Result<Vec<PermitRecord>, DenialReason> {
        let backup: BTreeMap<ConnectionId, ServiceState> = claims
            .iter()
            .map(|claim| claim.connection_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter_map(|id| {
                self.services
                    .get(&id)
                    .map(|state| (id.clone(), state.clone()))
            })
            .collect();
        let permits_before = self.permits.clone();
        let granted_before = self.granted_total;
        let mut granted = Vec::new();
        for claim in claims {
            match self.try_admit(claim, now_ms) {
                AdmitOutcome::Granted(record) => granted.push(record),
                AdmitOutcome::Busy(reason) | AdmitOutcome::Denied(reason) => {
                    self.services = backup;
                    self.permits = permits_before;
                    self.granted_total = granted_before;
                    return Err(reason);
                }
            }
        }
        Ok(granted)
    }

    /// 入队等待。队列满立刻 `QueueFull`，**不**静默阻塞。
    pub fn enqueue(
        &mut self,
        claim: &BudgetClaim,
        now_ms: u64,
        deadline_ms: u64,
    ) -> Result<WaiterId, DenialReason> {
        if claim.slots != 1 {
            return Err(DenialReason::BatchNotQueueable {
                requested: claim.slots,
            });
        }
        if let Some(worker_id) = &claim.worker {
            if self.draining_nodes.contains(worker_id) {
                return Err(DenialReason::NodeLost {
                    worker_id: worker_id.clone(),
                });
            }
        }
        let class_cap = self.config.queue_cap_per_class_per_service;
        let user_cap = self.config.queue_cap_per_user;
        let id = WaiterId(self.next_waiter);
        let Some(mut service) = self.services.remove(&claim.connection_id) else {
            return Err(DenialReason::UnknownConnection {
                connection_id: claim.connection_id.clone(),
            });
        };
        let class_depth = service.queues[claim.class.index()].depth();
        if class_depth as u32 >= class_cap {
            self.services.insert(claim.connection_id.clone(), service);
            return Err(DenialReason::QueueFull {
                scope: QueueScope::ClassPerService,
                cap: class_cap,
                depth: class_depth,
            });
        }
        let user_depth = service.user_queue_depth(&claim.principal);
        if user_depth as u32 >= user_cap {
            self.services.insert(claim.connection_id.clone(), service);
            return Err(DenialReason::QueueFull {
                scope: QueueScope::PerUser,
                cap: user_cap,
                depth: user_depth,
            });
        }
        service.queues[claim.class.index()].push(Waiter {
            id,
            organization_id: claim.organization_id.clone(),
            connection_id: claim.connection_id.clone(),
            principal: claim.principal.clone(),
            class: claim.class,
            slots: claim.slots,
            enqueued_at_ms: now_ms,
            deadline_ms,
        });
        self.services.insert(claim.connection_id.clone(), service);
        self.next_waiter = self.next_waiter.saturating_add(1);
        Ok(id)
    }

    /// 按 id 精确取消一个等待者。
    ///
    /// 幂等：重复取消返回 `false` 且**不改任何计数**。等待者本来一个槽都没占，
    /// 所以取消之后共享池水位必须与取消前逐位相同。
    pub fn cancel(&mut self, waiter: WaiterId) -> bool {
        for service in self.services.values_mut() {
            for class in datazen_platform_api::ports::budget::ResourceClass::ALL {
                if service.queues[class.index()].cancel(waiter).is_some() {
                    return true;
                }
            }
        }
        false
    }

    /// 把一张 permit 登记进账本。
    pub(crate) fn commit_grant(
        &mut self,
        claim: &BudgetClaim,
        slot: SlotKind,
        waiter: Option<WaiterId>,
        now_ms: u64,
    ) -> PermitRecord {
        self.next_permit = self.next_permit.saturating_add(1);
        let record = PermitRecord {
            permit_id: LeaseId::new(format!("permit-{:010}", self.next_permit)),
            organization_id: claim.organization_id.clone(),
            connection_id: claim.connection_id.clone(),
            principal: claim.principal.clone(),
            class: claim.class,
            slot,
            pinned: claim.pinned,
            worker: claim.worker.clone(),
            waiter,
            granted_at_ms: now_ms,
        };
        self.permits
            .insert(record.permit_id.clone(), record.clone());
        self.granted_total = self.granted_total.saturating_add(1);
        record
    }

    // ------------------------------------------------------------ 核销

    /// 幂等核销。
    ///
    /// 名额**按原槽退回**。重复核销返回 [`ReleaseResult::AlreadyReleased`] 且不二次记账；
    /// 从未签发的 permit 返回 [`ReleaseResult::Unknown`]——这两者必须分得开，
    /// 否则调用方的真 bug 会被"幂等"这个说法吃掉。
    pub fn release(&mut self, permit: &BudgetPermit, consumed: bool, now_ms: u64) -> ReleaseResult {
        let Some(record) = self.permits.remove(&permit.permit_id) else {
            return match self.retired.get(&permit.permit_id) {
                Some(retirement) => ReleaseResult::AlreadyReleased {
                    record: retirement.record.clone(),
                },
                None => ReleaseResult::Unknown {
                    permit_id: permit.permit_id.clone(),
                },
            };
        };
        if let Some(service) = self.services.get_mut(&record.connection_id) {
            service
                .classes
                .bucket_mut(record.class)
                .release(record.slot);
            if record.slot == SlotKind::Shared {
                service.shared_used = service.shared_used.saturating_sub(1);
            }
        }
        if consumed {
            self.consumed_total = self.consumed_total.saturating_add(1);
        } else {
            self.returned_total = self.returned_total.saturating_add(1);
        }
        self.retired.insert(
            record.permit_id.clone(),
            Retirement {
                record: record.clone(),
                outcome_is_consumed: consumed,
                released_at_ms: now_ms,
            },
        );
        ReleaseResult::Released { record }
    }
}

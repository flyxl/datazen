//! 物理资源表与 `PoolKey` 分片索引。
//!
//! 这一层是**账本的数据结构**，不含门面逻辑（门面在 `manager` / `rotation` / `adoption`）。
//! 它只做四件事：落表、按池键建空闲索引、空桶元数据按 LRU 收口、槽位回收。
//!
//! 物理连接只以 [`PhysicalHandle`] 的形式存在于**宿主内部**：它连 `pub` 都不是，
//! 也不出现在任何公共类型或返回值里（§3.2 明令不导出物理连接句柄）。

use std::collections::BTreeSet;

use indexmap::{IndexMap, IndexSet};

use crate::connection::{ConfigRevision, ConnectionId, LeaseId, ResourceId};
use crate::resource::cleanup::ExecutionCancellationRecord;
use crate::resource::generation::RotationOutcome;
use crate::resource::lease::{LeaseRecord, LeaseRequest, PoolKeyGeneration};
use crate::resource::EMPTY_POOL_METADATA_LRU_LIMIT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhysicalHandle {
    slot: u64,
}

impl PhysicalHandle {
    pub(crate) const fn new(slot: u64) -> Self {
        Self { slot }
    }

    pub(crate) const fn slot(self) -> u64 {
        self.slot
    }
}

/// 台账里的一条物理资源：账目 + 不透明身份。
///
/// 两者绑定在一起，但**只有账目**会流到公共 API；`handle` 永远不离开 `ResourceTable`。
pub(super) struct PhysicalResource {
    handle: PhysicalHandle,
    lease: LeaseRecord,
}

/// 池键桶变空时留下的元数据（§9.2：上限 32 条）。
#[derive(Debug, Clone, Copy)]
struct EmptyPoolSlot {
    last_touched_nanos: u64,
}

/// 宿主台账的物理结构：资源表 + 空闲池索引 + 空桶 LRU + 槽位池。
pub(super) struct ResourceTable {
    entries: IndexMap<ResourceId, PhysicalResource>,
    by_lease: IndexMap<LeaseId, ResourceId>,
    /// `PoolKey` → 该键下可再次签发的空闲租约（**只按数据库与策略分片**，A3.4）。
    idle_pool: IndexMap<PoolKeyGeneration, IndexSet<LeaseId>>,
    /// 空桶的 LRU 元数据；超过上限就淘汰最久没碰过的。
    empty_pool_lru: IndexMap<PoolKeyGeneration, EmptyPoolSlot>,
    free_slots: BTreeSet<u64>,
    next_slot: u64,
    lru_limit: usize,
}

impl ResourceTable {
    pub(super) fn new() -> Self {
        Self {
            entries: IndexMap::new(),
            by_lease: IndexMap::new(),
            idle_pool: IndexMap::new(),
            empty_pool_lru: IndexMap::new(),
            free_slots: BTreeSet::new(),
            next_slot: 0,
            lru_limit: EMPTY_POOL_METADATA_LRU_LIMIT,
        }
    }

    /// 槽位复用：优先复用已释放的最小槽位，因此**活跃槽位数 == 物理并发数**。
    fn take_slot(&mut self) -> PhysicalHandle {
        match self.free_slots.iter().next().copied() {
            Some(slot) => {
                self.free_slots.remove(&slot);
                PhysicalHandle::new(slot)
            }
            None => {
                self.next_slot += 1;
                PhysicalHandle::new(self.next_slot)
            }
        }
    }

    fn release_slot(&mut self, handle: PhysicalHandle) {
        self.free_slots.insert(handle.slot());
    }

    pub(super) fn insert(&mut self, resource_id: ResourceId, lease: LeaseRecord) -> PhysicalHandle {
        let handle = self.take_slot();
        self.by_lease
            .insert(lease.lease_id.clone(), resource_id.clone());
        self.entries.insert(
            resource_id,
            PhysicalResource {
                handle,
                lease: lease.clone(),
            },
        );
        handle
    }

    pub(super) fn forget(&mut self, lease_id: &LeaseId) -> Option<PhysicalResource> {
        let resource_id = self.by_lease.shift_remove(lease_id)?;
        let entry = self.entries.shift_remove(&resource_id)?;
        self.release_slot(entry.handle);
        Some(entry)
    }

    pub(super) fn lease(&self, lease_id: &LeaseId) -> Option<&LeaseRecord> {
        let resource_id = self.by_lease.get(lease_id)?;
        self.entries.get(resource_id).map(|e| &e.lease)
    }

    pub(super) fn lease_mut(&mut self, lease_id: &LeaseId) -> Option<&mut LeaseRecord> {
        let resource_id = self.by_lease.get(lease_id)?.clone();
        self.entries.get_mut(&resource_id).map(|e| &mut e.lease)
    }

    pub(super) fn all_leases(&self) -> Vec<LeaseRecord> {
        self.entries.values().map(|e| e.lease.clone()).collect()
    }

    pub(super) fn occupied_slots(&self) -> usize {
        self.entries.len()
    }

    // ---- 空闲池 ----

    pub(super) fn push_idle(
        &mut self,
        lease_id: LeaseId,
        pool_key: PoolKeyGeneration,
        now_nanos: u64,
    ) {
        self.empty_pool_lru.shift_remove(&pool_key);
        self.idle_pool.entry(pool_key).or_default().insert(lease_id);
        let _ = now_nanos;
    }

    pub(super) fn drop_idle(
        &mut self,
        lease_id: &LeaseId,
        pool_key: &PoolKeyGeneration,
        now_nanos: u64,
    ) {
        let mut became_empty = false;
        if let Some(bucket) = self.idle_pool.get_mut(pool_key) {
            bucket.shift_remove(lease_id);
            became_empty = bucket.is_empty();
        }
        if became_empty {
            self.touch_empty(pool_key.clone(), now_nanos);
        }
    }

    /// 桶变空 ⇒ 记一条元数据；桶从空变非空 ⇒ 撤掉这条元数据。
    fn touch_empty(&mut self, pool_key: PoolKeyGeneration, now_nanos: u64) {
        // 先摘后插：被再次触碰过的桶必须挪到队尾，否则 `IndexMap` 会保留它原来的位置，
        // 队首就不再是「最久没被碰到」的那一条，LRU 就退化成了 FIFO。
        self.empty_pool_lru.shift_remove(&pool_key);
        self.empty_pool_lru.insert(
            pool_key.clone(),
            EmptyPoolSlot {
                last_touched_nanos: now_nanos,
            },
        );
        while self.empty_pool_lru.len() > self.lru_limit {
            if let Some((oldest, slot)) = self.empty_pool_lru.shift_remove_index(0) {
                tracing::debug!(
                    pool_key = oldest.fingerprint.as_str(),
                    untouched_for_nanos = now_nanos.saturating_sub(slot.last_touched_nanos),
                    "evicting empty pool metadata beyond the LRU limit"
                );
            }
        }
    }

    pub(super) fn empty_pool_metadata_len(&self) -> usize {
        self.empty_pool_lru.len()
    }

    pub(super) fn idle_len(&self, pool_key: &PoolKeyGeneration) -> usize {
        self.idle_pool.get(pool_key).map_or(0, IndexSet::len)
    }

    pub(super) fn total_idle(&self) -> usize {
        self.idle_pool.values().map(IndexSet::len).sum()
    }

    /// 从空闲池里签发一条租约。同时把**已过 TTL 的空闲**挑出来交给调用方关闭。
    pub(super) fn take_idle(
        &mut self,
        pool_key: &PoolKeyGeneration,
        now_nanos: u64,
        ttl_seconds: u64,
    ) -> (Option<LeaseId>, Vec<LeaseId>) {
        let mut expired = Vec::new();
        let mut taken = None;
        let Some(bucket) = self.idle_pool.get(pool_key) else {
            return (None, expired);
        };
        for lease_id in bucket.iter() {
            let Some(record) = self.lease(lease_id) else {
                continue;
            };
            if record.idle_expired_at(now_nanos, ttl_seconds) {
                expired.push(lease_id.clone());
                continue;
            }
            if record.is_reusable() {
                taken = Some(lease_id.clone());
                break;
            }
        }
        if let Some(lease_id) = &taken {
            if let Some(bucket) = self.idle_pool.get_mut(pool_key) {
                bucket.shift_remove(lease_id);
            }
        }
        for lease_id in &expired {
            if let Some(bucket) = self.idle_pool.get_mut(pool_key) {
                bucket.shift_remove(lease_id);
            }
        }
        if self.idle_len(pool_key) == 0 {
            self.touch_empty(pool_key.clone(), now_nanos);
        }
        (taken, expired)
    }

    pub(super) fn idle_leases_for_connection(
        &self,
        connection_id: &ConnectionId,
        keep: &PoolKeyGeneration,
    ) -> Vec<LeaseId> {
        self.idle_pool
            .iter()
            .filter(|(pool_key, _)| *pool_key != keep)
            .filter_map(|(_, bucket)| {
                bucket.iter().find(|lease_id| {
                    self.lease(lease_id)
                        .is_some_and(|record| &record.connection_id == connection_id)
                })
            })
            .cloned()
            .collect()
    }
}

/// CM-38：一条在用的会话与当前配置版本之间的可见漂移。
///
/// **不静默重配**是这里的核心：旧会话继续按它自己的 `config_revision` 服务，
/// 漂移只被**报告**，是否替换由上层走 CM-68 的替换流程决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigRevisionDrift {
    pub lease_revision: ConfigRevision,
    pub current_revision: ConfigRevision,
}

impl ConfigRevisionDrift {
    pub const fn requires_replacement(&self) -> bool {
        self.lease_revision.get() != self.current_revision.get()
    }
}

/// CM-39：禁用/删除一份连接配置时的**实测**处置结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisableOutcome {
    pub connection_id: ConnectionId,
    /// 被直接丢弃、**从不执行**的排队申请数。
    pub dropped_requests: usize,
    /// 已关闭的空闲租约。
    pub closed_idle: Vec<LeaseId>,
    /// 已经在跑的执行，按**真实**取消处置记账。
    pub cancellations: Vec<ExecutionCancellationRecord>,
    /// 进入隔离、暂不强关（关闭状态不明）的租约。
    pub quarantined: Vec<LeaseId>,
}

impl DisableOutcome {
    /// 谎报风险点：只有这一种能叫「已取消」。
    pub fn cancelled_count(&self) -> usize {
        self.cancellations
            .iter()
            .filter(|record| record.outcome.cancelled())
            .count()
    }
}

/// 排队中的一条申请（§9.2：默认 10 秒获取超时）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedLease {
    pub request: LeaseRequest,
    pub enqueued_at_nanos: u64,
    /// 超过它仍未签发 ⇒ 该申请**过期**（对应 `ResourceBusy`，这里只记账不等待）。
    pub deadline_nanos: u64,
}

/// 一次排空的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueDrain {
    /// 可以继续尝试签发的申请。
    pub ready: Vec<LeaseRequest>,
    /// 等待超时、被丢弃的申请（它们**从未被执行**）。
    pub expired: Vec<LeaseRequest>,
    /// 因归属被禁用而拦下的申请。
    pub suppressed: Vec<LeaseRequest>,
}

/// A3.4：一次池键轮换的完整效果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RotationReport {
    pub outcome: RotationOutcome,
    /// 因为属于旧代而不再签发、并被立刻关闭的空闲租约。
    pub retired_idles: Vec<LeaseId>,
    /// 关闭失败、转入隔离的租约。
    pub quarantined: Vec<LeaseId>,
}

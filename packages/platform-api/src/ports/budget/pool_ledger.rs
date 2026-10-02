//! driver pool 物理预算的**进程内记账实现**。
//!
//! 契约仍然留在 [`crate::ports::budget::pool`]：类型、[`PoolReturnVerdict::decide`] 之类
//! 规则判定与错误形状都不在这里重写一遍。本文件只补契约里没有的那件事——
//! 按 §9.3 记账「每条物理连接占着哪些额度格」，并按 §9.6 让同一个 [`PoolKey`] 的
//! 连接能被下一条租约复用。
//!
//! 三条不可绕过的规则落在代码里，而不是只写在注释里：
//!
//! * **归池不释放预算**。额度释放的唯一出口是
//!   [`ReleaseDisposition::Closed`]（由 [`ReleaseReceipt::for_disposition`] 决定条数），
//!   [`DriverPoolBudgetPort::return_to_pool`] 只改 `occupancy`（`IdlePooled` / `Cleaning`）。
//! * **control 类别只有保留额度**。本实现**拒绝**用池端口申请
//!   [`ResourceClass::Control`]——§9.5 的 control socket 归
//!   [`ControlResourcePort`](crate::ports::budget::control::ControlResourcePort)，
//!   [`ControlResourceLease`](crate::ports::budget::control::ControlResourceLease)
//!   连 `class` 字段都没有，池端口无权代发。
//! * **保留额度不可借用**。[`ResourceClass::may_borrow_reserved`] 恒为 `false`，
//!   共享部分只以 [`ServiceQuota::capacity_of`] 的形状计入上限，外加一道维度总额封顶，
//!   因此任何类别都超发不了整个维度。共享额度按权重轮转是 P3 的调度。
//!
//! 本文件**不做**的事写在这里，避免调用方误以为已经做了：
//!
//! * **不排队、不等待**。没有等待队列，所以 `PoolAcquireRequest::acquire_timeout_ms`
//!   不会产出 [`QuotaDenial::WaitTimeout`]，[`QuotaDenial::QueueFull`] 也不会出现：
//!   额度不足一律立刻 [`QuotaDenial::Insufficient`]（§4.1 允许立即拒绝）。
//! * **不做 idle TTL 驱逐**。本端口没有清扫方法，idle 连接的关闭由调用方以
//!   [`ReleaseSource::IdleEvicted`] 发起 [`DriverPoolBudgetPort::release_physical`]，
//!   所以 `idle_ttl_ms` 只出现在快照里。LRU + 关闭确认后释放属 §9.5 的 P3 部分。
//! * **不把归属写进租约**。[`PoolLease`] 没有 owner 字段（归属只存在于
//!   [`PoolAcquireRequest::owner`]），所以账本也无处记录——见 `PoolStats` 上方的说明。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;

use crate::error::PortError;
use crate::id::{Counter, LeaseId, ResourceId, Timestamp};
use crate::ports::budget::coordinator::{DrainScope, DrainStatus};
use crate::ports::budget::dimension::{
    BudgetDimension, BudgetUsage, PhysicalOccupancy, QuotaDenial, QuotaKey, ResourceClass,
    ServiceQuota,
};
use crate::ports::budget::pool::{
    DriverPoolBudgetPort, PoolAcquireRequest, PoolAcquisition, PoolKey, PoolLease, PoolReturnCheck,
    PoolReturnVerdict, PoolStats, QuarantineReason, ReleaseDisposition, ReleaseReceipt,
    ReleaseSource,
};

/// 池预算配置：每个额度维度的封顶 + 池自身的两个限额。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolBudgetConfig {
    /// 维度 → 封顶。§9.5 的「总额低于保留之和则拒绝」由 [`ServiceQuota::new`] 在构造时校验，
    /// 重复出现同一维度时以**第一条**为准。
    ///
    /// **没有出现在这里的维度一律拒绝发放**（fail closed）：在没有封顶的维度上开口子，
    /// 等于把 §9.3 的占用集合变成上限不明的黑箱。
    pub dimension_quotas: Vec<(BudgetDimension, ServiceQuota)>,
    /// idle TTL（毫秒）。§9.6 的起点是 60 秒；本端口不主动清扫，因此它只进快照。
    pub idle_ttl_ms: u64,
    /// 空 pool 元数据条数上限。§9.6 的起点是 32。
    pub max_empty_entries: u32,
}

impl PoolBudgetConfig {
    /// 按 §9.6 的起点构造：idle TTL 60 秒、空条目上限 32。
    pub fn new(dimension_quotas: Vec<(BudgetDimension, ServiceQuota)>) -> Self {
        Self {
            dimension_quotas,
            idle_ttl_ms: 60_000,
            max_empty_entries: 32,
        }
    }

    /// 覆盖池自身的两个限额。它们只影响 [`PoolStats`] 的展示与 P3 的驱逐策略。
    pub fn with_pool_limits(mut self, idle_ttl_ms: u64, max_empty_entries: u32) -> Self {
        self.idle_ttl_ms = idle_ttl_ms;
        self.max_empty_entries = max_empty_entries;
        self
    }
}

/// 进程内的 [`DriverPoolBudgetPort`]：单进程 `Mutex` 保护的账本。
///
/// 线程安全来自 `Mutex` 而不是 actor：§9.6 说 P3 才上 actor，这里要的是一份**可被单测
/// 直接验证**的记账逻辑。`Send + Sync + 'static`（§4.2 的 `Arc<dyn X>` 注入）由
/// `Mutex<Ledger>` 与时间源的 `Send + Sync` 约束保证。
pub struct InMemoryDriverPoolBudget {
    /// 配置在构造时整体搬进账本，之后不可改：运行中改封顶会让已发放的额度无处安放。
    ledger: Mutex<Ledger>,
    /// 时间源由外部注入：端口不自带时钟，也不读全局。
    now: Arc<dyn Fn() -> Timestamp + Send + Sync>,
}

impl InMemoryDriverPoolBudget {
    /// 构造。`now` 注入时间源，`acquired_at` 与回执时间戳全部取自它。
    pub fn new(config: PoolBudgetConfig, now: Arc<dyn Fn() -> Timestamp + Send + Sync>) -> Self {
        let ledger = Ledger {
            quotas: config.dimension_quotas.clone(),
            used: HashMap::new(),
            pools: Vec::new(),
            lease_sequence: 0,
            physical_sequence: 0,
            idle_ttl_ms: config.idle_ttl_ms,
            max_empty_entries: config.max_empty_entries,
        };
        Self {
            ledger: Mutex::new(ledger),
            now,
        }
    }

    /// 取账本。上一次 panic 留下的毒化锁在这里恢复：账本字段都是 `u32` 计数器与 `String`，
    /// 中途 panic 不会留下半个写坏的状态，丢掉毒化只会把一次 panic 变成两次。
    fn ledger(&self) -> MutexGuard<'_, Ledger> {
        self.ledger
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn now(&self) -> Timestamp {
        (self.now)()
    }
}

/// 额度格账目。
struct Ledger {
    quotas: Vec<(BudgetDimension, ServiceQuota)>,
    /// 每个额度格已占用的物理连接数。
    used: HashMap<QuotaKey, Counter>,
    /// 池表。`PoolKey` 故意**没有** `Hash`（§9.6 要类型化等值比较），所以用 `Vec` 线性查找；
    /// 池的数量是「连接数 × 键轮换次数」，线性扫描足够。
    pools: Vec<PoolRecord>,
    /// 与 [`PoolBudgetConfig`] 同源的两个池限额，只进 [`PoolStats`]。
    idle_ttl_ms: u64,
    max_empty_entries: u32,
    /// 租约标识的本地序号（`pool-lease-{n}`）。
    lease_sequence: u64,
    /// 物理标识的本地序号（`pool-physical-{n}`）。与租约序号分开，避免两类 id 交错读数。
    physical_sequence: u64,
}

/// 一个 [`PoolKey`] 对应的池条目。
struct PoolRecord {
    key: PoolKey,
    /// 最近一次**发放**租约的类别，供 [`PoolStats::class`] 展示。`PoolKey` 本身不含类别，
    /// 而一个池可以同时服务多个类别，契约没有定义这种池的 `class` 该取什么；
    /// 这里取最近一次发放的类别，并把该歧义标为契约缺口。
    last_class: ResourceClass,
    leases: Vec<PoolLease>,
}

/// 一条租约要扣哪些额度格：外层（组织）在前，内层（数据库服务）在后。
///
/// 这与 [`PoolLease::quota_keys`] 是同一条链，但租约要**先**造出来才能问它，
/// 而额度检查发生在造租约**之前**，所以这里按 [`BudgetDimension::capped_by`] 重算一次。
fn charge_chain(dimension: &BudgetDimension, class: ResourceClass) -> Vec<QuotaKey> {
    dimension
        .capped_by()
        .into_iter()
        .map(|layer| QuotaKey::new(layer, class))
        .collect()
}

fn to_counter(value: u32) -> Counter {
    Counter::new(u64::from(value))
}

impl Ledger {
    fn capacity_of(&self, key: &QuotaKey) -> Option<u32> {
        self.quotas
            .iter()
            .find(|(dimension, _)| dimension == &key.dimension)
            .map(|(_, quota)| quota.capacity_of(key.class))
    }

    fn dimension_total(&self, dimension: &BudgetDimension) -> Option<u32> {
        self.quotas
            .iter()
            .find(|(configured, _)| configured == dimension)
            .map(|(_, quota)| quota.total)
    }

    fn used(&self, key: &QuotaKey) -> Counter {
        self.used.get(key).copied().unwrap_or(Counter::ZERO)
    }

    /// 整个维度上已占用的物理连接数：四类之和。这是 §9.3 的「组织 / 数据库服务总额」那一层。
    fn dimension_used(&self, dimension: &BudgetDimension) -> Counter {
        let mut total = Counter::ZERO;
        for class in ResourceClass::ALL {
            total = total
                .saturating_increment_by(self.used(&QuotaKey::new(dimension.clone(), class)).get());
        }
        total
    }

    /// 一个额度格还能再发多少条：类别上限与维度总额**取小**。
    ///
    /// 未配置的维度两项都是 0，即 fail closed。
    fn available(&self, key: &QuotaKey) -> u32 {
        let class_remaining = match self.capacity_of(key) {
            Some(capacity) => {
                BudgetUsage::new(self.used(key), Counter::ZERO).remaining_physical(capacity)
            }
            None => 0,
        };
        let dimension_remaining = match self.dimension_total(&key.dimension) {
            Some(total) => BudgetUsage::new(self.dimension_used(&key.dimension), Counter::ZERO)
                .remaining_physical(total),
            None => 0,
        };
        class_remaining.min(dimension_remaining)
    }

    fn charge(&mut self, keys: &[QuotaKey], count: u32) {
        for key in keys {
            let used = self.used.entry(key.clone()).or_insert(Counter::ZERO);
            *used = used.saturating_increment_by(u64::from(count));
        }
    }

    fn uncharge(&mut self, keys: &[QuotaKey], count: u32) {
        for key in keys {
            if let Some(used) = self.used.get_mut(key) {
                *used = Counter::new(used.get().saturating_sub(u64::from(count)));
            }
        }
    }

    fn next_lease_id(&mut self) -> LeaseId {
        self.lease_sequence = self.lease_sequence.saturating_add(1);
        LeaseId::new(format!("pool-lease-{}", self.lease_sequence))
    }

    fn next_physical_id(&mut self) -> ResourceId {
        self.physical_sequence = self.physical_sequence.saturating_add(1);
        ResourceId::new(format!("pool-physical-{}", self.physical_sequence))
    }

    fn insert(&mut self, lease: PoolLease) {
        let last_class = lease.class;
        match self
            .pools
            .iter_mut()
            .find(|pool| pool.key == lease.pool_key)
        {
            Some(pool) => {
                pool.last_class = last_class;
                pool.leases.push(lease);
            }
            None => self.pools.push(PoolRecord {
                key: lease.pool_key.clone(),
                last_class,
                leases: vec![lease],
            }),
        }
    }

    /// 在池里定位一条租约。**池键必须同时相等**：只按 `lease_id` 匹配会让调用方拿着
    /// 别的池的租约来改本池的账。
    fn locate(&self, lease: &PoolLease) -> Option<(usize, usize)> {
        let pool = self
            .pools
            .iter()
            .position(|pool| pool.key == lease.pool_key)?;
        let index = self.pools[pool]
            .leases
            .iter()
            .position(|held| held.lease_id == lease.lease_id)?;
        Some((pool, index))
    }

    fn missing(lease: &PoolLease) -> PortError {
        PortError::NotFound(format!(
            "池端口没有这条租约：{}（池键不匹配或已释放）",
            lease.lease_id
        ))
    }

    fn acquire(
        &mut self,
        request: &PoolAcquireRequest,
        at: Timestamp,
    ) -> Result<PoolAcquisition, PortError> {
        if request.requested_physical == 0 {
            return Err(PortError::QuotaExceeded(
                "池端口不接受 0 条物理连接的申请".to_string(),
            ));
        }
        if request.class.is_control() {
            return Err(PortError::QuotaExceeded(
                "control 类别只能由 ControlResourcePort 发放，池端口无权代发".to_string(),
            ));
        }
        if let Some(lease) = self.reuse_idle(request, at.clone()) {
            return Ok(PoolAcquisition::Granted(lease));
        }
        // 先全部检查、再全部记账：漏查一层会让额度漂，漏记一层同样会（与
        // `BudgetCoordinator::reserve_many` 的全有全无同一条纪律）。
        let chain = charge_chain(&request.dimension, request.class);
        for key in &chain {
            let available = self.available(key);
            if available < request.requested_physical {
                return Ok(PoolAcquisition::Denied(QuotaDenial::Insufficient {
                    key: key.clone(),
                    requested: request.requested_physical,
                    available,
                }));
            }
        }
        self.charge(&chain, request.requested_physical);
        let lease = PoolLease {
            lease_id: self.next_lease_id(),
            pool_key: request.pool_key.clone(),
            class: request.class,
            dimension: request.dimension.clone(),
            // `physical` 只是账目标识，真实句柄留在 adapter 侧（见 pool.rs 的「不导出」）。
            physical: self.next_physical_id(),
            physical_count: request.requested_physical,
            occupancy: PhysicalOccupancy::InUse,
            acquired_at: at,
        };
        self.insert(lease.clone());
        Ok(PoolAcquisition::Granted(lease))
    }

    /// 复用一条 idle 连接。**类别、维度、批量条数三者全同才复用**：任一不同就新开一条物理连接。
    ///
    /// 复用不重复记账——socket 本来就占着那串额度格，交出去只是换了持有者。
    fn reuse_idle(&mut self, request: &PoolAcquireRequest, at: Timestamp) -> Option<PoolLease> {
        // 先定位再改：借用 `self.pools` 期间不能调用 `self.next_lease_id()`。
        let pool_index = self
            .pools
            .iter()
            .position(|pool| pool.key == request.pool_key)?;
        let lease_index = self.pools[pool_index].leases.iter().position(|held| {
            held.occupancy == PhysicalOccupancy::IdlePooled
                && held.dimension == request.dimension
                && held.class == request.class
                && held.physical_count == request.requested_physical
        })?;
        // 只有真的复用了才消耗序号：探测失败不留下编号空洞。
        let lease_id = self.next_lease_id();
        let idle = &mut self.pools[pool_index].leases[lease_index];
        idle.occupancy = PhysicalOccupancy::InUse;
        idle.lease_id = lease_id;
        idle.acquired_at = at;
        Some(idle.clone())
    }

    fn release(
        &mut self,
        lease: &PoolLease,
        source: ReleaseSource,
        at: Timestamp,
    ) -> Result<ReleaseReceipt, PortError> {
        let (pool, index) = self.locate(lease).ok_or_else(|| Self::missing(lease))?;
        let held = self.pools[pool].leases.remove(index);
        self.uncharge(
            &charge_chain(&held.dimension, held.class),
            held.physical_count,
        );
        let remaining_idle = self.idle_physical(&held.pool_key);
        // 空池直接除名：账本里留一个没有连接的池，`pool_stats` 就会一直报它存在。
        if self.pools[pool].leases.is_empty() {
            self.pools.remove(pool);
        }
        // 本端口只有「真的 close 了」这一条释放路径，所以处置恒为 `Closed`。
        Ok(ReleaseReceipt::for_disposition(
            ReleaseDisposition::Closed,
            source,
            held.physical_count,
            to_counter(remaining_idle),
            at,
        ))
    }

    fn return_to_pool(
        &mut self,
        lease: &PoolLease,
        check: PoolReturnCheck,
    ) -> Result<PoolReturnVerdict, PortError> {
        let (pool, index) = self.locate(lease).ok_or_else(|| Self::missing(lease))?;
        let verdict = PoolReturnVerdict::decide(check);
        let occupancy = if verdict.admits_to_idle_pool() {
            PhysicalOccupancy::IdlePooled
        } else {
            // 检查没过：额度照占（§9.3），但不再计入 idle，等待调用方 close + release。
            PhysicalOccupancy::Cleaning
        };
        self.pools[pool].leases[index].occupancy = occupancy;
        Ok(verdict)
    }

    /// 标记不可复用。额度**不释放**（`Quarantined` 仍在 §9.3 的占用集合里）。
    ///
    /// `reason` 不落账：契约里没有任何读回隔离原因的出口（`PoolStats` 没有该字段，
    /// 也没有诊断端口），存一份没人读的状态只会变成死字段。调用方自己知道原因。
    fn quarantine(
        &mut self,
        lease: &PoolLease,
        _reason: QuarantineReason,
    ) -> Result<PoolLease, PortError> {
        let (pool, index) = self.locate(lease).ok_or_else(|| Self::missing(lease))?;
        self.pools[pool].leases[index].occupancy = PhysicalOccupancy::Quarantined;
        Ok(self.pools[pool].leases[index].clone())
    }

    fn pool_stats(&self, pool_key: &PoolKey) -> Result<PoolStats, PortError> {
        let pool = self
            .pools
            .iter()
            .find(|pool| &pool.key == pool_key)
            .ok_or_else(|| PortError::NotFound(format!("池端口没有这个池键：{pool_key:?}")))?;
        let physical = self.physical_of(pool);
        let idle = self.idle_physical(pool_key);
        Ok(PoolStats {
            pool_key: pool_key.clone(),
            class: pool.last_class,
            physical: to_counter(physical),
            idle: to_counter(idle),
            // 额度格与整池会重复计数，所以这里报整池总量，`logical` 恒为 0：
            // 池端口只发物理额度，逻辑 session 归 BudgetCoordinator。
            usage: BudgetUsage::new(to_counter(physical), Counter::ZERO),
            idle_ttl_ms: self.idle_ttl_ms,
            max_empty_entries: self.max_empty_entries,
        })
    }

    /// 池内物理连接总数（含 idle）。
    fn physical_of(&self, pool: &PoolRecord) -> u32 {
        pool.leases
            .iter()
            .fold(0u32, |acc, held| acc.saturating_add(held.physical_count))
    }

    fn idle_physical(&self, pool_key: &PoolKey) -> u32 {
        self.pools
            .iter()
            .find(|pool| &pool.key == pool_key)
            .map(|pool| {
                pool.leases
                    .iter()
                    .filter(|held| held.occupancy == PhysicalOccupancy::IdlePooled)
                    .fold(0u32, |acc, held| acc.saturating_add(held.physical_count))
            })
            .unwrap_or(0)
    }

    /// 排空只做**存量汇报**，不驱动关闭：本端口没有后台 actor（§9.6 把 actor 留给 P3），
    /// 关闭由调用方逐条 `release_physical(ReleaseSource::AdminDrain)`。
    ///
    /// 契约没给 `DrainStatus::draining` 定义，这里取派生语义：**还有待关闭存量就还没排空完**。
    fn drain(&self, pool_key: &PoolKey, scope: &DrainScope) -> Result<DrainStatus, PortError> {
        if let DrainScope::Node(worker_id) = scope {
            return Err(PortError::NotFound(format!(
                "节点排空归 ClusterNodeBudgetPort，本端口不排空节点：{worker_id}"
            )));
        }
        self.pools
            .iter()
            .find(|pool| &pool.key == pool_key)
            .ok_or_else(|| PortError::NotFound(format!("池端口没有这个池键：{pool_key:?}")))?;
        // 统计面按范围取：连接范围含该连接的全部池键（键轮换后的旧池也在内）。
        let outstanding: u32 =
            self.pools
                .iter()
                .filter(|pool| in_scope(pool, scope))
                .fold(0u32, |acc, pool| {
                    // 待关闭（Cleaning / Quarantined）也算 outstanding：socket 还在，额度还占着。
                    acc.saturating_add(pending_physical(pool))
                });
        Ok(DrainStatus {
            scope: scope.clone(),
            outstanding: usize::try_from(outstanding).unwrap_or(usize::MAX),
            draining: outstanding > 0,
        })
    }
}

/// 池是否落在排空范围内。
fn in_scope(pool: &PoolRecord, scope: &DrainScope) -> bool {
    match scope {
        DrainScope::Node(_) => false,
        DrainScope::Organization(organization_id) => &pool.key.organization_id == organization_id,
        DrainScope::Connection(connection_id) => &pool.key.connection_id == connection_id,
    }
}

/// 池内**仍被持有**的物理连接数：idle 之外的全部租约。
fn pending_physical(pool: &PoolRecord) -> u32 {
    pool.leases
        .iter()
        .filter(|held| held.occupancy != PhysicalOccupancy::IdlePooled)
        .fold(0u32, |acc, held| acc.saturating_add(held.physical_count))
}

#[async_trait]
impl DriverPoolBudgetPort for InMemoryDriverPoolBudget {
    async fn acquire_physical(
        &self,
        request: &PoolAcquireRequest,
    ) -> Result<PoolAcquisition, PortError> {
        let at = self.now();
        self.ledger().acquire(request, at)
    }

    async fn release_physical(
        &self,
        lease: &PoolLease,
        source: ReleaseSource,
    ) -> Result<ReleaseReceipt, PortError> {
        let at = self.now();
        self.ledger().release(lease, source, at)
    }

    async fn return_to_pool(
        &self,
        lease: &PoolLease,
        check: PoolReturnCheck,
    ) -> Result<PoolReturnVerdict, PortError> {
        self.ledger().return_to_pool(lease, check)
    }

    async fn quarantine(
        &self,
        lease: &PoolLease,
        reason: QuarantineReason,
    ) -> Result<PoolLease, PortError> {
        self.ledger().quarantine(lease, reason)
    }

    async fn usage(&self, key: &QuotaKey) -> Result<BudgetUsage, PortError> {
        let used = self.ledger().used(key);
        // `logical` 恒为 0：本端口不发逻辑额度（§9.2 的逻辑 session 归 BudgetCoordinator）。
        Ok(BudgetUsage::new(used, Counter::ZERO))
    }

    async fn pool_stats(&self, pool_key: &PoolKey) -> Result<PoolStats, PortError> {
        self.ledger().pool_stats(pool_key)
    }

    async fn drain_pool(
        &self,
        pool_key: &PoolKey,
        scope: DrainScope,
    ) -> Result<DrainStatus, PortError> {
        self.ledger().drain(pool_key, &scope)
    }
}

/// 记账行为测试。与实现同模块树，见 [`pool_ledger::tests`] 的文件级说明。
#[cfg(test)]
mod tests;

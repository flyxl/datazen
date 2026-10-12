//! 租约门面的**编排**层：句柄登记 ⇄ 物理释放的顺序、预算签发/核销、池策略落地。
//!
//! ## 为什么需要这一层（§6.2 / §6.5 / §9.3 的三处硬约束）
//!
//! 1. **关闭确认之前不许核销预算**。`ResourceManager::release` 已经在做宿主页的裁决，
//!    但 `ResourceManager` **没有**任何 `BudgetCoordinator` 依赖（CM-32 的单台账审计
//!    [`R1`](crate::resource) 禁止给它加字段），所以「预算有没有真的释放」在那层只是
//!    一个**声明出来的**布尔值，没人执行核销。本层把协调器**组合**进来补上执行面，
//!    而不是往 `ResourceManager` 上加字段。
//! 2. **句柄必须在物理释放之前、在原资源上关闭并摘除**（§6.5）。`ResourceManager` 只看见
//!    调用方交给它的 `HostConditionSnapshot`；本层持有真正的句柄登记表，宿主条件由本层
//!    推导，调用方**无法**谎报「登记的句柄都空了」。
//! 3. **池容量不与预算重复**。池的上限（每池 max idle）是策略层的规则，由
//!    [`PoolManager`](super::pool::PoolManager) 裁决；服务的总物理预算只由
//!    [`InProcessBudgetCoordinator`] 记账。两者不同名、不同层，本层只负责把两者的结论
//!    串成一次调用里的顺序。
//!
//! ## 顺序（本层存在的全部理由）
//!
//! ```text
//! acquire:  台账发放 → 新开物理连接才签发预算许可（denied ⇒ 补偿关闭并拒绝）
//! release:  在原资源上关闭已登记句柄 → 摘除登记 → 台账裁决（close / 归池 / 隔离）
//!           → 预算核销（仅被确认的 close）→ 池策略（超标/无状态 ⇒ 立刻关闭）
//! ```
//!
//! ## 不变式（每条都有测试）
//!
//! * `permits.len() == 活着的物理租约数`：开一条记一条，**被确认关闭**才核销并抹除；
//!   归池与隔离都**保留**许可（§9.3）。
//! * `handles` 里登记着的句柄数 ≥ 0 且归还后必然归零；摘除只接受确实登记过的句柄。
//! * 无状态能力池键（[`PoolPolicy::stateless`](super::pool::PoolPolicy::stateless)）永远
//!   拿不到 `park`，所以它不会以「固定资源单连接池」的样子留在池里。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use datazen_platform_api::id::{HandleId, LeaseId};
use datazen_platform_api::ports::budget::coordinator::{BudgetPermit, BudgetRequest};
use datazen_platform_api::ports::budget::{BudgetPurpose, ResourceClass};

use crate::budget::{
    AdmitOutcome, BudgetClaim, InProcessBudgetCoordinator, OrganizationPrincipal,
    PrincipalResolver, ReleaseResult,
};
use crate::connection::{ConnectionId, ExecutionId, SessionHandleRef};
use crate::resource::generation::RotationKind;
use crate::resource::table::{DisableOutcome, RotationReport};
use crate::resource::{
    LeasePurpose, LeaseRecord, LeaseRequest, LeaseState, PoolKeyGeneration, PoolManager,
    ResourceError, ResourceManager,
};

use super::cleanup::{CleanupReport, DriverCleanVerdict, HostConditionSnapshot};
use super::pool::{PoolDecision, PoolPolicy};

/// 没有接线句柄关闭能力时的理由码。
pub const NO_HANDLE_CLOSER: &str = "session handle close is not wired on this host";

/// 驱动侧的句柄关闭能力。
///
/// 关闭会话句柄（游标 / 服务端预编译语句 / 事务句柄）是**驱动**的动作：
/// [`PhysicalTransport`](super::cleanup::PhysicalTransport) 只认台账 id，没有「关句柄」
/// 这个口。所以这一半由调用方接进来；本层负责另一半 ——
/// **先在原资源上关掉、再摘除登记、最后才允许物理释放**（§6.5）。
pub trait HandleCloser {
    /// 在**原资源**上关闭一个已登记句柄。返回 `Err` ⇒ 该句柄仍然登记着，归还被拒。
    fn close(&mut self, handle: &SessionHandleRef) -> Result<(), ResourceError>;
}

impl<F> HandleCloser for F
where
    F: FnMut(&SessionHandleRef) -> Result<(), ResourceError>,
{
    fn close(&mut self, handle: &SessionHandleRef) -> Result<(), ResourceError> {
        self(handle)
    }
}

/// 默认实现：**关不掉任何句柄**。租约上没有登记句柄时它永不生效；
/// 有登记句柄却用默认值归还 ⇒ 必然被拒，而不是悄悄放行（§6.5）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoHandleCloser;

impl HandleCloser for NoHandleCloser {
    fn close(&mut self, _handle: &SessionHandleRef) -> Result<(), ResourceError> {
        Err(ResourceError::TransportRefused(NO_HANDLE_CLOSER))
    }
}

/// 句柄登记表：**对称**的两半 —— 登记进来的必须摘得掉，摘不动的会挡住归还。
#[derive(Debug, Default)]
pub struct HandleRegistry {
    by_lease: HashMap<LeaseId, Vec<SessionHandleRef>>,
}

impl HandleRegistry {
    /// 登记一个句柄。重复登记同一 `handle_id` 直接拒绝：同一句柄登记两次意味着
    /// 有两个持有者，之后的关闭顺序就没有唯一定义。
    pub fn register(
        &mut self,
        lease_id: &LeaseId,
        handle: SessionHandleRef,
    ) -> Result<(), ResourceError> {
        let entry = self.by_lease.entry(lease_id.clone()).or_default();
        if entry
            .iter()
            .any(|existing| existing.handle_id == handle.handle_id)
        {
            return Err(ResourceError::HandleAlreadyRegistered(
                handle.handle_id.as_str().to_owned(),
            ));
        }
        entry.push(handle);
        Ok(())
    }

    /// 摘除登记。**没有登记过就是错误**，不是静默成功：静默成功会让「句柄还挂在会话上」
    /// 这件事在审计里消失（§6.5 的前置条件正是这条）。
    pub fn deregister(
        &mut self,
        lease_id: &LeaseId,
        handle_id: &HandleId,
    ) -> Result<SessionHandleRef, ResourceError> {
        let entry = self
            .by_lease
            .get_mut(lease_id)
            .ok_or_else(|| ResourceError::HandleNotRegistered(handle_id.as_str().to_owned()))?;
        let position = entry
            .iter()
            .position(|existing| &existing.handle_id == handle_id)
            .ok_or_else(|| ResourceError::HandleNotRegistered(handle_id.as_str().to_owned()))?;
        let handle = entry.remove(position);
        if entry.is_empty() {
            self.by_lease.remove(lease_id);
        }
        Ok(handle)
    }

    /// 清空一条租约的全部登记，返回被丢弃的句柄。
    ///
    /// 归还成功后正常路径下恒为空；非空说明宿主条件漏报了仍在会话上的句柄，
    /// 此时把它们一并丢弃 —— §6.5「永不复用旧句柄」。
    pub fn evict_lease(&mut self, lease_id: &LeaseId) -> Vec<SessionHandleRef> {
        self.by_lease.remove(lease_id).unwrap_or_default()
    }

    pub fn for_lease(&self, lease_id: &LeaseId) -> Vec<SessionHandleRef> {
        self.by_lease.get(lease_id).cloned().unwrap_or_default()
    }

    pub fn is_registered(&self, lease_id: &LeaseId, handle_id: &HandleId) -> bool {
        self.by_lease
            .get(lease_id)
            .is_some_and(|entry| entry.iter().any(|handle| &handle.handle_id == handle_id))
    }

    /// 全部登记中的句柄数。
    pub fn len(&self) -> usize {
        self.by_lease.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.by_lease.is_empty()
    }
}

/// 句柄侧的结果。放在 [`LeaseRelease`] 里是因为顺序本身就是结论的一部分：
/// `closed_handles == deregistered.len()` 才说明「关完即摘」，否则归还根本不该完成。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HandleRelease {
    /// 在原资源上关闭成功的句柄数。
    pub closed_handles: usize,
    /// 关闭后**立即**摘除登记的句柄（顺序证据）。
    pub deregistered: Vec<HandleId>,
    /// 归还完成后清空残留时丢弃的句柄。正常路径恒为空。
    pub residual_dropped: Vec<HandleId>,
}

impl HandleRelease {
    /// 归还后不该再留下任何登记。
    pub fn is_settled(&self) -> bool {
        self.closed_handles == self.deregistered.len() && self.residual_dropped.is_empty()
    }
}

/// 归还之后池策略要求再关一次的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleEviction {
    /// 池采纳了这条空闲连接，或归还压根没有进池。
    NotNeeded,
    /// 已按池策略关闭（无状态能力 / 超上限 / 固定会话）。
    Closed,
    /// 池要求关闭但关闭没被确认：连接仍在台账里，预算仍然占着（§6.2 的隔离义务）。
    Failed(&'static str),
}

/// 一次归还的完整结论 = 台账裁决 + 句柄顺序 + 池策略。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseRelease {
    pub cleanup: CleanupReport,
    pub handles: HandleRelease,
    pub pool: PoolDecision,
    pub eviction: IdleEviction,
    /// 物理预算是否**真的**被本层核销（不是台账声明的那个布尔值）。
    pub budget_settled: bool,
}

/// §6.2 的编排门面。
///
/// 组成关系（**不是**继承，也**不是**往 `ResourceManager` 上加字段）：
///
/// ```text
/// LeaseManager
///   ├─ ResourceManager      物理台账与宿主页裁决（唯一持有租约行的地方）
///   ├─ PoolManager          池策略：要不要留、留几条（不持有连接、不自建计数）
///   ├─ HandleRegistry       句柄登记 ⇄ 摘除（对称）
///   ├─ permits: HashMap     **许可句柄**登记，不是第二个占用计数器
///   └─ InProcessBudgetCoordinator  物理占用的唯一记账方
/// ```
///
/// `permits` 与 `ResourceTable` 的关系是**一一对应**而不是第二份事实：它只为把许可
/// *句柄*（`BudgetPermit`）从 `admit` 的返回值带到 `release_permit` 的调用点；
/// 真正的占用量始终由台账现算（`occupied_slots`）。归池与隔离都**不**抹除许可 ——
/// 只有被确认的 close 才抹（§9.3）。
pub struct LeaseManager {
    resources: ResourceManager,
    pools: PoolManager,
    handles: HandleRegistry,
    permits: HashMap<LeaseId, BudgetPermit>,
    budget: Arc<InProcessBudgetCoordinator>,
    principals: Arc<dyn PrincipalResolver>,
}

impl LeaseManager {
    pub fn new(resources: ResourceManager, budget: Arc<InProcessBudgetCoordinator>) -> Self {
        Self {
            resources,
            pools: PoolManager::new(),
            handles: HandleRegistry::default(),
            permits: HashMap::new(),
            budget,
            principals: Arc::new(OrganizationPrincipal),
        }
    }

    /// 换掉主体解析器。默认 [`OrganizationPrincipal`] 从申请的组织推导主体，
    /// **任何情况下都不接受调用方逐请求传主体**。
    pub fn with_principals(mut self, principals: Arc<dyn PrincipalResolver>) -> Self {
        self.principals = principals;
        self
    }

    // ---- 台账的只读视图 ----

    pub fn resources(&self) -> &ResourceManager {
        &self.resources
    }

    pub fn pools(&self) -> &PoolManager {
        &self.pools
    }

    pub fn pools_mut(&mut self) -> &mut PoolManager {
        &mut self.pools
    }

    pub fn handles(&self) -> &HandleRegistry {
        &self.handles
    }

    /// 在手的物理预算许可数（= 被确认关闭之外、仍然占着预算的租约数）。
    pub fn permits_outstanding(&self) -> usize {
        self.permits.len()
    }

    // ---- 池能力声明 ----

    /// 声明某个池键的容量与能力（§9.2 / CM-19 / CM-25）。
    pub fn declare_pool(
        &mut self,
        key: PoolKeyGeneration,
        policy: PoolPolicy,
    ) -> Result<(), ResourceError> {
        self.pools.declare(key, policy)
    }

    /// 声明「这个池键的驱动没有可复用状态」（sqlite 这类进程内单连接驱动）。
    pub fn declare_stateless_pool(&mut self, key: PoolKeyGeneration) -> Result<(), ResourceError> {
        self.pools.declare_stateless(key)
    }

    // ---- 获取 ----

    /// 发放一条租约：台账发放 → （只有**新开**物理连接才）签发预算许可。
    ///
    /// ## 为什么是「开完再准入」
    ///
    /// 预准入会误伤**空闲复用**：空闲连接按 §9.2「仍然受 service 总预算约束」已经占着
    /// 一个名额，再申请一次会被同一份账本拒掉，于是「有空闲就不用」这条最基本的收益
    /// 消失。所以本层的判据是**有没有已经握在手里的许可**：复用的租约 `permits` 里已经有
    /// 它（归池从不核销预算），新开的才没有。
    ///
    /// 代价是最坏情况多开再关一次连接。顺序上的替代方案（准入必须早于
    /// `PhysicalTransport::open`）需要预算端口新增「预留后未用可撤销」的能力 ——
    /// 冻结上游，不能自行扩端口，因此留作待裁定项。
    pub fn acquire(&mut self, request: &LeaseRequest) -> Result<LeaseRecord, ResourceError> {
        self.reconcile();
        let record = self.resources.acquire(request)?;
        let lease_id = record.lease_id.clone();
        if !self.permits.contains_key(&lease_id) {
            let permit = match self.admit_physical(&record) {
                Ok(permit) => permit,
                Err(error) => {
                    // 补偿：刚开的连接没有被任何许可能担保，必须立刻关闭而不是留在台账里。
                    // 没有许可 ⇒ 没有预算需要核销，`reconcile` 也就不会漏掉什么。
                    if let Err(close_error) = self.resources.retire(&lease_id) {
                        tracing::warn!(
                            lease_id = lease_id.as_str(),
                            reason = close_error.reason(),
                            "compensation close after a refused physical quota did not confirm"
                        );
                    }
                    return Err(error);
                }
            };
            self.permits.insert(lease_id, permit);
        }
        Ok(record)
    }

    /// 为一次**新**物理占用签发许可。
    fn admit_physical(&self, record: &LeaseRecord) -> Result<BudgetPermit, ResourceError> {
        let organization_id = self.budget.organization_id().clone();
        // 组织来自协调器（服务端上下文），主体由解析器从申请推导 —— 两者都不由调用方给。
        let probe = BudgetRequest::new(
            organization_id.clone(),
            record.connection_id.clone(),
            budget_purpose(record.purpose),
        );
        let principal = self.principals.resolve(&probe);
        let claim = BudgetClaim::single(
            organization_id,
            record.connection_id.clone(),
            principal,
            class_for(record.purpose),
        )
        // 已建立的连接不可被 drain / 回收抢占（§9.5 无抢占）。
        .pinned();
        match self.budget.admit(&claim) {
            AdmitOutcome::Granted(permit) => Ok(permit.to_port_permit()),
            // `Busy`（额度暂时不够）与 `Denied`（排队也没用）在这里**同义**：物理开启
            // 不排队。要让它排队就得把本层的 `acquire` 变成 async 并接上 `acquire_with`
            // 的等待者取消语义，那是端口形状之外的决定，留作待裁定项。
            AdmitOutcome::Busy(reason) | AdmitOutcome::Denied(reason) => {
                Err(ResourceError::PhysicalQuotaRefused(reason))
            }
        }
    }

    /// 把许可与台账里活着的租约行**对齐**：行已经不在（被确认关闭）却还握着许可的，
    /// 在这里核销并抹除。
    ///
    /// 存在的理由是台账里有**对调用方静默**的关闭：`acquire` 内部会顺手关掉过期的空闲
    /// 连接、轮换会关掉旧代空闲、`force_close` 的错误分支会留在隔离。任何持有按资源
    /// 记账的调用方都必须以台账为准做一次对齐，否则许可会永久悬着 —— 而悬着的许可
    /// 等于**占着预算的幽灵连接**。
    pub fn reconcile(&mut self) -> usize {
        let live: HashSet<LeaseId> = self
            .resources
            .leases()
            .iter()
            .map(|record| record.lease_id.clone())
            .collect();
        let orphans: Vec<LeaseId> = self
            .permits
            .keys()
            .filter(|lease_id| !live.contains(*lease_id))
            .cloned()
            .collect();
        let mut settled = 0;
        for lease_id in orphans {
            if self.settle(&lease_id) {
                settled += 1;
            }
        }
        settled
    }

    /// 核销并抹除一条许可。重复核销是幂等的（协调器返回 `AlreadyReleased`）。
    fn settle(&mut self, lease_id: &LeaseId) -> bool {
        match self.permits.remove(lease_id) {
            Some(permit) => {
                matches!(
                    self.budget.release_permit(&permit, true),
                    ReleaseResult::Released { .. } | ReleaseResult::AlreadyReleased { .. }
                )
            }
            // 没有许可 ⇒ 从来没有为它记过账，没什么可核销。
            None => false,
        }
    }

    // ---- 执行边界（§9.3 的占用集里「Cleaning」的前置条件）----

    pub fn accepts_execution(&self, lease_id: &LeaseId) -> Result<(), ResourceError> {
        self.resources.accepts_execution(lease_id)
    }

    pub fn begin_execution(
        &mut self,
        lease_id: &LeaseId,
        execution_id: ExecutionId,
    ) -> Result<(), ResourceError> {
        self.resources.begin_execution(lease_id, execution_id)
    }

    pub fn end_execution(&mut self, lease_id: &LeaseId) -> Result<(), ResourceError> {
        self.resources.end_execution(lease_id)
    }

    // ---- 归还 ----

    /// 归还一条租约。**句柄的关闭与摘除由本层串在最前面**（§6.5）：
    ///
    /// 1. 逐个在**原资源**上关闭登记中的句柄，成功即摘除登记；关不掉就停在这里，
    ///    租约保持原状（不放行、不归池、不关闭），调用方可以重试。
    /// 2. 把剩下的（调用方声明的）未释放句柄并进宿主条件 —— `CleanupPlan` 因此不可能
    ///    在有句柄挂着的情况下判归池。
    /// 3. 交给台账裁决。**被确认的 close** 才核销预算；归池与隔离都保留许可（§9.3）。
    /// 4. 若判归池，按池策略再裁决一次：策略拒绝就**立刻**关闭，不能只把它留在表里。
    pub fn release(
        &mut self,
        lease_id: &LeaseId,
        host: &HostConditionSnapshot,
        driver: &DriverCleanVerdict,
        closer: &mut dyn HandleCloser,
    ) -> Result<LeaseRelease, ResourceError> {
        self.reconcile();
        let record = self
            .resources
            .lease(lease_id)
            .ok_or_else(|| ResourceError::UnknownResource(lease_id.as_str().to_owned()))?
            .clone();
        let pool_key = record.pool_key.clone();
        let purpose = record.purpose;

        // (1) 先关句柄，再摘登记。
        let mut handle_release = HandleRelease::default();
        for handle in self.handles.for_lease(lease_id) {
            closer.close(&handle)?;
            handle_release.closed_handles += 1;
            let handle_id = handle.handle_id.clone();
            self.handles.deregister(lease_id, &handle_id)?;
            handle_release.deregistered.push(handle_id);
        }

        // (2) 宿主条件：调用方声明的那一份 **∪** 本层登记册**此刻还剩**的句柄。
        //
        // **不是「以调用方为准」。** 本层是句柄登记册的唯一权威（模块头第 2 条：
        // 「本层持有真正的句柄登记表，宿主条件由本层推导，调用方**无法**谎报
        // 『登记的句柄都空了』」）。旧写法把 `merged.unreleased_handles` 整个换成
        // 调用方那一份（只滤 `closed`），于是 `self.handles` 里真实存在的句柄
        // 对裁决完全不可见 —— 传一份 `HostConditionSnapshot::default()` 就能把
        // 「登记册里还有一条开着的事务」盖掉，`ReturnedToPool` 当场被记下来，
        // 而那条连接刚关掉一条事务句柄。§9.4「§6.5 已登记句柄为空」是归池的
        // 必要条件，它不该取决于调用方记不记得报。
        //
        // **为什么关掉并摘净之后反而允许归池**：§6.5 的次序是「先在原 resource 上
        // 回滚/关闭句柄并从 actor 注销，**确认后才释放资源**」。关掉 + 摘登记 =
        // 登记册已清空 = 那一项归池前置**已满足**，此时仍不放行就等于让一条彻底
        // 收尾过的租约永远回不了池。`lease_handle_registry.rs:102` 与
        // `lease_transitions_matrix.rs:570` 钉的正是这个，且断言消息写的是
        // "once the registry is clean the lease may go back to the idle pool"。
        // 这与 `HostConditionSnapshot::outstanding_handles` 过滤 `closed` 是同一条
        // 理由，两处口径必须逐字一致。
        //
        // 所以并入的是**本层登记册此刻还剩的**句柄：关得掉 ⇒ (1) 里已经摘干净 ⇒
        // 什么都不并；关不掉 ⇒ `?` 已在 (1) 提前返回，走不到这里。这条并集因此
        // 只对「登记册里有 (1) 管不到的残留」生效 —— 而那正是 (4) 要报的漏报。
        let mut merged = host.clone();
        merged.unreleased_handles = host
            .unreleased_handles
            .iter()
            .filter(|handle| !handle.closed)
            .cloned()
            .chain(self.handles.for_lease(lease_id))
            .collect();

        // (3) 台账裁决。摘登记发生在**之前**，所以这里的 close 一定排在句柄关闭之后。
        //
        // 空闲位计数必须取在裁决**之前**：`ResourceManager::release` 归池的那一刹
        // 会把本租约自己 push 进 idle_pool，事后再数就把自己算成"别人"，
        // 上限判定会整体偏移一格（`idle_after` 永远多报 1，等于池子少留一只）。
        let idle_before = self.resources.table.idle_len(&pool_key);
        let cleanup = self.resources.release(lease_id, merged, *driver)?;

        // (4) 残留清空：正常路径恒为空，非空即说明宿主条件漏报。
        for handle in self.handles.evict_lease(lease_id) {
            handle_release.residual_dropped.push(handle.handle_id);
        }

        // (5) 只有被确认的关闭才核销预算。
        let mut budget_settled = if cleanup.physical_budget_released {
            self.settle(lease_id)
        } else {
            false
        };

        // (6) 池策略：无状态能力 / 超上限 ⇒ 归池之后立刻关闭。
        //
        // 只有**真的进了 idle_pool** 的租约才轮到池策略表态。已关闭 / 已隔离的租约
        // 从来不是空闲候选，硬去问一次会得到 `park = true, idle_after = idle+1`，
        // 把一条关掉的连接报成「已停放」—— 调用方会据此以为还能复用。报 `NotOffered`。
        let pool = if cleanup.returned_to_pool() {
            self.pools.decide(&pool_key, purpose, idle_before)
        } else {
            PoolDecision::not_offered(idle_before)
        };
        let eviction = if cleanup.returned_to_pool() && !pool.park {
            match self.resources.retire(lease_id) {
                Ok(report) if report.physical_budget_released => {
                    // 逐出也是一次被确认的关闭：核销结果必须并进 `budget_settled`，
                    // 否则报告会说「未核销」，而台账里其实已经空了。
                    if self.settle(lease_id) {
                        budget_settled = true;
                    }
                    IdleEviction::Closed
                }
                Ok(_) => IdleEviction::Failed("pool eviction closed without a budget release"),
                Err(error) => {
                    tracing::warn!(
                        lease_id = lease_id.as_str(),
                        reason = error.reason(),
                        "pool policy refused the idle but the close did not confirm"
                    );
                    IdleEviction::Failed(error.reason())
                }
            }
        } else {
            IdleEviction::NotNeeded
        };

        Ok(LeaseRelease {
            cleanup,
            handles: handle_release,
            pool,
            eviction,
            budget_settled,
        })
    }

    /// 强制关闭一条租约（隔离排障、池淘汰）。确认关闭 ⇒ 核销预算。
    pub fn retire(&mut self, lease_id: &LeaseId) -> Result<CleanupReport, ResourceError> {
        self.reconcile();
        for handle in self.handles.evict_lease(lease_id) {
            tracing::warn!(
                lease_id = lease_id.as_str(),
                handle_id = handle.handle_id.as_str(),
                "retiring a lease that still had a registered handle"
            );
        }
        let report = self.resources.retire(lease_id)?;
        if report.physical_budget_released {
            self.settle(lease_id);
        }
        Ok(report)
    }

    // ---- 生命周期编排的转发（各自在前后各做一次对齐）----

    pub fn rotate(
        &mut self,
        kind: RotationKind,
        request: &LeaseRequest,
    ) -> Result<RotationReport, ResourceError> {
        self.reconcile();
        let report = self.resources.rotate(kind, request)?;
        self.reconcile();
        Ok(report)
    }

    pub fn disable(
        &mut self,
        connection_id: &ConnectionId,
    ) -> Result<DisableOutcome, ResourceError> {
        self.reconcile();
        let outcome = self.resources.disable(connection_id);
        self.reconcile();
        outcome
    }
}

/// 用途 ⇒ 预算类别。`Control` 只能由 `ControlResourcePort` 发放，池端口无权代发。
const fn class_for(purpose: LeasePurpose) -> ResourceClass {
    match purpose {
        LeasePurpose::FixedSession | LeasePurpose::ShortOperation => ResourceClass::Interactive,
        LeasePurpose::Metadata => ResourceClass::Metadata,
    }
}

/// 用途 ⇒ 预算用途。**后台作业流量不在这条路上**：它必须自己带
/// [`BudgetPurpose::BackgroundJob`] 走端口的 `acquire_with`，才进得了排队与取消语义。
const fn budget_purpose(purpose: LeasePurpose) -> BudgetPurpose {
    match purpose {
        LeasePurpose::FixedSession | LeasePurpose::ShortOperation | LeasePurpose::Metadata => {
            BudgetPurpose::UserInteractive
        }
    }
}

// ---- 句柄登记 ----

impl LeaseManager {
    /// 登记一个会话句柄（§6.5：**必须在 execution 返回终态之前**登记）。
    ///
    /// 只在**发放态**租约上登记：已关闭 / 隔离的租约上登记句柄意味着有人打算拿它继续
    /// 服务，而 §6.2 明确隔离态不可再获取。
    pub fn register_handle(
        &mut self,
        lease_id: &LeaseId,
        handle: SessionHandleRef,
    ) -> Result<(), ResourceError> {
        let record = self
            .resources
            .lease(lease_id)
            .ok_or_else(|| ResourceError::UnknownResource(lease_id.as_str().to_owned()))?
            .clone();
        if record.state != LeaseState::InUse {
            return Err(ResourceError::IllegalTransition {
                from: record.state,
                to: LeaseState::InUse,
            }
            .logged("register a session handle on a lease that is not issued"));
        }
        // 句柄必须挂在**这条租约自己的**资源上：换资源复用旧句柄就是 §6.5 说的蒙混。
        if handle.resource_id != record.resource_id {
            return Err(ResourceError::HandleResourceMismatch(
                handle.handle_id.as_str().to_owned(),
            ));
        }
        self.handles.register(lease_id, handle)
    }

    /// 摘除一个句柄的登记。**没有登记过就是错误**。
    ///
    /// 这不是「关句柄」：驱动侧的关闭走 [`HandleCloser`]。这里是给那些已经自行关闭、
    /// 但登记还挂着的事务 / 游标用的**对称另一半**（§6.5）。
    pub fn deregister_handle(
        &mut self,
        lease_id: &LeaseId,
        handle_id: &HandleId,
    ) -> Result<SessionHandleRef, ResourceError> {
        self.handles.deregister(lease_id, handle_id)
    }
}

//! `BudgetCoordinator` 的服务端适配器：单实例内存额度账（WR-D4，v1 形态）。
//!
//! [web-server-host §10.3](../../../../../docs/architecture/platform/web-server-host.md)
//! 的「`BudgetCoordinator` 首版为单实例实现」：进程重启后额度归零是单实例
//! 形态的自洽前提（旧物理连接已随进程终止）；持久化分配账
//! （persistence-model §3.11 `budget_allocations`）是 P9 全局预算的增量，
//! v1 **不落库**——与 SessionDirectory 的内存形态同一条登记口径。
//!
//! 端口契约（`crates/datazen/.../budget/coordinator.rs`）的记账规则逐条落地：
//!
//! * `acquire` **可取消等待**：超时是配置起点（构造参数；默认常量是文档的
//!   「首版配置起点而非硬编码」，部署覆盖走构造参数），超时报
//!   `PortError::ProviderTimeout`（用例层翻译为 `ResourceBusy`）。
//! * `try_acquire` 立即尝试：额度不足返回 `Ok(None)`，不等待。
//! * `reserve_many` **要么全部预留成功，要么整体失败并释放**：任何一端
//!   拿不到都把已拿到的许可按 `ReleaseOutcome::Unused` 归还，不留半边。
//! * `release` **幂等**（INV-10）：同一 permit 重复归还只释放一次额度。
//! * `renew_node_lease` 失败即视为失联：未知/过期租约返回 `NotFound`，
//!   调用方停止发放新资源。
//! * `drain` 单向：进入排空后**不再发放**匹配范围的新 permit；端口没有
//!   解除排空的方法，v1 不添加。
//!
//! ## 额度维度裁决（登记）
//!
//! 限额是**构造参数注入**的两级内存计数器：`node_limit`（本节点在飞许可
//! 总上限）与 `organization_limit`（每组织在飞许可上限，§10.3「组织级额度」
//! 的最小可执行形）。用户/数据库级额度与持久化记账归 P9，未在 v1 实现。
//! `BudgetSnapshot.organization_id` 在节点级实现里是**报告标签**
//! （构造时给定的 `snapshot_scope`）：v1 的快照是节点聚合值，多组织分账
//! 快照是已登记缺口。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;

use datazen::execution::application::budget::{
    BudgetCoordinator, BudgetPermit, BudgetPermitSet, BudgetRequest, BudgetSnapshot, DrainScope,
    DrainStatus, NodeLease, ReleaseOutcome,
};
use datazen::shared::error::PortError;
use datazen::shared::id::{Counter, LeaseId, OrganizationId, Timestamp, WorkerId};

/// `acquire` 的默认等待上限（§9.2「首版配置起点」；部署覆盖走构造参数）。
pub const DEFAULT_ACQUIRE_TIMEOUT_SECS: u64 = 10;
/// 节点租约的默认续期窗（v1 配置起点）。
pub const DEFAULT_NODE_LEASE_TTL_SECS: u64 = 60;

/// 排空范围的匹配规则：Node 范围扣住**全部**新许可（v1 许可不带 worker
/// 归属，按保守失败关闭处理），Organization / Connection 精确匹配。
fn drain_matches(scope: &DrainScope, request: &BudgetRequest) -> bool {
    match scope {
        DrainScope::Node(_) => true,
        DrainScope::Organization(organization) => *organization == request.organization_id,
        DrainScope::Connection(connection) => *connection == request.connection_id,
    }
}

#[derive(Default)]
struct BudgetState {
    /// 在飞许可（持有 = 占用额度）。
    permits: HashMap<LeaseId, BudgetPermit>,
    /// 已归还许可的登记（幂等核销的判据，INV-10）。
    released: HashSet<LeaseId>,
    /// 单向排空范围。
    drains: Vec<DrainScope>,
    /// 节点租约账（键是租约 id 字面量；`LeaseId` 无 `Hash` 约束问题——
    /// string_id 派生了 Hash，这里存字符串是为续约键的借用面更窄）。
    node_leases: HashMap<String, NodeLease>,
    /// 累计发放（快照的 granted_total）。
    granted_total: u64,
    /// 核销计数（Consumed / Unused 各自累计；v1 只作观测）。
    consumed: u64,
    unused: u64,
}

impl BudgetState {
    fn is_draining(&self, request: &BudgetRequest) -> bool {
        self.drains
            .iter()
            .any(|scope| drain_matches(scope, request))
    }

    fn organization_in_use(&self, organization_id: &OrganizationId) -> usize {
        self.permits
            .values()
            .filter(|permit| &permit.organization_id == organization_id)
            .count()
    }

    fn has_capacity(
        &self,
        request: &BudgetRequest,
        node_limit: u64,
        organization_limit: u64,
    ) -> bool {
        (self.permits.len() as u64) < node_limit
            && (self.organization_in_use(&request.organization_id) as u64) < organization_limit
    }
}

/// 共享内部状态：`Arc` 让 `spawn_blocking` 拿到 `'static` 句柄。
struct Inner {
    state: Mutex<BudgetState>,
    slot_available: Condvar,
    node_limit: u64,
    organization_limit: u64,
    acquire_timeout: Duration,
    lease_ttl: Duration,
    snapshot_scope: OrganizationId,
}

/// 预算协调器（单实例内存账）。
#[derive(Clone)]
pub struct ServerBudgetCoordinator {
    inner: Arc<Inner>,
}

impl ServerBudgetCoordinator {
    /// 规格默认配置（超时 10s、租约 60s）。
    pub fn new(node_limit: u64, organization_limit: u64, snapshot_scope: OrganizationId) -> Self {
        Self::with_configuration(
            node_limit,
            organization_limit,
            snapshot_scope,
            Duration::from_secs(DEFAULT_ACQUIRE_TIMEOUT_SECS),
            Duration::from_secs(DEFAULT_NODE_LEASE_TTL_SECS),
        )
    }

    /// 部署配置注入（超时/租约都是「首版配置起点」，不是协议常量）。
    pub fn with_configuration(
        node_limit: u64,
        organization_limit: u64,
        snapshot_scope: OrganizationId,
        acquire_timeout: Duration,
        lease_ttl: Duration,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(BudgetState::default()),
                slot_available: Condvar::new(),
                node_limit,
                organization_limit,
                acquire_timeout,
                lease_ttl,
                snapshot_scope,
            }),
        }
    }

    /// 登记一个节点租约（bootstrap / 管理路径的进入口）。
    pub fn register_node(&self, worker_id: WorkerId) -> NodeLease {
        let lease_id = uuid::Uuid::new_v4().to_string();
        let lease = NodeLease {
            lease_id: LeaseId::new(lease_id.clone()),
            worker_id,
            expires_at: Self::lease_deadline(self.inner.lease_ttl),
        };
        let mut state = self.lock();
        state.node_leases.insert(lease_id, lease.clone());
        lease
    }

    fn lease_deadline(ttl: Duration) -> Timestamp {
        Timestamp::new(
            (chrono::Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_default())
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        )
    }

    fn lock(&self) -> MutexGuard<'_, BudgetState> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn stamp() -> Timestamp {
        Timestamp::new(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
    }

    fn grant(state: &mut BudgetState, request: &BudgetRequest) -> BudgetPermit {
        let permit = BudgetPermit {
            permit_id: LeaseId::new(uuid::Uuid::new_v4().to_string()),
            organization_id: request.organization_id.clone(),
            connection_id: request.connection_id.clone(),
            granted_at: Self::stamp(),
        };
        state
            .permits
            .insert(permit.permit_id.clone(), permit.clone());
        state.granted_total = state.granted_total.saturating_add(1);
        permit
    }

    // ── 同步核心（trait 实现只做 spawn_blocking 包装） ──

    fn sync_acquire(&self, request: &BudgetRequest) -> Result<BudgetPermit, PortError> {
        let deadline = std::time::Instant::now() + self.inner.acquire_timeout;
        let mut state = self.lock();
        loop {
            // 排空即拒绝：等待毫无意义（排空是单向的）。
            if state.is_draining(request) {
                return Err(PortError::QuotaExceeded("budget is draining".into()));
            }
            if state.has_capacity(
                request,
                self.inner.node_limit,
                self.inner.organization_limit,
            ) {
                return Ok(Self::grant(&mut state, request));
            }
            let now = std::time::Instant::now();
            if now >= deadline {
                // 超时是业务拒绝事实（用例层翻译为 ResourceBusy）。
                return Err(PortError::ProviderTimeout(
                    "budget acquire timed out".into(),
                ));
            }
            let (guard, waited) = self
                .inner
                .slot_available
                .wait_timeout(state, deadline - now)
                .unwrap_or_else(PoisonError::into_inner);
            state = guard;
            if waited.timed_out()
                && !state.has_capacity(
                    request,
                    self.inner.node_limit,
                    self.inner.organization_limit,
                )
            {
                return Err(PortError::ProviderTimeout(
                    "budget acquire timed out".into(),
                ));
            }
        }
    }

    fn sync_try_acquire(&self, request: &BudgetRequest) -> Result<Option<BudgetPermit>, PortError> {
        let mut state = self.lock();
        if state.is_draining(request) {
            return Err(PortError::QuotaExceeded("budget is draining".into()));
        }
        if state.has_capacity(
            request,
            self.inner.node_limit,
            self.inner.organization_limit,
        ) {
            return Ok(Some(Self::grant(&mut state, request)));
        }
        Ok(None)
    }

    fn sync_reserve_many(&self, requests: &[BudgetRequest]) -> Result<BudgetPermitSet, PortError> {
        let mut granted: Vec<BudgetPermit> = Vec::with_capacity(requests.len());
        for request in requests {
            match self.sync_try_acquire(request) {
                Ok(Some(permit)) => granted.push(permit),
                Ok(None) | Err(_) => {
                    // 整体失败：已拿到的许可按 Unused 归还，不留半边。
                    for permit in &granted {
                        let _ = self.sync_release(permit, ReleaseOutcome::Unused);
                    }
                    return Err(PortError::QuotaExceeded(
                        "budget reserve_many could not reserve every leg".into(),
                    ));
                }
            }
        }
        Ok(BudgetPermitSet::new(granted))
    }

    fn sync_release(
        &self,
        permit: &BudgetPermit,
        outcome: ReleaseOutcome,
    ) -> Result<(), PortError> {
        let mut state = self.lock();
        if state.released.contains(&permit.permit_id) {
            // 幂等核销（INV-10）：重复归还不重复释放额度。
            return Ok(());
        }
        state.released.insert(permit.permit_id.clone());
        if state.permits.remove(&permit.permit_id).is_some() {
            match outcome {
                ReleaseOutcome::Consumed => state.consumed = state.consumed.saturating_add(1),
                ReleaseOutcome::Unused => state.unused = state.unused.saturating_add(1),
            }
        }
        // 未知的 permit 同样登记为已归还：释放是幂等的，第二次调用无害。
        drop(state);
        self.inner.slot_available.notify_all();
        Ok(())
    }

    fn sync_renew_node_lease(&self, lease: &NodeLease) -> Result<NodeLease, PortError> {
        let mut state = self.lock();
        let now = parse_ts(&Self::stamp());
        let registered = state
            .node_leases
            .get(lease.lease_id.as_str())
            .ok_or_else(|| PortError::NotFound("node lease".into()))?;
        // 已过期租约的续约 = 失联（调用方停止发放新资源）。
        if let (Some(expires), Some(now)) = (parse_ts(&registered.expires_at), now) {
            if expires < now {
                return Err(PortError::NotFound("node lease expired".into()));
            }
        }
        let renewed = NodeLease {
            lease_id: registered.lease_id.clone(),
            worker_id: registered.worker_id.clone(),
            expires_at: Self::lease_deadline(self.inner.lease_ttl),
        };
        state
            .node_leases
            .insert(lease.lease_id.as_str().to_string(), renewed.clone());
        Ok(renewed)
    }

    fn sync_drain(&self, scope: DrainScope) -> Result<DrainStatus, PortError> {
        let mut state = self.lock();
        let outstanding = match &scope {
            DrainScope::Node(_) => state.permits.len(),
            DrainScope::Organization(organization) => state.organization_in_use(organization),
            DrainScope::Connection(connection) => state
                .permits
                .values()
                .filter(|permit| &permit.connection_id == connection)
                .count(),
        };
        state.drains.push(scope.clone());
        Ok(DrainStatus {
            scope,
            outstanding,
            draining: true,
        })
    }

    fn sync_snapshot(&self) -> Result<BudgetSnapshot, PortError> {
        let state = self.lock();
        Ok(BudgetSnapshot {
            organization_id: self.inner.snapshot_scope.clone(),
            in_use: Counter::new(state.permits.len() as u64),
            granted_total: Counter::new(state.granted_total),
        })
    }
}

fn parse_ts(value: &Timestamp) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value.as_str())
        .ok()
        .map(|parsed| parsed.with_timezone(&chrono::Utc))
}

#[async_trait]
impl BudgetCoordinator for ServerBudgetCoordinator {
    async fn acquire(&self, request: BudgetRequest) -> Result<BudgetPermit, PortError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.sync_acquire(&request))
            .await
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }

    async fn try_acquire(&self, request: BudgetRequest) -> Result<Option<BudgetPermit>, PortError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.sync_try_acquire(&request))
            .await
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }

    async fn reserve_many(&self, requests: &[BudgetRequest]) -> Result<BudgetPermitSet, PortError> {
        let requests = requests.to_vec();
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.sync_reserve_many(&requests))
            .await
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }

    async fn renew_node_lease(&self, lease: &NodeLease) -> Result<NodeLease, PortError> {
        let lease = lease.clone();
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.sync_renew_node_lease(&lease))
            .await
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }

    async fn drain(&self, scope: DrainScope) -> Result<DrainStatus, PortError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.sync_drain(scope))
            .await
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }

    async fn release(
        &self,
        permit: &BudgetPermit,
        outcome: ReleaseOutcome,
    ) -> Result<(), PortError> {
        let permit = permit.clone();
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.sync_release(&permit, outcome))
            .await
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }

    async fn snapshot(&self) -> Result<BudgetSnapshot, PortError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.sync_snapshot())
            .await
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))?
    }
}

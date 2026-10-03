//! `BudgetCoordinator` 的进程内实现。
//!
//! 契约来源 `platform-api::ports::budget::coordinator`（**7 个方法**，签名冻结不可改）。
//! 本文件只做「适配」：把端口的异步语义落到 [`BudgetLedger`] 的同步语义上。
//!
//! ## 时间从哪来
//!
//! 端口 trait **没有**时钟参数，实现必须自己决定。默认值 [`MonotonicBudgetClock`] 用
//! [`std::time::Instant`] 的经过时长；测试注入自己的 [`BudgetClock`] 实现，于是整条
//! acquire 等待链在假时钟上同步跑完——不 `sleep`、不真实等待、不定时器竞态。
//!
//! acquire 的超时**是配置**（`BudgetConfig::acquire_timeout_ms`，默认 `defaults::ACQUIRE_TIMEOUT_MS`
//! = 10_000），可被 `BudgetRequest::acquire_timeout_ms` 逐请求覆盖。逻辑里没有任何写死的
//! 毫秒常量：把默认改成 250 只是换一份配置，不改一行判定。
//!
//! ## 取消
//!
//! 端口**没有** cancel 方法。等待必须可取消（§9.5），所以取消只能落在 future 被丢弃上：
//! [`WaiterGuard`] 跨 `await` 持有等待者 id，`Drop` 时按 id 精确摘除。
//! 这是唯一能让「调用方放弃等待」真正回到账本的接缝。

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{OrganizationId, PrincipalId};
use datazen_platform_api::ports::budget::coordinator::{
    BudgetCoordinator, BudgetPermit, BudgetPermitSet, BudgetRequest, BudgetSnapshot, DrainScope,
    DrainStatus, NodeLease, ReleaseOutcome,
};
use datazen_platform_api::ports::budget::{BudgetPurpose, ResourceClass};
use tokio::sync::Notify;

use crate::budget::classes::BudgetClaim;
use crate::budget::ledger::{
    AdmitOutcome, BudgetLedger, DenialReason, PermitRecord, PumpReport, ReleaseResult,
};
use crate::budget::queues::WaiterId;
use crate::budget::quotas::BudgetConfig;

// ------------------------------------------------------------------ 时钟

/// 预算侧唯一的时间源。
///
/// 端口没有时钟参数，账本又不含时间源，所以时间必须由实现层注入一次，之后所有判定
/// （排队截止、租约到期、快照时刻）都从它取。
pub trait BudgetClock: Send + Sync + 'static {
    /// 单调毫秒。
    fn now_ms(&self) -> u64;
}

/// 默认时钟：从构造时刻起算的 [`Instant`] 经过时长。
pub struct MonotonicBudgetClock {
    origin: Instant,
}

impl MonotonicBudgetClock {
    /// 以「此刻」为 0 起点。
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for MonotonicBudgetClock {
    fn default() -> Self {
        Self::new()
    }
}

impl BudgetClock for MonotonicBudgetClock {
    fn now_ms(&self) -> u64 {
        match u64::try_from(self.origin.elapsed().as_millis()) {
            Ok(value) => value,
            Err(_) => u64::MAX,
        }
    }
}

// ------------------------------------------------------------------ 主体解析

/// 把一次预算申请投影成「主体」。
///
/// §9.5 的主体轮转与 §9.2 的每用户上限都以主体为单位，而端口的 [`BudgetRequest`] 上
/// **没有**主体字段（只有 `organization_id` / `connection_id` / `purpose` / `acquire_timeout_ms`）。
/// 所以主体必须由实现层从服务端上下文推导——**绝不能**来自调用方参数。
pub trait PrincipalResolver: Send + Sync + 'static {
    /// 返回该申请的主体。
    fn resolve(&self, request: &BudgetRequest) -> PrincipalId;
}

/// 默认解析器：把主体投影成 `organization_id`。
///
/// 这只在「单组织单主体」下成立：同一组织的所有申请会落到同一个主体，于是每用户上限
/// 退化成每组织上限、同类内轮转退化成同主体 FIFO。真实部署应当注入按认证主体解析的实现。
pub struct OrganizationPrincipal;

impl PrincipalResolver for OrganizationPrincipal {
    fn resolve(&self, request: &BudgetRequest) -> PrincipalId {
        PrincipalId::new(request.organization_id.as_str())
    }
}

/// 把端口的 `purpose` 映射成服务端资源类别（§9.5）。
///
/// 端口的 `BudgetPurpose` 只有两值，所以映射也只有两值：**调用方无法自报优先级**——
/// 类别是这一层的推导结果，而不是请求里的一个字段。`Control` 与 `Metadata` 属于用例层
/// 内部用途（取消、健康恢复、元数据/补全），它们走 [`InProcessBudgetCoordinator::admit`]。
pub fn classify(purpose: BudgetPurpose) -> ResourceClass {
    match purpose {
        BudgetPurpose::UserInteractive => ResourceClass::Interactive,
        BudgetPurpose::BackgroundJob => ResourceClass::Job,
    }
}

/// 把账本拒绝理由投影成端口错误。**不新增 `PortError` 变体**（§4.1「不增不减」）。
///
/// 额度类事实走 `QuotaExceeded`（用例层再翻成 `ApiError{ResourceBusy}` / `SessionQuotaExceeded`）；
/// 服务未登记走 `NotFound`。
fn port_error(reason: &DenialReason) -> PortError {
    match reason {
        DenialReason::UnknownConnection { connection_id } => {
            PortError::NotFound(connection_id.as_str().to_string())
        }
        DenialReason::NodeLost { worker_id } => {
            PortError::cas_conflict("node_lease", worker_id.as_str())
        }
        other => PortError::QuotaExceeded(other.message()),
    }
}

fn timeout_error(deadline_ms: u64) -> PortError {
    PortError::ProviderTimeout(format!(
        "budget acquire deadline reached at {deadline_ms}ms"
    ))
}

// ------------------------------------------------------------------ 等待者守卫

/// 跨 `await` 持有等待者 id；future 被丢弃时按 id 精确摘除。
///
/// 端口没有 cancel 方法，等待又必须可取消，所以取消 = future 丢弃。这里**按 id**摘除，
/// 不是「从头扫」，因此取消 A 的等待者绝不会顺手摘掉 B 的。
struct WaiterGuard {
    ledger: Arc<Mutex<BudgetLedger>>,
    wakeup: Arc<Notify>,
    waiter: Option<WaiterId>,
}

impl WaiterGuard {
    fn disarm(&mut self) {
        self.waiter = None;
    }
}

impl Drop for WaiterGuard {
    fn drop(&mut self) {
        let Some(waiter) = self.waiter.take() else {
            return;
        };
        {
            let mut ledger = lock_ledger(&self.ledger);
            // 取消只改队列，不动任何额度计数：名额本来就没预留，取消不该改变可用额度。
            ledger.cancel(waiter);
        }
        self.wakeup.notify_waiters();
    }
}

fn lock_ledger(ledger: &Arc<Mutex<BudgetLedger>>) -> MutexGuard<'_, BudgetLedger> {
    match ledger.lock() {
        Ok(guard) => guard,
        // 账本内部没有 panic 路径，所以中毒只可能来自调用方在持锁期间 panic。
        // 这里选择恢复而不是传播：预算账本丢失一致性比一次 panic 更糟。
        Err(poisoned) => poisoned.into_inner(),
    }
}

// ------------------------------------------------------------------ 协调器

/// 进程内预算协调器。
///
/// 共享的一份 [`BudgetLedger`] + 一份可注入时钟 + 一份可注入主体解析器。
/// 状态在一把标准互斥锁后面；**所有锁都在同步区间内持有**，绝不跨 `.await`，
/// 所以 future 保持 `Send`，同时不需要异步锁的开销。
pub struct InProcessBudgetCoordinator {
    ledger: Arc<Mutex<BudgetLedger>>,
    clock: Arc<dyn BudgetClock>,
    principals: Arc<dyn PrincipalResolver>,
    /// `snapshot()` 的签名里没有组织参数，所以组织必须由实现层持有（端口形状所限）。
    organization_id: OrganizationId,
    /// 额度变化后唤醒所有等待者。
    wakeup: Arc<Notify>,
}

impl InProcessBudgetCoordinator {
    /// 构造一个协调器。
    ///
    /// `organization_id` 是因为端口的 `snapshot()` 无参——见本文件顶部说明。
    pub fn new(config: BudgetConfig, organization_id: OrganizationId) -> Self {
        Self {
            ledger: Arc::new(Mutex::new(BudgetLedger::new(config))),
            clock: Arc::new(MonotonicBudgetClock::new()),
            principals: Arc::new(OrganizationPrincipal),
            organization_id,
            wakeup: Arc::new(Notify::new()),
        }
    }

    /// 注入时钟（测试用假时钟，生产用默认值）。
    pub fn with_clock(mut self, clock: Arc<dyn BudgetClock>) -> Self {
        self.clock = clock;
        self
    }

    /// 注入主体解析器。
    pub fn with_principals(mut self, principals: Arc<dyn PrincipalResolver>) -> Self {
        self.principals = principals;
        self
    }

    /// 借账本看一眼（测试与上层观测用）。
    pub fn with_ledger<R>(&self, inspect: impl FnOnce(&BudgetLedger) -> R) -> R {
        let ledger = lock_ledger(&self.ledger);
        inspect(&ledger)
    }

    /// 推进一轮调度并唤醒所有等待者。
    ///
    /// 额度一有变化（核销、关连接、时钟到期）就必须调它，否则等待者会一直挂着。
    pub fn advance(&self) -> PumpReport {
        let now_ms = self.clock.now_ms();
        let report = {
            let mut ledger = lock_ledger(&self.ledger);
            ledger.pump(now_ms)
        };
        self.wakeup.notify_waiters();
        report
    }

    /// 类别直入口：一次带完整类别的准入尝试（四类都从这里进）。
    ///
    /// 端口的 `BudgetPurpose` 只有两值，所以 `Control` / `Metadata`（取消、健康恢复、
    /// 元数据）必须由用例层从这里进，端口 trait 本身覆盖不到。
    pub fn admit(&self, claim: &BudgetClaim) -> AdmitOutcome {
        let now_ms = self.clock.now_ms();
        let outcome = {
            let mut ledger = lock_ledger(&self.ledger);
            ledger.ensure_service(&claim.connection_id);
            ledger.try_admit(claim, now_ms)
        };
        if matches!(outcome, AdmitOutcome::Granted(_)) {
            self.wakeup.notify_waiters();
        }
        outcome
    }

    /// 申请一次许可，等不到就按 deadline 失败（端口 `acquire` 的内核）。
    pub async fn acquire_with(
        &self,
        request: &BudgetRequest,
        timeout_ms: u64,
    ) -> Result<BudgetPermit, PortError> {
        let claim = self.claim_of(request);
        let started_ms = self.clock.now_ms();
        let deadline_ms = started_ms.saturating_add(timeout_ms);

        {
            let mut ledger = lock_ledger(&self.ledger);
            ledger.ensure_service(&claim.connection_id);
            match ledger.try_admit(&claim, started_ms) {
                // 额度够：立刻拿，不必排队。
                AdmitOutcome::Granted(record) => return Ok(record.to_port_permit()),
                // 终态拒绝：排队也没用，直接回。
                AdmitOutcome::Denied(reason) => return Err(port_error(&reason)),
                // 额度暂时不够：排队。
                AdmitOutcome::Busy(_) => {}
            }
        }

        // deadline 先判一次：超时为 0 的请求必须**同步**返回超时，一个字都不能排。
        if self.clock.now_ms() >= deadline_ms {
            return Err(timeout_error(deadline_ms));
        }

        let waiter = {
            let mut ledger = lock_ledger(&self.ledger);
            // 队列满 → `QueueFull`，**不静默阻塞**（§9.5）。
            ledger
                .enqueue(&claim, self.clock.now_ms(), deadline_ms)
                .map_err(|reason| port_error(&reason))?
        };

        let mut guard = WaiterGuard {
            ledger: Arc::clone(&self.ledger),
            wakeup: Arc::clone(&self.wakeup),
            waiter: Some(waiter),
        };
        loop {
            // 先注册再复查：`Notified::enable()` 把「注册」提到复查之前，
            // 否则额度恰好在「复查」与「挂起」之间变化时会丢唤醒（丢唤醒 = 死等）。
            let notified = self.wakeup.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(result) = self.settle(&claim, waiter, deadline_ms) {
                guard.disarm();
                return result;
            }
            notified.await;
        }
    }

    /// 复查一轮：有结果就返回，没有就说明还得继续等。
    ///
    /// 顺序很重要：**先判 deadline，再试准入**。否则超时之后的一次机会会被当成正常放行。
    fn settle(
        &self,
        claim: &BudgetClaim,
        waiter: WaiterId,
        deadline_ms: u64,
    ) -> Option<Result<BudgetPermit, PortError>> {
        let now_ms = self.clock.now_ms();
        let mut ledger = lock_ledger(&self.ledger);
        if now_ms >= deadline_ms {
            return Some(Err(timeout_error(deadline_ms)));
        }
        match ledger.try_admit(claim, now_ms) {
            AdmitOutcome::Granted(record) => Some(Ok(record.to_port_permit())),
            AdmitOutcome::Denied(reason) => Some(Err(port_error(&reason))),
            AdmitOutcome::Busy(_) => {
                let report = ledger.pump(now_ms);
                if report.timed_out.contains(&waiter) {
                    return Some(Err(timeout_error(deadline_ms)));
                }
                report
                    .granted
                    .iter()
                    .find(|record| record.waiter == Some(waiter))
                    .map(|record| Ok(record.to_port_permit()))
                    // 放行也可能发生在**别的** pumping 点上（例如别人 release 触发的
                    // [`InProcessBudgetCoordinator::advance`]）：那次 pumping 已经把
                    // waiter 从队列里摘走并签发了 permit，这里自己的 pumping 自然什么也
                    // 放不出来。只看自己这一轮的报告会让已获批的等待者一直挂到 deadline。
                    .or_else(|| {
                        ledger
                            .permits()
                            .find(|record| record.waiter == Some(waiter))
                            .map(|record| Ok(record.to_port_permit()))
                    })
            }
        }
    }

    fn claim_of(&self, request: &BudgetRequest) -> BudgetClaim {
        BudgetClaim::single(
            request.organization_id.clone(),
            request.connection_id.clone(),
            self.principals.resolve(request),
            classify(request.purpose),
        )
    }
}

#[async_trait::async_trait]
impl BudgetCoordinator for InProcessBudgetCoordinator {
    async fn acquire(&self, request: BudgetRequest) -> Result<BudgetPermit, PortError> {
        // 逐请求覆盖 > 配置默认。默认是 `BudgetConfig` 里的 acquire_timeout_ms，不是写死的常量。
        let timeout_ms = match request.acquire_timeout_ms {
            Some(ms) => ms,
            None => lock_ledger(&self.ledger).config().acquire_timeout_ms,
        };
        self.acquire_with(&request, timeout_ms).await
    }

    async fn try_acquire(&self, request: BudgetRequest) -> Result<Option<BudgetPermit>, PortError> {
        let claim = self.claim_of(&request);
        let now_ms = self.clock.now_ms();
        let mut ledger = lock_ledger(&self.ledger);
        ledger.ensure_service(&claim.connection_id);
        match ledger.try_admit(&claim, now_ms) {
            AdmitOutcome::Granted(record) => Ok(Some(record.to_port_permit())),
            AdmitOutcome::Denied(reason) => Err(port_error(&reason)),
            // 忙就报「暂时没有」：**不排队、不阻塞**。
            AdmitOutcome::Busy(_) => {
                // 顺带推进一轮，让别处的释放不要因为这次探测而被延后；我们自己不因此获益。
                ledger.pump(now_ms);
                Ok(None)
            }
        }
    }

    async fn reserve_many(&self, requests: &[BudgetRequest]) -> Result<BudgetPermitSet, PortError> {
        let claims: Vec<BudgetClaim> = requests
            .iter()
            .map(|request| self.claim_of(request))
            .collect();
        let now_ms = self.clock.now_ms();
        let mut ledger = lock_ledger(&self.ledger);
        for claim in &claims {
            ledger.ensure_service(&claim.connection_id);
        }
        // 全有或全无（§9.5 Job 多端点）：任一端点拿不到就整体失败、整体不记账。
        let records = ledger
            .try_admit_many(&claims, now_ms)
            .map_err(|reason| port_error(&reason))?;
        Ok(BudgetPermitSet::new(
            records.iter().map(PermitRecord::to_port_permit).collect(),
        ))
    }

    async fn renew_node_lease(&self, lease: &NodeLease) -> Result<NodeLease, PortError> {
        let now_ms = self.clock.now_ms();
        lock_ledger(&self.ledger)
            .renew_node_lease(lease, now_ms)
            .map_err(|reason| port_error(&reason))
    }

    async fn drain(&self, scope: DrainScope) -> Result<DrainStatus, PortError> {
        let report = {
            let mut ledger = lock_ledger(&self.ledger);
            ledger.drain(scope)
        };
        self.wakeup.notify_waiters();
        Ok(BudgetLedger::drain_status(report))
    }

    async fn release(
        &self,
        permit: &BudgetPermit,
        outcome: ReleaseOutcome,
    ) -> Result<(), PortError> {
        let now_ms = self.clock.now_ms();
        let consumed = matches!(outcome, ReleaseOutcome::Consumed);
        let result = {
            let mut ledger = lock_ledger(&self.ledger);
            ledger.release(permit, consumed, now_ms)
        };
        match result {
            // 重复核销同一张 permit：幂等，不二次记账，也**不**当成错误。
            ReleaseResult::Released { .. } | ReleaseResult::AlreadyReleased { .. } => {
                self.advance();
                Ok(())
            }
            // 从未签发的 permit 是调用方 bug，不能静默吞掉。
            ReleaseResult::Unknown { permit_id } => {
                Err(PortError::NotFound(permit_id.as_str().to_string()))
            }
        }
    }

    async fn snapshot(&self) -> Result<BudgetSnapshot, PortError> {
        Ok(lock_ledger(&self.ledger).snapshot(&self.organization_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::ledger::QueueScope;
    use datazen_platform_api::id::ConnectionId;

    #[test]
    fn purpose_maps_to_a_class_server_side() {
        assert_eq!(
            classify(BudgetPurpose::UserInteractive),
            ResourceClass::Interactive
        );
        assert_eq!(classify(BudgetPurpose::BackgroundJob), ResourceClass::Job);
    }

    #[test]
    fn queue_full_is_terminal_and_exhausted_is_retryable() {
        let full = DenialReason::QueueFull {
            scope: QueueScope::PerUser,
            cap: 32,
            depth: 32,
        };
        let exhausted = DenialReason::Exhausted {
            connection_id: ConnectionId::new("c-1"),
            class: ResourceClass::Interactive,
            capacity: 4,
        };
        assert!(!full.is_retryable());
        assert!(exhausted.is_retryable());
    }

    #[test]
    fn over_quota_maps_to_quota_exceeded_and_missing_service_to_not_found() {
        let over = DenialReason::PhysicalExhausted {
            connection_id: ConnectionId::new("c-1"),
            cap: 4,
        };
        assert!(matches!(port_error(&over), PortError::QuotaExceeded(_)));
        let missing = DenialReason::UnknownConnection {
            connection_id: ConnectionId::new("c-404"),
        };
        assert!(matches!(port_error(&missing), PortError::NotFound(_)));
    }
}

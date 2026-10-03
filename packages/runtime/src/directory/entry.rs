//! 条目状态机。
//!
//! 一个目录条目就是 `owner` + 路由状态 + 期限 + 关闭原因。**没有连接、没有凭据**：
//! 目录只负责「这个 ID 现在该落到谁身上」，不负责它连着什么。
//!
//! 路由状态只有四个，且**单向**：
//!
//! ```text
//!                     (Prepared 候选)
//!                          │
//!                          ▼
//!   (register) ──► Routable ──► CommitBarrier ──┐
//!                    │  ▲                       │ 提交结果未知：屏障挂着，
//!                    │  │ rollback 还原          │ 两边都不可路由
//!                    │  └────────────────────────┤
//!                    ▼                           │
//!                  Closing ──(sweep)──► Closed ◄┘  Committed：旧条目改关闭路由
//!                    │                          │
//!                    └────(invalidate/release)───┘
//! ```
//!
//! 没有任何一条路径把 `Closing` / `Closed` 退回 `Routable`——过期和关闭都是终向的，
//! 唯一能“复活”条目的方式是 rollback，而 rollback 只作用在**还没提交**的替换上。

use datazen_platform_api::dto::session::SessionHandle;
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{DbSessionId, ExecutionId, RuntimeEpoch, Timestamp};
use datazen_platform_api::ports::session_directory::{
    CloseDisposition, InvalidationReason, SessionOwner,
};

use super::ttl::{DeadlineKind, DeadlineSet};
use super::{DirectoryClock, MonoInstant};

/// 条目的路由状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoutingState {
    /// 可路由：请求可以落到这个 owner。
    Routable,
    /// 替换屏障：提交结果未知，**既不能路由到旧的，也不能路由到新的**。
    CommitBarrier,
    /// 关闭中：已判到期或正在关闭，不可路由、不可重挂。
    Closing,
    /// 关闭：终态。
    Closed,
}

impl RoutingState {
    pub const fn as_str(self) -> &'static str {
        match self {
            RoutingState::Routable => "routable",
            RoutingState::CommitBarrier => "commitBarrier",
            RoutingState::Closing => "closing",
            RoutingState::Closed => "closed",
        }
    }

    pub const fn is_routable(self) -> bool {
        matches!(self, RoutingState::Routable)
    }

    /// `Closed` 之后不再可挂。`Closing` 也一样：期限已判死，重挂等于让已到期会话复活。
    pub const fn is_attachable(self) -> bool {
        matches!(self, RoutingState::Routable)
    }
}

/// 条目为什么不再路由。**原因必须可区分**，不能一律塌成「会话没了」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClosureReason {
    /// 调用方主动释放。
    Released(CloseDisposition),
    /// 被显式作废（带具体原因）。
    Invalidated(InvalidationReason),
    /// 期限到期。
    DeadlineExpired(DeadlineKind),
    /// 被一次已提交的替换顶掉。
    ReplacedBy(SessionHandle),
}

impl ClosureReason {
    /// 稳定的机器可读标签。
    pub const fn code(&self) -> &'static str {
        match self {
            ClosureReason::Released(CloseDisposition::Closed) => "released",
            ClosureReason::Released(CloseDisposition::ForceClosed) => "forceClosed",
            ClosureReason::Invalidated(reason) => invalidation_reason_code(reason),
            ClosureReason::DeadlineExpired(kind) => match kind {
                DeadlineKind::DisconnectGrace => "expiredDisconnectGrace",
                DeadlineKind::TransactionIdle => "expiredTransactionIdle",
                DeadlineKind::SessionIdle => "expiredSessionIdle",
            },
            ClosureReason::ReplacedBy(_) => "replaced",
        }
    }

    /// 到期之外的关闭。
    pub fn invalidation(&self) -> Option<InvalidationReason> {
        match self {
            ClosureReason::Invalidated(reason) => Some(*reason),
            _ => None,
        }
    }

    pub fn expiration(&self) -> Option<DeadlineKind> {
        match self {
            ClosureReason::DeadlineExpired(kind) => Some(*kind),
            _ => None,
        }
    }
}

/// `invalidate` 的四种原因必须互相可区分，因此各有各的稳定码。
pub const fn invalidation_reason_code(reason: &InvalidationReason) -> &'static str {
    match reason {
        InvalidationReason::WorkerLost => "workerLost",
        InvalidationReason::ResourceLost => "resourceLost",
        InvalidationReason::IdentityRotated => "identityRotated",
        InvalidationReason::PolicyChanged => "policyChanged",
    }
}

/// 路由被拒的**具体**理由。调用方要能区分「ID 根本不在这」和「ID 在但 epoch 旧了」——
/// 后者是 worker 重启的正常结果，必须显式失败，绝不能去找一个配置里的替代会话顶上去。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteRejection {
    /// 目录里没有这个 ID。
    Unknown,
    /// ID 在，但 `runtimeEpoch` 是上一代 worker 签发的。
    StaleEpoch { current: RuntimeEpoch },
    /// 处于替换屏障中：提交结果未知，两边都不可路由。
    CommitBarrier,
    /// 已判到期、正在关闭。
    Expired { kind: DeadlineKind },
    /// 正在关闭（尚无到期原因）。
    Closing,
    /// 已关闭。
    Closed { reason: &'static str },
}

impl RouteRejection {
    /// 稳定的机器可读标签。
    pub const fn code(&self) -> &'static str {
        match self {
            RouteRejection::Unknown => "unknownSession",
            RouteRejection::StaleEpoch { .. } => "staleRuntimeEpoch",
            RouteRejection::CommitBarrier => "replacementCommitPending",
            RouteRejection::Expired { .. } => "sessionExpired",
            RouteRejection::Closing => "sessionClosing",
            RouteRejection::Closed { reason } => reason,
        }
    }

    /// 端口方法的错误投影。**只用 `PortError` 已有的变体**——
    /// 契约层冻结，加变体不是本模块能做的事。
    pub fn to_port_error(&self, db_session_id: &DbSessionId) -> PortError {
        match self {
            RouteRejection::Unknown => {
                PortError::NotFound(format!("unknown db session id: {db_session_id}"))
            }
            // 陈旧句柄的报错必须点名**当前** epoch：调用方要能区分
            // 「ID 从来没注册过」与「ID 在，但持有它的是上一代 worker」。
            RouteRejection::StaleEpoch { current } => PortError::cas_conflict(
                "session",
                format!("{db_session_id}:{} (current={current})", self.code()),
            ),
            other => {
                PortError::cas_conflict("session", format!("{db_session_id}:{}", other.code()))
            }
        }
    }
}

/// 到期裁决的结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adjudication {
    /// 还没到点（或本来就没挂期限）。
    Pending,
    /// 到点了，已转入关闭中。`kind` 是被判死的那一类。
    Expired { kind: DeadlineKind },
}

impl Adjudication {
    pub const fn is_expired(self) -> bool {
        matches!(self, Adjudication::Expired { .. })
    }
}

/// 一个目录条目。
#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    owner: SessionOwner,
    state: RoutingState,
    deadlines: DeadlineSet,
    closure: Option<ClosureReason>,
    attached: bool,
    active_execution: Option<ExecutionId>,
    registered_at: MonoInstant,
    last_adjudicated_at: Option<MonoInstant>,
}

impl DirectoryEntry {
    /// 新登记的可路由条目。
    pub fn new(owner: SessionOwner, registered_at: MonoInstant) -> Self {
        Self {
            owner,
            state: RoutingState::Routable,
            deadlines: DeadlineSet::new(),
            closure: None,
            attached: false,
            active_execution: None,
            registered_at,
            last_adjudicated_at: None,
        }
    }

    /// 替换候选：登记时就压进屏障，**从出生起就不可路由**。
    pub fn prepared(owner: SessionOwner, registered_at: MonoInstant) -> Self {
        let mut entry = Self::new(owner, registered_at);
        entry.state = RoutingState::CommitBarrier;
        entry
    }

    pub fn owner(&self) -> &SessionOwner {
        &self.owner
    }

    /// 目录对外给的句柄（`dbSessionId` + `runtimeEpoch`）。**不是连接句柄。**
    pub fn handle(&self) -> SessionHandle {
        self.owner.to_handle()
    }

    pub fn db_session_id(&self) -> &DbSessionId {
        &self.owner.db_session_id
    }

    pub fn state(&self) -> RoutingState {
        self.state
    }

    pub fn closure(&self) -> Option<&ClosureReason> {
        self.closure.as_ref()
    }

    pub fn is_routable(&self) -> bool {
        self.state.is_routable()
    }

    pub fn is_attachable(&self) -> bool {
        self.state.is_attachable()
    }

    pub fn attached(&self) -> bool {
        self.attached
    }

    pub fn active_execution(&self) -> Option<&ExecutionId> {
        self.active_execution.as_ref()
    }

    pub fn has_active_execution(&self) -> bool {
        datazen_platform_api::ports::session_directory::has_active_execution(
            self.active_execution.as_ref(),
        )
    }

    pub fn registered_at(&self) -> MonoInstant {
        self.registered_at
    }

    pub fn deadlines(&self) -> &DeadlineSet {
        &self.deadlines
    }

    pub fn last_adjudicated_at(&self) -> Option<MonoInstant> {
        self.last_adjudicated_at
    }

    /// 挂期限。只在可路由状态下允许：给一个已经判死的条目续期等于让它复活。
    pub fn arm(&mut self, kind: DeadlineKind, at: MonoInstant) -> Result<(), RouteRejection> {
        self.require_routable()?;
        self.deadlines.arm(super::ttl::Deadline::new(kind, at));
        Ok(())
    }

    /// 摘期限。摘不存在的期限不是错误。
    pub fn cancel_deadline(&mut self, kind: DeadlineKind) -> bool {
        self.deadlines.cancel(kind)
    }

    /// `expiresAt`：最早期限的 UTC 投影；没有适用期限就是 `None`。
    /// 执行中且没有适用期限时正是 `None`，而**不是**一个编出来的远期时间。
    pub fn expiration_projection(&self, clock: &dyn DirectoryClock) -> Option<Timestamp> {
        if !self.state.is_routable() {
            return None;
        }
        self.deadlines.expiration_projection(clock)
    }

    /// 记录一次业务活动。它**只**更新 owner 上的时间戳，不动期限——
    /// 心跳/活动不是 TTL 的刷新键。
    pub fn record_business_activity(&mut self, business_activity: Timestamp) {
        self.owner.last_business_activity = business_activity;
    }

    /// 中心路由闸：epoch 必须对得上，状态必须可路由。
    ///
    /// 顺序是刻意的：**先查 epoch 再查状态**。旧 epoch 的句柄即使撞上一个正在关闭的
    /// 条目，也必须报 `StaleEpoch`——那是 worker 重启的结论，不是「会话没了」。
    pub fn route(&self, handle: &SessionHandle) -> Result<(), RouteRejection> {
        if &self.owner.db_session_id != &handle.db_session_id {
            return Err(RouteRejection::Unknown);
        }
        if self.owner.runtime_epoch != handle.runtime_epoch {
            return Err(RouteRejection::StaleEpoch {
                current: self.owner.runtime_epoch.clone(),
            });
        }
        match self.state {
            RoutingState::Routable => Ok(()),
            RoutingState::CommitBarrier => Err(RouteRejection::CommitBarrier),
            RoutingState::Closing => {
                match self.closure.as_ref().and_then(ClosureReason::expiration) {
                    Some(kind) => Err(RouteRejection::Expired { kind }),
                    None => Err(RouteRejection::Closing),
                }
            }
            RoutingState::Closed => Err(RouteRejection::Closed {
                reason: self.closure.as_ref().map_or("closed", ClosureReason::code),
            }),
        }
    }

    /// attach 的准入：路由闸 + 可挂状态 + 无活跃执行。
    pub fn admit_attachment(&self, handle: &SessionHandle) -> Result<(), RouteRejection> {
        self.route(handle)?;
        if !self.state.is_attachable() {
            return Err(RouteRejection::Closed {
                reason: self.closure.as_ref().map_or("closed", ClosureReason::code),
            });
        }
        Ok(())
    }

    /// 串行到期裁决。由**同一个 actor** 在同一把锁内调用，因此不存在两个并发裁决互相打架。
    pub fn adjudicate(&mut self, now: MonoInstant) -> Adjudication {
        if !self.state.is_routable() {
            return Adjudication::Pending;
        }
        match self.deadlines.due_kind(now) {
            Some(kind) => {
                self.state = RoutingState::Closing;
                self.attached = false;
                self.closure = Some(ClosureReason::DeadlineExpired(kind));
                self.last_adjudicated_at = Some(now);
                Adjudication::Expired { kind }
            }
            None => Adjudication::Pending,
        }
    }

    /// 挂上 attachment。重复挂是幂等的（返回 `false` 表示本来就是挂着的）。
    pub fn attach(&mut self) -> bool {
        if self.attached {
            return false;
        }
        self.attached = true;
        true
    }

    /// 断开 attachment。**不延长任何期限**——重复 detach 不能把 deadline 往后推。
    pub fn detach(&mut self) -> bool {
        let was = self.attached;
        self.attached = false;
        was
    }

    /// 占位执行。已经有执行在跑就拒绝（同一会话不允许两个并发执行）。
    pub fn begin_execution(&mut self, execution_id: ExecutionId) -> Result<(), ExecutionId> {
        if let Some(current) = &self.active_execution {
            return Err(current.clone());
        }
        self.active_execution = Some(execution_id);
        Ok(())
    }

    pub fn end_execution(&mut self, execution_id: &ExecutionId) -> bool {
        if self.active_execution.as_ref() == Some(execution_id) {
            self.active_execution = None;
            true
        } else {
            false
        }
    }

    /// 关闭并写明原因。第一次关闭生效，重复关闭不改写原因。
    pub fn close(&mut self, reason: ClosureReason) {
        self.state = RoutingState::Closed;
        self.attached = false;
        self.deadlines.clear();
        if self.closure.is_none() {
            self.closure = Some(reason);
        }
    }

    /// 进入替换屏障：不可路由，等待提交结果。已是终态的条目**不会**被拉回屏障。
    ///
    /// `replaced_by` 只有旧条目需要——它确实被换掉了；新候选只是「候选」，
    /// 还没有替换掉任何东西，因此不写关闭原因。
    pub fn hold_commit_barrier(&mut self, replaced_by: Option<SessionHandle>) -> bool {
        if matches!(self.state, RoutingState::Closed | RoutingState::Closing) {
            return false;
        }
        self.state = RoutingState::CommitBarrier;
        self.attached = false;
        if let Some(replaced_by) = replaced_by {
            self.closure = Some(ClosureReason::ReplacedBy(replaced_by));
        }
        true
    }

    /// 从屏障放行（已提交）。新条目用。
    pub fn publish(&mut self) {
        self.state = RoutingState::Routable;
        self.closure = None;
        self.attached = false;
        self.deadlines.clear();
    }

    /// 从屏障还原（回滚）。旧条目用：**不动**已有期限，也不改挂载状态。
    pub fn restore(&mut self) -> bool {
        if self.state != RoutingState::CommitBarrier {
            return false;
        }
        self.state = RoutingState::Routable;
        self.closure = None;
        true
    }

    fn require_routable(&self) -> Result<(), RouteRejection> {
        if self.state.is_routable() {
            Ok(())
        } else {
            Err(RouteRejection::Closed {
                reason: self.closure.as_ref().map_or("closed", ClosureReason::code),
            })
        }
    }

    /// 诊断投影。**不含任何令牌材料**——目录根本就没存过；也**不落盘**，
    /// 只是把内存里的条目读成一份可断言的视图。
    pub fn observe(&self, clock: &dyn DirectoryClock) -> SessionSnapshot {
        let earliest = self.deadlines.earliest();
        SessionSnapshot {
            db_session_id: self.owner.db_session_id.clone(),
            runtime_epoch: self.owner.runtime_epoch.clone(),
            state: self.state,
            attached: self.attached,
            has_active_execution: self.has_active_execution(),
            expires_at: earliest.map(|deadline| clock.project(deadline.at())),
            // 最早期限的原始单调值（纳秒），供测试断言「到底是哪个期限在起作用」。
            expires_in_nanos: earliest.map(|deadline| deadline.at().as_nanos()),
            closure: self.closure.as_ref().map(ClosureReason::code),
            last_business_activity: self.owner.last_business_activity.clone(),
        }
    }
}

/// 对外暴露的诊断投影。刻意不含 `AttachmentToken`、也不含令牌摘要：
/// 会进事件、进日志的东西不该带凭据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub db_session_id: DbSessionId,
    pub runtime_epoch: RuntimeEpoch,
    pub state: RoutingState,
    pub attached: bool,
    pub has_active_execution: bool,
    pub expires_at: Option<Timestamp>,
    /// 最早期限的原始单调值（纳秒），供测试断言「到底是哪个期限在起作用」。
    pub expires_in_nanos: Option<u128>,
    pub closure: Option<&'static str>,
    pub last_business_activity: Timestamp,
}

/// `invalidate` 留下的可查记录。
///
/// 目录不落盘，所以它只活在内存里；存在的意义是让调用方在**当场**就能区分
/// 「被作废了，原因是 worker 掉线」和「被作废了，原因是身份轮换」，
/// 而不是拿到一个笼统的「会话没了」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidationRecord {
    pub db_session_id: DbSessionId,
    pub reason: InvalidationReason,
    pub reason_code: &'static str,
    pub runtime_epoch: RuntimeEpoch,
    pub invalidated_at: Timestamp,
}

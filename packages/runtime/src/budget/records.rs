//! 账本的值类型与计数器：准入结果、permit 记录、拒绝理由、排空/轮转报告、服务状态。
//!
//! 这一层**只描述形状，不做决策**：它不持有时间源、不访问 I/O，也不自行改动任何计数。
//! 决策全部在 [`crate::budget::ledger`]（准入）、[`crate::budget::dispatch`]（轮转）与
//! [`crate::budget::sessions`]（逻辑/物理会话）里完成。
//!
//! 把它们与账本主体分开放，是为了让「返回值长什么样」这件事可以被单独读懂：
//! [`DenialReason`] 的每个变体都对应一条可写进断言的约束，
//! [`PermitRecord::slot`] 则是「名额按槽退回」这条硬规则的唯一凭据。

use std::collections::BTreeMap;

use datazen_platform_api::id::{
    ConnectionId, Counter, DbSessionId, LeaseId, OrganizationId, PrincipalId, Timestamp, WorkerId,
};
use datazen_platform_api::ports::budget::{BudgetPermit, DrainScope};

use crate::budget::classes::{ServiceClasses, SlotKind};
use crate::budget::ledger::BudgetLedger;
use crate::budget::queues::{ClassQueue, SharedRoundRobin, WaiterId};

/// 队列满了：「任一满即返回 `QueueFull`，不得静默阻塞」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueScope {
    /// 每类每服务的上限（32）。
    ClassPerService,
    /// 每用户的上限（32）。两个队列都要查。
    PerUser,
}

/// 逻辑 session 超限（单用户 100 / 单组织 1000，超限 `SessionQuotaExceeded`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionScope {
    PerUser,
    PerOrganization,
}

/// 拒绝理由。**额度暂时不够不在这里**——它走 [`AdmitOutcome::Busy`]，由上层决定排队与否。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DenialReason {
    /// 队列已满，再排也没位置。
    QueueFull {
        scope: QueueScope,
        cap: u32,
        depth: usize,
    },
    /// 逻辑 session 超过上限（`New` 也计入）。
    SessionQuotaExceeded {
        scope: SessionScope,
        cap: u32,
        used: u32,
    },
    /// 本类的保留额与共享额都已用尽。
    Exhausted {
        connection_id: ConnectionId,
        class: datazen_platform_api::ports::budget::ResourceClass,
        capacity: u32,
    },
    /// 服务正在排空：不接新 permit（drain）。
    ServiceDraining { connection_id: ConnectionId },
    /// 物理连接达到上限。
    PhysicalExhausted {
        connection_id: ConnectionId,
        cap: u32,
    },
    /// 单用户已连接编辑器数达到上限（5）。
    ConnectedEditorsExhausted { principal: PrincipalId, cap: u32 },
    /// 节点失联：租约续期失败即按失联处理，调用方应停止发放新资源。
    NodeLost { worker_id: WorkerId },
    /// 多名额申请不接受排队：要么一次性全拿到，要么失败。
    BatchNotQueueable { requested: u32 },
    /// 连接服务未登记。
    UnknownConnection { connection_id: ConnectionId },
}

impl DenialReason {
    /// 是否值得重试。`QueueFull` / `SessionQuotaExceeded` 是终态——重试只会再被拒一次。
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            DenialReason::Exhausted { .. }
                | DenialReason::PhysicalExhausted { .. }
                | DenialReason::ConnectedEditorsExhausted { .. }
                | DenialReason::NodeLost { .. }
        )
    }

    /// 端口层消息。**不含机密**：只有 id 与计数。
    pub fn message(&self) -> String {
        match self {
            DenialReason::QueueFull { scope, cap, depth } => {
                format!("queue full ({scope:?}, cap {cap}, depth {depth})")
            }
            DenialReason::SessionQuotaExceeded { scope, cap, used } => {
                format!("session quota exceeded ({scope:?}, cap {cap}, used {used})")
            }
            DenialReason::Exhausted {
                connection_id,
                class,
                capacity,
            } => format!("{class:?} exhausted on {connection_id} (capacity {capacity})"),
            DenialReason::ServiceDraining { connection_id } => {
                format!("{connection_id} is draining")
            }
            DenialReason::PhysicalExhausted { connection_id, cap } => {
                format!("physical connections exhausted on {connection_id} (cap {cap})")
            }
            DenialReason::ConnectedEditorsExhausted { principal, cap } => {
                format!("connected editors exhausted for {principal} (cap {cap})")
            }
            DenialReason::NodeLost { worker_id } => format!("node {worker_id} lost"),
            DenialReason::BatchNotQueueable { requested } => {
                format!("batch of {requested} permits is not queueable")
            }
            DenialReason::UnknownConnection { connection_id } => {
                format!("unknown connection service {connection_id}")
            }
        }
    }
}

/// 一次准入尝试的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmitOutcome {
    /// 放行。
    Granted(PermitRecord),
    /// 额度暂时不够：调用方可以排队（`acquire`）或直接报「暂时没有」（`try_acquire`）。
    Busy(DenialReason),
    /// 终态拒绝：排队也没用。
    Denied(DenialReason),
}

/// 账本里的一张 permit。`permit_id` 是幂等核销的唯一依据（端口契约的幂等核销条目）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermitRecord {
    pub permit_id: LeaseId,
    pub organization_id: OrganizationId,
    pub connection_id: ConnectionId,
    pub principal: PrincipalId,
    pub class: datazen_platform_api::ports::budget::ResourceClass,
    pub slot: SlotKind,
    pub pinned: bool,
    pub worker: Option<WorkerId>,
    /// 是哪一位等待者被放行的。直接即时放行的是 `None`；排队后被 `pump` 放行的是
    /// `Some(waiter_id)`——`acquire` 靠它把自己的 future 和 permit 对上号。
    pub waiter: Option<WaiterId>,
    pub granted_at_ms: u64,
}

impl PermitRecord {
    /// 转成端口的 [`BudgetPermit`]。
    pub fn to_port_permit(&self) -> BudgetPermit {
        BudgetPermit {
            permit_id: self.permit_id.clone(),
            organization_id: self.organization_id.clone(),
            connection_id: self.connection_id.clone(),
            granted_at: mono_timestamp(self.granted_at_ms),
        }
    }
}

/// 核销结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseResult {
    /// 第一次核销：名额已按**原槽**退回。
    Released { record: PermitRecord },
    /// 重复核销同一张 permit：幂等，**不**二次记账。
    AlreadyReleased { record: PermitRecord },
    /// 这张 permit 从未被签发：调用方有 bug，必须报 `NotFound`，不能静默吞掉。
    Unknown { permit_id: LeaseId },
}

/// 排空结果（对应端口的 `DrainStatus`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrainReport {
    pub scope: DrainScope,
    /// 作用域内仍持有 permit 且**未被钉住**的数：随正常释放自然排空。
    pub outstanding: usize,
    /// 被钉住的 permit 数（已建立的连接 / 活跃事务 / 游标）：明令**不得抢占**。
    pub pinned: usize,
    pub draining: bool,
}

/// `pump` 的产出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PumpReport {
    pub granted: Vec<PermitRecord>,
    /// 到期离队的等待者。它们**没有**持有任何额度，所以这里没有对应的回收动作。
    pub timed_out: Vec<WaiterId>,
}

/// 核销终态，用于区分「重复核销」与「permit 不存在」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retirement {
    pub record: PermitRecord,
    pub outcome_is_consumed: bool,
    pub released_at_ms: u64,
}

/// 单个 DB 服务的记账状态。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServiceState {
    pub classes: ServiceClasses,
    /// 共享池水位。保留额不计入——保留是按类专属的。
    pub shared_used: u32,
    pub queues: [ClassQueue; 4],
    pub rr: SharedRoundRobin,
    /// 物理连接占用（occupancy 集合）。
    pub physical: u32,
    pub user_connected: BTreeMap<PrincipalId, u32>,
    pub draining: bool,
}

impl ServiceState {
    /// 新建一个服务状态。
    pub fn new() -> Self {
        Self::default()
    }

    /// 某主体在该服务**所有类别**里的等待总数（每用户队列上限口径）。
    pub fn user_queue_depth(&self, principal: &PrincipalId) -> usize {
        self.queues
            .iter()
            .map(|queue| queue.depth_for(principal))
            .sum()
    }

    /// 所有类别的等待总数。
    pub fn queue_depth(&self) -> usize {
        self.queues.iter().map(|queue| queue.depth()).sum()
    }
}

/// 单调毫秒 → 契约层时间戳。
///
/// [`Timestamp`] 是**定宽字符串 newtype**：契约层不解析时间点，`Ord` 就是字符串序，
/// 所以只有写定宽形式才能让「先发生」与「序」一致。这里用零填充的十进制毫秒。
pub fn mono_timestamp(now_ms: u64) -> Timestamp {
    Timestamp::new(format!("{now_ms:013}"))
}

/// 逻辑/物理会话记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub ticket: DbSessionId,
    pub organization_id: OrganizationId,
    pub principal: PrincipalId,
    pub connection_id: ConnectionId,
    /// `None` = `New`：没有 socket，**只**计逻辑。
    pub occupancy: Option<datazen_platform_api::ports::budget::PhysicalOccupancy>,
}

/// 供 `sessions.rs` 复用的物理额度读取。
pub(crate) fn physical_of(ledger: &BudgetLedger, connection_id: &ConnectionId) -> u32 {
    ledger
        .services
        .get(connection_id)
        .map_or(0, |service| service.physical)
}

/// 供 `sessions.rs` 复用的已连接读取。
pub(crate) fn connected_of(
    ledger: &BudgetLedger,
    connection_id: &ConnectionId,
    principal: &PrincipalId,
) -> u32 {
    ledger.services.get(connection_id).map_or(0, |service| {
        service.user_connected.get(principal).copied().unwrap_or(0)
    })
}

/// 计数器构造（平台层的 `Counter` 是定宽无符号，跨 JS 边界安全）。
pub(crate) fn counter(value: usize) -> Counter {
    Counter::new(value as u64)
}

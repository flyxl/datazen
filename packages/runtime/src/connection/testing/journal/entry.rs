//! journal 条目类型与纯函数（fake-runtime-fixtures.md §5.1/§5.2）。
//!
//! 本子模块只描述「记什么」，不含任何状态容器；状态容器在 `core.rs`，
//! 变化点断言在 `asserts.rs`。三者都只向下依赖 `clock` 与非夹具的
//! `connection::{port, session, types, execution}`。
//!
//! §13 日志脱敏：条目里没有任何可以承载 `Secret` 的字段 —— 凭据、附件令牌与
//! 幂等 nonce 没有入口，结构上无法被误记。

use crate::connection::execution::{EffectOutcome, ExecutionErrorCode, TruncationRecord};
use crate::connection::port::{BudgetClass, PermitId, PermitReason};
use crate::connection::session::HandleKind;
use crate::connection::types::{
    Counter, ExecutionId, OwnerRef, PoolKeyFingerprint, ResourceId,
};

pub(crate) fn live_resources_in(entries: &[JournalEntry]) -> Vec<ResourceId> {
    let mut live: Vec<ResourceId> = Vec::new();
    for entry in entries {
        if let JournalEntry::Resource { resource_id, event, .. } = entry {
            if matches!(event, ResourceEvent::Created) {
                live.push(resource_id.clone());
            } else if event.leaves_live_set() {
                live.retain(|id| id != resource_id);
            }
        }
    }
    live
}

// ---------------------------------------------------------------------------
// 条目
// ---------------------------------------------------------------------------

/// 建连 / 关闭序列事件（§5 L251）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceEvent {
    Created,
    OpeningReady,
    /// 已确认关闭。只有它才归还 permit（§5.3 规则 2）。
    Closed,
    /// 关闭未确认：预算占用保持不变，不立即归零（§4.2 F11）。
    CloseUnconfirmed,
    /// 资源丢失（初始化后协议损坏等）：禁止后续执行，占用保持到确认关闭（§4.2 F3）。
    Lost,
    /// 隔离：不归还，保留预算占用（§4.2 F8 rollback 失败）。
    Quarantined,
    /// 归池尝试。§9.4 前置检查全满足才允许发生，因此必须被单独记录以便断言它**未**发生。
    ReturnedToPool { protocol_drained: bool, registered_handles: usize },
}

impl ResourceEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceEvent::Created => "Created",
            ResourceEvent::OpeningReady => "OpeningReady",
            ResourceEvent::Closed => "Closed",
            ResourceEvent::CloseUnconfirmed => "CloseUnconfirmed",
            ResourceEvent::Lost => "Lost",
            ResourceEvent::Quarantined => "Quarantined",
            ResourceEvent::ReturnedToPool { .. } => "ReturnedToPool",
        }
    }

    /// 是否把资源移出 live 集合（`Closed` 才释放占用；`Quarantined` 亦不可再被 acquire）。

    /// 刻意**不**在这里提供「是否归还 permit」的判据：permit 收支由
    /// `Accounting::occupied` 这个权威标志决定（见 `fake_resource::state`），
    /// 那是防重复 `-1`（§4.3 I1）的那一位。扫台账事件只能重算出「看起来对」，
    /// 判不出「已经归还过一次」——所以第二份弱判据只会误导接线的人，故不提供。
    pub(crate) fn leaves_live_set(&self) -> bool {
        matches!(self, ResourceEvent::Closed | ResourceEvent::Quarantined)
    }
}

/// 句柄登记动作（§5 L254）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleAction {
    Registered,
    Closed,
    Rejected,
    Orphaned,
}

impl HandleAction {
    pub fn as_str(self) -> &'static str {
        match self {
            HandleAction::Registered => "registered",
            HandleAction::Closed => "closed",
            HandleAction::Rejected => "rejected",
            HandleAction::Orphaned => "orphaned",
        }
    }
}

/// journal 条目。§5 L251–254 的四类全部落在这里，`seq` 来自单一原子计数器，
/// 因此并发写入的相对顺序是确定的（§5 L269）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalEntry {
    Resource {
        seq: u64,
        resource_id: ResourceId,
        event: ResourceEvent,
        owner: OwnerRef,
        pool_key: PoolKeyFingerprint,
        budget_class: BudgetClass,
    },
    Execution {
        seq: u64,
        resource_id: ResourceId,
        execution_id: ExecutionId,
        command_id: String,
        started_at_mono: u64,
        ended_at_mono: Option<u64>,
        effect_outcome: Option<EffectOutcome>,
        error_code: Option<ExecutionErrorCode>,
        protocol_drained: Option<bool>,
        truncation: Option<TruncationRecord>,
    },
    Permit {
        seq: u64,
        permit_id: PermitId,
        delta: i32,
        reason: PermitReason,
        budget_class: BudgetClass,
    },
    Handle {
        seq: u64,
        handle_id: String,
        kind: HandleKind,
        resource_id: ResourceId,
        runtime_epoch: Counter,
        action: HandleAction,
        reason: String,
    },
}

impl JournalEntry {
    pub fn seq(&self) -> u64 {
        match self {
            JournalEntry::Resource { seq, .. }
            | JournalEntry::Execution { seq, .. }
            | JournalEntry::Permit { seq, .. }
            | JournalEntry::Handle { seq, .. } => *seq,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            JournalEntry::Resource { .. } => "resource",
            JournalEntry::Execution { .. } => "execution",
            JournalEntry::Permit { .. } => "permit",
            JournalEntry::Handle { .. } => "handle",
        }
    }
}

/// permit 收支事件（§5 L253）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermitEvent {
    pub seq: u64,
    pub permit_id: PermitId,
    pub delta: i32,
    pub reason: PermitReason,
    pub budget_class: BudgetClass,
}

/// 句柄登记记录（§5.1 `handle_registry`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandleRecord {
    pub handle_id: String,
    pub kind: HandleKind,
    pub resource_id: ResourceId,
    pub runtime_epoch: Counter,
    pub closed: bool,
    /// 登记发生在哪个执行上 —— §5.3 规则 5 断言登记资源与登记记录必须一致。
    pub execution_id: Option<ExecutionId>,
}

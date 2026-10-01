//! FakeResource —— 假资源的状态机（fake-runtime-fixtures.md §3.1、§4）。
//!
//! 字段与 §3.1 的 `FakeResource` 定义一一对应；`accounting` 子结构是为 §5.3 的
//! permit 台账补的最小记账锚点（§3.1 没列它，但 §5.3 要求「每次 permit 变化」可核对，
//! 而台账需要 `budget_class` 与 `permit_id`，只能挂在资源上）。
//!
//! 本文件**不依赖** `journal`、不依赖 `script`、不依赖任何其它夹具模块，只依赖
//! `connection::{port, session, types, capability}`，与 §2 「依赖方向单向向下」一致。

use indexmap::IndexMap;

use crate::connection::capability::CapabilitySnapshot;
use crate::connection::execution::ExecutionErrorCode;
use crate::connection::port::{
    BudgetClass, CloseOutcome, PermitId, PermitReason, PermitSet, ResourceHealth,
};
use crate::connection::session::{HandleKind, SessionContext, SessionHandleRef, TransactionState};
use crate::connection::types::{
    Counter, DbSessionId, ExecutionTarget, OwnerRef, PoolKeyFingerprint, ResourceId,
};

/// 假资源的状态。§3 的建连 → 就绪 → 执行 → 复用前重置 → 关闭序列。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakeResourceState {
    /// `Created`：已记 permit +1，但尚未 Opening（§5.3 规则 1 的变化点）。
    Created,
    /// `Opening`：正在建连 / 初始化。
    Opening,
    /// `Ready`：可执行。
    Ready,
    /// `Executing`：有一次执行在途。
    Executing,
    /// `Reconfiguring`：上下文切换中。
    Reconfiguring,
    /// `Closing`：关闭握手在途。
    Closing,
    /// `Closed`：已确认关闭，permit -1。
    Closed,
    /// `Quarantined`：隔离，预算占用保留。
    Quarantined,
    /// `Lost`：资源丢失，后续执行一律拒绝。
    Lost,
}

impl FakeResourceState {
    pub fn as_str(self) -> &'static str {
        match self {
            FakeResourceState::Created => "Created",
            FakeResourceState::Opening => "Opening",
            FakeResourceState::Ready => "Ready",
            FakeResourceState::Executing => "Executing",
            FakeResourceState::Reconfiguring => "Reconfiguring",
            FakeResourceState::Closing => "Closing",
            FakeResourceState::Closed => "Closed",
            FakeResourceState::Quarantined => "Quarantined",
            FakeResourceState::Lost => "Lost",
        }
    }

    /// 是否还可以被 `acquireResource` 复用。
    pub fn is_reusable(self) -> bool {
        matches!(self, FakeResourceState::Ready)
    }

    /// 是否仍占用预算（§5.3 规则 2、规则 3 的判据）。
    pub fn occupies_budget(self) -> bool {
        !matches!(self, FakeResourceState::Closed)
    }
}

/// §5.3 permit 台账的记账锚点。
#[derive(Debug, Clone)]
pub struct Accounting {
    pub budget_class: BudgetClass,
    pub permit_id: PermitId,
    /// 预算是否已经占用。`true` 期间任何变化点都不得把余额当零。
    pub occupied: bool,
}

impl Accounting {
    pub fn new(budget_class: BudgetClass, permit_id: PermitId) -> Self {
        Self {
            budget_class,
            permit_id,
            occupied: true,
        }
    }

    /// 归还预算时返回应写入 journal 的事件；已归还过则返回 `None`（防重复 -1）。
    pub fn release(&mut self) -> Option<PermitEventPlan> {
        if !self.occupied {
            return None;
        }
        self.occupied = false;
        Some(PermitEventPlan {
            delta: -1,
            reason: PermitReason::Close,
        })
    }
}

/// permit 收支的待写入计划，交给 `ops.rs` 真正落进 journal。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermitEventPlan {
    pub delta: i32,
    pub reason: PermitReason,
}

/// 一个假资源。
#[derive(Debug, Clone)]
pub struct FakeResource {
    // ---- §3.1 规定的字段 ----
    pub resource_id: ResourceId,
    pub resource_key: String,
    pub pool_key: PoolKeyFingerprint,
    pub owner: Option<OwnerRef>,
    pub target: ExecutionTarget,
    pub state: FakeResourceState,
    pub capabilities: CapabilitySnapshot,
    pub handles: IndexMap<String, SessionHandleRef>,
    pub protocol_drained: bool,
    pub health: ResourceHealth,
    pub execution_seq: u64,
    // ---- §5.3 台账所需的记账锚点 ----
    pub accounting: Accounting,
    pub db_session_id: DbSessionId,
    pub runtime_epoch: Counter,
    pub context: SessionContext,
    pub context_revision: Counter,
    pub transaction_state: TransactionState,
    /// 最近一次执行失败码；`commit → Unknown` 时它就是 journal 里必须记的 `errorCode`（§9.2）。
    pub last_error_code: Option<ExecutionErrorCode>,
}

impl FakeResource {
    pub fn new(
        resource_id: ResourceId,
        resource_key: impl Into<String>,
        pool_key: PoolKeyFingerprint,
        owner: OwnerRef,
        target: ExecutionTarget,
        capabilities: CapabilitySnapshot,
        accounting: Accounting,
        db_session_id: DbSessionId,
        runtime_epoch: Counter,
        context: SessionContext,
    ) -> Self {
        Self {
            resource_id,
            resource_key: resource_key.into(),
            pool_key,
            owner: Some(owner),
            target,
            state: FakeResourceState::Created,
            capabilities,
            handles: IndexMap::new(),
            protocol_drained: true,
            health: ResourceHealth::Healthy,
            execution_seq: 0,
            accounting,
            db_session_id,
            runtime_epoch,
            context,
            context_revision: Counter::ZERO,
            transaction_state: TransactionState::None,
            last_error_code: None,
        }
    }

    pub fn owner_ref(&self) -> Option<&OwnerRef> {
        self.owner.as_ref()
    }

    pub fn budget_class(&self) -> BudgetClass {
        self.accounting.budget_class.clone()
    }

    pub fn permit_set(&self) -> PermitSet {
        PermitSet {
            class: self.accounting.budget_class.clone(),
            permit_ids: vec![self.accounting.permit_id.clone()],
        }
    }

    /// §9.4 归池前置检查：协议已排空 **且** 无已登记句柄。
    pub fn can_return_to_pool(&self) -> bool {
        self.protocol_drained && self.handles.is_empty()
    }

    pub fn registered_handles(&self) -> usize {
        self.handles.len()
    }

    pub fn open_handles(&self) -> Vec<&SessionHandleRef> {
        self.handles
            .values()
            .filter(|handle| !handle.closed)
            .collect()
    }

    pub fn has_open_transaction(&self) -> bool {
        matches!(self.transaction_state, TransactionState::Active)
            || self
                .open_handles()
                .iter()
                .any(|handle| handle.kind == HandleKind::Transaction)
    }

    /// §9.2 `close_session_cursor` 等注销路径使用。
    pub fn deregister_handle(&mut self, handle_id: &str) -> Option<SessionHandleRef> {
        self.handles.shift_remove(handle_id)
    }

    pub fn register_handle(&mut self, handle: SessionHandleRef) {
        self.handles
            .insert(handle.handle_id.as_str().to_string(), handle);
    }

    pub fn next_execution_seq(&mut self) -> u64 {
        self.execution_seq += 1;
        self.execution_seq
    }

    /// 失效资源：拒绝后续执行（§4.2 F3 资源丢失）。
    pub fn mark_lost(&mut self) {
        self.state = FakeResourceState::Lost;
        self.health = ResourceHealth::Lost;
        self.protocol_drained = false;
    }

    /// 隔离：预算占用保留（§5.3 规则 3）。
    pub fn quarantine(&mut self) {
        self.state = FakeResourceState::Quarantined;
        self.health = ResourceHealth::Degraded;
    }

    /// §3.2 (i)：`resetResource` 返回 `Clean` **不等于**事务已终结。
    /// 本方法只改复用相关的字段，**不动** `transaction_state`。
    pub fn reset_for_reuse(&mut self) {
        self.state = FakeResourceState::Ready;
        self.protocol_drained = true;
        self.health = ResourceHealth::Healthy;
        self.last_error_code = None;
    }

    pub fn close_outcome(&self) -> CloseOutcome {
        match self.state {
            FakeResourceState::Closed => CloseOutcome::Closed,
            _ => CloseOutcome::CloseUnconfirmed,
        }
    }

    /// 租约是否还钉在这张资源上（§9.3 R1 被驱逐时必须为真）。
    pub fn is_pinned(&self) -> bool {
        self.owner.is_some() && self.state.occupies_budget()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::types::{
        ConfigRevision, ConnectionId, HandleId, NamespaceTarget, PoolKeyInputs,
    };

    fn fixture_target() -> ExecutionTarget {
        ExecutionTarget {
            connection_id: ConnectionId::new("conn-fixture-p"),
            namespace: NamespaceTarget::default(),
            object: None,
        }
    }

    fn resource() -> FakeResource {
        let resource_id = ResourceId::new("res_w1_0001");
        FakeResource {
            resource_id: resource_id.clone(),
            resource_key: "res_w1_0001".to_string(),
            pool_key: PoolKeyFingerprint::derive(&PoolKeyInputs {
                connection_id: ConnectionId::new("conn-fixture-p"),
                config_revision: ConfigRevision::new(7),
                driver_id: "fake-sql".to_string(),
                namespace: NamespaceTarget::default(),
                execution_identity_key: "exec-identity-shared".to_string(),
                policy_isolation_key: "policy-shared".to_string(),
            }),
            owner: None,
            target: fixture_target(),
            state: FakeResourceState::Created,
            capabilities: CapabilitySnapshot::defaults("fake-sql", "0.0.1"),
            handles: IndexMap::new(),
            protocol_drained: true,
            health: ResourceHealth::Healthy,
            execution_seq: 0,
            accounting: Accounting::new(BudgetClass::ShortOpPool, PermitId("pmt_0001".to_string())),
            db_session_id: DbSessionId::new("dbs_w1_0001"),
            runtime_epoch: Counter::new(1),
            context: SessionContext::unknown(&NamespaceTarget::default(), "exec-identity-shared"),
            context_revision: Counter::ZERO,
            transaction_state: TransactionState::None,
            last_error_code: None,
        }
    }

    #[test]
    fn reset_for_reuse_never_terminates_a_transaction() {
        let mut resource = resource();
        resource.transaction_state = TransactionState::Active;
        resource.state = FakeResourceState::Executing;
        resource.reset_for_reuse();
        assert_eq!(
            resource.transaction_state,
            TransactionState::Active,
            "§3.2(i)：reset ≠ 事务终结"
        );
        assert_eq!(resource.state, FakeResourceState::Ready);
    }

    #[test]
    fn only_closed_releases_the_budget_and_closed_quarantine_both_leave_the_live_set() {
        assert!(FakeResourceState::Quarantined.occupies_budget());
        assert!(FakeResourceState::Lost.occupies_budget());
        assert!(!FakeResourceState::Closed.occupies_budget());
    }

    #[test]
    fn a_permit_is_returned_at_most_once() {
        let mut accounting =
            Accounting::new(BudgetClass::ShortOpPool, PermitId("pmt_0001".to_string()));
        assert_eq!(accounting.release().map(|plan| plan.delta), Some(-1));
        assert!(accounting.release().is_none(), "重复关闭不得二次归还预算");
    }

    #[test]
    fn return_to_pool_requires_drained_protocol_and_no_handles() {
        let mut resource = resource();
        assert!(resource.can_return_to_pool());
        resource.protocol_drained = false;
        assert!(!resource.can_return_to_pool());
        resource.protocol_drained = true;
        resource.handles.insert(
            "hdl_1".to_string(),
            SessionHandleRef::new(
                HandleId::new("hdl_1"),
                HandleKind::Cursor,
                resource.resource_id.clone(),
                Counter::new(1),
            ),
        );
        assert!(!resource.can_return_to_pool());
    }
}

//! FakeResource —— 假资源的状态机（fake-runtime-fixtures.md §3.1、§4）。
//!
//! 字段与 §3.1 的 `FakeResource` 定义一一对应；`accounting` 子结构是为 §5.3 的
//! permit 台账补的最小记账锚点（§3.1 没列它，但 §5.3 要求「每次 permit 变化」可核对，
//! 而台账需要 `budget_class` 与 `permit_id`，只能挂在资源上）。
//!
//! 本文件**不依赖** `journal`、不依赖 `script`、不依赖任何其它夹具模块，只依赖
//! `connection::{port, session, types, capability, error}`，与 §2 「依赖方向单向向下」一致。
//!
//! `closeResource` 的**全部**状态迁移住在 [`FakeResource::prepare_close`]（§9.4(b)）：
//! 判归池要消费的输入、被注销的句柄、permit 归还计划、关闭结论，都由这一次调用交出。
//! `ops.rs` 拿不到第二次读 slot 的入口，所以「记录一个量、消费另一个量」在结构上不可接线。

use indexmap::IndexMap;

use crate::connection::capability::CapabilitySnapshot;
use crate::connection::error::ProviderError;
use crate::connection::execution::ExecutionErrorCode;
use crate::connection::port::{
    BudgetClass, CloseOutcome, PermitId, PermitReason, PermitSet, ResourceHandle, ResourceHealth,
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

/// 一次 permit 归还的完整事实：计划 + 写给台账所需的 `permitId` / `budgetClass`。
///
/// 三者都来自**同一张 slot**（`Accounting`），所以 `ops.rs` 不需要为它们再查一次资源表 ——
/// 那正是 §9.4(b) 里「两次独立求值」的形态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermitRelease {
    pub plan: PermitEventPlan,
    pub permit_id: PermitId,
    pub budget_class: BudgetClass,
}

/// `closeResource` 的**归池前置**记录，由 [`FakeResource::prepare_close`] 在实际注销的那把锁里
/// 算出来（§9.4(b)）。
///
/// 三个字段的关系是这条记录存在的全部理由：
///
/// - `measured_handles` —— 被调方**实际测到**的「关闭前登记句柄数」，归池判据只消费它；
/// - `declared_handles` —— 调用方（宿主）按 §6.5 在 `CloseResourceRequest` 里**声称**的同一个量；
/// - `protocol_drained` —— 排空标志在 `prepare_close` 内合并后的值。判据读它，
///   不再第二次读 slot。
///
/// 判据的输入**只有一个来源**（`measured_handles`），而 `declared_handles` 只是被**对照**的账：
/// 两者分叉时台账里留得下证据、规则 9 判负 ——「记录一个量、消费另一个量」因此不可分叉。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosePrecondition {
    pub protocol_drained: bool,
    pub measured_handles: usize,
    pub declared_handles: usize,
    /// F11：这次关闭**没有**得到确认。为真时归池判据不参与判定（资源根本没关成），
    /// 且 `ops.rs` 必须走 `CloseUnconfirmed` 那条记账分支。
    pub unconfirmed: bool,
}

impl ClosePrecondition {
    /// 该不该记 `ReturnedToPool`。
    ///
    /// §9.4 把归池前置写成 **AND**：「宿主检查执行终态、无活跃消费者/取消句柄、
    /// 已收到 protocolDrained、预算和 owner 合法、以及 **§6.5 已登记句柄为空**」，
    /// 并且「任一失败都关闭，即使 driver 返回 Clean 也不能绕过宿主检查」。因此这里
    /// 四个合取项一个都不能少：
    ///
    /// 1. `!unconfirmed` —— F11 连「资源已销毁」都没证明，谈不上归池；
    /// 2. `protocol_drained` —— 排空证据；
    /// 3. `measured_handles == 0` —— **被调方**在同一把锁里、注销之前实测到的登记数；
    /// 4. `declared_handles == measured_handles` —— **宿主**按 §6.5 的账。
    ///    两份账不一致时不归池：§6.5 规定句柄只能经登记/注销改变，所以分叉就意味着
    ///    有人绕过登记册动了句柄，或宿主拿着一份陈旧账本在关资源 —— 两种都不能伪称
    ///    「宿主检查全部通过」。注意分叉**不影响** `Closed` 与 permit 归还：资源确实关掉了。
    ///
    /// 判据消费的**全部**输入都在这一个记录里，且都来自同一次求值（`prepare_close`）。
    /// 旧实现把「测注销前快照」与「消费宿主声明」写成两个独立表达式，二者可以悄悄分叉
    /// 而门禁全绿 —— 这就是 §9.4(b) 登记的缺陷，`declared_handles` 现在被真正消费了。
    pub fn returns_to_pool(&self) -> bool {
        !self.unconfirmed
            && self.protocol_drained
            && self.measured_handles == 0
            && self.declared_handles == self.measured_handles
    }

    /// 宿主的账与被调方实测的账是否一致。`false` 时 `ops.rs` 只记 `Closed`、不归池。
    pub fn declaration_matches_measurement(&self) -> bool {
        self.declared_handles == self.measured_handles
    }
}

/// [`FakeResource::prepare_close`] 的返回：关闭这一次产生的全部**已记账事实**。
///
/// 之所以打包成一个结构而不是四五个元组元素：`ops.rs` 需要用它写 journal，
/// 而journal 侧的每一个输入都必须能追溯到「被调方实际算的那个值」。元组会把
/// 「哪个数是测出来的、哪个数是声明的」这件事退化成调用点的顺序约定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseAttempt {
    pub precondition: ClosePrecondition,
    /// 迁移**之后**资源侧的关闭结论。回执的 `state` 与 `resource_release` 由它映射，
    /// 不是由 `ops.rs` 再判一遍 `state == Closed`（同 §9.4(b)：一个结论只有一处口径）。
    pub close_outcome: CloseOutcome,
    /// 本次被注销的句柄（确认路径下就是 `measured_handles` 个；F11 下为空）。
    pub deregistered: Vec<SessionHandleRef>,
    /// permit 归还计划。`None` = 不归还（F11，或幂等关闭时已归还过，§4.3 I1）。
    pub permit: Option<PermitRelease>,
    /// 关闭**开始前**资源上是否挂着未终结事务，供回执诚实决定 `effectOutcome`。
    pub had_transaction: bool,
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

    /// §3.1「每次操作都要校验 `resourceId` + `runtimeEpoch` + owner」的**资源侧**判据。
    ///
    /// [`Self::resolve`]（`handles.rs`，返回快照的那一个入口）与本模块的
    /// [`FakeResource::prepare_close`] 共用这一个方法，所以「校验口径」只有一份定义：
    /// 两条路径不可能各写一份而悄悄漂移。错误顺序（id → epoch → owner）与 `resolve` 一致，
    /// §9.2 的两条反例仍各自落在自己的错误码上。
    ///
    /// 无 owner 的资源无法验证凭证 —— 与 `resolve` 同样报 `SessionLost`。
    pub fn verify_presentation(&self, handle: &ResourceHandle) -> Result<(), ProviderError> {
        let owner = self.owner.as_ref().ok_or_else(|| {
            ProviderError::SessionLost(format!(
                "资源 {} 没有 owner，无法验证句柄",
                self.resource_id.as_str()
            ))
        })?;
        handle.verify(&self.resource_id, &self.runtime_epoch, owner)
    }

    /// `closeResource` 的全部状态迁移，**一次**持锁调用完成，并交出被调方实际算出的判据输入。
    ///
    /// 顺序是刻意的，而且整段只有一次 slot 求值：
    /// 「校验凭证 → 测**注销前**的句柄数 → 按 `unconfirmed` 分流 →（确认路径）无条件注销句柄
    /// →（按脚本）清事务态 → 落 `Closed` / 停在 `Closing` → 尝试归还 permit」。
    ///
    /// 调用方（`ops.rs`）拿得到的只有返回的 [`ClosePrecondition`]，**没有**第二次求值的入口 ——
    /// §9.4(b) 登记的「记录一个量、消费另一个量」正是从这条缝里长出来的。
    ///
    /// - `unconfirmed = true`（F11 关闭未确认）：资源**不**销毁 —— 留在 `Closing`、
    ///   `protocol_drained = false`、句柄保持登记、permit **不**归还（§5.3 规则 3：余额不变）。
    ///   注销之所以不做：未确认的关闭不能伪称句柄随资源一起死了，否则后续隔离/重试路径
    ///   会读到一张「句柄已清空、其实没关成」的资源。
    /// - `unconfirmed = false`：CM-74 的确认关闭。句柄注销**无条件**发生（它是关闭语义，
    ///   不是回滚语义），且排在 `Closed` 与 permit 归还之前。
    ///
    /// `rolled_back` 只由脚本标志决定：真的回滚过才清事务态，回执才诚实报 `RolledBack`。
    ///
    /// 返回里的 [`CloseOutcome`] 是**迁移之后**从状态机派生的结论（本结构唯一的产出口径），
    /// 回执的 `state` / `resource_release` 都必须由它映射 —— 别处不再自己判一遍
    /// 「`state == Closed`」。这一条正是核实 `CloseOutcome::CloseUnconfirmed`（`state.rs`）
    /// 与 `FaultKind::CloseUnconfirmed`（F11）关系的结论：**两条不同路径、同一个结论口径**，
    /// 前者是从状态机派生的答案，后者是让状态机停在非 `Closed` 的注入原因。
    pub fn prepare_close(
        &mut self,
        handle: &ResourceHandle,
        request_protocol_drained: bool,
        declared_handles: usize,
        unconfirmed: bool,
        rolled_back: bool,
    ) -> Result<CloseAttempt, ProviderError> {
        self.verify_presentation(handle)?;
        // 归池判定的输入：关闭开始前这张资源上还挂着几个句柄 / 是否已经关过。
        // 两者都必须在任何状态迁移**之前**测 —— 之后测到的是恒 0 / 恒 true，判据会退化。
        let measured_handles = self.registered_handles();
        let had_transaction = self.has_open_transaction();

        if unconfirmed {
            // F11：关闭未确认 —— 状态停在 Closing、排空证据撤回、句柄与 permit 原样保留。
            self.state = FakeResourceState::Closing;
            self.protocol_drained = false;
            return Ok(CloseAttempt {
                precondition: ClosePrecondition {
                    protocol_drained: self.protocol_drained,
                    measured_handles,
                    declared_handles,
                    unconfirmed: true,
                },
                close_outcome: self.close_outcome(),
                deregistered: Vec::new(),
                permit: None,
                had_transaction,
            });
        }

        self.protocol_drained = request_protocol_drained && self.protocol_drained;
        // CM-74：句柄随资源一起死，必须在物理关闭前注销（journal 先记 `closed`）。
        let deregistered: Vec<SessionHandleRef> = self
            .handles
            .values_mut()
            .map(|handle| {
                handle.closed = true;
                handle.clone()
            })
            .collect();
        self.handles.clear();
        // 回滚语义仍以脚本标志为门：真的回滚过才清事务态。
        if rolled_back {
            self.transaction_state = TransactionState::None;
        }
        self.state = FakeResourceState::Closed;
        // permit 归还只在 `Closed` 这条路径上尝试，且由 `Accounting::release` 保证至多一次。
        let permit = self.accounting.release().map(|plan| PermitRelease {
            plan,
            permit_id: self.accounting.permit_id.clone(),
            budget_class: self.accounting.budget_class.clone(),
        });
        Ok(CloseAttempt {
            precondition: ClosePrecondition {
                protocol_drained: self.protocol_drained,
                measured_handles,
                declared_handles,
                unconfirmed: false,
            },
            close_outcome: self.close_outcome(),
            deregistered,
            permit,
            had_transaction,
        })
    }

    /// 资源侧的关闭结论（§5.1 `closeResource` 的 `Closed` / `CloseUnconfirmed`）。
    ///
    /// **与 F11 的关系（核实结论：两条不同路径，同一个结论口径）**：
    /// - `FaultKind::CloseUnconfirmed`（§4.1 F11）是**注入原因**：脚本让这一次 closeResource
    ///   的关闭握手拿不到确认，`prepare_close(unconfirmed = true)` 据此把状态钉在 `Closing`、
    ///   撤回排空证据、**不**归还 permit；
    /// - [`CloseOutcome::CloseUnconfirmed`]（本方法）是**从状态机派生的结论**：任何没落到
    ///   `Closed` 的状态（`Closing` / `Quarantined` / `Lost` …）都算「未确认关闭」。
    ///
    /// 二者不同源：F11 之外（如 F8 回滚失败导致的 `Quarantined`）本方法同样报
    /// `CloseUnconfirmed`，因为预算确实还没回收。而 `prepare_close` 是**唯一**产出这个结论
    /// 给回执的地方（`ops.rs` 从 `CloseAttempt::close_outcome` 映射 `state` /
    /// `resource_release`，不再自己判一遍 `state == Closed`）—— 这就是 §9.4(b) 要的
    /// 「一个结论只有一处口径」。`close_outcome` 在夹具里此前没有任何调用者，属于悬空判据；
    /// F11 那组用例点名它：`f11_and_the_derived_close_outcome_never_disagree_with_the_receipt`
    /// （注入 ⇒ `CloseUnconfirmed`）与 `f11_close_unconfirmed_keeps_the_permit_and_records_exactly_one_event`
    /// （台账后果）。
    ///
    /// 幂等关闭：关掉一张**已经** `Closed` 的资源仍会走一遍确认路径（fixtures §3 契约表
    /// 「幂等：首次 `Closed`，重复调用仍 `Closed」/ 连接 §3 INV-10），
    /// 但 `Accounting::release()` 返回 `None`，permit 不会被二次归还（§4.3 I1）。
    pub fn close_outcome(&self) -> CloseOutcome {
        match self.state {
            FakeResourceState::Closed => CloseOutcome::Closed,
            _ => CloseOutcome::CloseUnconfirmed,
        }
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

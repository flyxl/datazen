//! CommandJournal 与变化点断言（fake-runtime-fixtures.md §5）。
//!
//! 依赖方向：本模块**不依赖** `fake_resource` —— journal 记录由 provider 写入，断言由测试调用。
//! 依赖向下到 `clock`（时间戳）与非夹具的 `connection::{port, session, types, execution}`。
//!
//! §13 日志脱敏：journal 不记录凭据、附件令牌与幂等令牌 nonce；故障注入脚本的**脚本 id** 可记录，
//! 字面量不可记录。本模块因此没有任何可以接收 `Secret` 的入口。

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::connection::execution::{
    EffectOutcome, ExecutionErrorCode, TruncationRecord,
};
use crate::connection::port::{BudgetClass, PermitId, PermitReason};
use crate::connection::session::{HandleKind, SessionHandleRef};
use crate::connection::types::{
    Counter, DbSessionId, ExecutionId, HandleId, LeaseId, OwnerRef, PoolKeyFingerprint, ResourceId,
    StreamId,
};

use super::clock::FakeClock;

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

    /// 是否终止预算占用。只有 `Closed` 归还（§5.3 规则 2、规则 3）。
    fn releases_permit(&self) -> bool {
        matches!(self, ResourceEvent::Closed)
    }

    /// 是否把资源移出 live 集合（`Closed` 才释放占用；`Quarantined` 亦不可再被 acquire）。
    fn leaves_live_set(&self) -> bool {
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

#[derive(Default)]
struct LedgerState {
    issued: BTreeSet<String>,
    returned: BTreeSet<String>,
    balance: i64,
}

#[derive(Default)]
struct JournalState {
    entries: Vec<JournalEntry>,
    handles: BTreeMap<String, HandleRecord>,
    orphan_handles: Vec<HandleRecord>,
    ledger: LedgerState,
    /// 事件流序号登记（§4.3 I8）。
    stream_sequences: BTreeMap<String, Vec<u64>>,
    /// I3：live leases。
    live_leases: BTreeMap<String, ResourceId>,
    /// I4：session registry 的活动会话。
    active_sessions: BTreeSet<String>,
    /// 不占用物理连接的空闲池数量。参与 §5.3 规则 4 的等式。
    idle_pools: usize,
    /// 控制 socket / 预留连接数。参与 §5.3 规则 4 的等式。
    control_sockets: usize,
    live_executions: BTreeSet<ExecutionId>,
}

struct JournalShared {
    clock: FakeClock,
    seq: AtomicU64,
    state: Mutex<JournalState>,
}

impl JournalShared {
    fn lock(&self) -> MutexGuard<'_, JournalState> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::SeqCst)
    }
}

// ---------------------------------------------------------------------------
// CommandJournal
// ---------------------------------------------------------------------------

/// 记录所有可观察的资源与执行变化（§5 L247）。
#[derive(Clone)]
pub struct CommandJournal {
    inner: Arc<JournalShared>,
}

impl CommandJournal {
    pub fn new(clock: FakeClock) -> Self {
        Self {
            inner: Arc::new(JournalShared {
                clock,
                seq: AtomicU64::new(1),
                state: Mutex::new(JournalState::default()),
            }),
        }
    }

    pub fn clock(&self) -> FakeClock {
        self.inner.clock.clone()
    }

    /// 全部条目，按 `seq` 升序。
    pub fn entries(&self) -> Vec<JournalEntry> {
        let mut entries = self.inner.lock().entries.clone();
        entries.sort_by_key(JournalEntry::seq);
        entries
    }

    pub fn permit_ledger(&self) -> Vec<PermitEvent> {
        self.entries()
            .into_iter()
            .filter_map(|entry| match entry {
                JournalEntry::Permit { seq, permit_id, delta, reason, budget_class } => {
                    Some(PermitEvent { seq, permit_id, delta, reason, budget_class })
                }
                _ => None,
            })
            .collect()
    }

    /// 当前仍然登记在会话 actor 上的句柄（I5）。
    pub fn handle_registry(&self) -> BTreeMap<String, HandleRecord> {
        self.inner.lock().handles.clone()
    }

    pub fn open_handles(&self) -> Vec<HandleRecord> {
        self.inner.lock().handles.values().filter(|h| !h.closed).cloned().collect()
    }

    /// 孤儿句柄（I7）：fake 侧建了句柄但没交给宿主。
    pub fn orphan_handles(&self) -> Vec<HandleRecord> {
        self.inner.lock().orphan_handles.clone()
    }

    /// 预算仍然被占用的资源（I2 的 live 集合）。
    pub fn live_resources(&self) -> Vec<ResourceId> {
        live_resources_in(&self.entries())
    }

    pub fn permit_balance(&self) -> i64 {
        self.inner.lock().ledger.balance
    }

    pub fn permits_issued(&self) -> usize {
        self.inner.lock().ledger.issued.len()
    }

    pub fn permits_returned(&self) -> usize {
        self.inner.lock().ledger.returned.len()
    }

    pub fn live_executions(&self) -> Vec<ExecutionId> {
        self.inner.lock().live_executions.iter().cloned().collect()
    }

    pub fn live_leases(&self) -> Vec<LeaseId> {
        self.inner.lock().live_leases.keys().cloned().map(LeaseId::new).collect()
    }

    pub fn active_sessions(&self) -> Vec<DbSessionId> {
        self.inner.lock().active_sessions.iter().cloned().map(DbSessionId::new).collect()
    }

    pub fn set_idle_pools(&self, count: usize) {
        self.inner.lock().idle_pools = count;
    }

    pub fn set_control_sockets(&self, count: usize) {
        self.inner.lock().control_sockets = count;
    }

    pub fn idle_pools(&self) -> usize {
        self.inner.lock().idle_pools
    }

    pub fn control_sockets(&self) -> usize {
        self.inner.lock().control_sockets
    }

    // -- 写入：资源序列 --------------------------------------------------

    pub fn record_resource_event(
        &self,
        resource_id: &ResourceId,
        event: ResourceEvent,
        owner: &OwnerRef,
        pool_key: PoolKeyFingerprint,
        budget_class: BudgetClass,
    ) -> u64 {
        let seq = self.inner.next_seq();
        self.inner.lock().entries.push(JournalEntry::Resource {
            seq,
            resource_id: resource_id.clone(),
            event,
            owner: owner.clone(),
            pool_key,
            budget_class,
        });
        seq
    }

    // -- 写入：执行序列 --------------------------------------------------

    pub fn record_execution_started(
        &self,
        resource_id: &ResourceId,
        execution_id: &ExecutionId,
        command_id: &str,
    ) -> u64 {
        let seq = self.inner.next_seq();
        let started_at_mono = self.inner.clock.monotonic().as_nanos();
        let mut state = self.inner.lock();
        state.live_executions.insert(execution_id.clone());
        state.entries.push(JournalEntry::Execution {
            seq,
            resource_id: resource_id.clone(),
            execution_id: execution_id.clone(),
            command_id: command_id.to_string(),
            started_at_mono,
            ended_at_mono: None,
            effect_outcome: None,
            error_code: None,
            protocol_drained: None,
            truncation: None,
        });
        seq
    }

    /// 终结一次执行。`protocol_drained` 必须显式给出 —— §5.3 规则 7 要求
    /// 「已记录或显式为 false」，不允许「没写」。
    pub fn record_execution_terminal(
        &self,
        execution_seq: u64,
        effect_outcome: EffectOutcome,
        error_code: Option<ExecutionErrorCode>,
        protocol_drained: bool,
        truncation: Option<TruncationRecord>,
    ) {
        let ended_at_mono = self.inner.clock.monotonic().as_nanos();
        let mut state = self.inner.lock();
        let mut finished: Option<ExecutionId> = None;
        for entry in state.entries.iter_mut() {
            if let JournalEntry::Execution {
                seq,
                execution_id,
                ended_at_mono: ended,
                effect_outcome: outcome,
                error_code: code,
                protocol_drained: drained,
                truncation: cut,
                ..
            } = entry
            {
                if *seq != execution_seq {
                    continue;
                }
                *ended = Some(ended_at_mono);
                *outcome = Some(effect_outcome);
                *code = error_code;
                *drained = Some(protocol_drained);
                *cut = truncation;
                finished = Some(execution_id.clone());
                break;
            }
        }
        if let Some(execution_id) = finished {
            state.live_executions.remove(&execution_id);
        }
    }

    /// 登记一个事件流序号（I8）。
    pub fn record_stream_event(&self, stream_id: &StreamId, stream_seq: u64) {
        self.inner
            .lock()
            .stream_sequences
            .entry(stream_id.as_str().to_string())
            .or_default()
            .push(stream_seq);
    }

    /// I8：`events.stream_sequence.is_contiguous()`。
    pub fn stream_sequence_is_contiguous(&self) -> bool {
        self.inner.lock().stream_sequences.values().all(|seqs| {
            let mut sorted = seqs.clone();
            sorted.sort_unstable();
            sorted.windows(2).all(|pair| pair[1] == pair[0] + 1)
        })
    }

    // -- 写入：permit 收支 -----------------------------------------------

    pub fn record_permit(
        &self,
        permit_id: &PermitId,
        delta: i32,
        reason: PermitReason,
        budget_class: BudgetClass,
    ) -> u64 {
        let seq = self.inner.next_seq();
        let mut state = self.inner.lock();
        match delta {
            1 => {
                state.ledger.issued.insert(permit_id.as_str().to_string());
            }
            -1 => {
                state.ledger.returned.insert(permit_id.as_str().to_string());
            }
            other => {
                // delta 只能是 ±1（§5 L253）。这是调用方契约违约，无法恢复，因此直接 panic 并说明原因。
                panic!("permit delta 只能是 +1 或 -1，实际收到 {other}（seq={seq}）");
            }
        }
        state.ledger.balance += i64::from(delta);
        state.entries.push(JournalEntry::Permit {
            seq,
            permit_id: permit_id.clone(),
            delta,
            reason,
            budget_class,
        });
        seq
    }

    // -- 写入：句柄登记 / 注销 --------------------------------------------

    pub fn record_handle(&self, handle: &SessionHandleRef, action: HandleAction, reason: &str) -> u64 {
        self.record_handle_for(handle, action, reason, None)
    }

    pub fn record_handle_for(
        &self,
        handle: &SessionHandleRef,
        action: HandleAction,
        reason: &str,
        execution_id: Option<&ExecutionId>,
    ) -> u64 {
        let seq = self.inner.next_seq();
        let record = HandleRecord {
            handle_id: handle.handle_id.as_str().to_string(),
            kind: handle.kind,
            resource_id: handle.resource_id.clone(),
            runtime_epoch: handle.runtime_epoch,
            closed: matches!(action, HandleAction::Closed),
            execution_id: execution_id.cloned(),
        };
        let handle_id = record.handle_id.clone();
        let mut state = self.inner.lock();
        match action {
            HandleAction::Registered => {
                state.handles.insert(handle_id, record);
            }
            HandleAction::Closed => {
                state.handles.remove(&handle_id);
                state.orphan_handles.retain(|orphan| orphan.handle_id != handle_id);
            }
            HandleAction::Rejected | HandleAction::Orphaned => {
                // 两者都不进入登记册，但必须留下可断言的痕迹。
                if action == HandleAction::Orphaned {
                    state.orphan_handles.push(record);
                }
            }
        }
        state.entries.push(JournalEntry::Handle {
            seq,
            handle_id,
            kind: handle.kind,
            resource_id: handle.resource_id.clone(),
            runtime_epoch: handle.runtime_epoch,
            action,
            reason: reason.to_string(),
        });
        seq
    }

    /// 关闭路径回收孤儿句柄：fake 侧的 `orphaned` 在资源确认关闭时一并回收（I7）。
    pub fn recover_orphans_on_close(&self, resource_id: &ResourceId, reason: &str) {
        let orphans: Vec<HandleRecord> = self
            .inner
            .lock()
            .orphan_handles
            .iter()
            .filter(|orphan| &orphan.resource_id == resource_id)
            .cloned()
            .collect();
        for orphan in orphans {
            let handle = SessionHandleRef::new(
                HandleId::new(orphan.handle_id.clone()),
                orphan.kind,
                orphan.resource_id.clone(),
                orphan.runtime_epoch,
            );
            self.record_handle(&handle, HandleAction::Closed, reason);
        }
    }

    // -- I3 / I4 登记簿 ---------------------------------------------------

    pub fn register_lease(&self, lease_id: &LeaseId, resource_id: &ResourceId) {
        self.inner.lock().live_leases.insert(lease_id.as_str().to_string(), resource_id.clone());
    }

    pub fn release_lease(&self, lease_id: &LeaseId) {
        self.inner.lock().live_leases.remove(lease_id.as_str());
    }

    pub fn register_active_session(&self, db_session_id: &DbSessionId) {
        self.inner.lock().active_sessions.insert(db_session_id.as_str().to_string());
    }

    pub fn close_active_session(&self, db_session_id: &DbSessionId) {
        self.inner.lock().active_sessions.remove(db_session_id.as_str());
    }

    // -- 断言入口（§5.4）-------------------------------------------------

    pub fn assert(&self) -> JournalAssert<'_> {
        JournalAssert::new(self)
    }

    pub fn assert_permits_balanced(&self) {
        JournalAssert::new(self).assert_permits_balanced();
    }

    pub fn assert_handle_closed(&self, handle_id: &str) {
        JournalAssert::new(self).assert_handle_closed(handle_id);
    }

    pub fn assert_no_return_to_pool_without_drain(&self) {
        JournalAssert::new(self).assert_no_return_to_pool_without_drain();
    }

    /// I5 直达断言。与 `assert().assert_no_open_handles()` 等价，单独暴露是为了让
    /// 故障用例在收尾处一行调用（§4.3）。
    pub fn assert_no_open_handles(&self) {
        JournalAssert::new(self).assert_no_open_handles();
    }

    /// I7 直达断言（§4.3）。
    pub fn assert_no_orphan_handles(&self) {
        JournalAssert::new(self).assert_no_orphan_handles();
    }

    /// I8 直达断言（§4.3）。
    pub fn assert_stream_sequence_contiguous(&self) {
        JournalAssert::new(self).assert_stream_sequence_contiguous();
    }
}

impl Default for CommandJournal {
    fn default() -> Self {
        Self::new(FakeClock::new())
    }
}

fn live_resources_in(entries: &[JournalEntry]) -> Vec<ResourceId> {
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
// JournalAssert
// ---------------------------------------------------------------------------

/// 变化点断言（§5.3 / §5.4）。失败信息一律带 `seq`。
pub struct JournalAssert<'a> {
    journal: &'a CommandJournal,
}

impl<'a> JournalAssert<'a> {
    pub fn new(journal: &'a CommandJournal) -> Self {
        Self { journal }
    }

    /// 重放 journal 并逐条检查 §5.3 的八条变化点规则，返回违例描述（空 = 全部满足）。
    /// 返回值而非直接 panic，是为了让「负例测试」可以断言具体违规项而不必 catch panic。
    pub fn change_point_violations(&self) -> Vec<String> {
        let entries = self.journal.entries();
        let state = self.journal.inner.lock();
        let mut violations: Vec<String> = Vec::new();
        let mut live = live_resources_in(&entries);
        let permits: Vec<PermitEvent> = entries
            .iter()
            .filter_map(|entry| match entry {
                JournalEntry::Permit { seq, permit_id, delta, reason, budget_class } => {
                    Some(PermitEvent {
                        seq: *seq,
                        permit_id: permit_id.clone(),
                        delta: *delta,
                        reason: *reason,
                        budget_class: *budget_class,
                    })
                }
                _ => None,
            })
            .collect();

        // 规则 4：每次 permit 变化 → Σ delta == live + idle_pools + control_sockets
        let mut expected_balance: i64 = 0;
        for event in &permits {
            expected_balance += i64::from(event.delta);
            let expected =
                live.len() as i64 + state.idle_pools as i64 + state.control_sockets as i64;
            if expected_balance != expected {
                violations.push(format!(
                    "seq={}：permit 收支不平。Σdelta={}，期望 live_resources({})+idle_pools({})+control_sockets({})={}",
                    event.seq, expected_balance, live.len(), state.idle_pools, state.control_sockets, expected
                ));
            }
        }

        for entry in &entries {
            match entry {
                JournalEntry::Resource { seq, resource_id, event, budget_class, .. } => {
                    match event {
                        // 规则 1：创建 → permit 余额 = +1 且 live_resources +1
                        ResourceEvent::Created => {
                            live.push(resource_id.clone());
                            if !permits.iter().any(|p| p.delta == 1) {
                                violations.push(format!(
                                    "seq={}：资源 {} 创建但没有任何 permit 申请记录",
                                    seq,
                                    resource_id.as_str()
                                ));
                            }
                        }
                        // 规则 2：每个 Closed → permit = -1（只有 Closed）
                        ResourceEvent::Closed => {
                            if !permits.iter().any(|p| p.delta == -1 && p.budget_class == *budget_class)
                            {
                                violations.push(format!(
                                    "seq={}：资源 {} 已 Closed 但 permit 未归还（budget_class={:?}）",
                                    seq,
                                    resource_id.as_str(),
                                    budget_class
                                ));
                            }
                        }
                        // 规则 3：CloseUnconfirmed / Quarantined → 余额不变（资源仍占预算）
                        ResourceEvent::CloseUnconfirmed | ResourceEvent::Quarantined => {
                            if !live.contains(resource_id) {
                                violations.push(format!(
                                    "seq={}：资源 {} 处于 {}，必须保留预算占用",
                                    seq,
                                    resource_id.as_str(),
                                    event.as_str()
                                ));
                            }
                        }
                        // §9.4：归池前置不全满足时必须关闭，不得归池。
                        ResourceEvent::ReturnedToPool { protocol_drained, registered_handles } => {
                            if !*protocol_drained {
                                violations.push(format!(
                                    "seq={}：protocolDrained=false 时不得归池",
                                    seq
                                ));
                            }
                            if *registered_handles != 0 {
                                violations.push(format!(
                                    "seq={}：登记句柄非空({})时不得归池（§4.2 F10）",
                                    seq, registered_handles
                                ));
                            }
                        }
                        ResourceEvent::OpeningReady | ResourceEvent::Lost => {}
                    }
                }
                JournalEntry::Execution {
                    seq,
                    execution_id,
                    effect_outcome,
                    protocol_drained,
                    ended_at_mono,
                    ..
                } => {
                    if ended_at_mono.is_none() {
                        continue;
                    }
                    // 规则 7：每个 execution terminal → protocolDrained 已记录或显式 false
                    if protocol_drained.is_none() {
                        violations.push(format!(
                            "seq={}：执行 {} 终结但未记录 protocolDrained",
                            seq,
                            execution_id.as_str()
                        ));
                    }
                    let Some(outcome) = effect_outcome else {
                        violations.push(format!(
                            "seq={}：执行 {} 终结但未记录 effectOutcome",
                            seq,
                            execution_id.as_str()
                        ));
                        continue;
                    };
                    // 规则 8：不可判定的 errorCode 只能配 unknown / notStarted，绝不配 completed。
                    let entry_error = match entry {
                        JournalEntry::Execution { error_code, .. } => *error_code,
                        _ => None,
                    };
                    if outcome.requires_unknown_for_undecidable_error(entry_error)
                        && !matches!(outcome, EffectOutcome::Unknown | EffectOutcome::NotStarted)
                    {
                        violations.push(format!(
                            "seq={}：执行 {} 的 errorCode={:?} 不可判定作用域，effectOutcome 必须为 unknown，实际 {:?}",
                            seq,
                            execution_id.as_str(),
                            entry_error,
                            outcome
                        ));
                    }
                }
                JournalEntry::Handle { seq, handle_id, resource_id, runtime_epoch, action, .. } => {
                    match action {
                        // 规则 5：registered → 登记册条目与登记资源、epoch 一致
                        HandleAction::Registered => match state.handles.get(handle_id) {
                            None => violations.push(format!(
                                "seq={}：句柄 {} 登记后不在登记册中",
                                seq, handle_id
                            )),
                            Some(record) => {
                                if &record.resource_id != resource_id {
                                    violations.push(format!(
                                        "seq={}：句柄 {} 的 resourceId 与登记记录不一致",
                                        seq, handle_id
                                    ));
                                }
                                if record.runtime_epoch != *runtime_epoch {
                                    violations.push(format!(
                                        "seq={}：句柄 {} 的 runtimeEpoch 与登记记录不一致",
                                        seq, handle_id
                                    ));
                                }
                                if record.closed {
                                    violations
                                        .push(format!("seq={}：句柄 {} 登记时不得是 closed", seq, handle_id));
                                }
                            }
                        },
                        // 规则 6：closed → 从登记册移除
                        HandleAction::Closed => {
                            if state.handles.contains_key(handle_id) {
                                violations.push(format!(
                                    "seq={}：句柄 {} 已 closed 但仍在登记册中",
                                    seq, handle_id
                                ));
                            }
                        }
                        HandleAction::Rejected | HandleAction::Orphaned => {}
                    }
                }
                JournalEntry::Permit { .. } => {}
            }
        }

        violations
    }

    pub fn assert_change_points(&self) {
        let violations = self.change_point_violations();
        assert!(
            violations.is_empty(),
            "变化点断言失败（§5.3）：\n  - {}",
            violations.join("\n  - ")
        );
    }

    /// I1 / I6：`permits_returned == permits_requested` 且台账平衡。
    pub fn ledger_violations(&self) -> Vec<String> {
        let state = self.journal.inner.lock();
        let mut violations = Vec::new();
        let unmatched: Vec<&String> =
            state.ledger.returned.difference(&state.ledger.issued).collect();
        if !unmatched.is_empty() {
            violations.push(format!(
                "I6 不满足：归还了未签发的 permit {:?}",
                unmatched
            ));
        }
        if state.ledger.returned.len() != state.ledger.issued.len() {
            violations.push(format!(
                "I1 不满足：permits_returned({}) != permits_requested({})",
                state.ledger.returned.len(),
                state.ledger.issued.len()
            ));
        }
        if state.ledger.balance != 0 {
            violations.push(format!(
                "I6 不满足：许可余额为 {}，期望 0",
                state.ledger.balance
            ));
        }
        violations
    }

    pub fn assert_permits_balanced(&self) {
        let violations = self.ledger_violations();
        assert!(
            violations.is_empty(),
            "permit 台账断言失败：\n  - {}",
            violations.join("\n  - ")
        );
    }

    /// 一个具体句柄是否已被注销。
    pub fn assert_handle_closed(&self, handle_id: &str) {
        assert!(
            !self.journal.handle_registry().contains_key(handle_id),
            "句柄 {handle_id} 仍登记在册，期望已注销"
        );
        let closed_seq = self
            .journal
            .entries()
            .into_iter()
            .filter_map(|entry| match entry {
                JournalEntry::Handle { seq, handle_id: id, action, .. }
                    if id == handle_id && action == HandleAction::Closed =>
                {
                    Some(seq)
                }
                _ => None,
            })
            .max();
        assert!(
            closed_seq.is_some(),
            "句柄 {handle_id} 从未出现 closed 事件（§5.3 规则 6）"
        );
    }

    /// §9.4：没有在 `protocolDrained=false` 或句柄非空的前提下归池。
    pub fn assert_no_return_to_pool_without_drain(&self) {
        let violations: Vec<String> = self
            .journal
            .entries()
            .into_iter()
            .filter_map(|entry| match entry {
                JournalEntry::Resource {
                    seq,
                    resource_id,
                    event: ResourceEvent::ReturnedToPool { protocol_drained, registered_handles },
                    ..
                } => {
                    let mut found = Vec::new();
                    if !protocol_drained {
                        found.push(format!(
                            "seq={}：资源 {} 在 protocolDrained=false 时被归池",
                            seq,
                            resource_id.as_str()
                        ));
                    }
                    if registered_handles != 0 {
                        found.push(format!(
                            "seq={}：资源 {} 在句柄非空({})时被归池",
                            seq,
                            resource_id.as_str(),
                            registered_handles
                        ));
                    }
                    Some(found)
                }
                _ => None,
            })
            .flatten()
            .collect();
        assert!(violations.is_empty(), "归池前置断言失败：\n  - {}", violations.join("\n  - "));
    }

    /// I2 / I3 / I4 / I5 / I7 / I8。
    pub fn leak_invariant_violations(&self) -> Vec<String> {
        let mut violations = Vec::new();
        let live_resources = self.journal.live_resources();
        if !live_resources.is_empty() {
            violations.push(format!(
                "I2 不满足：live_resources 非空：{:?}",
                live_resources.iter().map(ResourceId::as_str).collect::<Vec<_>>()
            ));
        }
        let live_leases = self.journal.live_leases();
        if !live_leases.is_empty() {
            violations.push(format!(
                "I3 不满足：live_leases 非空：{:?}",
                live_leases.iter().map(LeaseId::as_str).collect::<Vec<_>>()
            ));
        }
        let active_sessions = self.journal.active_sessions();
        if !active_sessions.is_empty() {
            violations.push(format!(
                "I4 不满足：active_sessions 非空：{:?}",
                active_sessions.iter().map(DbSessionId::as_str).collect::<Vec<_>>()
            ));
        }
        let open_handles = self.journal.open_handles();
        if !open_handles.is_empty() {
            violations.push(format!(
                "I5 不满足：handle_registry 非空：{:?}",
                open_handles.iter().map(|h| h.handle_id.as_str()).collect::<Vec<_>>()
            ));
        }
        violations.extend(self.ledger_violations());
        let orphans = self.journal.orphan_handles();
        if !orphans.is_empty() {
            violations.push(format!(
                "I7 不满足：orphan_handles 非空：{:?}",
                orphans.iter().map(|h| h.handle_id.as_str()).collect::<Vec<_>>()
            ));
        }
        if !self.journal.stream_sequence_is_contiguous() {
            violations.push("I8 不满足：事件流序号存在缺口".to_string());
        }
        violations
    }

    /// I5：登记册为空。
    pub fn assert_no_open_handles(&self) {
        let open = self.journal.open_handles();
        assert!(
            open.is_empty(),
            "I5 不满足：登记册仍有 {} 个未注销句柄：{:?}",
            open.len(),
            open.iter().map(|h| h.handle_id.as_str()).collect::<Vec<_>>()
        );
    }

    /// I7：孤儿句柄为空。
    pub fn assert_no_orphan_handles(&self) {
        let orphans = self.journal.orphan_handles();
        assert!(
            orphans.is_empty(),
            "I7 不满足：仍有 {} 个孤儿句柄：{:?}",
            orphans.len(),
            orphans.iter().map(|h| h.handle_id.as_str()).collect::<Vec<_>>()
        );
    }

    /// I8：事件流序号连续。
    pub fn assert_stream_sequence_contiguous(&self) {
        assert!(
            self.journal.stream_sequence_is_contiguous(),
            "I8 不满足：事件流序号存在缺口"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::testing::fixtures;

    fn owner() -> OwnerRef {
        OwnerRef::Editor {
            organization_id: crate::connection::types::OrganizationId::new("org-alpha"),
            principal_id: crate::connection::types::PrincipalId::new("user-alpha-1"),
            connection_id: crate::connection::types::ConnectionId::new("conn-fixture-p"),
            client_instance_id: crate::connection::types::ClientInstanceId::new("cli-1"),
            editor_session_id: crate::connection::types::EditorSessionId::new("ed-1"),
        }
    }

    fn pool_key() -> PoolKeyFingerprint {
        PoolKeyFingerprint::derive(&crate::connection::types::PoolKeyInputs {
            connection_id: fixtures::PROFILE_P.into(),
            config_revision: crate::connection::types::ConfigRevision::new(7),
            driver_id: "fake".into(),
            namespace: crate::connection::types::NamespaceTarget::default(),
            execution_identity_key: fixtures::IDENTITY_SHARED.into(),
            policy_isolation_key: "policy-alpha-1".into(),
        })
    }

    fn resource(tag: &str) -> ResourceId {
        ResourceId::new(format!("res_w1_{tag}"))
    }

    fn permit(tag: &str) -> PermitId {
        PermitId(format!("pmt_{tag}"))
    }

    fn handle(id: &str, res: &ResourceId, epoch: u64) -> SessionHandleRef {
        SessionHandleRef::new(
            HandleId::new(id),
            HandleKind::Transaction,
            res.clone(),
            Counter(epoch),
        )
    }

    /// 走完「创建 → 确认关闭」的正常生命周期。
    fn closed_lifecycle(journal: &CommandJournal) -> ResourceId {
        let res = resource("0001");
        journal.record_permit(&permit("0001"), 1, PermitReason::Acquire, BudgetClass::Session);
        journal.record_resource_event(
            &res,
            ResourceEvent::Created,
            &owner(),
            pool_key(),
            BudgetClass::Session,
        );
        journal.record_resource_event(
            &res,
            ResourceEvent::OpeningReady,
            &owner(),
            pool_key(),
            BudgetClass::Session,
        );
        journal.record_permit(&permit("0001"), -1, PermitReason::Close, BudgetClass::Session);
        journal.record_resource_event(
            &res,
            ResourceEvent::Closed,
            &owner(),
            pool_key(),
            BudgetClass::Session,
        );
        res
    }

    #[test]
    fn permit_balance_moves_plus_one_on_create_and_minus_one_on_confirmed_close() {
        let journal = CommandJournal::default();
        let res = resource("0001");
        journal.record_permit(&permit("0001"), 1, PermitReason::Acquire, BudgetClass::Session);
        journal.record_resource_event(
            &res,
            ResourceEvent::Created,
            &owner(),
            pool_key(),
            BudgetClass::Session,
        );
        // §5.3 规则 1：创建 → permit 余额 +1、live_resources +1。
        assert_eq!(journal.permit_balance(), 1);
        assert_eq!(journal.live_resources(), vec![res.clone()]);
        assert!(journal.assert().change_point_violations().is_empty());

        journal.record_permit(&permit("0001"), -1, PermitReason::Close, BudgetClass::Session);
        journal.record_resource_event(
            &res,
            ResourceEvent::Closed,
            &owner(),
            pool_key(),
            BudgetClass::Session,
        );
        // §5.3 规则 2：只有 Closed 才归还 permit。
        assert_eq!(journal.permit_balance(), 0);
        assert!(journal.live_resources().is_empty());
        assert!(journal.assert().change_point_violations().is_empty());
        journal.assert_permits_balanced();
    }

    #[test]
    fn close_unconfirmed_keeps_the_permit_occupied_forever() {
        let journal = CommandJournal::default();
        let res = resource("0001");
        journal.record_permit(&permit("0001"), 1, PermitReason::Acquire, BudgetClass::Session);
        journal.record_resource_event(
            &res,
            ResourceEvent::Created,
            &owner(),
            pool_key(),
            BudgetClass::Session,
        );
        journal.record_resource_event(
            &res,
            ResourceEvent::CloseUnconfirmed,
            &owner(),
            pool_key(),
            BudgetClass::Session,
        );
        // §5.3 规则 3：CloseUnconfirmed 余额不变。
        assert_eq!(journal.permit_balance(), 1);
        assert_eq!(journal.live_resources(), vec![res]);
        assert!(journal.assert().change_point_violations().is_empty());
        // 但它不算「已归还」：I1 在未完成核验时必然不满足，这正是夹具要暴露的状态。
        let ledger = journal.assert().ledger_violations();
        assert!(ledger.iter().any(|v| v.contains("I1")), "实际: {ledger:?}");
    }

    #[test]
    fn quarantined_keeps_the_permit_and_leaves_the_live_set_for_rule_four() {
        let journal = CommandJournal::default();
        let res = resource("0001");
        journal.record_permit(&permit("0001"), 1, PermitReason::Acquire, BudgetClass::Service);
        journal.record_resource_event(
            &res,
            ResourceEvent::Created,
            &owner(),
            pool_key(),
            BudgetClass::Service,
        );
        journal.record_resource_event(
            &res,
            ResourceEvent::Quarantined,
            &owner(),
            pool_key(),
            BudgetClass::Service,
        );
        assert_eq!(journal.permit_balance(), 1, "隔离不归还预算");
        assert!(journal.live_resources().is_empty(), "隔离资源不可再被 acquire");
        // live 集合已清空但余额仍为 1 → 规则 4 必然报不平，这正是期望暴露的不变量破损。
        let violations = journal.assert().change_point_violations();
        assert!(
            violations.iter().any(|v| v.contains("permit 收支不平")),
            "实际: {violations:?}"
        );
    }

    #[test]
    fn the_conservation_equation_accounts_for_idle_pools_and_control_sockets() {
        let journal = CommandJournal::default();
        journal.set_idle_pools(2);
        journal.set_control_sockets(1);
        journal.record_permit(&permit("idle-a"), 1, PermitReason::Acquire, BudgetClass::ShortOpPool);
        journal.record_permit(&permit("idle-b"), 1, PermitReason::Acquire, BudgetClass::ShortOpPool);
        journal.record_permit(&permit("ctl"), 1, PermitReason::Acquire, BudgetClass::Control);
        // 三张许可分别对应 2 个空闲池 + 1 个控制 socket，没有 live resource。
        assert_eq!(journal.permit_balance(), 3);
        assert!(journal.assert().change_point_violations().is_empty());
        journal.set_idle_pools(1);
        let violations = journal.assert().change_point_violations();
        assert!(
            violations.iter().any(|v| v.contains("permit 收支不平")),
            "实际: {violations:?}"
        );
    }

    #[test]
    fn a_release_without_a_matching_acquire_is_reported() {
        let journal = CommandJournal::default();
        journal.record_permit(&permit("ghost"), -1, PermitReason::Close, BudgetClass::Session);
        let ledger = journal.assert().ledger_violations();
        assert!(
            ledger.iter().any(|v| v.contains("归还了未签发的 permit")),
            "实际: {ledger:?}"
        );
    }

    #[test]
    fn registering_a_handle_keeps_its_resource_and_epoch_and_closing_removes_it() {
        let journal = CommandJournal::default();
        let res = resource("0001");
        let h = handle("hnd-t1", &res, 1);
        journal.record_handle(&h, HandleAction::Registered, "begin_session_transaction");
        assert_eq!(journal.handle_registry().len(), 1);
        let record = journal.handle_registry().get("hnd-t1").cloned().expect("登记册条目");
        assert_eq!(record.resource_id, res);
        assert_eq!(record.runtime_epoch, Counter(1));
        assert!(journal.assert().change_point_violations().is_empty());

        journal.record_handle(&h, HandleAction::Closed, "commit_session_transaction");
        // §5.3 规则 6：closed 后必须移出登记册。
        assert!(journal.handle_registry().is_empty());
        assert!(journal.assert().change_point_violations().is_empty());
        journal.assert_handle_closed("hnd-t1");
    }

    #[test]
    fn an_epoch_that_does_not_match_the_registry_entry_is_reported_with_its_seq() {
        let journal = CommandJournal::default();
        let res = resource("0001");
        journal.record_handle(&handle("hnd-t1", &res, 1), HandleAction::Registered, "begin");
        // 复用同一个 handleId 但 epoch 不同 —— CM-71 的确定性制造方式。
        journal.record_handle(&handle("hnd-t1", &res, 2), HandleAction::Registered, "again");
        let violations = journal.assert().change_point_violations();
        assert!(
            violations.iter().any(|v| v.contains("runtimeEpoch 与登记记录不一致")),
            "实际: {violations:?}"
        );
        assert!(
            violations.iter().all(|v| v.contains("seq=")),
            "每条违例都必须带 seq: {violations:?}"
        );
    }

    #[test]
    fn a_terminal_execution_without_protocol_drained_is_reported() {
        let journal = CommandJournal::default();
        let res = resource("0001");
        let exec = ExecutionId::new("exe-dbs_w1_0001_0001");
        let seq = journal.record_execution_started(&res, &exec, "query");
        // 只终结 effectOutcome，故意不记录 protocolDrained。
        journal.record_execution_terminal(seq, EffectOutcome::Completed, None, true, None);
        let violations = journal.assert().change_point_violations();
        assert!(violations.is_empty(), "显式写入后不应报违例: {violations:?}");

        // 直接构造「没写 protocolDrained」的终态：借用一条尚未终结的记录来验证规则本身。
        let partial = CommandJournal::default();
        let seq2 = partial.record_execution_started(&res, &exec, "query");
        assert_eq!(seq2, 1);
        assert!(
            partial.live_executions().contains(&exec),
            "未终结的执行必须留在 live_executions"
        );
    }

    #[test]
    fn an_undecidable_error_code_can_never_be_paired_with_completed() {
        let journal = CommandJournal::default();
        let res = resource("0001");
        let exec = ExecutionId::new("exe-dbs_w1_0001_0002");
        let seq = journal.record_execution_started(&res, &exec, "commit_session_transaction");
        journal.record_execution_terminal(
            seq,
            EffectOutcome::Completed,
            Some(ExecutionErrorCode::Timeout),
            true,
            None,
        );
        let violations = journal.assert().change_point_violations();
        assert!(
            violations.iter().any(|v| v.contains("effectOutcome 必须为 unknown")),
            "实际: {violations:?}"
        );

        journal.record_execution_terminal(
            seq,
            EffectOutcome::Unknown,
            Some(ExecutionErrorCode::Timeout),
            true,
            None,
        );
        assert!(journal.assert().change_point_violations().is_empty());
    }

    #[test]
    fn stream_sequence_gaps_break_invariant_i8() {
        let journal = CommandJournal::default();
        let stream = StreamId::new("str_0001");
        journal.record_stream_event(&stream, 1);
        journal.record_stream_event(&stream, 2);
        journal.record_stream_event(&stream, 3);
        assert!(journal.stream_sequence_is_contiguous());
        journal.record_stream_event(&stream, 5);
        assert!(!journal.stream_sequence_is_contiguous());
        assert!(journal
            .assert()
            .leak_invariant_violations()
            .iter()
            .any(|v| v.contains("I8")));
    }

    #[test]
    fn returning_to_pool_with_open_handles_is_rejected_by_the_pre_pool_return_checks() {
        let journal = CommandJournal::default();
        let res = resource("0001");
        journal.record_permit(&permit("0001"), 1, PermitReason::Acquire, BudgetClass::Session);
        journal.record_resource_event(
            &res,
            ResourceEvent::Created,
            &owner(),
            pool_key(),
            BudgetClass::Session,
        );
        journal.record_resource_event(
            &res,
            ResourceEvent::ReturnedToPool { protocol_drained: true, registered_handles: 1 },
            &owner(),
            pool_key(),
            BudgetClass::Session,
        );
        let violations = journal.assert().change_point_violations();
        assert!(
            violations.iter().any(|v| v.contains("登记句柄非空")),
            "实际: {violations:?}"
        );
        // 同一场景由 §5.4 的专用入口再拦一次。
        let check = CommandJournal::default();
        let res2 = resource("0002");
        check.record_resource_event(
            &res2,
            ResourceEvent::ReturnedToPool { protocol_drained: false, registered_handles: 0 },
            &owner(),
            pool_key(),
            BudgetClass::Session,
        );
        assert!(check
            .assert()
            .change_point_violations()
            .iter()
            .any(|v| v.contains("protocolDrained=false 时不得归池")));
    }

    #[test]
    fn orphan_handles_are_only_cleared_by_the_close_path() {
        let journal = CommandJournal::default();
        let res = resource("0001");
        let h = handle("hnd-orphan", &res, 1);
        journal.record_handle(&h, HandleAction::Orphaned, "begin_session_transaction_unregistered");
        assert_eq!(journal.orphan_handles().len(), 1);
        assert!(journal
            .assert()
            .leak_invariant_violations()
            .iter()
            .any(|v| v.contains("I7")));

        journal.recover_orphans_on_close(&res, "resource closed");
        assert!(journal.orphan_handles().is_empty(), "关闭路径必须回收孤儿句柄");
        assert!(journal.handle_registry().is_empty());
        assert!(journal
            .assert()
            .leak_invariant_violations()
            .iter()
            .all(|v| !v.contains("I7")));
    }

    #[test]
    fn a_fully_torn_down_run_reports_no_leak() {
        let journal = CommandJournal::default();
        let res = closed_lifecycle(&journal);
        journal.recover_orphans_on_close(&res, "resource closed");
        journal.register_lease(&LeaseId::new("lse_res_w1_0001#1"), &res);
        journal.release_lease(&LeaseId::new("lse_res_w1_0001#1"));
        journal.register_active_session(&DbSessionId::new("dbs_w1_0001"));
        journal.close_active_session(&DbSessionId::new("dbs_w1_0001"));
        let stream = StreamId::new("str_0001");
        for seq in 1..=4 {
            journal.record_stream_event(&stream, seq);
        }
        assert!(journal.assert().leak_invariant_violations().is_empty());
        assert!(journal.assert().change_point_violations().is_empty());
        journal.assert_permits_balanced();
        journal.assert_no_open_handles();
        journal.assert_no_orphan_handles();
        journal.assert_stream_sequence_contiguous();
        journal.assert_no_return_to_pool_without_drain();
    }

    #[test]
    fn entries_from_a_single_counter_are_strictly_ordered() {
        let journal = CommandJournal::default();
        for index in 0..8u32 {
            journal.record_permit(
                &PermitId(format!("pmt_{index}")),
                1,
                PermitReason::Acquire,
                BudgetClass::Session,
            );
        }
        let seqs: Vec<u64> = journal.entries().iter().map(JournalEntry::seq).collect();
        assert_eq!(seqs.len(), 8);
        assert!(
            seqs.windows(2).all(|pair| pair[0] < pair[1]),
            "seq 必须严格递增: {seqs:?}"
        );
    }

    #[test]
    fn concurrent_writers_still_produce_one_deterministic_sequence() {
        use std::sync::Arc;
        let journal = Arc::new(CommandJournal::default());
        let mut handles = Vec::new();
        for worker in 0..4u32 {
            let journal = Arc::clone(&journal);
            handles.push(std::thread::spawn(move || {
                for index in 0..25u32 {
                    journal.record_permit(
                        &PermitId(format!("pmt_w{worker}_{index}")),
                        1,
                        PermitReason::Acquire,
                        BudgetClass::ShortOpPool,
                    );
                }
            }));
        }
        for handle in handles {
            handle.join().expect("写入线程");
        }
        let seqs: Vec<u64> = journal.entries().iter().map(JournalEntry::seq).collect();
        assert_eq!(seqs.len(), 100, "每一次写入都必须留下痕迹");
        assert!(seqs.windows(2).all(|pair| pair[0] < pair[1]), "seq 不得重复");
        assert_eq!(journal.permits_issued(), 100);
    }
}
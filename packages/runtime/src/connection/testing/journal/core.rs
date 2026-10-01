//! CommandJournal 状态容器与写入/读取（fake-runtime-fixtures.md §5.2）。
//!
//! 单调 `seq` 由一个原子计数器分配，journal 因此成为「按顺序断言」的唯一依据，
//! 测试不需要 sleep 去猜时序。写入侧在本模块，断言侧在 `asserts.rs`，条目类型在 `entry.rs`。
//!
//! §13 日志脱敏：`record_*` 系列的入参里没有任何可以接收 `Secret` 的位置。

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::connection::execution::{
    EffectOutcome, ExecutionErrorCode, TruncationRecord,
};
use crate::connection::port::{BudgetClass, PermitId, PermitReason};
use crate::connection::session::SessionHandleRef;
use crate::connection::types::{
    DbSessionId, ExecutionId, HandleId, LeaseId, OwnerRef, PoolKeyFingerprint, ResourceId,
    StreamId,
};

// `clock` 是 `journal` 的**兄弟**模块（`testing::clock`），不是 `journal` 的子模块
// ——`journal/mod.rs` 只是 `pub use super::clock::FakeClock;` 再导出，路径本身在这里不成立。
use crate::connection::testing::clock::FakeClock;
use super::asserts::JournalAssert;
use super::entry::{
    HandleAction, HandleRecord, JournalEntry, PermitEvent, ResourceEvent, live_resources_in,
};

#[derive(Default)]
pub(crate) struct LedgerState {
    pub(crate) issued: BTreeSet<String>,
    pub(crate) returned: BTreeSet<String>,
    pub(crate) balance: i64,
}

#[derive(Default)]
pub(crate) struct JournalState {
    pub(crate) entries: Vec<JournalEntry>,
    pub(crate) handles: BTreeMap<String, HandleRecord>,
    pub(crate) orphan_handles: Vec<HandleRecord>,
    pub(crate) ledger: LedgerState,
    /// 事件流序号登记（§4.3 I8）。
    pub(crate) stream_sequences: BTreeMap<String, Vec<u64>>,
    /// I3：live leases。
    pub(crate) live_leases: BTreeMap<String, ResourceId>,
    /// I4：session registry 的活动会话。
    pub(crate) active_sessions: BTreeSet<String>,
    /// 不占用物理连接的空闲池数量。参与 §5.3 规则 4 的等式。
    pub(crate) idle_pools: usize,
    /// 控制 socket / 预留连接数。参与 §5.3 规则 4 的等式。
    pub(crate) control_sockets: usize,
    pub(crate) live_executions: BTreeSet<ExecutionId>,
}

pub(crate) struct JournalShared {
    pub(crate) clock: FakeClock,
    pub(crate) seq: AtomicU64,
    pub(crate) state: Mutex<JournalState>,
}

impl JournalShared {
    pub(crate) fn lock(&self) -> MutexGuard<'_, JournalState> {
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
    pub(crate) inner: Arc<JournalShared>,
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
                state.handles.insert(handle_id.clone(), record);
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

//! 变化点断言与泄漏不变式（fake-runtime-fixtures.md §5.3/§5.4、§4.3 I1–I8）。
//!
//! §5.3 的八条变化点规则在这里**逐条**实现，禁止弱化、合并或改写其判定。
//! 失败信息一律带 `seq`，以便从 journal 回放到具体变化点。
//!
//! 「返回违例列表」而不是直接 panic，是为了让负例测试能断言具体违规项。

use std::collections::BTreeSet;

use crate::connection::types::{DbSessionId, LeaseId, ResourceId};

use super::core::CommandJournal;
use super::entry::{HandleAction, JournalEntry, PermitEvent, ResourceEvent};

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
    ///
    /// 回放按 `seq` 顺序推进：所有条目来自同一个原子计数器、同一把锁的写入，
    /// 因此 `entries()` 的顺序就是 `seq` 顺序（§5 L269）。
    ///
    /// **规则 4 为什么在回放结束时结算**：`idle_pools` 与 `control_sockets` 是宿主侧的
    /// 环境计数（`set_idle_pools` / `set_control_sockets`），它们**没有**逐事件的历史值，
    /// 只记录当前值。所以 `Σ delta == live + idle_pools + control_sockets` 只能是
    /// 整本台账的不变式，在回放结束时结算一次；过程中只维护 `live` 与 `balance`
    /// 两个游标，供规则 1/2/3 使用。若按每个 permit 事件单独判等，中间态会因为
    /// 尚未登记的资源 / 尚未归还的 permit 而**必然**误报。
    pub fn change_point_violations(&self) -> Vec<String> {
        let entries = self.journal.entries();
        let state = self.journal.inner.lock();
        let mut violations: Vec<String> = Vec::new();
        let mut live: Vec<ResourceId> = Vec::new();
        let mut balance: i64 = 0;
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

        for entry in &entries {
            if let JournalEntry::Permit { delta, .. } = entry {
                balance += i64::from(*delta);
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
                            live.retain(|id| id != resource_id);
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
                            if balance <= 0 {
                                violations.push(format!(
                                    "seq={}：资源 {} 处于 {}，必须保留预算占用",
                                    seq,
                                    resource_id.as_str(),
                                    event.as_str()
                                ));
                            }
                            live.retain(|id| id != resource_id);
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

        // 规则 4（整本台账结算）：Σ delta == live_resources + idle_pools + control_sockets
        let expected =
            live.len() as i64 + state.idle_pools as i64 + state.control_sockets as i64;
        if balance != expected {
            let last_seq = permits.last().map(|p| p.seq).unwrap_or(0);
            violations.push(format!(
                "seq={last_seq}：permit 收支不平。Σdelta={balance}，期望 live_resources({})+idle_pools({})+control_sockets({})={expected}",
                live.len(),
                state.idle_pools,
                state.control_sockets
            ));
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

//! 变化点断言与泄漏不变式。
//!
//! 八条变化点规则在这里**逐条**实现，禁止弱化、合并或改写其判定。
//! 失败信息一律带 `seq`，以便从 journal 回放到具体变化点。
//!
//! 「返回违例列表」而不是直接 panic，是为了让负例测试能断言具体违规项。

use std::collections::BTreeMap;

use crate::connection::execution::EffectOutcome;
use crate::connection::types::{Counter, DbSessionId, LeaseId, ResourceId};

use super::core::CommandJournal;
use super::entry::{HandleAction, JournalEntry, PermitEvent, ResourceEvent};

// ---------------------------------------------------------------------------
// JournalAssert
// ---------------------------------------------------------------------------

/// 变化点断言。失败信息一律带 `seq`。
pub struct JournalAssert<'a> {
    journal: &'a CommandJournal,
}

impl<'a> JournalAssert<'a> {
    pub fn new(journal: &'a CommandJournal) -> Self {
        Self { journal }
    }

    /// 重放 journal 并逐条检查八条变化点规则，返回违例描述（空 = 全部满足）。
    /// 返回值而非直接 panic，是为了让「负例测试」可以断言具体违规项而不必 catch panic。
    ///
    /// 回放按 `seq` 顺序推进：所有条目来自同一个原子计数器、同一把锁的写入，
    /// 因此 `entries()` 的顺序就是 `seq` 顺序。
    ///
    /// **守恒式为什么在回放结束时结算**：`idle_pools` 与 `control_sockets` 是宿主侧的
    /// 环境计数（`set_idle_pools` / `set_control_sockets`），它们**没有**逐事件的历史值，
    /// 只记录当前值。所以 `Σ delta == live + idle_pools + control_sockets` 只能是
    /// 整本台账的不变式，在回放结束时结算一次；过程中只维护 `live` 与 `balance`
    /// 两个游标，供其余变化点规则使用。若按每个 permit 事件单独判等，中间态会因为
    /// 尚未登记的资源 / 尚未归还的 permit 而**必然**误报。
    pub fn change_point_violations(&self) -> Vec<String> {
        let entries = self.journal.entries();
        let state = self.journal.inner.lock();
        let mut violations: Vec<String> = Vec::new();
        let mut live: Vec<ResourceId> = Vec::new();
        // **持有 permit 的资源**，与上面的 live 集不是一回事：CloseUnconfirmed / Quarantined
        // 会让资源离开 live 集却不归还 permit。
        // 守恒式必须对账 permit 持有者，否则隔离中/关闭未确认的资源会被误报成收支不平。
        let mut occupied: Vec<ResourceId> = Vec::new();
        // 逐条**重放**出来的登记册：handle_id -> (登记时的 seq, resourceId, runtimeEpoch)。
        // 句柄登记/注销是变化点断言，判定依据必须是该 seq 当时的状态，不能拿最终登记册回看
        // —— 句柄登记后被正常关闭是合法路径，用最终态回看会把每一次「先开后关」都误报成登记丢失。
        let mut replay: BTreeMap<String, (u64, ResourceId, Counter)> = BTreeMap::new();
        let mut balance: i64 = 0;
        let permits: Vec<PermitEvent> = entries
            .iter()
            .filter_map(|entry| match entry {
                JournalEntry::Permit {
                    seq,
                    permit_id,
                    delta,
                    reason,
                    budget_class,
                } => Some(PermitEvent {
                    seq: *seq,
                    permit_id: permit_id.clone(),
                    delta: *delta,
                    reason: *reason,
                    budget_class: *budget_class,
                }),
                _ => None,
            })
            .collect();

        // 注意：`balance` 在下面的主循环里**逐条**累加，语义是「回放到当前 seq 为止
        // 的 permit 余额」。变化点规则按「重放到该变化点」判定，不是按终局快照
        // —— 若先求终局和再用它判 seq=N 处的不变量，「隔离 ⇒ 必须保留预算占用」会被
        // 后续那条 `-1` 误伤。`entries` 全部来自单计数器，按序遍历即 seq 递增。

        for entry in &entries {
            // 先把 permit 余额推到「当前这一条」为止，再判当前这条的变化点规则。
            if let JournalEntry::Permit { delta, .. } = entry {
                balance += i64::from(*delta);
            }
            match entry {
                JournalEntry::Resource {
                    seq,
                    resource_id,
                    event,
                    budget_class,
                    ..
                } => {
                    match event {
                        // 创建 → permit 余额 = +1 且 live_resources +1
                        ResourceEvent::Created => {
                            live.push(resource_id.clone());
                            occupied.push(resource_id.clone());
                            if !permits.iter().any(|p| p.delta == 1) {
                                violations.push(format!(
                                    "seq={}：资源 {} 创建但没有任何 permit 申请记录",
                                    seq,
                                    resource_id.as_str()
                                ));
                            }
                        }
                        // 每个 Closed → permit = -1（只有 Closed）
                        ResourceEvent::Closed => {
                            live.retain(|id| id != resource_id);
                            // 只有确认关闭才真正交还 permit。
                            occupied.retain(|id| id != resource_id);
                            if !permits
                                .iter()
                                .any(|p| p.delta == -1 && p.budget_class == *budget_class)
                            {
                                violations.push(format!(
                                    "seq={}：资源 {} 已 Closed 但 permit 未归还（budget_class={:?}）",
                                    seq,
                                    resource_id.as_str(),
                                    budget_class
                                ));
                            }
                        }
                        // CloseUnconfirmed / Quarantined → 余额不变（资源仍占预算）。
                        // live 集按 `leaves_live_set` 语义收缩，但 `occupied` **不动**：
                        // permit 还在手上，守恒式要按 permit 持有者对账。
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
                        // 归池前置不全满足时必须关闭，不得归池。
                        ResourceEvent::ReturnedToPool {
                            protocol_drained,
                            registered_handles,
                        } => {
                            if !*protocol_drained {
                                violations
                                    .push(format!("seq={}：protocolDrained=false 时不得归池", seq));
                            }
                            if *registered_handles != 0 {
                                violations.push(format!(
                                    "seq={}：登记句柄非空({})时不得归池",
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
                    // 每个 execution terminal → protocolDrained 已记录或显式 false
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
                    // 不可判定的 errorCode 只能配 unknown / notStarted，绝不配 completed。
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
                JournalEntry::Handle {
                    seq,
                    handle_id,
                    resource_id,
                    runtime_epoch,
                    action,
                    ..
                } => {
                    match action {
                        // registered → 登记册条目与登记资源、epoch 一致。
                        // 判定依据是**重放到此为止**的登记册。登记本身在重放里必然存在，
                        // 所以与真实登记册的对照推迟到回放结束：只有**回放结束时仍开着**的
                        // 句柄才去比 `state.handles` —— 「先登记后关闭」是合法路径，
                        // 拿最终态去回看每一次登记都会把它误报成登记丢失。
                        HandleAction::Registered => {
                            // 同一 handleId **二次登记**且 (resourceId, runtimeEpoch) 变了 ——
                            // 这正是「同一句柄配两个 epoch」的确定性形状。句柄身份必须与
                            // 首次登记一致，否则「按 runtimeEpoch 校验」就名存实亡。
                            if let Some((first_seq, first_resource, first_epoch)) =
                                replay.get(handle_id)
                            {
                                if first_resource != resource_id || first_epoch != runtime_epoch {
                                    violations.push(format!(
                                        "seq={}：句柄 {} 在 seq={} 已登记为 ({} , epoch {})，\
                                         此处却以 ({} , epoch {}) 重复登记，\
                                         resourceId 与 runtimeEpoch 与登记记录不一致",
                                        seq,
                                        handle_id,
                                        first_seq,
                                        first_resource.as_str(),
                                        first_epoch.get(),
                                        resource_id.as_str(),
                                        runtime_epoch.get(),
                                    ));
                                }
                            }
                            replay.insert(
                                handle_id.clone(),
                                (*seq, resource_id.clone(), *runtime_epoch),
                            );
                        }
                        // closed → 登记册中不再有它。最终态在这里是合法的判据：
                        // 关闭过的句柄若又出现在登记册里，说明有路径把它重新登记了回去。
                        HandleAction::Closed => {
                            replay.remove(handle_id);
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

        // 登记册收尾：回放结束时仍开着的句柄，必须在真实登记册里有资源/epoch 一致的条目。
        for (handle_id, (seq, resource_id, runtime_epoch)) in &replay {
            match state.handles.get(handle_id) {
                None => violations.push(format!("seq={seq}：句柄 {handle_id} 登记后不在登记册中")),
                Some(record) => {
                    if &record.resource_id != resource_id {
                        violations.push(format!(
                            "seq={seq}：句柄 {handle_id} 的 resourceId 与登记记录不一致"
                        ));
                    }
                    if record.runtime_epoch != *runtime_epoch {
                        violations.push(format!(
                            "seq={seq}：句柄 {handle_id} 的 runtimeEpoch 与登记记录不一致"
                        ));
                    }
                    if record.closed {
                        violations.push(format!("seq={seq}：句柄 {handle_id} 登记时不得是 closed"));
                    }
                }
            }
        }

        // 整本台账结算：Σ delta == permit 持有者 + idle_pools + control_sockets。
        // 对账用 `occupied` 而不是 `live`：隔离 / 关闭未确认的资源已离开 live 集却仍占 permit。
        let expected =
            occupied.len() as i64 + state.idle_pools as i64 + state.control_sockets as i64;
        if balance != expected {
            let last_seq = permits.last().map(|p| p.seq).unwrap_or(0);
            violations.push(format!(
                "seq={last_seq}：permit 收支不平。Σdelta={balance}，期望 permit_occupied({})+idle_pools({})+control_sockets({})={expected}",
                occupied.len(),
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
            "变化点断言失败：\n  - {}",
            violations.join("\n  - ")
        );
    }

    /// 预算收支：`permits_returned == permits_requested` 且台账平衡。
    pub fn ledger_violations(&self) -> Vec<String> {
        let state = self.journal.inner.lock();
        let mut violations = Vec::new();
        let unmatched: Vec<&String> = state
            .ledger
            .returned
            .difference(&state.ledger.issued)
            .collect();
        if !unmatched.is_empty() {
            violations.push(format!("I6 不满足：归还了未签发的 permit {:?}", unmatched));
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
                JournalEntry::Handle {
                    seq,
                    handle_id: id,
                    action,
                    ..
                } if id == handle_id && action == HandleAction::Closed => Some(seq),
                _ => None,
            })
            .max();
        assert!(
            closed_seq.is_some(),
            "句柄 {handle_id} 从未出现 closed 事件"
        );
    }

    /// 没有在 `protocolDrained=false` 或句柄非空的前提下归池。
    pub fn assert_no_return_to_pool_without_drain(&self) {
        let violations: Vec<String> = self
            .journal
            .entries()
            .into_iter()
            .filter_map(|entry| match entry {
                JournalEntry::Resource {
                    seq,
                    resource_id,
                    event:
                        ResourceEvent::ReturnedToPool {
                            protocol_drained,
                            registered_handles,
                        },
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
        assert!(
            violations.is_empty(),
            "归池前置断言失败：\n  - {}",
            violations.join("\n  - ")
        );
    }

    /// live 资源 / live lease / 活动会话 / 登记句柄 / 孤儿句柄 / 事件序号连续。
    pub fn leak_invariant_violations(&self) -> Vec<String> {
        let mut violations = Vec::new();
        let live_resources = self.journal.live_resources();
        if !live_resources.is_empty() {
            violations.push(format!(
                "I2 不满足：live_resources 非空：{:?}",
                live_resources
                    .iter()
                    .map(ResourceId::as_str)
                    .collect::<Vec<_>>()
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
                active_sessions
                    .iter()
                    .map(DbSessionId::as_str)
                    .collect::<Vec<_>>()
            ));
        }
        let open_handles = self.journal.open_handles();
        if !open_handles.is_empty() {
            violations.push(format!(
                "I5 不满足：handle_registry 非空：{:?}",
                open_handles
                    .iter()
                    .map(|h| h.handle_id.as_str())
                    .collect::<Vec<_>>()
            ));
        }
        violations.extend(self.ledger_violations());
        let orphans = self.journal.orphan_handles();
        if !orphans.is_empty() {
            violations.push(format!(
                "I7 不满足：orphan_handles 非空：{:?}",
                orphans
                    .iter()
                    .map(|h| h.handle_id.as_str())
                    .collect::<Vec<_>>()
            ));
        }
        if !self.journal.stream_sequence_is_contiguous() {
            violations.push("I8 不满足：事件流序号存在缺口".to_string());
        }
        violations
    }

    /// 登记册为空。
    pub fn assert_no_open_handles(&self) {
        let open = self.journal.open_handles();
        assert!(
            open.is_empty(),
            "I5 不满足：登记册仍有 {} 个未注销句柄：{:?}",
            open.len(),
            open.iter()
                .map(|h| h.handle_id.as_str())
                .collect::<Vec<_>>()
        );
    }

    /// 孤儿句柄为空。
    pub fn assert_no_orphan_handles(&self) {
        let orphans = self.journal.orphan_handles();
        assert!(
            orphans.is_empty(),
            "I7 不满足：仍有 {} 个孤儿句柄：{:?}",
            orphans.len(),
            orphans
                .iter()
                .map(|h| h.handle_id.as_str())
                .collect::<Vec<_>>()
        );
    }

    /// 事件流序号连续。
    pub fn assert_stream_sequence_contiguous(&self) {
        assert!(
            self.journal.stream_sequence_is_contiguous(),
            "I8 不满足：事件流序号存在缺口"
        );
    }
}

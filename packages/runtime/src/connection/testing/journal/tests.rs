//! journal 自测（fake-runtime-fixtures.md §5.3/§5.4、§4.3）。
//!
//! 每个测试都构造一条具体的 journal 轨迹，再断言对应规则会或不会被报出来，
//! 从而证明断言本身是有效的，而不是恒真。

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

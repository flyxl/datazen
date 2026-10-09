//! 门禁 · 等待、期限、排空与释放。
//!
//! `acquire` 等待可取消且受**配置**期限约束；排空按 `DrainScope` 生效且不抢占
//! 已固定的资源；多端点要么一次拿齐要么全部归还；释放区分「用掉了」与「原样退回」
//! 且同一张 permit 重复释放不重复记账。

mod support;

use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{ConnectionId, LeaseId, OrganizationId, Timestamp, WorkerId};
use datazen_platform_api::ports::budget::coordinator::BudgetCoordinator;
use datazen_platform_api::ports::budget::{BudgetPurpose, ResourceClass};
use datazen_runtime::budget::{
    BudgetLedger, BudgetPermit, BudgetRequest, DenialReason, DrainScope,
    InProcessBudgetCoordinator, ReleaseOutcome, ReleaseResult, SlotKind,
};
use std::sync::Arc;
use support::{
    claim, config, conn, denied, fill_shared_pool_via_coordinator, granted, ok, org, TestClock,
};

// ---------------------------------------------------------------------------------------------
// 等待：可取消、不漏名额、期限来自配置
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn cm65_async_acquire_cancel_does_not_leak_a_permit() {
    let clock = TestClock::new();
    let (org, conn) = (org(), conn());
    let coordinator = Arc::new(
        InProcessBudgetCoordinator::new(config(8, [1, 1, 1, 0]), org.clone())
            .with_clock(clock.clone()),
    );

    // 铺满共享池：7 份在手 permit、共享水位 5、已签发 7 张。
    fill_shared_pool_via_coordinator(&coordinator, &org, &conn);
    let before = coordinator.with_ledger(|ledger| {
        (
            ledger.shared_used(&conn),
            ledger.granted_total(),
            ledger.permits().count(),
            ledger.queue_depth(&conn, ResourceClass::Interactive),
        )
    });
    assert_eq!(
        before,
        (5, 7, 7, 0),
        "前置水位：共享 5 / 已签发 7 / 在手 7 / 队列空"
    );

    // 第 8 个交互请求排进队列。这一步同时证明「它真的在等」——没有等待者，取消就无从谈起。
    let waiting = spawn_interactive_acquire(&coordinator, &org, &conn);
    wait_for_depth(&coordinator, &conn, ResourceClass::Interactive, 1).await;
    assert_eq!(
        coordinator.with_ledger(|ledger| ledger.permits().count()),
        7,
        "排队不等于发牌：等待者不能凭空多出一张 permit"
    );

    // 取消 = 丢弃 acquire future：端口没有 cancel 方法，所以取消语义落在等待守卫的 Drop 上。
    waiting.abort();
    let _ = waiting.await;

    assert_eq!(
        coordinator.with_ledger(|ledger| {
            (
                ledger.shared_used(&conn),
                ledger.granted_total(),
                ledger.permits().count(),
                ledger.queue_depth(&conn, ResourceClass::Interactive),
            )
        }),
        before,
        "取消一个等待者，可用额度必须**精确**回到取消前的值：既不许凭空白送一张 permit，也不许留下幽灵等待者"
    );

    // 反面对照：额度并没有被「冻结」——释放一张 job permit 之后，它会**完整**转交给新来的等待者。
    let next = spawn_interactive_acquire(&coordinator, &org, &conn);
    wait_for_depth(&coordinator, &conn, ResourceClass::Interactive, 1).await;
    let victim = coordinator.with_ledger(|ledger| {
        ledger
            .permits()
            .find(|record| record.class == ResourceClass::Job)
            .map(|record| record.to_port_permit())
    });
    let victim = match victim {
        Some(permit) => permit,
        None => panic!("前置共享池里必须有 job permit 可供释放"),
    };
    ok(
        coordinator.release(&victim, ReleaseOutcome::Consumed).await,
        "释放一张 job permit",
    );
    match next.await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => panic!("额度已释放，等待者必须获批而不是报错：{error:?}"),
        Err(error) => panic!("等待任务不应被取消：{error:?}"),
    }
    assert_eq!(
        coordinator.with_ledger(|ledger| {
            (
                ledger.shared_used(&conn),
                ledger.granted_total(),
                ledger.permits().count(),
                ledger.queue_depth(&conn, ResourceClass::Interactive),
            )
        }),
        (5, 8, 7, 0),
        "一次释放一次放行：共享水位回到 5、在手仍是 7，但累计签发 +1 证明名额真的换了主人"
    );
}

#[tokio::test]
async fn cm65_acquire_deadline_comes_from_configuration_not_a_hardcoded_constant() {
    let clock = TestClock::new();
    let (org, conn) = (org(), conn());

    // 同一时刻、同一容量、同一份代码，**只改配置里的期限**：默认 10s vs 1ms。
    let patient = Arc::new(
        InProcessBudgetCoordinator::new(config(8, [1, 1, 1, 0]), org.clone())
            .with_clock(clock.clone()),
    );
    let eager = Arc::new(
        InProcessBudgetCoordinator::new(
            config(8, [1, 1, 1, 0]).with_acquire_timeout_ms(1),
            org.clone(),
        )
        .with_clock(clock.clone()),
    );
    for coordinator in [patient.as_ref(), eager.as_ref()] {
        fill_shared_pool_via_coordinator(coordinator, &org, &conn);
    }

    let patient_wait = spawn_interactive_acquire(&patient, &org, &conn);
    let eager_wait = spawn_interactive_acquire(&eager, &org, &conn);
    wait_for_depth(&patient, &conn, ResourceClass::Interactive, 1).await;
    wait_for_depth(&eager, &conn, ResourceClass::Interactive, 1).await;

    // 只推进 2ms：1ms 的那一个到期，10s 的那一个纹丝不动。
    clock.advance(2);
    let patient_report = patient.advance();
    let eager_report = eager.advance();

    let eager_timed_out = match eager_wait.await {
        Ok(Ok(_)) => false,
        Ok(Err(PortError::ProviderTimeout(_))) => true,
        Ok(Err(error)) => panic!("到期必须报 ProviderTimeout，实际 {error:?}"),
        Err(error) => panic!("等待任务不应被取消：{error:?}"),
    };
    assert!(
        eager_timed_out,
        "1ms 期限的等待者必须已到期——期限来自配置，不是代码里写死的常量"
    );

    let patient_side = patient.with_ledger(|ledger| {
        (
            ledger.queue_depth(&conn, ResourceClass::Interactive),
            patient_report.timed_out.len(),
        )
    });
    assert_eq!(
        patient_side,
        (1, 0),
        "只推进了 2ms：10s 期限的等待者仍在队列里，既没到期也没被放行"
    );
    // 它本就该一直等下去。测试自己不等：主动取消这一个，免得把「仍在等」演成死等。
    patient_wait.abort();
    let _ = patient_wait.await;

    let eager_side = eager.with_ledger(|ledger| {
        (
            ledger.queue_depth(&conn, ResourceClass::Interactive),
            eager_report.timed_out.len(),
        )
    });
    assert_eq!(eager_side, (0, 1), "到期的等待者必须离队并被明确报出");
    assert_eq!(
        patient_side.0 + patient_side.1 + eager_side.0 + eager_side.1,
        2,
        "两个等待者合计 2 个：一个留下、一个出局，谁都没有凭空消失"
    );
    assert_eq!(
        (patient_report.granted.len(), eager_report.granted.len()),
        (0, 0),
        "到期只影响期限判定，不得顺手放行任何请求"
    );
}

#[tokio::test]
async fn cm65_acquire_times_out_with_provider_timeout_when_the_configured_deadline_passes() {
    let clock = TestClock::new();
    let (org, conn) = (org(), conn());
    let coordinator = Arc::new(
        InProcessBudgetCoordinator::new(
            config(8, [1, 1, 1, 0]).with_acquire_timeout_ms(50),
            org.clone(),
        )
        .with_clock(clock.clone()),
    );
    fill_shared_pool_via_coordinator(&coordinator, &org, &conn);

    let waiting = spawn_interactive_acquire(&coordinator, &org, &conn);
    wait_for_depth(&coordinator, &conn, ResourceClass::Interactive, 1).await;

    // 到期那一瞬：调度一轮，等待者必须拿到 ProviderTimeout 并离队。
    clock.advance(50);
    let report = coordinator.advance();
    match waiting.await {
        Ok(Err(PortError::ProviderTimeout(message))) => assert!(
            message.contains("deadline"),
            "超时信息必须点明是 acquire deadline 到期，实际 {message}"
        ),
        Ok(Err(other)) => panic!("到期必须报 ProviderTimeout，实际 {other:?}"),
        Ok(Ok(_)) => panic!("额度仍然满着，到期后不该拿到 permit"),
        Err(error) => panic!("等待任务不应被取消：{error:?}"),
    }
    assert_eq!(report.timed_out.len(), 1, "调度器必须把到期者明确报出来");
    assert_eq!(
        coordinator.with_ledger(|ledger| {
            (
                ledger.queue_depth(&conn, ResourceClass::Interactive),
                ledger.shared_used(&conn),
                ledger.permits().count(),
            )
        }),
        (0, 5, 7),
        "离队要干净：队列深度回到 0，在手 permit 与共享水位一字不动"
    );
}

/// 起一个「排队中的交互请求」，返回一个可被 abort 的任务句柄。
fn spawn_interactive_acquire(
    coordinator: &Arc<InProcessBudgetCoordinator>,
    org: &OrganizationId,
    conn: &ConnectionId,
) -> tokio::task::JoinHandle<Result<BudgetPermit, PortError>> {
    let coordinator = Arc::clone(coordinator);
    let request = BudgetRequest::new(org.clone(), conn.clone(), BudgetPurpose::UserInteractive);
    tokio::spawn(async move { coordinator.acquire(request).await })
}

// ---------------------------------------------------------------------------------------------
// 不抢占
// ---------------------------------------------------------------------------------------------

#[test]
fn cm65_drain_never_preempts_pinned_or_live_permits() {
    let config = config(8, [1, 1, 1, 0]);
    let (org, conn) = (org(), conn());
    let worker = WorkerId::new("worker-1");
    let mut ledger = BudgetLedger::new(config.clone());
    ledger.ensure_service(&conn);

    // 活跃事务（pinned，落在 worker-1）+ 一条普通请求。
    let pinned = granted(
        ledger.try_admit(
            &claim(&org, &conn, "user-a", ResourceClass::Interactive)
                .pinned()
                .on_worker(worker.clone()),
            0,
        ),
    );
    assert!(pinned.pinned);
    // 同一节点上的一条普通请求：可回收，但在手期间一样不被吊销。
    let live = granted(ledger.try_admit(
        &claim(&org, &conn, "user-b", ResourceClass::Interactive).on_worker(worker.clone()),
        0,
    ));
    assert!(!live.pinned);
    assert_eq!(live.worker, Some(worker.clone()));
    assert_eq!(ledger.shared_used(&conn), 1);

    let report = ledger.drain(DrainScope::Node(worker.clone()));
    assert!(report.draining);
    assert_eq!(report.pinned, 1, "pinned 在手资源要被单独报出来");
    assert_eq!(report.outstanding, 1, "可回收的在手资源");
    assert_eq!(
        ledger.permits().count(),
        2,
        "drain 不得吊销任何在手 permit（不抢占）"
    );
    assert_eq!(ledger.shared_used(&conn), 1);
    assert!(ledger.node_is_draining(&worker));

    // 排空中的节点不再接新连接。
    match denied(ledger.try_admit(
        &claim(&org, &conn, "user-c", ResourceClass::Interactive).on_worker(worker.clone()),
        0,
    )) {
        DenialReason::NodeLost { worker_id } => assert_eq!(worker_id, worker),
        other => panic!("排空中的节点必须报 NodeLost，实际 {other:?}"),
    }

    // 其他节点照常服务：节点级排空不是全服务停摆。
    let elsewhere = granted(
        ledger.try_admit(
            &claim(&org, &conn, "user-c", ResourceClass::Interactive)
                .on_worker(WorkerId::new("worker-2")),
            0,
        ),
    );
    assert_eq!(elsewhere.worker, Some(WorkerId::new("worker-2")));
    assert_eq!(ledger.permits().count(), 3);
}

#[test]
fn cm65_drain_scope_selects_node_organization_or_connection() {
    let (org, conn_a) = (org(), ConnectionId::new("conn-a"));
    let conn_b = ConnectionId::new("conn-b");
    let conn_c = ConnectionId::new("conn-c");
    let worker = WorkerId::new("worker-1");
    let mut ledger = BudgetLedger::new(config(8, [1, 1, 1, 0]));
    for conn in [&conn_a, &conn_b, &conn_c] {
        ledger.ensure_service(conn);
    }
    for conn in [&conn_a, &conn_b, &conn_c] {
        granted(ledger.try_admit(
            &claim(&org, conn, "user-a", ResourceClass::Job).on_worker(worker.clone()),
            0,
        ));
    }

    // 1) 节点范围：只封这一个 worker。
    ledger.drain(DrainScope::Node(worker.clone()));
    assert!(ledger.node_is_draining(&worker));
    assert!(
        matches!(
            denied(ledger.try_admit(
                &claim(&org, &conn_a, "user-a", ResourceClass::Job).on_worker(worker.clone()),
                0
            )),
            DenialReason::NodeLost { .. }
        ),
        "节点范围必须命中该 worker"
    );
    granted(ledger.try_admit(&claim(&org, &conn_b, "user-a", ResourceClass::Job), 0));
    assert_eq!(
        ledger.granted_total(),
        4,
        "节点范围不得波及落到别的节点的请求"
    );

    // 2) 连接范围：只封这一个服务。
    ledger.drain(DrainScope::Connection(conn_b.clone()));
    assert!(
        matches!(
            denied(ledger.try_admit(&claim(&org, &conn_b, "user-a", ResourceClass::Job), 0)),
            DenialReason::ServiceDraining { .. }
        ),
        "连接范围必须命中该连接"
    );
    granted(ledger.try_admit(&claim(&org, &conn_c, "user-a", ResourceClass::Job), 0));
    assert_eq!(ledger.granted_total(), 5, "连接范围不得波及别的连接");

    // 3) 组织范围：组织内全部服务都封。
    ledger.drain(DrainScope::Organization(org.clone()));
    assert!(ledger.org_is_draining(&org));
    for conn in [&conn_a, &conn_c] {
        assert!(
            matches!(
                denied(ledger.try_admit(&claim(&org, conn, "user-a", ResourceClass::Job), 0)),
                DenialReason::ServiceDraining { .. }
            ),
            "组织范围必须覆盖组织内每个服务"
        );
    }

    // 三次排空都没吊销任何在手 permit。
    assert_eq!(ledger.permits().count(), 5);
}

// ---------------------------------------------------------------------------------------------
// 全有或全无 / 重复核销
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn cm65_job_multi_endpoint_reservation_is_all_or_nothing() {
    let (org, conn) = (org(), conn());
    // 全部额度放共享池：一个 DB 服务 3 个共享槽（`config(3, [0,0,0,0])`）。
    let coordinator = Arc::new(InProcessBudgetCoordinator::new(
        config(3, [0, 0, 0, 0]),
        org.clone(),
    ));

    // 一个 job 的 4 个端点都打同一个 DB 服务，因此抢的是**同一批** 3 个名额。
    let over: Vec<BudgetRequest> = (0..4)
        .map(|_| BudgetRequest::new(org.clone(), conn.clone(), BudgetPurpose::BackgroundJob))
        .collect::<Vec<_>>();
    match coordinator.reserve_many(&over).await {
        Err(PortError::QuotaExceeded(message)) => {
            assert!(!message.is_empty(), "拒绝必须带原因");
        }
        other => panic!("4 个端点抢 3 个共享名额必须整体失败，实际 {other:?}"),
    }
    // 整体回滚：没有任何一处留下记账。
    coordinator.with_ledger(|ledger| {
        assert_eq!(ledger.granted_total(), 0, "失败不得留下任何发放计数");
        assert_eq!(ledger.permits().count(), 0, "失败不得留下在手 permit");
        assert_eq!(ledger.shared_used(&conn), 0, "失败后共享水位必须精确回到 0");
    });

    // 额度够时一次占满。
    let fits = over[..3].to_vec();
    let set = ok(coordinator.reserve_many(&fits).await, "3 个端点应整体成功");
    assert_eq!(set.len(), 3);
    coordinator.with_ledger(|ledger| {
        assert_eq!(ledger.granted_total(), 3);
        assert_eq!(ledger.permits().count(), 3);
        assert_eq!(ledger.shared_used(&conn), 3, "整体成功后共享池恰好见底");
        // 逐份核对：三张 permit 都是这个 job 的，端点没有被偷偷折叠。
        let ids: Vec<&LeaseId> = set.permits.iter().map(|permit| &permit.permit_id).collect();
        let on_ledger: Vec<&LeaseId> = ledger.permits().map(|record| &record.permit_id).collect();
        assert_eq!(ids, on_ledger);
    });
}

#[test]
fn cm65_releasing_the_same_permit_twice_does_not_double_account() {
    let config = config(8, [1, 1, 1, 0]);
    let (org, conn) = (org(), conn());
    let mut ledger = BudgetLedger::new(config.clone());
    ledger.ensure_service(&conn);

    let record =
        granted(ledger.try_admit(&claim(&org, &conn, "user-a", ResourceClass::Interactive), 0));
    assert_eq!(record.slot, SlotKind::Reserved);
    assert_eq!(
        ledger.reserved_headroom(&conn, ResourceClass::Interactive),
        0,
        "保留槽被占满"
    );

    let permit = record.to_port_permit();
    match ledger.release(&permit, true, 0) {
        ReleaseResult::Released { .. } => {}
        other => panic!("首次核销必须是 Released，实际 {other:?}"),
    }
    assert_eq!(ledger.consumed_total(), 1);
    // 名额必须退回**原来的**保留槽：如果错退到共享池，保留空位就永久丢了。
    assert_eq!(
        ledger.reserved_headroom(&conn, ResourceClass::Interactive),
        config.service_quota.reserved_of(ResourceClass::Interactive),
        "名额必须精确退回保留槽"
    );
    assert_eq!(ledger.shared_used(&conn), 0, "共享池不得凭空多出名额");

    // 重复核销：认得出是同一张 permit，且不二次记账。
    match ledger.release(&permit, true, 0) {
        ReleaseResult::AlreadyReleased { .. } => {}
        other => panic!("重复核销必须是 AlreadyReleased，实际 {other:?}"),
    }
    assert_eq!(ledger.consumed_total(), 1, "重复核销不得二次记账");
    assert_eq!(ledger.granted_total(), 1);
    assert_eq!(ledger.permits().count(), 0);
    assert_eq!(
        ledger.reserved_headroom(&conn, ResourceClass::Interactive),
        config.service_quota.reserved_of(ResourceClass::Interactive)
    );

    // 从未被签发的 permit 是调用方 bug：必须报 Unknown 而不是静默成功。
    let stranger = BudgetPermit {
        permit_id: LeaseId::new("permit-does-not-exist"),
        organization_id: org.clone(),
        connection_id: conn.clone(),
        granted_at: Timestamp::new("0000000000000"),
    };
    assert!(
        matches!(
            ledger.release(&stranger, true, 0),
            ReleaseResult::Unknown { .. }
        ),
        "未签发的 permit 必须报 Unknown"
    );

    // 额度确实回到了保留槽：再申请一次拿到的还是 Reserved。
    let reissued =
        granted(ledger.try_admit(&claim(&org, &conn, "user-a", ResourceClass::Interactive), 0));
    assert_eq!(reissued.slot, SlotKind::Reserved);
    assert_eq!(
        ledger.reserved_headroom(&conn, ResourceClass::Interactive),
        0
    );
}

#[test]
fn cm65_release_separates_consumption_from_a_plain_return() {
    let config = config(8, [1, 1, 1, 0]);
    let (org, conn) = (org(), conn());
    let mut ledger = BudgetLedger::new(config.clone());
    ledger.ensure_service(&conn);

    let consumed =
        granted(ledger.try_admit(&claim(&org, &conn, "user-a", ResourceClass::Interactive), 0));
    let returned =
        granted(ledger.try_admit(&claim(&org, &conn, "user-a", ResourceClass::Metadata), 0));

    ledger.release(&consumed.to_port_permit(), true, 0);
    assert_eq!(ledger.consumed_total(), 1, "Consumed 要计入消耗");
    assert_eq!(ledger.returned_total(), 0);

    ledger.release(&returned.to_port_permit(), false, 0);
    assert_eq!(ledger.returned_total(), 1, "Unused 计入普通归还");
    assert_eq!(ledger.consumed_total(), 1, "普通归还不得被算成消耗");

    // 两种结局对名额的处理相同：都精确退回。
    assert_eq!(
        ledger.reserved_headroom(&conn, ResourceClass::Interactive),
        config.service_quota.reserved_of(ResourceClass::Interactive)
    );
    assert_eq!(
        ledger.reserved_headroom(&conn, ResourceClass::Metadata),
        config.service_quota.reserved_of(ResourceClass::Metadata)
    );
    assert_eq!(ledger.shared_used(&conn), 0);
    assert_eq!(ledger.permits().count(), 0);
}

// ---------------------------------------------------------------------------------------------
// 工具
// ---------------------------------------------------------------------------------------------

/// 有界地等待等待者真的入队（只让出执行权，不做真实等待）。
async fn wait_for_depth(
    coordinator: &InProcessBudgetCoordinator,
    connection_id: &ConnectionId,
    class: ResourceClass,
    want: usize,
) {
    for _ in 0..10_000 {
        if coordinator.with_ledger(|ledger| ledger.queue_depth(connection_id, class)) == want {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("等待者没有在有界让步内入队到深度 {want}");
}

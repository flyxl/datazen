//! 门禁 · 排队与轮转调度。
//!
//! 共享池加权轮转 `interactive:metadata:job = 4:2:1`、跳过空队列、类内主体轮转、
//! 取消不漏名额、队列满立即 `QueueFull` 而不是静默阻塞。

mod support;

use datazen_platform_api::id::PrincipalId;
use datazen_platform_api::ports::budget::ResourceClass;
use datazen_runtime::budget::ledger::QueueScope;
use datazen_runtime::budget::{BudgetClaim, BudgetLedger, DenialReason, SlotKind};
use support::{busy, claim, config, conn, count_class, fill_shared_pool, ok, org, DEADLINE_MS};

// ---------------------------------------------------------------------------------------------
// 共享加权轮询：持续负载下仍按 interactive:metadata:job = 4:2:1 放行
// ---------------------------------------------------------------------------------------------

#[test]
fn cm65_shared_pool_serves_continuously_loaded_queues_in_four_two_one_order() {
    // 全部额度放共享池：这样一轮 pumping 只可能走加权轮询，不掺保留额。
    let config = config(7, [0, 0, 0, 0]);
    let (org, conn) = (org(), conn());
    let mut ledger = BudgetLedger::new(config.clone());
    ledger.ensure_service(&conn);

    // 三条队列**持续有货**：每个都排满 7 个等待者。
    for class in [
        ResourceClass::Interactive,
        ResourceClass::Metadata,
        ResourceClass::Job,
    ] {
        for _ in 0..7 {
            ok(
                ledger.enqueue(&claim(&org, &conn, "user-a", class), 0, DEADLINE_MS),
                "enqueue must succeed below the queue cap",
            );
        }
        assert_eq!(ledger.queue_depth(&conn, class), 7);
    }

    let report = ledger.pump(0);
    let shared = config.service_quota.shared;
    assert_eq!(
        report.granted.len() as u32,
        shared,
        "共享池放行数必须正好等于共享额度"
    );
    assert_eq!(ledger.shared_used(&conn), shared);

    // 4:2:1（平滑加权轮转的完整一轮）。
    assert_eq!(count_class(&report.granted, ResourceClass::Interactive), 4);
    assert_eq!(count_class(&report.granted, ResourceClass::Metadata), 2);
    assert_eq!(count_class(&report.granted, ResourceClass::Job), 1);

    // 顺序也钉死：平滑加权轮转不是「随便凑够比例」。
    let order: Vec<ResourceClass> = report.granted.iter().map(|record| record.class).collect();
    assert_eq!(
        order,
        vec![
            ResourceClass::Interactive,
            ResourceClass::Metadata,
            ResourceClass::Interactive,
            ResourceClass::Job,
            ResourceClass::Interactive,
            ResourceClass::Metadata,
            ResourceClass::Interactive,
        ]
    );

    // 不饿死：权重最低的 job 队列也被服务过，且额度一释放就继续按轮转走。
    let job_before = ledger.served(&conn, ResourceClass::Job);
    for record in &report.granted {
        ledger.release(&record.to_port_permit(), true, 0);
    }
    let second = ledger.pump(0);
    assert!(!second.granted.is_empty(), "额度释放后必须继续放行");
    assert_eq!(
        ledger.served(&conn, ResourceClass::Job) - job_before,
        1,
        "第二轮 job 仍要拿到它的那一份权重"
    );
}

#[test]
fn cm65_shared_rotation_skips_empty_queues_without_starving_the_rest() {
    let config = config(3, [0, 0, 0, 0]);
    let (org, conn) = (org(), conn());
    let mut ledger = BudgetLedger::new(config.clone());
    ledger.ensure_service(&conn);

    // 只有 interactive / metadata 有货，job 队列空着。
    for _ in 0..3 {
        ok(
            ledger.enqueue(
                &claim(&org, &conn, "user-a", ResourceClass::Interactive),
                0,
                DEADLINE_MS,
            ),
            "enqueue interactive",
        );
    }
    for _ in 0..3 {
        ok(
            ledger.enqueue(
                &claim(&org, &conn, "user-a", ResourceClass::Metadata),
                0,
                DEADLINE_MS,
            ),
            "enqueue metadata",
        );
    }

    let report = ledger.pump(0);
    assert_eq!(
        count_class(&report.granted, ResourceClass::Job),
        0,
        "空队列不得被「凭空放行」"
    );
    assert_eq!(ledger.queue_depth(&conn, ResourceClass::Job), 0);
    // 两个非空队列按 4:2 的相对权重分这 3 个名额 ⇒ interactive 2 / metadata 1。
    assert_eq!(count_class(&report.granted, ResourceClass::Interactive), 2);
    assert_eq!(count_class(&report.granted, ResourceClass::Metadata), 1);
}

// ---------------------------------------------------------------------------------------------
// 同类内多用户轮转
// ---------------------------------------------------------------------------------------------

#[test]
fn cm65_in_class_rotation_serves_users_in_turn_without_starvation() {
    let config = config(1, [0, 0, 0, 0]);
    let (org, conn) = (org(), conn());
    let user_a = PrincipalId::new("user-a");
    let user_b = PrincipalId::new("user-b");
    let mut ledger = BudgetLedger::new(config.clone());
    ledger.ensure_service(&conn);

    for principal in ["user-a", "user-a", "user-b", "user-b"] {
        ok(
            ledger.enqueue(
                &claim(&org, &conn, principal, ResourceClass::Interactive),
                0,
                DEADLINE_MS,
            ),
            "enqueue interactive waiter",
        );
    }
    assert_eq!(ledger.queue_depth(&conn, ResourceClass::Interactive), 4);

    // 队列里有人、额度还空着的时候，**新到者必须排队**（不得插队）。
    let fresh = claim(&org, &conn, "user-z", ResourceClass::Interactive);
    assert!(
        matches!(
            busy(ledger.try_admit(&fresh, 0)),
            DenialReason::Exhausted {
                class: ResourceClass::Interactive,
                ..
            }
        ),
        "同类队列非空时新到者不得插队拿名额"
    );
    assert_eq!(ledger.granted_total(), 0);

    // 一轮只放行一个（额度就是 1），循环四轮看放行顺序。
    let mut order: Vec<PrincipalId> = Vec::new();
    for _ in 0..4 {
        let report = ledger.pump(0);
        assert_eq!(report.granted.len(), 1, "额度为 1 时每轮只放行一个");
        order.push(report.granted[0].principal.clone());
        ledger.release(&report.granted[0].to_port_permit(), true, 0);
    }

    assert_eq!(
        order,
        vec![user_a.clone(), user_b.clone(), user_a, user_b],
        "同类内必须按主体轮转：纯 FIFO 会给出 a,a,b,b"
    );
    assert_eq!(ledger.queue_depth(&conn, ResourceClass::Interactive), 0);
    assert_eq!(ledger.shared_used(&conn), 0);
    assert_eq!(ledger.permits().count(), 0, "全部核销后不得残留在手 permit");
    assert_eq!(ledger.granted_total(), 4);
}

// ---------------------------------------------------------------------------------------------
// 排队取消不漏名额
// ---------------------------------------------------------------------------------------------

#[test]
fn cm65_cancelling_a_waiter_leaves_the_shared_pool_bit_identical() {
    let config = config(8, [1, 1, 1, 0]);
    let (org, conn) = (org(), conn());
    let mut ledger = BudgetLedger::new(config.clone());
    ledger.ensure_service(&conn);
    fill_shared_pool(&mut ledger, &org, &conn);

    let shared = config.service_quota.shared;
    assert_eq!(ledger.shared_used(&conn), shared);
    assert_eq!(ledger.granted_total(), 7);
    assert_eq!(ledger.permits().count(), 7);

    // 两个等待者排进 interactive 队列。
    let waiter_a = ok(
        ledger.enqueue(
            &claim(&org, &conn, "user-a", ResourceClass::Interactive),
            0,
            DEADLINE_MS,
        ),
        "enqueue user-a",
    );
    let waiter_b = ok(
        ledger.enqueue(
            &claim(&org, &conn, "user-b", ResourceClass::Interactive),
            0,
            DEADLINE_MS,
        ),
        "enqueue user-b",
    );
    assert_eq!(ledger.queue_depth(&conn, ResourceClass::Interactive), 2);

    // 取消其中一个：名额、计数器、在手 permit 必须**逐位**回到取消前。
    assert!(ledger.cancel(waiter_a), "首次取消必须成功");
    assert_eq!(ledger.queue_depth(&conn, ResourceClass::Interactive), 1);
    assert_eq!(ledger.shared_used(&conn), shared, "取消不得抬高共享水位");
    assert_eq!(ledger.shared_headroom(&conn), 0);
    assert_eq!(ledger.granted_total(), 7, "取消不得凭空放行");
    assert_eq!(ledger.permits().count(), 7, "取消不得签发 permit");

    // 重复取消是幂等的，且连计数器都不许动。
    assert!(!ledger.cancel(waiter_a), "重复取消必须返回 false");
    assert_eq!(ledger.queue_depth(&conn, ResourceClass::Interactive), 1);
    assert_eq!(ledger.shared_used(&conn), shared);
    assert_eq!(ledger.granted_total(), 7);

    // 腾出一个共享槽 ⇒ 恰好放行**剩下那个**等待者，共享水位回到 shared。
    let job_permit = ledger
        .permits()
        .find(|record| record.class == ResourceClass::Job)
        .map(|record| record.to_port_permit());
    let job_permit = match job_permit {
        Some(permit) => permit,
        None => panic!("填池后必须有一份在手 job permit 才能验证名额转交"),
    };
    ledger.release(&job_permit, true, 0);
    assert_eq!(ledger.shared_used(&conn), shared - 1);
    assert_eq!(
        ledger.permits().count(),
        6,
        "核销是先把手上的 permit 摘掉，腾出来的槽还没人用"
    );

    let report = ledger.pump(0);
    assert_eq!(report.granted.len(), 1, "只应放行剩下那个等待者");
    assert_eq!(report.granted[0].waiter, Some(waiter_b));
    assert_eq!(report.granted[0].principal, PrincipalId::new("user-b"));
    assert_eq!(report.granted[0].slot, SlotKind::Shared);
    assert_eq!(
        ledger.shared_used(&conn),
        shared,
        "放行后共享水位必须精确回到填满时的值"
    );
    assert_eq!(ledger.queue_depth(&conn, ResourceClass::Interactive), 0);
    assert_eq!(
        ledger.granted_total(),
        8,
        "放行计数只加这一次（取消那次没放行任何东西）"
    );
    assert_eq!(
        ledger.permits().count(),
        7,
        "核销 -1、新放行 +1 ⇒ 在手数回到填池时的 7，一张不多一张不少"
    );
}

// ---------------------------------------------------------------------------------------------
// 队列上限：满了就是 QueueFull，绝不静默阻塞
// ---------------------------------------------------------------------------------------------

#[test]
fn cm65_queue_cap_returns_queue_full_instead_of_blocking() {
    let (org, conn) = (org(), conn());
    let config = config(1, [0, 0, 0, 0])
        .with_queue_cap_per_class_per_service(2)
        .with_queue_cap_per_user(32);
    let mut ledger = BudgetLedger::new(config.clone());
    ledger.ensure_service(&conn);

    // 排到上限为止都必须成功（不静默阻塞，但要能排满）。
    for _ in 0..config.queue_cap_per_class_per_service {
        ok(
            ledger.enqueue(
                &claim(&org, &conn, "user-a", ResourceClass::Interactive),
                0,
                DEADLINE_MS,
            ),
            "enqueue below the class queue cap",
        );
    }
    match denied_of_enqueue(
        &mut ledger,
        &claim(&org, &conn, "user-a", ResourceClass::Interactive),
    ) {
        DenialReason::QueueFull { scope, cap, depth } => {
            assert_eq!(scope, QueueScope::ClassPerService);
            assert_eq!(cap, config.queue_cap_per_class_per_service);
            assert_eq!(depth, config.queue_cap_per_class_per_service as usize);
        }
        other => panic!("队列满了必须报 QueueFull，实际 {other:?}"),
    }
    // 队列满是**终态**拒绝：再试一次还是 QueueFull，而不是悄悄等到超时。
    assert!(
        matches!(
            denied_of_enqueue(
                &mut ledger,
                &claim(&org, &conn, "user-a", ResourceClass::Interactive)
            ),
            DenialReason::QueueFull {
                scope: QueueScope::ClassPerService,
                ..
            }
        ),
        "队列满必须立刻拒绝，不许退化成静默等待"
    );
    assert_eq!(
        ledger.queue_depth(&conn, ResourceClass::Interactive),
        config.queue_cap_per_class_per_service as usize,
        "被拒的请求不得挤进队列"
    );
}

/// `enqueue` 失败时把 `Err(DenialReason)` 取出来断言。
fn denied_of_enqueue(ledger: &mut BudgetLedger, request: &BudgetClaim) -> DenialReason {
    match ledger.enqueue(request, 0, DEADLINE_MS) {
        Ok(waiter) => panic!("队列已满，却排进了 {waiter:?}"),
        Err(reason) => reason,
    }
}

#[test]
fn cm65_per_user_queue_cap_is_checked_separately_from_the_class_cap() {
    let (org, conn) = (org(), conn());
    let config = config(1, [0, 0, 0, 0])
        .with_queue_cap_per_class_per_service(32)
        .with_queue_cap_per_user(2);
    let mut ledger = BudgetLedger::new(config.clone());
    ledger.ensure_service(&conn);

    for _ in 0..config.queue_cap_per_user {
        ok(
            ledger.enqueue(
                &claim(&org, &conn, "user-a", ResourceClass::Interactive),
                0,
                DEADLINE_MS,
            ),
            "enqueue user-a",
        );
    }
    match denied_of_enqueue(
        &mut ledger,
        &claim(&org, &conn, "user-a", ResourceClass::Metadata),
    ) {
        DenialReason::QueueFull { scope, cap, depth } => {
            assert_eq!(scope, QueueScope::PerUser);
            assert_eq!(cap, config.queue_cap_per_user);
            assert_eq!(depth, config.queue_cap_per_user as usize);
        }
        other => panic!("单用户队列上限必须报 QueueFull(PerUser)，实际 {other:?}"),
    }

    // 类别深度还没到 32，是**用户**维度先满的；换个用户照样能排。
    assert_eq!(
        ledger.queue_depth(&conn, ResourceClass::Interactive),
        2,
        "被判满的类别深度必须原样保留"
    );
    ok(
        ledger.enqueue(
            &claim(&org, &conn, "user-b", ResourceClass::Interactive),
            0,
            DEADLINE_MS,
        ),
        "另一个用户必须仍能排队",
    );
}

//! §6.2 租约状态机的公开面集成测试：**每一条合法迁移、每一条非法迁移、每一个边界**。
//!
//! 替身与夹具在 [`resource_support`]：`tests/` 编译 lib 时不带 `cfg(test)`，
//! 所以那里按生产代码自己声明的公开接缝（`MonotonicSource` / `PhysicalTransport` /
//! `HandleCloser`）自建替身，而不是去放宽生产代码的可见性。
//!
//! 时间只经 `MonotonicTestClock::advance` 手动推进，**不出现 `sleep`**。

mod resource_support;
mod support;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::Duration;

use datazen::execution::runtime::budget::DenialReason;
use datazen::execution::runtime::connection::{ExecutionId, HandleId, SessionHandleRef};
use datazen::execution::runtime::resource::{
    CleanupDisposition, DriverCleanVerdict, IdleEviction, LeaseState, NoHandleCloser, PoolRefusal,
    ResourceError, LEASE_TRANSITIONS,
};

use resource_support::{
    clear_host, counters, counters as counters_of, fixed_request, fixture, handle, main_request,
    tight_fixture,
};
use support::ok;

// ===========================================================================
// 1. 迁移表本身：三要素齐全 + 终态无出口 + 从任一状态可达终态
// ===========================================================================

#[test]
fn every_declared_transition_states_how_it_enters_behaves_and_exits() {
    assert!(
        !LEASE_TRANSITIONS.is_empty(),
        "a lease state machine must declare at least one edge"
    );
    for rule in LEASE_TRANSITIONS {
        let edge = format!("{} -> {}", rule.from.as_str(), rule.to.as_str());
        assert!(
            !rule.enters_when.trim().is_empty(),
            "{edge} has no entry condition"
        );
        assert!(
            !rule.behaves_as.trim().is_empty(),
            "{edge} has no in-state behaviour"
        );
        assert!(
            !rule.exits_when.trim().is_empty(),
            "{edge} has no exit condition — an entry without an exit is a one-way deadlock"
        );
    }
}

#[test]
fn the_terminal_state_has_no_exit_and_every_state_reaches_it() {
    for from in LeaseState::ALL {
        let outgoing = LEASE_TRANSITIONS
            .iter()
            .filter(|rule| rule.from == from)
            .count();
        if from.is_terminal() {
            assert_eq!(
                outgoing,
                0,
                "terminal state {} must not declare an outgoing edge",
                from.as_str()
            );
        } else {
            assert!(
                outgoing > 0,
                "state {} has no exit at all: an entry without an exit is a one-way deadlock",
                from.as_str()
            );
        }
    }

    // 广度优先：从 `Acquired` 出发必须能走到终态，否则存在无法归还的租约。
    let mut queue = VecDeque::from([LeaseState::Acquired]);
    let mut seen = vec![LeaseState::Acquired];
    while let Some(state) = queue.pop_front() {
        for rule in LEASE_TRANSITIONS.iter().filter(|rule| rule.from == state) {
            if !seen.contains(&rule.to) {
                seen.push(rule.to);
                queue.push_back(rule.to);
            }
        }
    }
    assert!(
        seen.iter().any(|state| state.is_terminal()),
        "no terminal state is reachable from acquired: reachable = {:?}",
        seen.iter().map(|state| state.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn the_runtime_table_agrees_with_the_declared_transition_matrix() {
    for from in LeaseState::ALL {
        for to in LeaseState::ALL {
            let declared = LEASE_TRANSITIONS
                .iter()
                .any(|rule| rule.from == from && rule.to == to);
            let runtime = from.transition(to).is_ok();
            assert_eq!(
                declared,
                runtime,
                "{} -> {}: the declared matrix and the runtime table disagree",
                from.as_str(),
                to.as_str()
            );
        }
    }
}

// ===========================================================================
// 2. 合法迁移：每一条都有可观测的落点
// ===========================================================================

#[test]
fn a_fresh_acquire_lands_in_use_and_takes_exactly_one_budget_permit() {
    let mut fixture = fixture();
    let record = ok(fixture.manager.acquire(&main_request()), "first acquire");

    assert_eq!(record.state, LeaseState::InUse);
    assert_eq!(fixture.transport.count("open"), 1);
    assert_eq!(fixture.manager.resources().occupied_slots(), 1);
    assert_eq!(
        counters(&fixture.budget),
        (1, 1, 0, 0),
        "one physical occupancy costs exactly one permit, and it is not consumed yet"
    );

    // 已签发 ⇒ 允许执行（§6.2 在态行为）。
    ok(
        fixture.manager.accepts_execution(&record.lease_id),
        "an issued lease accepts execution",
    );
}

#[test]
fn releasing_a_clear_short_operation_parks_it_in_the_idle_pool_without_touching_budget() {
    let mut fixture = fixture();
    let record = ok(fixture.manager.acquire(&main_request()), "acquire");

    let released = ok(
        fixture.manager.release(
            &record.lease_id,
            &clear_host(),
            &DriverCleanVerdict::reported_clean(),
            &mut NoHandleCloser,
        ),
        "a clean driver report and a clear host snapshot admit the lease",
    );

    assert_eq!(
        released.cleanup.disposition,
        CleanupDisposition::ReturnedToPool
    );
    assert!(released.cleanup.returned_to_pool());
    assert!(
        !released.cleanup.physical_budget_released,
        "a parked lease still holds a physical connection, so its budget stays taken"
    );
    assert!(!released.budget_settled);
    assert!(released.pool.park);
    assert_eq!(released.pool.idle_after, 1);
    assert_eq!(released.eviction, IdleEviction::NotNeeded);
    assert_eq!(
        fixture.transport.count("close"),
        0,
        "parking must not close anything"
    );
    assert_eq!(fixture.manager.resources().idle_lease_count(), 1);
    assert_eq!(
        fixture
            .manager
            .resources()
            .lease(&record.lease_id)
            .map(|r| r.state),
        Some(LeaseState::Acquired)
    );
}

#[test]
fn releasing_a_fixed_session_closes_it_and_settles_budget_exactly_once() {
    let mut fixture = fixture();
    let record = ok(fixture.manager.acquire(&fixed_request()), "acquire");

    let released = ok(
        fixture.manager.release(
            &record.lease_id,
            &clear_host(),
            &DriverCleanVerdict::reported_clean(),
            &mut NoHandleCloser,
        ),
        "a fixed session closes instead of parking",
    );

    assert_eq!(released.cleanup.disposition, CleanupDisposition::Closed);
    assert!(released.cleanup.physical_budget_released);
    assert!(released.budget_settled);
    assert_eq!(released.eviction, IdleEviction::NotNeeded);
    assert_eq!(
        fixture.transport.labels(),
        vec!["open", "close"],
        "closing is the only physical action after a clean open"
    );
    assert_eq!(fixture.manager.resources().lease_count(), 0);
    assert_eq!(counters(&fixture.budget), (0, 1, 1, 0));
}

#[test]
fn a_running_execution_quarantines_the_lease_and_keeps_its_budget() {
    let mut fixture = fixture();
    let record = ok(fixture.manager.acquire(&main_request()), "acquire");
    let execution = ExecutionId::new("exec-it-1");
    ok(
        fixture
            .manager
            .begin_execution(&record.lease_id, execution.clone()),
        "begin execution",
    );

    let released = ok(
        fixture.manager.release(
            &record.lease_id,
            &clear_host().with_active_execution(execution),
            &DriverCleanVerdict::reported_clean(),
            &mut NoHandleCloser,
        ),
        "an active execution quarantines instead of parking",
    );

    assert_eq!(
        released.cleanup.disposition,
        CleanupDisposition::Quarantined
    );
    assert!(!released.cleanup.physical_budget_released);
    assert!(!released.budget_settled);
    assert_eq!(
        released
            .cleanup
            .blockers
            .first()
            .map(|reason| reason.reason_code()),
        Some("hostActiveExecution")
    );
    assert_eq!(
        fixture.transport.count("close"),
        0,
        "a quarantined lease is not closed: the operator still needs it"
    );
    assert_eq!(counters(&fixture.budget), (1, 1, 0, 0));

    // 隔离的出口：排障完成后强制退休。
    ok(
        fixture.manager.retire(&record.lease_id),
        "retire the quarantined lease",
    );
    assert_eq!(fixture.manager.resources().lease_count(), 0);
    assert_eq!(counters(&fixture.budget), (0, 1, 1, 0));
}

#[test]
fn an_unconfirmed_close_moves_closing_to_quarantined_instead_of_closed() {
    let mut fixture = fixture();
    let record = ok(fixture.manager.acquire(&fixed_request()), "acquire");
    fixture.transport.refuse_close();

    let released = ok(
        fixture.manager.release(
            &record.lease_id,
            &clear_host(),
            &DriverCleanVerdict::reported_clean(),
            &mut NoHandleCloser,
        ),
        "a refused close handshake does not stop the release",
    );

    assert_eq!(
        released.cleanup.disposition,
        CleanupDisposition::Quarantined,
        "closing may only reach closed on a confirmed close"
    );
    assert!(
        !released.cleanup.physical_budget_released,
        "an unconfirmed close must not settle the budget"
    );
    assert!(!released.budget_settled);
    assert_eq!(fixture.transport.count("close"), 1);
    assert_eq!(counters(&fixture.budget), (1, 1, 0, 0));
    assert_eq!(
        fixture
            .manager
            .resources()
            .lease(&record.lease_id)
            .map(|r| r.state),
        Some(LeaseState::Quarantined)
    );

    // 出口：握手恢复后强制退休，这次关闭被确认 ⇒ 预算核销。
    fixture.transport.accept_close();
    ok(
        fixture.manager.retire(&record.lease_id),
        "retire after close confirms",
    );
    assert_eq!(counters(&fixture.budget), (0, 1, 1, 0));
}

#[test]
fn a_disabled_owner_quarantines_and_holds_the_budget_until_the_close_confirms() {
    let mut fixture = fixture();
    let record = ok(fixture.manager.acquire(&main_request()), "acquire");

    let released = ok(
        fixture.manager.release(
            &record.lease_id,
            &clear_host().with_disabled_owner("owner-a"),
            &DriverCleanVerdict::reported_clean(),
            &mut NoHandleCloser,
        ),
        "a disabled owner quarantines",
    );

    assert_eq!(
        released.cleanup.disposition,
        CleanupDisposition::Quarantined
    );
    assert!(!released.budget_settled);
    assert_eq!(
        released
            .cleanup
            .blockers
            .first()
            .map(|reason| reason.reason_code()),
        Some("hostOwnerDisabled")
    );
    assert_eq!(fixture.transport.count("close"), 0);
    assert_eq!(counters(&fixture.budget), (1, 1, 0, 0));

    ok(fixture.manager.retire(&record.lease_id), "retire");
    assert_eq!(fixture.transport.count("close"), 1);
    assert_eq!(counters(&fixture.budget), (0, 1, 1, 0));
}

#[test]
fn an_idle_deadline_closes_the_parked_lease_and_reconcile_settles_its_permit() {
    let mut fixture = fixture();
    let request = main_request();
    let parked = ok(fixture.manager.acquire(&request), "first acquire");
    ok(
        fixture.manager.release(
            &parked.lease_id,
            &clear_host(),
            &DriverCleanVerdict::reported_clean(),
            &mut NoHandleCloser,
        ),
        "park",
    );
    assert_eq!(fixture.manager.resources().idle_lease_count(), 1);

    // 越过 `IDLE_POOL_TTL_SECONDS`：下一次签发必须顺手关掉过期空闲，而不是复用。
    fixture.clock.advance(Duration::from_secs(61));
    let fresh = ok(
        fixture.manager.acquire(&request),
        "acquire after the idle deadline",
    );

    assert_ne!(
        fresh.lease_id, parked.lease_id,
        "an expired idle must not be handed out again"
    );
    assert_eq!(fixture.transport.count("close"), 1);
    assert_eq!(fixture.manager.resources().idle_lease_count(), 0);
    assert_eq!(
        counters(&fixture.budget),
        (2, 2, 0, 0),
        "the orphan permit is still held until reconciliation aligns it"
    );

    // 台账里已经没有那条租约了 ⇒ 许可必须被核销，否则就是占着预算的幽灵连接。
    assert_eq!(fixture.manager.reconcile(), 1);
    assert_eq!(counters(&fixture.budget), (1, 2, 1, 0));
}

// ===========================================================================
// 3. 边界与非法迁移：必须被拒绝
// ===========================================================================

#[test]
fn a_refused_physical_quota_closes_what_it_opened_and_leaks_no_permit() {
    let mut fixture = tight_fixture();
    let request = main_request();
    ok(
        fixture.manager.acquire(&request),
        "first acquire fits the quota",
    );
    ok(
        fixture.manager.acquire(&request),
        "second acquire fits the quota",
    );

    let error: ResourceError = match fixture.manager.acquire(&request) {
        Ok(_) => panic!("the third physical occupancy must exceed the total quota"),
        Err(error) => error,
    };
    match &error {
        ResourceError::PhysicalQuotaRefused(DenialReason::PhysicalExhausted { cap, .. }) => {
            assert_eq!(*cap, 2, "the physical occupancy cap is enforced");
        }
        other => panic!("expected an exhausted physical quota, got {other:?}"),
    }
    assert_eq!(error.reason(), "resourcePhysicalQuotaRefused");

    assert_eq!(
        fixture.transport.count("open"),
        3,
        "the refused acquisition did open before it was refused — it must also close"
    );
    assert_eq!(
        fixture.transport.count("close"),
        1,
        "the compensating close is what keeps the ledger free of ghost occupancy"
    );
    assert_eq!(fixture.manager.resources().lease_count(), 2);
    assert_eq!(counters(&fixture.budget), (2, 2, 0, 0));
}

#[test]
fn releasing_an_already_closed_lease_is_refused_and_settles_nothing_extra() {
    let mut fixture = fixture();
    let record = ok(fixture.manager.acquire(&fixed_request()), "acquire");
    ok(
        fixture.manager.release(
            &record.lease_id,
            &clear_host(),
            &DriverCleanVerdict::reported_clean(),
            &mut NoHandleCloser,
        ),
        "first release closes",
    );

    let error = match fixture.manager.release(
        &record.lease_id,
        &clear_host(),
        &DriverCleanVerdict::reported_clean(),
        &mut NoHandleCloser,
    ) {
        Ok(_) => panic!("a closed lease must not be released twice"),
        Err(error) => error,
    };
    assert_eq!(error.reason(), "resourceUnknown");
    assert_eq!(fixture.transport.count("close"), 1);
    assert_eq!(
        counters(&fixture.budget),
        (0, 1, 1, 0),
        "a refused release must not touch the ledger"
    );
}

#[test]
fn twenty_concurrent_releases_settle_the_budget_exactly_once() {
    let mut fixture = fixture();
    let record = ok(fixture.manager.acquire(&fixed_request()), "acquire");

    let manager = Arc::new(Mutex::new(fixture.manager));
    // `fixture.manager` 已被移走，所以断言改用提前复制出来的物理替身与台账句柄。
    let transport = Arc::clone(&fixture.transport);
    let budget = Arc::clone(&fixture.budget);
    let barrier = Arc::new(Barrier::new(20));
    let accepted = Arc::new(AtomicU64::new(0));

    let mut workers = Vec::new();
    for _ in 0..20 {
        let manager = Arc::clone(&manager);
        let barrier = Arc::clone(&barrier);
        let accepted = Arc::clone(&accepted);
        let lease_id = record.lease_id.clone();
        workers.push(thread::spawn(move || {
            let mut closer = NoHandleCloser;
            barrier.wait();
            if manager
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .release(
                    &lease_id,
                    &clear_host(),
                    &DriverCleanVerdict::reported_clean(),
                    &mut closer,
                )
                .is_ok()
            {
                accepted.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }
    for worker in workers {
        ok(worker.join(), "every release worker must exit cleanly");
    }

    // 管理器留在共享单元格里；断言只用到独立的物理替身与预算台账句柄，
    // 所以不需要把它取回来 —— 避免为了一行断言去和 `Arc`/`Mutex` 的 `into_inner` 歧义较劲。
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        1,
        "exactly one release may win the race"
    );
    assert_eq!(transport.count("close"), 1);
    assert_eq!(transport.labels(), vec!["open", "close"]);
    assert_eq!(
        counters_of(&budget),
        (0, 1, 1, 0),
        "budget is released exactly once no matter how many callers raced"
    );
}

// ---- 句柄的登记与摘除：登记进来的必须摘得掉 ----

#[test]
fn a_registered_handle_must_be_closed_before_the_lease_may_return_to_the_pool() {
    let mut fixture = fixture();
    let record = ok(fixture.manager.acquire(&main_request()), "acquire");
    ok(
        fixture
            .manager
            .register_handle(&record.lease_id, handle("h-1", &record.resource_id)),
        "register a cursor",
    );
    assert_eq!(fixture.manager.handles().len(), 1);

    let error = match fixture.manager.release(
        &record.lease_id,
        &clear_host(),
        &DriverCleanVerdict::reported_clean(),
        &mut NoHandleCloser,
    ) {
        Ok(_) => panic!("a registered handle blocks the return — §6.5"),
        Err(error) => error,
    };
    assert_eq!(error.reason(), "resourceTransportRefused");
    assert_eq!(
        fixture
            .manager
            .resources()
            .lease(&record.lease_id)
            .map(|r| r.state),
        Some(LeaseState::InUse),
        "a refused release leaves the lease exactly where it was"
    );
    assert_eq!(fixture.manager.handles().len(), 1);
    assert_eq!(fixture.transport.count("close"), 0);
    assert_eq!(counters(&fixture.budget), (1, 1, 0, 0));

    // 这次带上真的会关句柄的关闭能力：先关、再摘、才允许物理释放。
    let mut closer = |_: &SessionHandleRef| Ok(());
    let released = ok(
        fixture.manager.release(
            &record.lease_id,
            &clear_host(),
            &DriverCleanVerdict::reported_clean(),
            &mut closer,
        ),
        "a closer that can close the handle lets the return through",
    );
    assert_eq!(released.handles.closed_handles, 1);
    assert!(released.handles.is_settled());
    assert_eq!(
        released.handles.deregistered,
        vec![HandleId::new("h-1".to_owned())]
    );
    assert_eq!(fixture.manager.handles().len(), 0);
    assert!(released.cleanup.returned_to_pool());
    assert_eq!(fixture.manager.resources().idle_lease_count(), 1);
}

#[test]
fn an_unreleased_handle_declared_by_the_host_blocks_the_return_too() {
    let mut fixture = fixture();
    let record = ok(fixture.manager.acquire(&main_request()), "acquire");
    let host = clear_host().with_unreleased_handle(handle("h-2", &record.resource_id));

    let released = ok(
        fixture.manager.release(
            &record.lease_id,
            &host,
            &DriverCleanVerdict::reported_clean(),
            &mut NoHandleCloser,
        ),
        "the release itself succeeds with a remedy",
    );

    assert!(!released.cleanup.returned_to_pool());
    assert_eq!(
        released
            .cleanup
            .blockers
            .first()
            .map(|reason| reason.reason_code()),
        Some("hostUnreleasedSessionHandle")
    );
    // 未归还的句柄逼出一次原资源上的复位：回滚事务、解除映射。复位成功后
    // `outstanding_handles` 按设计清空（`cleanup.rs:410`）——没有任何句柄还能提交。
    assert_eq!(
        released.cleanup.outstanding_handles,
        Vec::new(),
        "a performed reset clears the outstanding handle list"
    );
    assert!(released.cleanup.session_reset_performed);
    // 归还被拒 ⇒ 直接关闭，而不是停在池里等 TTL。被关闭确认 ⇒ 预算当场核销。
    assert_eq!(released.cleanup.disposition, CleanupDisposition::Closed);
    assert!(released.cleanup.physical_budget_released);
    assert!(released.budget_settled);
    assert!(!released.pool.park);
    assert_eq!(
        released.pool.refusal,
        Some(PoolRefusal::NotOffered),
        "a closed lease was never offered to the pool; reporting park=true would invite a reuse of a dead connection"
    );
    assert_eq!(released.pool.idle_after, 0);
    assert_eq!(fixture.manager.resources().idle_lease_count(), 0);
    assert_eq!(fixture.manager.resources().lease_count(), 0);
    assert_eq!(counters(&fixture.budget), (0, 1, 1, 0));
}

#[test]
fn an_unclean_driver_verdict_closes_instead_of_parking_but_never_claims_a_clean_reset() {
    let mut fixture = fixture();
    let record = ok(fixture.manager.acquire(&main_request()), "acquire");

    let released = ok(
        fixture.manager.release(
            &record.lease_id,
            &clear_host(),
            &DriverCleanVerdict::reported_unclean(),
            &mut NoHandleCloser,
        ),
        "an unclean verdict still produces a disposition",
    );

    assert!(!released.cleanup.returned_to_pool());
    assert_eq!(
        released
            .cleanup
            .blockers
            .first()
            .map(|reason| reason.reason_code()),
        Some("driverUnclean")
    );
    assert!(released.cleanup.physical_budget_released);
    assert!(!released.cleanup.session_reset_performed);
    assert_eq!(
        fixture.transport.labels(),
        vec!["open", "close"],
        "an unclean verdict must not be laundered through a reset"
    );
    assert_eq!(counters(&fixture.budget), (0, 1, 1, 0));
}

// ===========================================================================
// 5. 归池前的复位（§9.4）：三本账必须一起改口
// ===========================================================================

/// 归池前的复位**成功**时，池层看到的仍然是停放 —— 多加的那一次物理动作不许
/// 让 `pool.park`、`idle_after` 或预算核销任何一处漂移。
#[test]
fn a_confirmed_pre_pool_reset_parks_the_lease_and_leaves_the_pool_verdict_untouched() {
    let mut fixture = fixture();
    let record = ok(fixture.manager.acquire(&main_request()), "acquire");

    let released = ok(
        fixture.manager.release(
            &record.lease_id,
            &clear_host(),
            &DriverCleanVerdict::reported_clean(),
            &mut NoHandleCloser,
        ),
        "a clean report and a clear host admit the lease",
    );

    assert!(released.cleanup.returned_to_pool());
    assert!(
        released.cleanup.session_reset_performed,
        "§9.4/§7.3：归池前必跑复位，且必须被记成已执行"
    );
    assert_eq!(
        fixture.transport.labels(),
        vec!["open", "reset"],
        "归池不许关闭连接，但必须出现一次复位"
    );
    assert!(released.pool.park);
    assert_eq!(released.pool.idle_after, 1);
    assert_eq!(released.eviction, IdleEviction::NotNeeded);
    assert!(
        !released.budget_settled,
        "回池不释放物理预算（§9.3）—— 复位这个额外动作不得改变这一格"
    );
    assert_eq!(counters(&fixture.budget), (1, 1, 0, 0));
}

/// 归池前的复位**失败**时，池层必须看到「压根没来」而不是「来了但池子不要」：
/// `park = false` + `NotOffered` + 预算随确认关闭当场核销。
///
/// 这是缺陷在**门面层**的可观测后果：旧实现从不跑复位，于是这条复位失败的路径
/// 结构上不存在，池层永远被问到 `park = true`。
#[test]
fn a_park_refused_by_the_pre_pool_reset_is_reported_to_the_pool_as_not_offered() {
    let mut fixture = fixture();
    let record = ok(fixture.manager.acquire(&main_request()), "acquire");
    fixture.transport.refuse_reset();

    let released = ok(
        fixture.manager.release(
            &record.lease_id,
            &clear_host(),
            &DriverCleanVerdict::reported_clean(),
            &mut NoHandleCloser,
        ),
        "a refused park is still a disposition, not a ledger failure",
    );

    assert!(
        !released.cleanup.returned_to_pool(),
        "§9.4：reset 失败直接关闭，绝不留在空闲池"
    );
    assert_eq!(released.cleanup.disposition, CleanupDisposition::Closed);
    assert!(released.cleanup.physical_budget_released);
    assert!(
        !released.cleanup.session_reset_performed,
        "失败的复位不得被记成已完成"
    );

    // 池层的三格：没来 ⇒ NotOffered（不是 Stateless/PoolFull，也不是 park=true）。
    assert!(!released.pool.park);
    assert_eq!(released.pool.refusal, Some(PoolRefusal::NotOffered));
    assert_eq!(released.pool.idle_after, 0);
    // 关闭被确认 ⇒ 门面层核销预算；不需要额外的池淘汰动作。
    assert!(released.budget_settled);
    assert_eq!(released.eviction, IdleEviction::NotNeeded);
    assert_eq!(
        fixture.transport.labels(),
        vec!["open", "reset", "close"],
        "复位尝试过（入账）⇒ 关闭是复位失败后的替代落点"
    );
    assert_eq!(counters(&fixture.budget), (0, 1, 1, 0));
    assert_eq!(fixture.manager.resources().idle_lease_count(), 0);
    assert_eq!(fixture.manager.resources().lease_count(), 0);
}

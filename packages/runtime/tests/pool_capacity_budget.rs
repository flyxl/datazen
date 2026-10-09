//! §9 连接池：容量裁决、归还路径、以及**池容量与预算额度不重复记账**。
//!
//! 这一份测试盯住 Task B 的三条硬要求：
//!
//! 1. **池不另起一本账**：物理占用只由 `BudgetCoordinator` 记账，池只决定「归不归池」。
//!    每条用例都把池的空闲数与台账的在场许可数并排断言，两者必须同增同减 ——
//!    只要有人偷偷加了一个池内计数器，这条不变量立刻塌。
//! 2. **无状态能力驱动不得伪装成单连接池**：sqlite 这类进程内单连接驱动，
//!    池容量恒为 0，归还即关闭，绝不「留一条备用」。
//! 3. **清理语义与 `ResourceManager` 对齐**：归池、逐出、隔离三条出口各有各的
//!    物理与预算后果，不能互相顶替。

mod resource_support;
mod support;

use datazen_runtime::resource::{
    CleanupDisposition, DriverCleanVerdict, IdleEviction, NoHandleCloser, PoolPolicy, PoolRefusal,
    ResourceError, ResourceModel, DEFAULT_MAX_IDLE_PER_POOL,
};

use resource_support::{clear_host, counters, fixed_request, fixture, main_request};
use support::ok;

/// 归还一条租约并要求成功：这里只关心「出口成功」，失败信息不带场景上下文会很难读。
fn release(
    manager: &mut datazen_runtime::resource::LeaseManager,
    lease_id: &datazen_runtime::connection::LeaseId,
) -> datazen_runtime::resource::LeaseRelease {
    ok(
        manager.release(
            lease_id,
            &clear_host(),
            &DriverCleanVerdict::reported_clean(),
            &mut NoHandleCloser,
        ),
        "release a clean short-operation lease",
    )
}

// ===========================================================================
// 1. 容量上限：池的空闲数与台账的在场许可数必须同时变化
// ===========================================================================

#[test]
fn pool_idle_capacity_never_drifts_from_the_budget_ledger() {
    let mut fixture = fixture();
    let request = main_request();
    ok(
        fixture
            .manager
            .declare_pool(request.pool_key(), PoolPolicy::stateful(1)),
        "declare a stateful pool of one",
    );

    let first = ok(fixture.manager.acquire(&request), "first acquire");
    let second = ok(fixture.manager.acquire(&request), "second acquire");
    assert_eq!(counters(&fixture.budget), (2, 2, 0, 0));
    assert_eq!(fixture.manager.resources().occupied_slots(), 2);

    // 归池：物理连接留着，**额度也留着** —— §9.3 说「所有占用都吃预算」，空闲连接不例外。
    let parked = release(&mut fixture.manager, &first.lease_id);
    assert!(parked.pool.park);
    assert_eq!(parked.pool.idle_after, 1);
    assert_eq!(
        parked.cleanup.disposition,
        CleanupDisposition::ReturnedToPool
    );
    assert_eq!(parked.eviction, IdleEviction::NotNeeded);
    assert!(
        !parked.budget_settled,
        "a parked lease still holds its permit; settling here would hand out free capacity"
    );
    assert_eq!(
        counters(&fixture.budget),
        (2, 2, 0, 0),
        "parking moves nothing in the ledger"
    );

    // 池满：第二条被逐出 ⇒ 物理关闭 + 额度核销，两本账在同一个动作里一起动。
    let evicted = release(&mut fixture.manager, &second.lease_id);
    assert!(!evicted.pool.park);
    assert_eq!(
        evicted.pool.refusal,
        Some(PoolRefusal::PoolFull {
            idle: 1,
            max_idle: 1
        })
    );
    assert_eq!(evicted.eviction, IdleEviction::Closed);
    assert!(evicted.budget_settled);
    assert_eq!(
        counters(&fixture.budget),
        (1, 2, 1, 0),
        "the evicted lease is the only one that gave its permit back"
    );
    assert_eq!(fixture.manager.resources().idle_lease_count(), 1);
    assert_eq!(fixture.manager.resources().lease_count(), 1);
    assert_eq!(fixture.transport.count("close"), 1);
}

#[test]
fn an_undeclared_pool_key_falls_back_to_the_documented_default_capacity() {
    let mut fixture = fixture();
    let request = main_request();
    // 不登记 = 按 §9.2 默认走。这条用例把「默认值」钉成事实，而不是留在注释里。
    assert_eq!(
        fixture.manager.pools().declared(),
        0,
        "nothing is declared yet"
    );

    let leases: Vec<_> = (0..3)
        .map(|index| {
            ok(
                fixture.manager.acquire(&request),
                &format!("acquire #{index} against the shared quota"),
            )
        })
        .collect();
    assert_eq!(counters(&fixture.budget), (3, 3, 0, 0));

    let first = release(&mut fixture.manager, &leases[0].lease_id);
    assert!(first.pool.park);
    let second = release(&mut fixture.manager, &leases[1].lease_id);
    assert!(
        second.pool.park,
        "the default capacity is {DEFAULT_MAX_IDLE_PER_POOL}, so a second idle fits"
    );
    let third = release(&mut fixture.manager, &leases[2].lease_id);

    assert_eq!(
        third.pool.refusal,
        Some(PoolRefusal::PoolFull {
            idle: DEFAULT_MAX_IDLE_PER_POOL,
            max_idle: DEFAULT_MAX_IDLE_PER_POOL,
        }),
        "the default per-pool capacity must be the documented {DEFAULT_MAX_IDLE_PER_POOL}"
    );
    assert_eq!(third.eviction, IdleEviction::Closed);
    assert_eq!(fixture.manager.resources().idle_lease_count(), 2);
    assert_eq!(counters(&fixture.budget), (2, 3, 1, 0));
}

// ===========================================================================
// 2. 能力模型：无状态驱动与固定会话都不进池
// ===========================================================================

#[test]
fn a_stateless_capability_key_is_never_parked_and_is_closed_on_release() {
    let mut fixture = fixture();
    let request = main_request();
    ok(
        fixture.manager.declare_stateless_pool(request.pool_key()),
        "declare a stateless key (sqlite-style single connection)",
    );

    let record = ok(fixture.manager.acquire(&request), "acquire");
    let released = release(&mut fixture.manager, &record.lease_id);

    assert_eq!(
        released.pool.refusal,
        Some(PoolRefusal::Stateless),
        "a driver without reusable state must never get a parked connection"
    );
    assert!(!released.pool.park);
    assert_eq!(released.pool.idle_after, 0);
    assert_eq!(
        released.cleanup.disposition,
        CleanupDisposition::ReturnedToPool,
        "the cleanup verdict only speaks about physical hygiene; the destination is the pool's call"
    );
    assert_eq!(
        released.eviction,
        IdleEviction::Closed,
        "so the authoritative outcome is the eviction: the connection was closed, not parked"
    );
    assert!(released.budget_settled);
    assert_eq!(fixture.manager.resources().idle_lease_count(), 0);
    assert_eq!(fixture.transport.count("close"), 1);
    assert_eq!(counters(&fixture.budget), (0, 1, 1, 0));

    let policy = fixture
        .manager
        .pools()
        .pool(&request.pool_key())
        .map(|pool| pool.policy())
        .unwrap_or_else(|| panic!("the key was declared, so it must be in the registry"));
    assert_eq!(policy.model(), ResourceModel::Stateless);
    assert!(
        !policy.parkable(),
        "a stateless policy cannot express any idle capacity at all"
    );
    assert_eq!(policy.max_idle(), 0);
}

#[test]
fn a_fixed_session_is_refused_by_pool_policy_even_when_the_host_is_clear() {
    let mut fixture = fixture();
    let request = fixed_request();
    ok(
        fixture
            .manager
            .declare_pool(request.pool_key(), PoolPolicy::default()),
        "declare the default pool for the fixed-session key",
    );

    let record = ok(fixture.manager.acquire(&request), "acquire fixed session");
    let released = release(&mut fixture.manager, &record.lease_id);

    assert_eq!(
        released.cleanup.disposition,
        CleanupDisposition::Closed,
        "a fixed session is bound to its caller, so the cleanup layer closes it unconditionally"
    );
    assert_eq!(
        released.pool.refusal,
        Some(PoolRefusal::NotOffered),
        "which is why the idle pool is never even consulted for it"
    );
    assert!(!released.pool.park);
    assert_eq!(released.eviction, IdleEviction::NotNeeded);
    assert!(released.budget_settled);
    assert_eq!(fixture.manager.resources().idle_lease_count(), 0);
    assert_eq!(fixture.transport.count("close"), 1);
    assert_eq!(counters(&fixture.budget), (0, 1, 1, 0));

    // 策略层本身也把「固定会话永不进池」写成硬规则，任何直接问策略的调用方都拿不到 park。
    let direct = fixture.manager.pools_mut().decide(
        &request.pool_key(),
        datazen_runtime::resource::LeasePurpose::FixedSession,
        0,
    );
    assert_eq!(direct.refusal, Some(PoolRefusal::FixedSession));
    assert!(!direct.park);
}

// ===========================================================================
// 3. 声明一致性
// ===========================================================================

#[test]
fn redeclaring_the_same_policy_is_idempotent_but_a_contradiction_is_refused() {
    let mut fixture = fixture();
    let key = main_request().pool_key();

    ok(
        fixture
            .manager
            .declare_pool(key.clone(), PoolPolicy::stateful(4)),
        "first declaration",
    );
    ok(
        fixture
            .manager
            .declare_pool(key.clone(), PoolPolicy::stateful(4)),
        "the same declaration twice is a no-op, not a conflict",
    );
    assert_eq!(fixture.manager.pools().declared(), 1);

    let error = match fixture.manager.declare_stateless_pool(key.clone()) {
        Ok(()) => panic!("a key cannot be stateful and stateless at the same time"),
        Err(error) => error,
    };
    match &error {
        ResourceError::PoolPolicyConflict {
            declared,
            requested,
            ..
        } => {
            assert_eq!(*declared, ResourceModel::Stateful.name());
            assert_eq!(*requested, ResourceModel::Stateless.name());
        }
        other => panic!("expected a pool policy conflict, got {other:?}"),
    }
    assert_eq!(error.reason(), "resourcePoolPolicyConflict");
    assert_eq!(
        fixture.manager.pools().declared(),
        1,
        "a refused declaration leaves the registry untouched"
    );
    assert_eq!(
        fixture
            .manager
            .pools()
            .pool(&key)
            .map(|pool| pool.policy().max_idle()),
        Some(4),
        "the surviving policy is still the first one"
    );
}

// ===========================================================================
// 4. 非归池出口：绝不能被报成「已停放」
// ===========================================================================

#[test]
fn a_lease_that_was_never_offered_to_the_pool_says_so() {
    let mut fixture = fixture();
    // 固定会话一定走 close 路径，所以拒绝握手才能真的制造出「关不掉」。
    let request = fixed_request();
    ok(
        fixture
            .manager
            .declare_pool(request.pool_key(), PoolPolicy::stateful(4)),
        "declare a roomy pool so the refusal cannot be blamed on capacity",
    );
    fixture.transport.refuse_close();
    let record = ok(fixture.manager.acquire(&request), "acquire");

    let released = release(&mut fixture.manager, &record.lease_id);
    assert_eq!(
        released.cleanup.disposition,
        CleanupDisposition::Quarantined
    );
    assert_eq!(
        released.pool.refusal,
        Some(PoolRefusal::NotOffered),
        "a quarantined lease never reached the idle pool; reporting park=true would invite a reuse of a dead connection"
    );
    assert!(!released.pool.park);
    assert_eq!(released.pool.idle_after, 0);
    assert_eq!(released.eviction, IdleEviction::NotNeeded);
    assert!(!released.budget_settled);
}

#[test]
fn a_quarantined_lease_holds_its_permit_but_takes_no_idle_capacity() {
    let mut fixture = fixture();
    let request = fixed_request();
    ok(
        fixture
            .manager
            .declare_pool(request.pool_key(), PoolPolicy::stateful(4)),
        "declare a roomy pool",
    );
    fixture.transport.refuse_close();
    let record = ok(fixture.manager.acquire(&request), "acquire");

    let released = release(&mut fixture.manager, &record.lease_id);
    assert_eq!(
        released.cleanup.disposition,
        CleanupDisposition::Quarantined
    );
    assert!(
        !released.cleanup.physical_budget_released,
        "the close was never confirmed, so the physical slot may not be billed as free"
    );
    assert!(!released.budget_settled);
    assert_eq!(counters(&fixture.budget), (1, 1, 0, 0));
    assert_eq!(
        fixture.manager.resources().idle_lease_count(),
        0,
        "quarantine must not consume idle capacity either"
    );

    // 出口：握手恢复后退休，这次关闭被确认 ⇒ 额度与物理一起归零。
    fixture.transport.accept_close();
    ok(
        fixture.manager.retire(&record.lease_id),
        "retire once the close is confirmed",
    );
    assert_eq!(counters(&fixture.budget), (0, 1, 1, 0));
    assert_eq!(fixture.manager.resources().lease_count(), 0);
}

// ===========================================================================
// 5. 派生视图
// ===========================================================================

#[test]
fn a_pool_snapshot_only_reports_numbers_derived_from_the_ledger() {
    let mut fixture = fixture();
    let request = main_request();
    ok(
        fixture
            .manager
            .declare_pool(request.pool_key(), PoolPolicy::stateful(3)),
        "declare a pool of three",
    );
    let first = ok(fixture.manager.acquire(&request), "first acquire");
    ok(fixture.manager.acquire(&request), "second acquire");
    release(&mut fixture.manager, &first.lease_id);

    let snapshot = fixture.manager.pools().snapshot(
        request.pool_key(),
        fixture.manager.resources().idle_lease_count(),
        fixture.manager.resources().lease_count(),
        fixture.manager.resources().occupied_slots(),
    );
    assert_eq!(snapshot.model, ResourceModel::Stateful);
    assert_eq!(snapshot.max_idle, 3);
    assert_eq!(snapshot.idle_leases, 1);
    assert_eq!(snapshot.issued_leases, 2);
    assert_eq!(
        snapshot.occupied_slots, 2,
        "both the parked lease and the in-use one still hold a physical slot"
    );
    assert_eq!(
        counters(&fixture.budget).0,
        snapshot.occupied_slots,
        "the snapshot's occupancy is the ledger's occupancy, not a second count"
    );
}

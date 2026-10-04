//! 归还接线旅程：**一次资源归还 = 恰好一次隧道释放**。
//!
//! 这是本模块与资源生命周期之间**唯一**的接线点，也是 CM-32「不重复释放、
//! 不漏释放」在接线层的落点。它不是可选路径：`TunnelLedger` 根本不提供
//! 「只摘依赖、不减引用」的口子，想绕开就得自己改计数 —— 那就等于第二份账。
//!
//! 观测点仍然是**计数**（`transport.close_calls()`、`ledger.ref_count()`、
//! `live_tunnels()`），不是 `is_ok()`。

use crate::tunnel::harness::TunnelHarness;
use crate::tunnel::{TunnelError, TunnelState};

/// 一次归还 ⇒ 一次释放；重复归还不再释放；最后一份释放才关闭。
#[test]
fn one_return_releases_exactly_once() {
    let mut harness = TunnelHarness::new();
    let spec = harness.same_spec();
    let first = harness.lease_id("session-first");
    let second = harness.lease_id("session-second");

    harness.ledger.acquire(Some(&spec), &first).expect("first");
    harness
        .ledger
        .acquire(Some(&spec), &second)
        .expect("second");
    assert_eq!(harness.ledger.ref_count(&spec), Some(2));

    // 第一份归还：释放一次，隧道**照常**留着。
    let release = harness
        .ledger
        .return_resource(&first)
        .expect("the first resource held a reference");
    assert!(
        !release.closed,
        "the second resource is still using the tunnel"
    );
    assert_eq!(release.refs, 1);
    assert_eq!(harness.ledger.ref_count(&spec), Some(1));
    assert_eq!(
        harness.transport.close_calls(),
        0,
        "closing the first return would break the second"
    );
    assert!(harness.transport.is_open(&spec));

    // **重复**归还同一条资源：一次归还只对应一次释放，绝不多减、绝不二次 close。
    assert!(
        harness.ledger.return_resource(&first).is_none(),
        "a repeated return must not release a second time"
    );
    assert_eq!(
        harness.ledger.ref_count(&spec),
        Some(1),
        "a repeated return must not touch the surviving reference"
    );
    assert_eq!(harness.transport.close_calls(), 0);

    // 第二个资源结束 ⇒ 最后一份引用释放 ⇒ 关闭，且恰好一次。
    let release = harness
        .ledger
        .return_resource(&second)
        .expect("the second resource held a reference");
    assert!(release.closed);
    assert_eq!(release.refs, 0);
    assert_eq!(harness.transport.close_calls(), 1);
    assert_eq!(harness.ledger.live_tunnels(), 0);
    assert!(!harness.transport.is_open(&spec));
}

/// 不经隧道的资源归还时是**空操作**：没有隧道可释放，也不能凭空 close 一条。
#[test]
fn returning_a_direct_resource_touches_nothing() {
    let mut harness = TunnelHarness::new();
    let direct = harness.lease_id("session-direct");

    assert!(
        harness.ledger.return_resource(&direct).is_none(),
        "a resource that never held a tunnel has nothing to return"
    );
    assert_eq!(harness.transport.close_calls(), 0);
    assert_eq!(harness.ledger.live_tunnels(), 0);
    assert_eq!(harness.transport.journal().len(), 0);
}

/// 同一条资源不得对同一条隧道持**两份**引用。
///
/// 这不是为了好管：计数与依赖清单必须一一对应，否则一次归还只减 1，
/// 多出来的那份引用**永远没人还**，隧道泄漏且无人报错。
#[test]
fn a_resource_may_not_hold_the_same_tunnel_twice() {
    let mut harness = TunnelHarness::new();
    let spec = harness.same_spec();
    let holder = harness.lease_id("session-hold");

    harness.ledger.acquire(Some(&spec), &holder).expect("first");

    let error = harness
        .ledger
        .acquire(Some(&spec), &holder)
        .expect_err("a duplicate reference is refused, not absorbed");
    assert!(matches!(error, TunnelError::AlreadyHeld { .. }));
    assert_eq!(error.reason(), "tunnelAlreadyHeld");
    assert_eq!(
        harness.ledger.ref_count(&spec),
        Some(1),
        "the refused acquire must not inflate the count"
    );

    // 拒绝之后账仍然是平的：一次归还就干净地关闭它。
    assert!(
        harness
            .ledger
            .return_resource(&holder)
            .expect("held")
            .closed
    );
    assert_eq!(harness.transport.close_calls(), 1);
}

/// `return_resource` 与 `release` 是**同一条**归零路径的两端，可以混用。
#[test]
fn returns_and_releases_share_one_draining_path() {
    let mut harness = TunnelHarness::new();
    let spec = harness.same_spec();
    let returned = harness.lease_id("session-return");
    let released = harness.lease_id("session-release");

    harness.ledger.acquire(Some(&spec), &returned).expect("a");
    harness.ledger.acquire(Some(&spec), &released).expect("b");

    assert!(
        !harness
            .ledger
            .return_resource(&returned)
            .expect("held")
            .closed
    );
    assert_eq!(harness.transport.close_calls(), 0);
    assert!(harness.ledger.release(&spec).closed);
    assert_eq!(harness.transport.close_calls(), 1);
    assert_eq!(harness.ledger.live_tunnels(), 0);
}

/// 隧道中途失败：归还入口照样把**每一条**依赖资源都引到归零，且只关闭一次。
///
/// 这是 CM-32 第三条断言在接线层的形状：失败**不**减引用，引用归零的时机
/// 仍然由各持有方自己的归还决定 —— 谁都不能被别人顺带减掉。
#[test]
fn every_dependent_still_returns_after_a_failure() {
    let mut harness = TunnelHarness::new();
    let spec = harness.same_spec();
    let first = harness.lease_id("session-first");
    let second = harness.lease_id("session-second");

    harness.ledger.acquire(Some(&spec), &first).expect("first");
    harness
        .ledger
        .acquire(Some(&spec), &second)
        .expect("second");

    let affected = harness.ledger.report_failure(&spec);
    assert_eq!(
        affected.len(),
        2,
        "the failure reaches every dependent resource"
    );
    assert_eq!(harness.ledger.state(&spec), TunnelState::Failed);
    assert_eq!(
        harness.ledger.ref_count(&spec),
        Some(2),
        "a failure must not decrement anything"
    );

    let release = harness
        .ledger
        .return_resource(&first)
        .expect("a failed tunnel still owes its holders a return path");
    assert!(!release.closed, "the second dependent is still counted");
    assert_eq!(release.state, TunnelState::Failed);
    assert_eq!(harness.transport.close_calls(), 0);

    assert!(
        harness
            .ledger
            .return_resource(&second)
            .expect("last holder")
            .closed
    );
    assert_eq!(harness.transport.close_calls(), 1);
    assert_eq!(harness.ledger.live_tunnels(), 0);
}

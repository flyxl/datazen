//! 隧道失败传播（CM-32 第三条断言）：
//!
//! > 断言：隧道失败传播给依赖资源。
//!
//! 隧道是**共享**资源，所以「传播给依赖资源」的对象不是某个持有者，
//! 而是**全部登记过的依赖租约**。三个注入点各自钉一条：
//!
//! | 注入点 | 现象 | 台账行为 | 依赖资源看到什么 |
//! | --- | --- | --- | --- |
//! | F1 | 建隧道失败 | **不落账**（不建 entry、不增引用） | 申请直接失败，**拿不到指向死端口的句柄** |
//! | F2 | 隧道中途死亡 | entry → `Failed`，**引用不减**，交出全部 `dependents` | 每个依赖方都被点名，无一漏网 |
//! | F3 | 关闭失败（结果不明） | entry → `Unconfirmed` 且**保留** | 重复释放幂等，不再二次 close |

use crate::tunnel::harness::TunnelHarness;
use crate::tunnel::{TunnelState, TunnelState::*};

/// F1：建隧道失败 ⇒ **不落账**，依赖资源**申请不到**隧道。
///
/// 这里刻意用 `reason()` 与台账状态做**计数/内容型**断言，而不是 `is_err()`。
#[test]
fn a_failed_open_never_leaves_a_ledger_entry() {
    let mut harness = TunnelHarness::new();
    let spec = harness.same_spec();
    let lease = harness.lease_id("session-first");
    harness.transport.fail_open(&spec);

    let refused = harness
        .ledger
        .acquire(Some(&spec), &lease)
        .expect_err("a failed open must not hand back a lease");

    assert_eq!(
        refused.reason(),
        "tunnelTransportOpenFailed",
        "the refusal must name the open failure, not a generic error"
    );
    assert_eq!(harness.ledger.live_tunnels(), 0, "no ledger entry");
    assert_eq!(harness.ledger.state(&spec), Absent);
    assert_eq!(
        harness.ledger.ref_count(&spec),
        None,
        "a failed open must not create a reference"
    );
    assert_eq!(harness.transport.close_calls(), 0);
    assert!(
        !harness.transport.is_open(&spec),
        "no physical tunnel was ever opened"
    );
}

/// F1 的下游后果：前一个持有者失败**不得**牵连另一个**本可成功**的 spec。
#[test]
fn one_failing_tunnel_does_not_take_a_healthy_one_down_with_it() {
    let mut harness = TunnelHarness::new();
    let broken = harness.spec("jump-broken", 2222);
    let healthy = harness.same_spec();
    let first = harness.lease_id("session-first");
    let second = harness.lease_id("session-second");

    harness.transport.fail_open(&broken);
    assert!(harness.ledger.acquire(Some(&broken), &first).is_err());

    harness
        .ledger
        .acquire(Some(&healthy), &second)
        .expect("an unrelated spec is unaffected");
    assert_eq!(harness.ledger.live_tunnels(), 1);
    assert!(harness.transport.is_open(&healthy));
}

/// F2：隧道中途死亡 ⇒ **全部**依赖租约被点名，引用**不减**。
///
/// 引用不减是刻意的：各持有方还没归还，账要等它们自己走归还流程。
/// 在这里替它们减引用，正是 CM-28「隧道不多减引用」要防的错误。
#[test]
fn a_mid_life_failure_names_every_dependent_and_does_not_decrement() {
    let mut harness = TunnelHarness::new();
    let spec = harness.same_spec();
    let first = harness.lease_id("session-first");
    let second = harness.lease_id("job-second");
    let third = harness.lease_id("job-third");

    for dependent in [&first, &second, &third] {
        harness
            .ledger
            .acquire(Some(&spec), dependent)
            .expect("acquire a shared tunnel");
    }
    assert_eq!(harness.ledger.ref_count(&spec), Some(3));

    let affected = harness.ledger.report_failure(&spec);

    assert_eq!(
        affected.len(),
        3,
        "CM-32 third assertion: EVERY dependent resource is told, not just the caller"
    );
    assert_eq!(affected, vec![first.clone(), second.clone(), third.clone()]);

    assert_eq!(
        harness.ledger.ref_count(&spec),
        Some(3),
        "failure must NOT decrement references — each holder still owes a release"
    );
    assert_eq!(harness.ledger.state(&spec), Failed);
    assert!(
        harness.transport.is_open(&spec),
        "the ledger does not close on a reported failure — it has no confirmed teardown"
    );
}

/// F2 之后：这条隧道**不再发新引用**，但既有持有方仍能正常归还。
///
/// 不发新引用是必须的 —— 那是 CM-28「隧道不多减引用」失效的前置状态。
#[test]
fn a_failed_tunnel_refuses_new_references_but_still_accepts_releases() {
    let mut harness = TunnelHarness::new();
    let spec = harness.same_spec();
    let first = harness.lease_id("session-first");
    let second = harness.lease_id("session-second");
    harness
        .ledger
        .acquire(Some(&spec), &first)
        .expect("first acquire");
    harness
        .ledger
        .acquire(Some(&spec), &second)
        .expect("second acquire");

    harness.ledger.report_failure(&spec);

    let latecomer = harness.lease_id("session-late");
    let refused = harness
        .ledger
        .acquire(Some(&spec), &latecomer)
        .expect_err("a failed tunnel must not be shared any further");
    assert_eq!(refused.reason(), "tunnelNotShareable");

    // 既有持有方的归还照常进行，最终把隧道真正收掉。
    let released = harness.ledger.release(&spec);
    assert!(!released.closed, "one holder still owes a release");
    let released = harness.ledger.release(&spec);
    assert!(
        released.closed,
        "once every holder has released, the tunnel is torn down for real"
    );
    assert_eq!(harness.transport.close_calls(), 1);
    assert_eq!(harness.ledger.live_tunnels(), 0);
}

/// F2 之后：不再接受新引用，也**不会**给失败状态发句柄（防退化的最后一道）。
#[test]
fn a_failed_tunnel_never_hands_out_another_handle() {
    let mut harness = TunnelHarness::new();
    let spec = harness.same_spec();
    let holder = harness.lease_id("session-first");
    harness
        .ledger
        .acquire(Some(&spec), &holder)
        .expect("acquire");
    harness.ledger.report_failure(&spec);

    let latecomer = harness.lease_id("session-late");
    assert!(harness.ledger.acquire(Some(&spec), &latecomer).is_err());
    assert_eq!(
        harness.transport.open_calls(),
        1,
        "a failed tunnel is never silently re-opened behind the caller's back"
    );
}

/// F3：`close` 失败 ⇒ 结果不明，entry **保留**，重复释放**不二次 close**。
///
/// 这是端口 `:86`「重复释放是幂等的」在失败路径上的兑现：
/// 拆除失败后若允许再次 close，就会对同一条隧道发两次拆除命令。
#[test]
fn a_failed_close_is_never_retried_behind_the_callers_back() {
    let mut harness = TunnelHarness::new();
    let spec = harness.same_spec();
    let first = harness.lease_id("session-first");
    let second = harness.lease_id("session-second");
    harness
        .ledger
        .acquire(Some(&spec), &first)
        .expect("first acquire");
    harness
        .ledger
        .acquire(Some(&spec), &second)
        .expect("second acquire");

    harness.transport.fail_close(&spec);
    let released = harness.ledger.release(&spec);
    assert!(!released.closed, "still one holder left");
    let released = harness.ledger.release(&spec);

    assert!(
        !released.closed,
        "the teardown was attempted and its outcome is unknown"
    );
    assert_eq!(released.state, Unconfirmed);
    assert_eq!(
        released.refs, 0,
        "the reference count itself did reach zero"
    );
    assert_eq!(
        harness.ledger.state(&spec),
        Unconfirmed,
        "an unknown teardown is NEVER silently forgotten"
    );
    assert_eq!(
        harness.ledger.live_tunnels(),
        1,
        "the entry survives so the unknown teardown stays visible"
    );

    // 重复释放：幂等，且**不**对同一条隧道发第二次 close。
    for _ in 0..3 {
        let repeat = harness.ledger.release(&spec);
        assert!(!repeat.closed);
        assert_eq!(repeat.state, Unconfirmed);
        assert_eq!(
            repeat.refs, 0,
            "repeated release is idempotent and never goes below zero"
        );
    }
    assert_eq!(
        harness
            .transport
            .journal()
            .iter()
            .filter(|event| matches!(event, crate::tunnel::harness::TunnelEvent::Close(_)))
            .count(),
        0,
        "a refused close is never retried implicitly"
    );
}

/// 状态字面码稳定（不随文案改写而漂移）。
#[test]
fn every_state_has_a_stable_wire_literal() {
    assert_eq!(TunnelState::Absent.as_str(), "absent");
    assert_eq!(TunnelState::Establishing.as_str(), "establishing");
    assert_eq!(TunnelState::Live.as_str(), "live");
    assert_eq!(TunnelState::Closing.as_str(), "closing");
    assert_eq!(TunnelState::Unconfirmed.as_str(), "unconfirmed");
    assert_eq!(TunnelState::Failed.as_str(), "failed");
    assert!(Live.accepts_reference());
    assert!(!Failed.accepts_reference());
    assert!(!Closing.accepts_reference());
    assert!(!Unconfirmed.accepts_reference());
}

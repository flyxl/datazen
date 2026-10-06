//! CM-32 主旅程：**两个 session / Job 共用同版本隧道**
//!
//! > - 前置：两个 session/Job 共用同版本隧道。
//! > - 步骤：关闭第一个，继续第二个；第二个结束。
//! > - 断言：第一步不关闭隧道；最后一个引用释放才关闭。

use crate::tunnel::harness::{TunnelEvent, TunnelHarness};
use crate::tunnel::{TunnelState, TunnelTransport};
use datazen_platform_api::id::NetworkRouteRef;

/// **第二条断言**：最后一个引用释放才关闭。
///
/// 观测点是**计数**而非返回值：`transport.close_calls()` 与 `journal()` 里
/// `Close` 事件的出现位置。
#[test]
fn closing_the_first_holder_leaves_the_tunnel_running_for_the_second() {
    let mut harness = TunnelHarness::new();
    let spec = harness.same_spec();
    let first = harness.lease_id("session-first");
    let second = harness.lease_id("session-second");

    // 前置：两个 session 共用同版本隧道 ⇒ **只开一次**。
    let first_lease = harness
        .ledger
        .acquire(Some(&spec), &first)
        .expect("first session acquires the tunnel")
        .expect("first session does get a tunnel");
    let second_lease = harness
        .ledger
        .acquire(Some(&spec), &second)
        .expect("second session shares the tunnel")
        .expect("second session does get a tunnel");

    assert!(first_lease.opened, "the first session opens the tunnel");
    assert!(
        !second_lease.opened,
        "the second session must REUSE the tunnel, not open a second one"
    );
    assert_eq!(
        harness.transport.open_calls(),
        1,
        "same TunnelSpec ⇒ exactly one physical tunnel"
    );
    assert_eq!(harness.ledger.ref_count(&spec), Some(2));
    assert_eq!(harness.ledger.state(&spec), TunnelState::Live);

    // 步骤：关闭第一个。**第一步不关闭隧道。**
    let released = harness.ledger.release(&spec);

    assert!(
        !released.closed,
        "the first release must NOT close a still-referenced tunnel"
    );
    assert_eq!(released.refs, 1);
    assert_eq!(released.state, TunnelState::Live);
    assert_eq!(
        harness.transport.close_calls(),
        0,
        "CM-32 first step: closing one of two holders must not close the tunnel"
    );
    assert!(
        harness.transport.is_open(&spec),
        "the tunnel is still physically open after the first holder is gone"
    );
    assert_eq!(
        harness.ledger.state(&spec),
        TunnelState::Live,
        "the second holder keeps the tunnel shareable"
    );

    // 继续第二个：它仍能正常引用同一条隧道（不许因为有人先走了就发一条死隧道）。
    let third = harness.lease_id("session-third");
    let third_lease = harness
        .ledger
        .acquire(Some(&spec), &third)
        .expect("the surviving holder keeps the tunnel usable")
        .expect("third session gets the tunnel");
    assert!(!third_lease.opened);
    assert_eq!(harness.transport.open_calls(), 1);
    assert_eq!(harness.ledger.ref_count(&spec), Some(2));

    // 第二个结束 —— 但第三个还在，所以**仍然不关**。
    let released = harness.ledger.release(&spec);
    assert!(!released.closed, "still one holder left");
    assert_eq!(released.refs, 1);
    assert_eq!(harness.transport.close_calls(), 0);

    // **最后一个引用释放才关闭。**
    let released = harness.ledger.release(&spec);

    assert!(
        released.closed,
        "the LAST reference release is what tears the tunnel down"
    );
    assert_eq!(released.refs, 0);
    assert_eq!(released.state, TunnelState::Absent);
    assert_eq!(
        harness.transport.close_calls(),
        1,
        "CM-32 second step: exactly one close, on the last reference"
    );
    assert!(
        !harness.transport.is_open(&spec),
        "the tunnel is physically gone after the last release"
    );
    assert_eq!(harness.ledger.live_tunnels(), 0);
    assert_eq!(harness.ledger.state(&spec), TunnelState::Absent);
}

/// 共享判定必须落在 `TunnelSpec` **全等**上：跳板不同 ⇒ 各开各的隧道。
///
/// 上游 `network.rs` 已用端口测试锁过同一条规则（`:101-112`）；
/// 本测试钉住的是**实现方没有把判定写松**（例如只比 `route_ref`）。
#[test]
fn sharing_is_keyed_by_the_whole_spec_not_just_the_route() {
    let mut harness = TunnelHarness::new();
    let alpha = harness.spec("jump-a.internal", 2222);
    let beta = harness.spec("jump-b.internal", 2222);

    let first = harness.lease_id("session-first");
    let second = harness.lease_id("session-second");
    harness
        .ledger
        .acquire(Some(&alpha), &first)
        .expect("acquire alpha");
    harness
        .ledger
        .acquire(Some(&beta), &second)
        .expect("acquire beta");

    assert_eq!(
        harness.transport.open_calls(),
        2,
        "same route but a different jump host ⇒ two separate tunnels"
    );
    assert!(!harness.ledger.depends_on(&alpha, &second));
    assert!(!harness.ledger.depends_on(&beta, &first));

    // 关掉其中一个，另一个必须毫发无损。
    let released = harness.ledger.release(&alpha);
    assert!(released.closed, "alpha had exactly one holder");
    assert_eq!(
        harness.transport.close_calls(),
        1,
        "releasing alpha must not touch beta"
    );
    assert!(harness.transport.is_open(&beta));
    assert_eq!(harness.ledger.state(&beta), TunnelState::Live);
    assert_eq!(harness.ledger.ref_count(&beta), Some(1));
}

/// 不经隧道的连接**不占共享计数**（等价宿主 `TunnelKind::None` 直连）。
///
/// `TunnelSpec { via_port: 0 }` 就是端口约定里的「不建隧道」，台账不该为它建 entry。
#[test]
fn a_direct_connection_does_not_enter_the_ledger() {
    let mut harness = TunnelHarness::new();
    let direct = harness.spec("", 0);
    let lease = harness.lease_id("session-direct");

    let acquired = harness
        .ledger
        .acquire(Some(&direct), &lease)
        .expect("a direct connection never fails at the tunnel layer");

    assert!(
        acquired.is_none(),
        "no tunnel lease is issued for a direct connection"
    );
    assert_eq!(
        harness.transport.journal().len(),
        0,
        "the physical layer is never asked to build a tunnel without a jump port"
    );
    assert_eq!(
        harness.ledger.live_tunnels(),
        0,
        "a direct connection must not create a ledger entry"
    );
    assert!(harness.ledger.ref_count(&direct).is_none());
}

/// 物理接缝自己判定「这条 spec 不需要隧道」时，同样不占共享计数。
///
/// 两条「无隧道」路径都要各自判掉：没有跳板端口是**台账**能自己看出来的，
/// 而别的判定依据在**物理接缝**手里。两条都不得留下 entry。
#[test]
fn a_transport_declared_tunnel_free_spec_is_not_counted() {
    let mut harness = TunnelHarness::new();
    let spec_under_test = harness.spec("direct.local", 2200);
    harness.transport.without_tunnel(&spec_under_test);

    let lease = harness.lease_id("session-direct");
    let acquired = harness
        .ledger
        .acquire(Some(&spec_under_test), &lease)
        .expect("acquire");

    assert!(
        acquired.is_none(),
        "a tunnel-free spec yields no tunnel lease"
    );
    assert_eq!(
        harness.transport.consulted(),
        1,
        "the physical layer was consulted exactly once and reported `no tunnel`"
    );
    assert!(
        matches!(
            harness.transport.journal().first(),
            Some(TunnelEvent::OpenWithoutTunnel(spec)) if spec == &spec_under_test
        ),
        "the single consultation is an `OpenWithoutTunnel`, not a build"
    );
    assert_eq!(
        harness.transport.open_calls(),
        0,
        "no physical tunnel was built for a tunnel-free spec"
    );
    assert_eq!(
        harness.ledger.live_tunnels(),
        0,
        "`OpenWithoutTunnel` must not create a ledger entry"
    );
    assert!(harness.ledger.ref_count(&spec_under_test).is_none());

    // 没有 entry ⇒ 归还也必须是空操作：不凭空 close，也不报错。
    let released = harness.ledger.release(&spec_under_test);
    assert!(!released.closed);
    assert_eq!(released.refs, 0);
    assert_eq!(harness.transport.close_calls(), 0);
}

/// 共享键是 `TunnelSpec` 全等，**不是**路由版本。
///
/// 路由版本是 `PoolKeyGeneration` 的分量，由 `ResourceManager` 的陈旧检查把关；
/// 隧道台账**不得**把它当成共享键的一部分 —— 否则同一 `TunnelSpec` 在路由版本
/// 递增后会被拆成两条隧道，CM-32 的「同版本共用」就变味了。
#[test]
fn a_route_revision_bump_does_not_split_a_shared_tunnel() {
    let mut harness = TunnelHarness::new();
    let spec = harness.same_spec();
    let first = harness.lease_id("session-first");
    let second = harness.lease_id("session-second");

    harness
        .ledger
        .acquire(Some(&spec), &first)
        .expect("first acquire");
    harness.transport.set_revision(&harness.route_ref, 7);

    let lease = harness
        .ledger
        .acquire(Some(&spec), &second)
        .expect("the second holder shares the same spec regardless of the revision")
        .expect("a shared spec always yields a tunnel lease");
    assert!(
        !lease.opened,
        "the second holder must not build a second tunnel"
    );
    assert_eq!(lease.binding.ref_count, 2);
    assert_eq!(harness.transport.open_calls(), 1);
    assert_eq!(harness.transport.consulted(), 1);
    assert_eq!(
        harness.transport.journal().first().map(TunnelEvent::spec),
        Some(&spec),
        "the only physical event so far is the first holder's build of THIS spec"
    );
    assert_eq!(harness.transport.close_calls(), 0);

    assert!(!harness.ledger.release(&spec).closed);
    assert!(harness.ledger.release(&spec).closed);
    assert_eq!(harness.transport.close_calls(), 1);
}

/// 未登记的路由没有版本可报：必须是稳定的业务拒绝码，而不是 panic。
#[test]
fn an_unknown_route_has_no_revision_to_report() {
    let harness = TunnelHarness::new();
    let unknown = NetworkRouteRef::new("route-unknown");

    let error = TunnelTransport::revision(&*harness.transport, &unknown)
        .expect_err("an unregistered route carries no revision");
    assert_eq!(error.reason(), "tunnelRevisionUnavailable");
}

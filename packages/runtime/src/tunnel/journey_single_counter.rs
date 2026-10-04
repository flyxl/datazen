//! 唯一计数铁律的代数不变量。
//!
//! 端口契约要求（`network.rs:86`）：「引用计数归零才真正拆除；**重复释放是幂等的**」。
//! 本模块把这句话变成可执行的不变量：
//!
//! > N 次 `acquire`（同 spec）+ M 次 `release` ⇒
//! > * `transport.close` 被调用**恰好** `max(已归零的隧道数, 0)` 次；
//! > * 权威计数在任何时刻**不低于 0**；
//! > * 重复 `release` **不**触发第二次 `close`。
//!
//! 这条不变量是 CM-28「隧道不多减引用」与 CM-27「许可归零」的地基：
//! 一旦出现第二份账，两边各自看起来都对，但两条会**同时**失效。

use datazen_platform_api::ports::network::TunnelSpec;

use crate::connection::types::LeaseId;
use crate::tunnel::harness::TunnelHarness;
use crate::tunnel::TunnelState;

/// 同一条 spec 上 N 次 acquire + M 次 release 的完整代数。
#[test]
fn single_counter_algebra_holds() {
    for acquires in 1..=6usize {
        for releases in 1..=6usize {
            let mut harness = TunnelHarness::new();
            let spec = harness.same_spec();

            for index in 0..acquires {
                let dependent = harness.lease_id(&format!("holder-{index}"));
                harness
                    .ledger
                    .acquire(Some(&spec), &dependent)
                    .unwrap_or_else(|error| panic!("acquire {index}: {error}"));
                assert_eq!(
                    harness.ledger.ref_count(&spec),
                    Some(index as u32 + 1),
                    "acquires={acquires} releases={releases}: the count must rise by exactly one"
                );
            }
            assert_eq!(
                harness.transport.open_calls(),
                1,
                "acquires={acquires}: one spec ⇒ exactly one physical tunnel"
            );

            // M ≤ N。释放 M 次时，只有 M == N（计数真的归零）的那一次才关隧道；
            // M < N 时从头到尾一次都不能关 —— 这正是 CM-32 第一条断言的代数形式。
            for index in 0..releases {
                let released = harness.ledger.release(&spec);
                if index + 1 == acquires {
                    assert!(
                        released.closed,
                        "acquires={acquires} releases={releases}: the LAST release closes"
                    );
                    assert_eq!(released.refs, 0);
                    assert_eq!(released.state, TunnelState::Absent);
                    assert_eq!(
                        harness.transport.close_calls(),
                        1,
                        "acquires={acquires} releases={releases}: exactly ONE close"
                    );
                    assert_eq!(harness.ledger.live_tunnels(), 0);
                } else if index + 1 > acquires {
                    // 归还次数超过持有次数：**幂等**，不关第二次，也不把计数减成负数。
                    assert!(
                        !released.closed,
                        "acquires={acquires} releases={releases}: an extra release is idempotent"
                    );
                    assert_eq!(released.refs, 0);
                    assert_eq!(released.state, TunnelState::Absent);
                    assert_eq!(harness.transport.close_calls(), 1);
                } else {
                    assert!(
                        !released.closed,
                        "acquires={acquires} releases={releases}: release #{index} is not the last one"
                    );
                    assert_eq!(
                        released.refs,
                        (acquires - (index + 1)) as u32,
                        "acquires={acquires} releases={releases}: the authoritative count drops by one and never below zero"
                    );
                    assert_eq!(
                        harness.transport.close_calls(),
                        0,
                        "acquires={acquires} releases={releases}: nobody may be closed while a holder remains"
                    );
                    assert_eq!(harness.ledger.live_tunnels(), 1);
                }
            }

            if acquires > 0 && releases >= acquires {
                // 再多释放 3 次：幂等，**不**二次 close，计数**不为负**。
                for _ in 0..3 {
                    let repeat = harness.ledger.release(&spec);
                    assert!(!repeat.closed);
                    assert_eq!(repeat.refs, 0);
                    assert_eq!(repeat.state, TunnelState::Absent);
                }
                assert_eq!(
                    harness.transport.close_calls(),
                    1,
                    "acquires={acquires}: repeated release must NOT close a second time"
                );
                assert_eq!(
                    harness.ledger.close_calls(),
                    1,
                    "the ledger's own close tally agrees with the transport journal"
                );
            } else {
                // 还没归零就收工：隧道必须仍然开着，一分账都没少。
                assert_eq!(
                    harness.ledger.ref_count(&spec),
                    Some(acquires as u32 - releases as u32)
                );
                assert_eq!(harness.transport.close_calls(), 0);
                assert!(harness.transport.is_open(&spec));
            }
        }
    }
}

/// 从来没有持有者却释放：不许凭空关掉一条隧道（计数不得凭空从 0 掉到 −1）。
#[test]
fn releasing_a_tunnel_nobody_ever_acquired_closes_nothing() {
    let mut harness = TunnelHarness::new();
    let spec = harness.same_spec();

    for _ in 0..3 {
        let released = harness.ledger.release(&spec);
        assert!(!released.closed);
        assert_eq!(
            released.refs, 0,
            "the count stays at zero instead of going negative"
        );
        assert_eq!(released.state, TunnelState::Absent);
    }
    assert_eq!(harness.transport.close_calls(), 0);
    assert_eq!(harness.transport.journal().len(), 0);
}

/// 三条 spec 同时在册：每条独立计数，关闭一条**不得**波及另外两条。
#[test]
fn independent_specs_never_share_a_count() {
    let mut harness = TunnelHarness::new();
    let specs = [
        harness.spec("jump-a.internal", 1001),
        harness.spec("jump-b.internal", 1002),
        harness.spec("jump-c.internal", 1003),
    ];

    // 各条 spec 的引用数刻意不同（3 / 2 / 1），用来证明计数没有串台。
    // 每份引用属于**不同**的持有方：一条资源对同一条隧道只持一份，
    // 否则一次归还只减 1，多出来的那份永远没人还。
    let mut holders: Vec<(TunnelSpec, LeaseId)> = Vec::new();
    for (round, spec) in specs.iter().enumerate() {
        for nth in 0..(3 - round) {
            let holder = harness.lease_id(&format!("holder-{round}-{nth}"));
            harness
                .ledger
                .acquire(Some(spec), &holder)
                .unwrap_or_else(|error| panic!("acquire {spec:?}: {error}"));
            holders.push((spec.clone(), holder));
        }
    }
    assert_eq!(harness.ledger.ref_count(&specs[0]), Some(3));
    assert_eq!(harness.ledger.ref_count(&specs[1]), Some(2));
    assert_eq!(harness.ledger.ref_count(&specs[2]), Some(1));
    assert_eq!(harness.transport.open_calls(), 3);

    // 先把只有一个引用的那条归还掉 —— 走**资源归还**入口，另一条不受影响。
    let last_of_third = holders.pop().expect("a holder on the third spec");
    assert_eq!(last_of_third.0, specs[2]);
    assert!(
        harness
            .ledger
            .return_resource(&last_of_third.1)
            .expect("held")
            .closed
    );
    assert_eq!(harness.transport.close_calls(), 1);
    assert_eq!(harness.ledger.ref_count(&specs[0]), Some(3));
    assert_eq!(harness.ledger.ref_count(&specs[1]), Some(2));
    assert_eq!(harness.ledger.live_tunnels(), 2);

    // 另外两条要归零，得各自被归还到零。归还顺序交叉，计数仍不串台：
    // 每条 spec 只减**自己**的计数，且只有自己归零时才关闭。
    let mut remaining = [3u32, 2u32];
    for (spec, holder) in holders.into_iter().rev() {
        let slot = usize::from(spec != specs[0]);
        remaining[slot] -= 1;
        let release = harness.ledger.return_resource(&holder).expect("held");
        assert_eq!(
            release.refs, remaining[slot],
            "{spec:?} drains one holder at a time, and only its own count"
        );
        assert_eq!(
            release.closed,
            remaining[slot] == 0,
            "{spec:?} closes exactly when its own count reaches zero"
        );
    }
    assert_eq!(harness.transport.close_calls(), 3);
    assert_eq!(harness.ledger.live_tunnels(), 0);
    assert!(!harness.transport.is_open(&specs[0]));
    assert!(!harness.transport.is_open(&specs[1]));
}

/// `TunnelBinding.ref_count` 只是**观测快照**，绝不参与释放判断。
///
/// 把观测值当成权威计数，正是 CM-28「隧道不多减引用」会悄悄失效的那一步：
/// 一旦有人按 `binding.ref_count` 判定能否释放，多一次或少一次 acquire
/// 就会永久性地把账带歪。
///
/// **判别性要求（CM-32 repair round 1）**：断言点上必须让「台账读到的数」与
/// 「快照里的数」**不相等**，否则本用例杀不掉「把权威读换成冻结快照」这个变异
/// —— 两个来源都等于 1 时，任何 `assert_eq!(…, Some(1))` 都同时接受两者。
/// 因此这里做三次 acquire，并把这一条写成 `assert_ne!` 自证。
#[test]
fn the_binding_snapshot_never_decides_whether_to_release() {
    let mut harness = TunnelHarness::new();
    let spec = harness.same_spec();
    let first = harness.lease_id("session-first");
    let second = harness.lease_id("session-second");
    let third = harness.lease_id("session-third");

    let first_lease = harness
        .ledger
        .acquire(Some(&spec), &first)
        .expect("first acquire")
        .expect("tunnel lease");
    let second_lease = harness
        .ledger
        .acquire(Some(&spec), &second)
        .expect("second acquire")
        .expect("tunnel lease");

    // 快照确实在跟随计数（可观测），但…
    assert_eq!(first_lease.binding.ref_count, 1);
    assert_eq!(second_lease.binding.ref_count, 2);

    // …按快照值决定释放是错的：拿着**第一份**句柄（ref_count=1）的那一方
    // 仍然只是一次引用，把它当成「这是最后一份」会立刻拆掉别人的隧道。
    //
    // 第三次 acquire 是**判别性设计**，不是凑数：它把台账的权威计数推到 3，
    // 而 `first_lease.binding.ref_count` 永远冻结在 1。若只做两次 acquire，
    // 释放一次后两个来源恰好都等于 1，下面的断言无法区分读的是哪一份 ——
    // 把权威读换成冻结快照照样全绿，这条用例就什么都没钉住。
    let third_lease = harness
        .ledger
        .acquire(Some(&spec), &third)
        .expect("third acquire")
        .expect("tunnel lease");
    assert!(
        !third_lease.opened,
        "the third session joins the open tunnel"
    );
    assert_eq!(third_lease.binding.ref_count, 3);

    let released = harness.ledger.release(&spec);
    assert!(
        !released.closed,
        "a stale snapshot must not close the tunnel"
    );
    assert_eq!(
        released.refs, 2,
        "one reference left two, so the tunnel must stay up"
    );
    assert_eq!(
        second_lease.binding.ref_count, 2,
        "the snapshot handed to a holder is frozen at acquire time"
    );

    // 先钉住两个来源在这一点上**确实不同**——否则后面的断言无判别力。
    assert_ne!(
        harness.ledger.ref_count(&spec),
        Some(first_lease.binding.ref_count),
        "the ledger must read a DIFFERENT number than the frozen snapshot here, \
         otherwise this test cannot tell the two sources apart"
    );
    assert_eq!(
        harness.ledger.ref_count(&spec),
        Some(2),
        "the AUTHORITATIVE count, not the snapshot, decides"
    );
    assert_eq!(
        harness.ledger.ref_count(&spec),
        Some(first_lease.binding.ref_count + 1),
        "the ledger counts every holder; the snapshot counts only what its owner saw"
    );
}

/// 台账必须能对上端口契约里「同一 `TunnelSpec` 共享引用计数」的语义，
/// 且 `TunnelSpec` 的全等性就是共享身份 —— 这两条都直接读冻结上游的类型。
#[test]
fn the_sharing_identity_is_the_frozen_port_spec() {
    let harness = TunnelHarness::new();
    let spec = harness.same_spec();
    let rebuilt = datazen_platform_api::ports::network::TunnelSpec::new(
        harness.route_ref.clone(),
        "jump.internal".to_string(),
        2222,
    );

    assert_eq!(
        spec, rebuilt,
        "two independently built specs that compare equal MUST share one tunnel"
    );
    assert_ne!(
        spec,
        harness.spec("jump.internal", 2223),
        "the jump port is part of the sharing identity"
    );
}

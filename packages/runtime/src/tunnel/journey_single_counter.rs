//! 唯一计数铁律的代数不变量。
//!
//! 端口契约要求：「引用计数归零才真正拆除；**重复释放是幂等的**」。
//! 本模块把这句话变成可执行的不变量：
//!
//! > N 次 `acquire`（同 spec）+ M 次 `release` ⇒
//! > * `transport.close` 被调用**恰好** `max(已归零的隧道数, 0)` 次；
//! > * 权威计数在任何时刻**不低于 0**；
//! > * 重复 `release` **不**触发第二次 `close`。
//!
//! 这条不变量是 CM-28「隧道不多减引用」与 CM-27「许可归零」的地基：
//! 一旦出现第二份账，两边各自看起来都对，但两条会**同时**失效。
//!
//! 本模块只跑**代数**这一半。铁律本身的机械闸门（源码结构审计 + 植入变异 kill test）
//! 住在 `tunnel::single_counter_audit`，那里管的是「不许长出第二本账」这个结构问题；
//! 这里管的是「已有的那一份账算得对不对」。

use datazen_platform_api::ports::network::TunnelSpec;

use crate::connection::types::LeaseId;
use crate::tunnel::harness::{TunnelEvent, TunnelHarness};
use crate::tunnel::{TunnelState, TunnelTransport};

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

/// 端口读数**必须**是同一份折法的投影（CM-32-FU1 的运行时不变量那一半）。
///
/// 源码审计（`tunnel::single_counter_audit`）管结构，本条管值：沿一整条旅程，每一步都
/// 用**最原始**的事件计数（对 journal 现场 `filter`+`count`）与端口四个读数逐一对账。
/// 谁往端口里塞一份会漂移的自存账 —— 漏计、重计、把 `OpenWithoutTunnel` 也算进 `opened`
/// —— 都会在**某一步**上对不上，而不是恰好对上。
///
/// 序列里刻意含：三次 acquire（只开一条）、一次带故障 close、`Closing` 期间的幂等释放、
/// 一条不经隧道的 spec、以及最后一条干净归零。
#[test]
fn every_port_reading_is_the_projection_of_one_journal_fold() {
    let mut harness = TunnelHarness::new();
    let spec = harness.same_spec();
    let direct = harness.spec("direct.local", 2200);
    harness.transport.without_tunnel(&direct);
    let holders: Vec<LeaseId> = (0..3)
        .map(|index| harness.lease_id(&format!("holder-{index}")))
        .collect();

    // 「原始账」= 现场从 journal 数出来，**不借端口任何读数**。
    // 逐 spec 那一格用来对照 `is_open`：全局开合数与单条 spec 的开合数是两回事，
    // 只对照全局会把「把两者混为一谈」的伪装放过去。
    let raw = |harness: &TunnelHarness| -> (usize, usize, usize, usize, usize, usize) {
        let journal = harness.transport.journal();
        let opened = journal
            .iter()
            .filter(|event| matches!(event, TunnelEvent::Open(_)))
            .count();
        let closed = journal
            .iter()
            .filter(|event| matches!(event, TunnelEvent::Close(_)))
            .count();
        let without = journal
            .iter()
            .filter(|event| matches!(event, TunnelEvent::OpenWithoutTunnel(_)))
            .count();
        // 逐 spec 的开与合：全局数与单条数不是一回事，只对照全局会把
        // 「把两者混为一谈」的伪装放过去。
        let spec_open = journal
            .iter()
            .filter(|event| matches!(event, TunnelEvent::Open(s) if *s == spec))
            .count();
        let spec_close = journal
            .iter()
            .filter(|event| matches!(event, TunnelEvent::Close(s) if *s == spec))
            .count();
        (
            opened,
            closed,
            journal.len(),
            without,
            spec_open,
            spec_close,
        )
    };
    let assert_projection = |harness: &TunnelHarness, step: &str| {
        let (opened, closed, consulted, without, spec_open, spec_close) = raw(harness);
        assert_eq!(harness.transport.open_calls(), opened, "{step}: open 读数");
        assert_eq!(
            harness.transport.close_calls(),
            closed,
            "{step}: close 读数"
        );
        assert_eq!(harness.transport.consulted(), consulted, "{step}: 触碰总数");
        assert_eq!(
            consulted,
            opened + closed + without,
            "{step}: 「问过但没建隧道」必须自成一格，不能被并进 open 或凭空消失"
        );
        assert_eq!(
            harness.transport.is_open(&spec),
            spec_open > spec_close,
            "{step}: is_open 的唯一依据是该 spec 自己的开合之差"
        );
    };

    assert_projection(&harness, "刚建好夹具");
    for holder in &holders {
        harness
            .ledger
            .acquire(Some(&spec), holder)
            .expect("a healthy transport opens");
        assert_projection(&harness, "每次 acquire 之后");
    }
    assert_eq!(harness.transport.open_calls(), 1, "三条引用只开一条隧道");

    // 不经隧道的 spec：台账问了，但没建 —— 这一格必须单独看见。
    assert!(harness
        .ledger
        .acquire(Some(&direct), &harness.lease_id("holder-direct"))
        .expect("consultation succeeds")
        .is_none());
    assert_projection(&harness, "问过一条不经隧道的 spec 之后");
    assert_eq!(harness.transport.consulted(), 2);
    assert_eq!(harness.transport.open_calls(), 1);

    // 先把引用还到只剩一份，再让最后一次 close 失败。
    harness.ledger.release(&spec);
    harness.ledger.return_resource(&holders[1]);
    assert_projection(&harness, "前两份归还之后");
    harness.transport.fail_close(&spec);
    let unconfirmed = harness.ledger.release(&spec);
    assert!(!unconfirmed.closed);
    assert_eq!(unconfirmed.state, TunnelState::Unconfirmed);
    assert_projection(&harness, "close 失败之后");

    // `Unconfirmed` 期间的重复释放：幂等、不隐式重试，读数一动不动。
    let before = raw(&harness);
    for _ in 0..3 {
        harness.ledger.release(&spec);
    }
    assert_projection(&harness, "Unconfirmed 期间重复释放之后");
    assert_eq!(raw(&harness), before, "结果不明期间不得隐式重试拆除");

    // 干净地再来一条 spec 并归零：读数继续跟随同一个折法。
    let other = harness.spec("jump-b.internal", 3333);
    let lone = harness.lease_id("holder-lone");
    harness
        .ledger
        .acquire(Some(&other), &lone)
        .expect("a second spec opens its own tunnel");
    assert_projection(&harness, "第二条 spec 建立之后");
    assert!(harness.ledger.return_resource(&lone).expect("held").closed);
    assert_projection(&harness, "第二条 spec 归零关闭之后");
    // 端口被**触碰**过 4 次、只成功**开出** 2 条、只**拆掉** 1 条：三个数互不相等，
    // 一份把三者混为一谈的自存账会在这里露出来。
    assert_eq!(harness.transport.consulted(), 4);
    assert_eq!(harness.transport.open_calls(), 2);
    assert_eq!(harness.transport.close_calls(), 1);
}

/// 台账与端口是**两本职责不同的账**，本条把它们对到一起（CM-32-FU1）。
///
/// * `transport` 那本记**物理事实**：journal 折出来的 `close` 次数。
/// * `ledger.close_calls()` 那本记**台账自己发了多少次 close**（`teardown_calls` 字段）。
///
/// 两条归还入口（`release` / `return_resource`）共用同一个 `drain`，所以二者在**任意**
/// 交错下都必须相等。测试**直接经 trait 调 `close`**、绕过台账，制造出
/// 「物理关过、台账没记」的分叉，再断言台账读数**不**跟随 —— 这一笔正是
/// 「把端口那份账当成台账那份账」的伪装会在哪里露馅，也是下面那句
/// 「相等必须来自构造，不来自巧合」的实证。
#[test]
fn the_ledger_and_the_port_tallies_agree_only_because_both_count_the_same_drain() {
    let mut harness = TunnelHarness::new();
    let spec = harness.same_spec();
    let holders: Vec<LeaseId> = (0..3)
        .map(|index| harness.lease_id(&format!("holder-{index}")))
        .collect();

    for holder in &holders {
        harness
            .ledger
            .acquire(Some(&spec), holder)
            .expect("a healthy transport opens");
    }
    assert_eq!(harness.transport.close_calls(), 0);
    assert_eq!(harness.ledger.close_calls(), 0);

    // 前两份引用归还：一本都没数到 close。
    harness.ledger.release(&spec);
    harness.ledger.return_resource(&holders[1]);
    assert_eq!(harness.transport.close_calls(), 0);
    assert_eq!(harness.ledger.close_calls(), 0);

    // 最后一份 ⇒ 归零 ⇒ 恰好一次：两本账同时走到 1。
    harness.ledger.return_resource(&holders[2]);
    assert_eq!(harness.transport.close_calls(), 1);
    assert_eq!(harness.ledger.close_calls(), 1);

    // 多余释放：两本账都纹丝不动（幂等）。
    for _ in 0..3 {
        harness.ledger.release(&spec);
    }
    assert_eq!(harness.transport.close_calls(), 1);
    assert_eq!(harness.ledger.close_calls(), 1);

    // 同一台账上再开一条、再归零：两本账一起走到 2。
    let other = harness.spec("jump-b.internal", 3333);
    let lone = harness.lease_id("holder-lone");
    harness.ledger.acquire(Some(&other), &lone).expect("second");
    harness.ledger.return_resource(&lone).expect("held");
    assert_eq!(harness.transport.close_calls(), 2);
    assert_eq!(harness.ledger.close_calls(), 2);

    // ★ 绕过台账，直接从物理接缝发一次 close：只有端口那本跟着走。
    //   这正是「第二本账伪装成第一本账」会露馅的地方 —— 若有人拿端口账去顶替台账账，
    //   这条断言当场转红。
    TunnelTransport::close(&*harness.transport, &spec).expect("direct close on the seam");
    assert_eq!(
        harness.transport.close_calls(),
        3,
        "端口记的是物理事实：多关了一次就得多数一次"
    );
    assert_eq!(
        harness.ledger.close_calls(),
        2,
        "台账只数**自己**经唯一归零路径发出的 close：它没发过第三次，就绝不数到 3"
    );
}

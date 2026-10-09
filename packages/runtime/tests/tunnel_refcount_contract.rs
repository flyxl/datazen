//! 接缝契约：`NetworkProvider` 的隧道部分在**进程外**如何成立。
//!
//! 这个二进制**不在 `packages/runtime` 内部**，因此它同时是两件事：
//!
//! 1. **接缝可实现性证明**：桌面 `NetworkProvider`（`src-tauri`）将来要在
//!    **另一个 crate** 里实现 `TunnelTransport`；这里用一份独立录写端口证明
//!    公共 API 足够 —— 包括 `TunnelHandle` 可以由宿主侧铸造。
//!    桌面实现被划到接缝期，所以这份可实现性结论必须先有测试兜住。
//! 2. **三条断言的接缝形状**：与 `src/tunnel/journey_*.rs` 的库内单测
//!    对照——那边证明台账内部自洽，这里证明**同一套结论在公开 API 上成立**。
//!
//! 所有断言读的是录写端口的**事件计数**（`open_calls` / `close_calls` /
//! `journal.len()`）与台账的权威计数快照，不依赖 `is_ok()` 之类的存在性判断。
//!
//! # 端口不许自带账
//!
//! 本文件的 `HostTunnelTransport` 与 `src/tunnel/harness.rs` 的夹具端口同形：
//! 只有事件日志这一份事实，所有读数由**唯一**的纯折函数 `tally` 现折。
//! 「唯一计数铁律」如今由 `src/tunnel/single_counter_audit/` 机械保证
//! （随 `--lib` 跑，因此真的进 CI）：
//! 那份闸门对本文件与 `cm28_concurrent_tunnel.rs` 的端口同样做字段审计 + 投影审计，
//! 并把原始反例作为植入变异喂给它自己做 kill test。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use datazen_platform_api::id::{NetworkRouteRef, NetworkRouteRevision};
use datazen_platform_api::ports::network::{TunnelBinding, TunnelSpec};
use datazen_runtime::connection::types::LeaseId;
use datazen_runtime::tunnel::{
    TunnelError, TunnelFault, TunnelHandle, TunnelLedger, TunnelState, TunnelTransport,
};

/// 宿主侧录写隧道端口。**刻意不持有任何引用计数，也不持有自存的开合计数** ——
/// 唯一计数在台账里，读数一律由 [`tally`] 从事件日志现折。
///
/// `transport.rs` 模块头登记过那个已实证的伪装（给端口加一份
/// `close_tally: Mutex<usize>` 并让观测方法改读它）。现在这条路被
/// `tunnel::single_counter_audit` 机械杀掉（字段审计 + 投影审计，各带 kill test）。
struct HostTunnelTransport {
    events: Mutex<Vec<&'static str>>,
    revisions: Mutex<BTreeMap<String, u64>>,
    fail_open: bool,
    fail_close: bool,
}

/// 端口账：物理开合读数，**只**由这一个纯折函数从事件日志算出。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PortTally {
    opened: usize,
    closed: usize,
}

/// 「事件 → 账」的唯一折法。同 `src/tunnel/harness.rs` 的 `tallies` 一个形状；
/// 本文件是外部 crate，看不见那个 `#[cfg(test)]` 私有模块，所以照同一规则就地写一份
/// —— 审计按「每份实现各自只有一个折函数」检查，不跨文件共享。
fn tally(events: &[&'static str]) -> PortTally {
    let mut out = PortTally::default();
    for event in events {
        match *event {
            "open" => out.opened += 1,
            "close" => out.closed += 1,
            _ => {}
        }
    }
    out
}

impl HostTunnelTransport {
    fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
            revisions: Mutex::new(BTreeMap::new()),
            fail_open: false,
            fail_close: false,
        }
    }

    fn record(&self, event: &'static str) {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(event);
    }

    fn set_revision(&self, route_ref: &NetworkRouteRef, value: u64) {
        self.revisions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(route_ref.as_str().to_string(), value);
    }

    /// 宿主记得的物理开合次数。**这是接缝上唯一的「有没有真的关掉」证据**，
    /// 且它只是 [`tally`] 的投影。
    fn opened(&self) -> usize {
        tally(&self.events()).opened
    }

    fn closed(&self) -> usize {
        tally(&self.events()).closed
    }

    /// 唯一事实源：事件日志本身。
    fn events(&self) -> Vec<&'static str> {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn is_open(&self) -> bool {
        let counts = tally(&self.events());
        counts.opened > counts.closed
    }
}

impl TunnelTransport for HostTunnelTransport {
    fn open(&self, spec: &TunnelSpec) -> Result<Option<TunnelHandle>, TunnelError> {
        if self.fail_open {
            return Err(TunnelError::Transport {
                spec: spec.clone(),
                fault: TunnelFault::Open,
            });
        }
        self.record("open");
        Ok(Some(TunnelHandle::new()))
    }

    fn close(&self, spec: &TunnelSpec) -> Result<(), TunnelError> {
        if self.fail_close {
            return Err(TunnelError::Transport {
                spec: spec.clone(),
                fault: TunnelFault::Close,
            });
        }
        self.record("close");
        Ok(())
    }

    fn revision(&self, route_ref: &NetworkRouteRef) -> Result<NetworkRouteRevision, TunnelError> {
        self.revisions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(route_ref.as_str())
            .copied()
            .map(NetworkRouteRevision::new)
            .ok_or(TunnelError::RevisionUnavailable)
    }
}

fn spec_for(route_ref: &NetworkRouteRef, host: &str, port: u16) -> TunnelSpec {
    TunnelSpec::new(route_ref.clone(), host.to_string(), port)
}

#[test]
fn the_seam_is_implementable_outside_the_runtime_crate() {
    let transport = Arc::new(HostTunnelTransport::new());
    let route = NetworkRouteRef::new("route://bastion/prod");
    transport.set_revision(&route, 3);
    let mut ledger = TunnelLedger::new(transport.clone());
    let spec = spec_for(&route, "jump.internal", 22);
    let lease = LeaseId::new("host-lease");

    // 宿主能铸造 handle、问出路由版本、把台账驱动起来 —— 全部只用公开 API。
    let binding = ledger
        .acquire(Some(&spec), &lease)
        .expect("acquire is infallible on a healthy transport")
        .expect("a tunnel is needed for this spec");
    assert!(binding.opened, "the very first holder opens the tunnel");
    assert!(
        binding.tunnel_handle.is_some(),
        "the host can mint a handle"
    );
    assert_eq!(binding.binding.spec, spec);
    assert_eq!(binding.binding.ref_count, 1);
    assert_eq!(transport.opened(), 1);

    assert_eq!(
        transport.revision(&route).expect("known route"),
        NetworkRouteRevision::new(3),
        "the host can still read the route revision through the seam"
    );
    let unknown = NetworkRouteRef::new("route://bastion/unknown");
    let unavailable = transport.revision(&unknown).expect_err("unknown route");
    assert!(
        matches!(unavailable, TunnelError::RevisionUnavailable),
        "an unknown route has no revision to report"
    );
    assert_eq!(unavailable.reason(), "tunnelRevisionUnavailable");

    assert!(ledger.return_resource(&lease).expect("held").closed);
    assert_eq!(transport.closed(), 1);
}

/// 断言一：关掉第一个 session 时，**隧道不关**。
#[test]
fn closing_the_first_holder_leaves_the_tunnel_running_for_the_second() {
    let transport = Arc::new(HostTunnelTransport::new());
    let route = NetworkRouteRef::new("route://bastion/prod");
    let mut ledger = TunnelLedger::new(transport.clone());
    let spec = spec_for(&route, "jump.internal", 22);
    let first = LeaseId::new("session-first");
    let second = LeaseId::new("session-second");

    let a = ledger
        .acquire(Some(&spec), &first)
        .expect("first")
        .expect("tunnel");
    let b = ledger
        .acquire(Some(&spec), &second)
        .expect("second")
        .expect("tunnel");
    assert!(a.opened && !b.opened, "only the first holder opens");
    assert_eq!(
        b.binding.ref_count, 2,
        "the binding exposes the count for observation"
    );
    assert_eq!(
        transport.opened(),
        1,
        "two sessions share ONE physical tunnel"
    );

    // 关掉第一个 session。
    let release = ledger.return_resource(&first).expect("held");
    assert!(!release.closed, "the second session is still running");
    assert_eq!(release.refs, 1);
    assert_eq!(
        transport.closed(),
        0,
        "the host must not have torn the tunnel down"
    );
    assert!(transport.is_open(), "the tunnel is still up");
    assert_eq!(ledger.ref_count(&spec), Some(1));

    // 第二个 session 结束 ⇒ 归零 ⇒ 关闭，且恰好一次。
    let release = ledger.return_resource(&second).expect("held");
    assert!(release.closed);
    assert_eq!(release.refs, 0);
    assert_eq!(release.state, TunnelState::Absent);
    assert_eq!(transport.closed(), 1, "exactly one teardown");
    assert!(!transport.is_open());
    assert_eq!(ledger.live_tunnels(), 0);
}

/// 断言二：只有**最后一个**引用释放才关闭。
#[test]
fn only_the_last_reference_releases_the_tunnel() {
    let transport = Arc::new(HostTunnelTransport::new());
    let route = NetworkRouteRef::new("route://bastion/prod");
    let mut ledger = TunnelLedger::new(transport.clone());
    let spec = spec_for(&route, "jump.internal", 22);
    let holders: Vec<LeaseId> = (0..4)
        .map(|index| LeaseId::new(format!("job-{index}")))
        .collect();

    for holder in &holders {
        ledger
            .acquire(Some(&spec), holder)
            .expect("acquire")
            .expect("tunnel");
    }
    assert_eq!(transport.opened(), 1, "four jobs, one tunnel");

    // 逐个结束：前三个人结束时隧道必须纹丝不动。
    for holder in &holders[..3] {
        let release = ledger.return_resource(holder).expect("held");
        assert!(!release.closed, "a remaining job forbids the teardown");
        assert_eq!(transport.closed(), 0);
        assert!(transport.is_open());
    }
    assert_eq!(ledger.ref_count(&spec), Some(1));

    let release = ledger.return_resource(&holders[3]).expect("held");
    assert!(release.closed, "the last reference releases the tunnel");
    assert_eq!(transport.closed(), 1);
    assert_eq!(ledger.live_tunnels(), 0);
}

/// 断言三：隧道失败传播给**依赖资源**，且**一条**都不误伤。
#[test]
fn a_tunnel_failure_reaches_every_dependent() {
    let transport = Arc::new(HostTunnelTransport::new());
    let route = NetworkRouteRef::new("route://bastion/prod");
    let mut ledger = TunnelLedger::new(transport.clone());
    let spec = spec_for(&route, "jump.internal", 22);
    let healthy = spec_for(&route, "other.internal", 2222);
    let dependents: Vec<LeaseId> = (0..3)
        .map(|index| LeaseId::new(format!("session-{index}")))
        .collect();

    for dependent in &dependents {
        ledger
            .acquire(Some(&spec), dependent)
            .expect("acquire")
            .expect("tunnel");
    }
    ledger
        .acquire(Some(&healthy), &LeaseId::new("healthy-session"))
        .expect("healthy")
        .expect("tunnel");
    assert_eq!(transport.opened(), 2);

    let affected = ledger.report_failure(&spec);
    assert_eq!(
        affected.len(),
        dependents.len(),
        "every dependent resource learns about the failure"
    );
    for dependent in &dependents {
        assert!(
            affected.contains(dependent),
            "{dependent:?} is told about the dead tunnel"
        );
    }
    assert_eq!(ledger.state(&spec), TunnelState::Failed);
    assert_eq!(
        ledger.ref_count(&spec),
        Some(3),
        "a failure decrements nothing — each holder still releases its own reference"
    );
    assert_eq!(
        ledger.state(&healthy),
        TunnelState::Live,
        "an unrelated tunnel is not dragged down with the dead one"
    );
    assert_eq!(transport.closed(), 0);

    // 失败的隧道不再接纳新引用，但**旧持有者仍能归还**。
    let latecomer = LeaseId::new("session-late");
    let refused = ledger
        .acquire(Some(&spec), &latecomer)
        .expect_err("a dead tunnel refuses new references");
    assert!(matches!(refused, TunnelError::NotShareable { .. }));
    for dependent in &dependents {
        assert!(
            ledger.return_resource(dependent).is_some(),
            "{dependent:?} still returns"
        );
    }
    assert_eq!(
        transport.closed(),
        1,
        "the dead tunnel is torn down once its holders are gone"
    );
}

/// 打开就失败：**不留**台账条目，也就不存在任何后续释放的义务。
#[test]
fn a_failed_open_leaves_nothing_behind() {
    let transport = Arc::new(HostTunnelTransport {
        fail_open: true,
        ..HostTunnelTransport::new()
    });
    let route = NetworkRouteRef::new("route://bastion/prod");
    let mut ledger = TunnelLedger::new(transport.clone());
    let spec = spec_for(&route, "jump.internal", 22);
    let lease = LeaseId::new("session-first");

    let error = ledger
        .acquire(Some(&spec), &lease)
        .expect_err("the open failed");
    assert!(matches!(
        error,
        TunnelError::Transport {
            fault: TunnelFault::Open,
            ..
        }
    ));
    assert_eq!(error.reason(), "tunnelTransportOpenFailed");
    assert_eq!(ledger.live_tunnels(), 0);
    assert_eq!(ledger.ref_count(&spec), None);
    assert_eq!(transport.opened(), 0);

    // 没有条目可还，也**不该**凭空去关一条从未开起来的隧道。
    assert!(ledger.return_resource(&lease).is_none());
    assert_eq!(transport.closed(), 0);
}

/// 唯一计数铁律在**资源归还入口**上的代数：N 次归还 ⇒ 恰好 N−1 次关闭前的沉默，
/// 多余的归还既不多减、也不二次关闭。
#[test]
fn one_return_releases_exactly_once_however_many_times_it_is_repeated() {
    for returns in 1..=6usize {
        let transport = Arc::new(HostTunnelTransport::new());
        let route = NetworkRouteRef::new("route://bastion/prod");
        let mut ledger = TunnelLedger::new(transport.clone());
        let spec = spec_for(&route, "jump.internal", 22);
        let holders: Vec<LeaseId> = (0..returns)
            .map(|index| LeaseId::new(format!("job-{index}")))
            .collect();

        for holder in &holders {
            ledger
                .acquire(Some(&spec), holder)
                .expect("acquire")
                .expect("tunnel");
        }
        assert_eq!(transport.opened(), 1);

        let mut closes = 0usize;
        let mut remaining = returns;
        for holder in &holders {
            let release = ledger.return_resource(holder).expect("held");
            remaining -= 1;
            assert_eq!(
                release.refs as usize, remaining,
                "returns={returns}: each return releases exactly one reference"
            );
            assert_eq!(release.closed, remaining == 0);
            closes += usize::from(release.closed);
            // 重复归还同一条资源是**空操作**，绝不二次释放。
            assert!(
                ledger.return_resource(holder).is_none(),
                "returns={returns}: a repeated return releases nothing"
            );
        }
        // 多归还 3 次：幂等。
        for _ in 0..3 {
            assert!(ledger.return_resource(&holders[0]).is_none());
        }

        assert_eq!(closes, 1, "returns={returns}: exactly one teardown");
        assert_eq!(
            transport.closed(),
            1,
            "returns={returns}: never closed twice"
        );
        assert_eq!(ledger.live_tunnels(), 0);
        assert!(ledger.ref_count(&spec).is_none());
    }
}

/// `TunnelBinding.ref_count` 是观测快照：它与台账一致，且**不**参与释放判断。
///
/// 端口文档 `network.rs` 写死了这一点（「仅供观测，不用于判断能否释放」）。
/// 若哪天有人按快照值决定能否释放，多一次或少一次 acquire 就会把账永久带歪。
///
/// **判别性要求**：断言点上「台账读到的数」必须与
/// 「快照里的数」**不相等**（`assert_ne!` 自证），否则本用例杀不掉
/// 「把权威读换成冻结快照」这个变异。
#[test]
fn the_binding_snapshot_never_decides_whether_to_release() {
    let transport = Arc::new(HostTunnelTransport::new());
    let route = NetworkRouteRef::new("route://bastion/prod");
    let mut ledger = TunnelLedger::new(transport.clone());
    let spec = spec_for(&route, "jump.internal", 22);
    let first = LeaseId::new("session-first");
    let second = LeaseId::new("session-second");
    let third = LeaseId::new("session-third");

    let first_binding: TunnelBinding = ledger
        .acquire(Some(&spec), &first)
        .expect("first")
        .expect("tunnel")
        .binding;
    let second_lease = ledger
        .acquire(Some(&spec), &second)
        .expect("second")
        .expect("tunnel")
        .binding;
    // 第三次 acquire 是判别性设计：台账走到 3，而 `first_binding` 冻结在 1。
    // 只有两者不相等，下面的 `ref_count` 断言才能区分读的是哪一份 ——
    // 两个来源都是 1 时，换成冻结快照同样全绿。
    let _third_binding: TunnelBinding = ledger
        .acquire(Some(&spec), &third)
        .expect("third")
        .expect("tunnel")
        .binding;
    assert_eq!(first_binding.ref_count, 1);
    assert_eq!(second_lease.ref_count, 2);

    // 快照值停在 2，调用方随便保留它；归还按**台账**走，与快照无关。
    assert!(!ledger.return_resource(&first).expect("held").closed);
    assert_eq!(transport.closed(), 0, "the snapshot is not consulted");
    assert_eq!(
        second_lease.ref_count, 2,
        "a snapshot never moves by itself"
    );
    assert_ne!(
        ledger.ref_count(&spec),
        Some(first_binding.ref_count),
        "the ledger and the frozen snapshot must DISAGREE here, \
         otherwise this assertion cannot tell the two sources apart"
    );
    assert_eq!(ledger.ref_count(&spec), Some(2));
    assert_eq!(ledger.ref_count(&spec), Some(first_binding.ref_count + 1));

    assert!(!ledger.return_resource(&second).expect("held").closed);
    assert_eq!(
        transport.closed(),
        0,
        "two references still hold the tunnel"
    );

    assert!(ledger.return_resource(&third).expect("held").closed);
    assert_eq!(
        transport.closed(),
        1,
        "the last reference closes exactly once"
    );
}

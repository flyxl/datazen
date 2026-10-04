//! CM-28（重复 release/close，高）第三格：**隧道**层的真实并发契约。
//!
//! 判据原文（`docs/architecture/platform/connection-management.md` §16，CM-28，行 1033-1037）：
//!
//! ```text
//! - 断言：driver close 至多一次有效关闭；预算不负数；隧道不多减引用；重复响应一致。
//! ```
//!
//! 物理那两格在同目录的 `cm28_concurrent_release.rs`。
//!
//! # 为什么这一格也要新写
//!
//! `tests/tunnel_refcount_contract.rs:352`
//! （`one_return_releases_exactly_once_however_many_times_it_is_repeated`）
//! 用一个**顺序** `for returns in 1..=6` 循环重复归还，而且数的是隧道端口的
//! `close`，不是权威计数。`src/tunnel/journey_single_counter.rs` 的
//! `single_counter_algebra_holds` 则是嵌套 `for` 的纯代数，没有线程。
//! 「隧道不多减引用」在**真并发**下才有的两种失败 —— 丢更新（20 减成 19）
//! 与多减（20-2）—— 两者都还没有任何测试能看见。
//!
//! # 并发形状
//!
//! `TunnelLedger` 拿 `&mut self`，因此必须放在 `Mutex` 后面；
//! 被测的性质正是「这份串行化让权威计数恰好逐份递减」。
//! 20 个任务用 [`tokio::sync::Barrier`] 同时放行，锁绝不跨 `.await`。
//!
//! 断言策略不是「归零了」，而是**权威计数的完整取值谱**：
//! 20 次并发归还不许重复任何中间值、不许跳过任何中间值。

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, MutexGuard};

use datazen_platform_api::id::{NetworkRouteRef, NetworkRouteRevision};
use datazen_platform_api::ports::network::TunnelSpec;
use datazen_runtime::connection::types::LeaseId;
use datazen_runtime::tunnel::{TunnelError, TunnelHandle, TunnelLedger, TunnelRelease, TunnelTransport};

/// CM-28 步骤：「并发释放 **20** 次」。
const CONCURRENCY: usize = 20;

// ------------------------------------------------------------ 隧道端口替身

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TunnelEvent {
    Open,
    Close,
}

/// 隧道端口：**只**记物理开合，不带任何引用计数（唯一计数在台账里）。
struct RecordingTunnelPort {
    events: Mutex<Vec<TunnelEvent>>,
}

impl RecordingTunnelPort {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            events: Mutex::new(Vec::new()),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Vec<TunnelEvent>> {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn opened(&self) -> usize {
        self.lock().iter().filter(|e| **e == TunnelEvent::Open).count()
    }

    fn closed(&self) -> usize {
        self.lock().iter().filter(|e| **e == TunnelEvent::Close).count()
    }
}

impl TunnelTransport for RecordingTunnelPort {
    fn open(&self, _spec: &TunnelSpec) -> Result<Option<TunnelHandle>, TunnelError> {
        self.lock().push(TunnelEvent::Open);
        Ok(Some(TunnelHandle::new()))
    }

    fn close(&self, _spec: &TunnelSpec) -> Result<(), TunnelError> {
        self.lock().push(TunnelEvent::Close);
        Ok(())
    }

    fn revision(&self, _route_ref: &NetworkRouteRef) -> Result<NetworkRouteRevision, TunnelError> {
        Ok(NetworkRouteRevision::new(1))
    }
}

type SharedLedger = Arc<Mutex<TunnelLedger>>;

fn ledger(shared: &SharedLedger) -> MutexGuard<'_, TunnelLedger> {
    shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn tunnel_spec() -> TunnelSpec {
    TunnelSpec::new(
        NetworkRouteRef::new("route://bastion/cm28"),
        "jump.internal".to_owned(),
        22,
    )
}

fn holders(prefix: &str, count: usize) -> Vec<LeaseId> {
    (0..count)
        .map(|index| LeaseId::new(format!("{prefix}-{index}")))
        .collect()
}

/// 建一条带 `count` 份引用的隧道，返回（台账、端口、spec）。
fn ledger_with_refs(
    spec: &TunnelSpec,
    dependents: &[LeaseId],
) -> (SharedLedger, Arc<RecordingTunnelPort>, usize) {
    let port = RecordingTunnelPort::new();
    let mut built = TunnelLedger::new(port.clone());
    for dependent in dependents {
        built
            .acquire(Some(spec), dependent)
            .expect("a healthy port never refuses to establish a tunnel")
            .expect("this spec needs a tunnel");
    }
    assert_eq!(built.ref_count(spec), Some(dependents.len() as u32));
    (
        Arc::new(Mutex::new(built)),
        port,
        dependents.len(),
    )
}

/// 启动 `dependents.len()` 个任务，同时抵达 `return_resource`。
async fn return_tunnel_storm(
    shared: &SharedLedger,
    dependents: Vec<LeaseId>,
) -> Vec<Option<TunnelRelease>> {
    let gate = Arc::new(tokio::sync::Barrier::new(dependents.len()));
    let mut handles = Vec::with_capacity(dependents.len());
    for dependent in dependents {
        let shared = Arc::clone(shared);
        let gate = Arc::clone(&gate);
        handles.push(tokio::spawn(async move {
            gate.wait().await;
            ledger(&shared).return_resource(&dependent)
        }));
    }
    let mut releases = Vec::with_capacity(handles.len());
    for handle in handles {
        releases.push(handle.await.expect("a spawned return task never panics"));
    }
    releases
}

/// 断言「隧道不多减引用」：并发归还不许把权威计数减丢或减多。
///
/// 证法是**取值谱**而不是终值：终值 0 在丢更新下同样成立，取值谱不会。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn twenty_concurrent_tunnel_returns_never_over_decrement_the_reference_count() {
    let spec = tunnel_spec();
    let dependents = holders("holder", CONCURRENCY);
    let (shared, port, refs) = ledger_with_refs(&spec, &dependents);
    assert_eq!(refs, CONCURRENCY);
    assert_eq!(port.opened(), 1, "同一条隧道只应被打开一次");

    let releases = return_tunnel_storm(&shared, dependents).await;

    let observed: Vec<u32> = releases
        .iter()
        .map(|release| {
            release
                .as_ref()
                .expect("每个持有者都持有过这条隧道")
                .refs
        })
        .collect();
    let distinct: BTreeSet<u32> = observed.iter().copied().collect();
    let expected: BTreeSet<u32> = (0..CONCURRENCY as u32).collect();
    assert_eq!(
        distinct, expected,
        "20 次并发归还的权威计数必须恰好各出现一次 0..=19 —— \
         重复的值意味着丢更新或多减，实得 {observed:?}"
    );
    assert_eq!(
        releases
            .iter()
            .filter(|release| release.as_ref().is_some_and(|r| r.closed))
            .count(),
        1,
        "只有归零那一次归还才允许真的拆隧道"
    );

    let guard = ledger(&shared);
    assert_eq!(
        guard.close_calls(),
        1,
        "台账只允许对同一条 spec 发一次 close"
    );
    assert_eq!(guard.live_tunnels(), 0);
    assert_eq!(
        guard.ref_count(&spec),
        None,
        "归零且关闭确认 ⇒ 条目消失"
    );
    drop(guard);
    assert_eq!(port.closed(), 1, "隧道端口只允许被拆一次");
}

/// 「重复释放是幂等的」在并发下：20 次归还**同一个**持有者，只许减一次。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn twenty_concurrent_repeats_of_one_tunnel_holder_decrement_exactly_once() {
    let spec = tunnel_spec();
    let dependents = holders("holder", CONCURRENCY);
    let (shared, port, refs) = ledger_with_refs(&spec, &dependents);

    let repeated = LeaseId::new("holder-repeated");
    ledger(&shared)
        .acquire(Some(&spec), &repeated)
        .expect("the repeated holder is a real holder")
        .expect("this spec needs a tunnel");
    assert_eq!(ledger(&shared).ref_count(&spec), Some(refs as u32 + 1));

    let releases = return_tunnel_storm(&shared, vec![repeated; CONCURRENCY]).await;

    let accepted: Vec<&TunnelRelease> = releases.iter().filter_map(|r| r.as_ref()).collect();
    assert_eq!(
        accepted.len(),
        1,
        "20 次归还同一个持有者，只有第一次被受理，其余必须是「没持有过」的答复"
    );
    assert_eq!(
        accepted[0].refs,
        refs as u32,
        "重复归还只许减掉**那一份**引用：{refs} - 1"
    );
    assert!(!accepted[0].closed, "引用没归零就不许拆隧道");

    let guard = ledger(&shared);
    assert_eq!(
        guard.ref_count(&spec),
        Some(refs as u32),
        "台账必须仍然看得到这 {refs} 份引用"
    );
    assert_eq!(guard.close_calls(), 0);
    drop(guard);
    assert_eq!(port.closed(), 0);
}

/// 负向对照：隧道端口的关闭计数**不是**天生等于 1。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn negative_control_the_tunnel_port_counter_reaches_more_than_one_close() {
    let port = RecordingTunnelPort::new();
    for round in 0..2u32 {
        let spec = tunnel_spec();
        let mut built = TunnelLedger::new(port.clone());
        let dependent = LeaseId::new(format!("holder-solo-{round}"));
        built
            .acquire(Some(&spec), &dependent)
            .expect("a healthy port never refuses to establish a tunnel")
            .expect("this spec needs a tunnel");
        assert!(built.return_resource(&dependent).expect("held").closed);
        assert_eq!(built.close_calls(), 1);
    }
    assert_eq!(
        port.closed(),
        2,
        "端口计数器必须能走到 2，证明上面那些「只拆一次」不是在断言一个常量"
    );
}

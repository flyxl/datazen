//! 重复 release/close 的第三格：**隧道**层的真实并发契约。
//!
//! 判据：
//!
//! ```text
//! - 断言：driver close 至多一次有效关闭；预算不负数；隧道不多减引用；重复响应一致。
//! ```
//!
//! 物理那两格在同目录的 `cm28_concurrent_release.rs`。
//!
//! # 为什么这一格也要新写
//!
//! `tests/tunnel_refcount_contract.rs` 里的
//! `one_return_releases_exactly_once_however_many_times_it_is_repeated`
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
//! **不要把 `flavor = "multi_thread"` 简化掉。** 实测：两个 CM-28 文件合计 8 处
//! `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]`（本文件 3 处）
//! 全部降级成默认 `#[tokio::test]`，**10 条测试仍然全绿** —— 因为 `&mut self` 的
//! 串行化保证了结论与线程数无关（这是断言稳健的标志，不是假并发：把
//! `cm28_concurrent_release.rs` 里两处 `Barrier::new(N)` 塌成 `Barrier::new(1)`
//! 立刻 `EXIT=101`，3 条变红）。
//! 保留 `multi_thread` 的唯一理由是：让 20 份引用读数与端口计数在**真实跨线程争用**下被压到。
//! Rust 无法内省 flavor，没有任何测试能钉住它，所以这条注释是它唯一的存活形式。
//!
//! 断言策略不是「归零了」，而是**权威计数的完整取值谱**：
//! 20 次并发归还不许重复任何中间值、不许跳过任何中间值。

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, MutexGuard};

use datazen_platform_api::id::{NetworkRouteRef, NetworkRouteRevision};
use datazen_platform_api::ports::network::TunnelSpec;
use datazen_runtime::connection::types::LeaseId;
use datazen_runtime::tunnel::{
    TunnelError, TunnelHandle, TunnelLedger, TunnelRelease, TunnelTransport,
};

/// CM-28 步骤：「并发释放 **20** 次」。
const CONCURRENCY: usize = 20;

// ------------------------------------------------------------ 隧道端口替身

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TunnelEvent {
    Open,
    Close,
}

/// 隧道端口：**只**记物理开合，不带任何引用计数，也不带自存的开合计数
/// （唯一计数在台账里；CM-32-FU1 登记的反例正是「端口自己再存一份账并让观测方法改读它」）。
struct RecordingTunnelPort {
    events: Mutex<Vec<TunnelEvent>>,
}

/// 端口账：物理开合读数，**只**由 [`tally`] 这一个纯折函数算出。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PortTally {
    opened: usize,
    closed: usize,
}

/// 「事件 → 账」的唯一折法。同 `src/tunnel/harness.rs` 的 `tallies` 一个形状；
/// 本文件是外部 crate，看不见那个 `#[cfg(test)]` 私有模块，所以照同一规则就地写一份
/// —— 审计按「每份实现各自只有一个折函数」检查，不跨文件共享。
fn tally(events: &[TunnelEvent]) -> PortTally {
    let mut out = PortTally::default();
    for event in events {
        match event {
            TunnelEvent::Open => out.opened += 1,
            TunnelEvent::Close => out.closed += 1,
        }
    }
    out
}

impl RecordingTunnelPort {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            events: Mutex::new(Vec::new()),
        })
    }

    /// 唯一事实源：事件日志本身（取锁 + 反毒）。
    fn lock(&self) -> MutexGuard<'_, Vec<TunnelEvent>> {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 读数一律从 `lock()` 现折 —— 端口里没有任何计数**字段**。
    fn opened(&self) -> usize {
        tally(&self.lock()).opened
    }

    fn closed(&self) -> usize {
        tally(&self.lock()).closed
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
    tunnel_spec_on("route://bastion/cm28")
}

/// 同一份 `TunnelSpec` 在台账里**只对应一条**隧道；想造两条**独立**隧道必须换路由。
fn tunnel_spec_on(route: &str) -> TunnelSpec {
    TunnelSpec::new(NetworkRouteRef::new(route), "jump.internal".to_owned(), 22)
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
    (Arc::new(Mutex::new(built)), port, dependents.len())
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
        .map(|release| release.as_ref().expect("每个持有者都持有过这条隧道").refs)
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
    assert_eq!(guard.ref_count(&spec), None, "归零且关闭确认 ⇒ 条目消失");
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
        accepted[0].refs, refs as u32,
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

/// 负向对照：**同一个台账**上的关闭计数必须能走到 2 —— 证明上面那些
/// `close_calls() == 1` 断言的是一个**计数**，不是常量 1。
///
/// 上一版这条负控写成 `for round in 0..2` 但**每轮都 `TunnelLedger::new()`**，
/// 只共享 `port`。于是台账每轮从零起算，`close_calls()` 恒为 1，负控自带**零证明力**：
/// 把 `src/tunnel/ledger.rs` 归零分支里的 `self.close_calls += 1` 改成 `self.close_calls = 1`，
/// 全仓 698 条测试仍然全绿 —— 一条抓不住回归的负控比没有负控更糟，它给出虚假保证。
///
/// 现在两条**独立**隧道（不同路由 ⇒ 不同 entry）跑在**同一个**台账上，`close_calls()`
/// 必须走到 2：`= 1` 的突变体立刻变红。实跑证据见报告 F-2 节。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn negative_control_the_tunnel_ledger_counter_reaches_two_on_one_ledger() {
    let port = RecordingTunnelPort::new();
    let shared: SharedLedger = Arc::new(Mutex::new(TunnelLedger::new(port.clone())));

    for round in 0..2u32 {
        // 不同路由 ⇒ 台账里两条独立 entry，不是「同一条隧道的两份引用」。
        let spec = tunnel_spec_on(&format!("route://bastion/cm28-ctrl-{round}"));
        let dependent = LeaseId::new(format!("holder-solo-{round}"));
        ledger(&shared)
            .acquire(Some(&spec), &dependent)
            .expect("a healthy port never refuses to establish a tunnel")
            .expect("this spec needs a tunnel");
        let release = ledger(&shared)
            .return_resource(&dependent)
            .expect("the holder is on the ledger");
        assert!(release.closed, "第 {round} 条隧道归零后必须被确认关闭");
    }

    let guard = ledger(&shared);
    assert_eq!(
        guard.close_calls(),
        2,
        "**同一个**台账上关掉两条独立隧道 ⇒ 台账自报的关闭数必须走到 2"
    );
    assert_eq!(guard.live_tunnels(), 0);
    drop(guard);
    assert_eq!(
        port.closed(),
        2,
        "物理端口同样必须能走到 2：那两条「只拆一次」不是在断言一个常量"
    );
}

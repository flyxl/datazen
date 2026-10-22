//! 接缝契约：`NetworkProvider` 的隧道部分**在进程内**如何成立。
//!
//! `tunnel_refcount_contract.rs` 已经证明这层接缝「可以在 runtime crate 之外实现」
//! （那份文件证的是宿主那侧）；本文件证的是另一件事：**一旦真有人这么实现了，
//! 共享与归还是否成立**。四个断言类各对应端口契约上的一句成文承诺：
//!
//! | 断言类 | 契约原文 | 用例 |
//! |---|---|---|
//! | 并发幂等 | 「同一路由/隧道被多处使用时返回同一 binding 的引用计数句柄」 | [`twenty_concurrent_ensures_of_one_tunnel_open_it_exactly_once`] |
//! | 失败回滚 | 引用归零才真正拆除 ⇒ 半开的引用必须还回去 | [`a_tunnel_that_opens_without_a_local_endpoint_is_rolled_back`] |
//! | 重复释放幂等 | 「重复释放是幂等的」 | [`releasing_one_binding_twenty_concurrent_times_closes_at_most_once`] |
//! | 引用不下溢 | 「引用计数归零才真正拆除」 | [`twenty_concurrent_repeats_of_one_holder_are_accepted_exactly_once`] |
//!
//! 替身纪律与 `cm28_concurrent_tunnel.rs`、`tunnel_refcount_contract.rs` 同形：
//! 端口**不存**任何整数计数，所有读数都由**同一个纯折函数** [`tally`] 从事件日志算出。
//! 这不是洁癖：`tunnel::single_counter_audit` 的 R4/R5 会机械杀掉第二本账。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use datazen_platform_api::context::RequestContext;
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{
    ClientInstanceId, NetworkRouteRef, NetworkRouteRevision, OrganizationId, PrincipalId, RequestId,
};
use datazen_platform_api::ports::network::{NetworkProvider, RoutePlan, TunnelBinding, TunnelSpec};
use datazen_runtime::tunnel::{
    RouteCatalog, RouteNetworkProvider, TunnelError, TunnelFault, TunnelHandle, TunnelState,
    TunnelTransport,
};

/// 并发档位。与 `cm28_concurrent_tunnel.rs` 同数：够多才会撞上真的交错。
const CONCURRENCY: usize = 20;

// ---------------------------------------------------------------------------------------------
// 端口替身
// ---------------------------------------------------------------------------------------------

/// 端口看得见的事实：一次物理开合。
#[derive(Debug, Clone, PartialEq, Eq)]
enum TunnelEvent {
    Opened(u16),
    Closed(u16),
}

/// 端口账：物理开合读数。**只**由 [`tally`] 从事件日志算出。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PortTally {
    opened: usize,
    closed: usize,
}

/// 「事件 → 账」的唯一折法。本文件是外部 crate，看不见 `src/tunnel/harness.rs` 那个
/// `#[cfg(test)]` 私有模块，所以照同一规则就地写一份 —— 审计按「每份实现各自只有一个
/// 折函数」检查，不跨文件共享。
fn tally(events: &[TunnelEvent]) -> PortTally {
    let mut out = PortTally::default();
    for event in events {
        match event {
            TunnelEvent::Opened(_) => out.opened += 1,
            TunnelEvent::Closed(_) => out.closed += 1,
        }
    }
    out
}

struct RecordingTunnelPort {
    events: Mutex<Vec<TunnelEvent>>,
    revisions: Mutex<BTreeMap<String, u64>>,
    fail_open: bool,
    fail_close: bool,
}

impl RecordingTunnelPort {
    fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
            revisions: Mutex::new(BTreeMap::new()),
            fail_open: false,
            fail_close: false,
        }
    }

    fn failing_open() -> Arc<Self> {
        Arc::new(Self {
            fail_open: true,
            ..Self::new()
        })
    }

    fn failing_close() -> Arc<Self> {
        Arc::new(Self {
            fail_close: true,
            ..Self::new()
        })
    }

    fn lock_events(&self) -> MutexGuard<'_, Vec<TunnelEvent>> {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn events(&self) -> Vec<TunnelEvent> {
        self.lock_events().clone()
    }

    fn record(&self, event: TunnelEvent) {
        self.lock_events().push(event);
    }

    fn set_revision(&self, route_ref: &NetworkRouteRef, value: u64) {
        self.revisions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(route_ref.as_str().to_string(), value);
    }

    /// 宿主记得的物理开合次数。**这是接缝上唯一的「有没有真的开/关掉」证据**。
    fn opened(&self) -> usize {
        tally(&self.events()).opened
    }

    fn closed(&self) -> usize {
        tally(&self.events()).closed
    }
}

impl TunnelTransport for RecordingTunnelPort {
    fn open(&self, spec: &TunnelSpec) -> Result<Option<TunnelHandle>, TunnelError> {
        // 先判失败再记账：一次没能建成的隧道不是「开过」。
        if self.fail_open {
            return Err(TunnelError::Transport {
                spec: spec.clone(),
                fault: TunnelFault::Open,
            });
        }
        self.record(TunnelEvent::Opened(spec.via_port));
        Ok(Some(TunnelHandle::new()))
    }

    fn close(&self, spec: &TunnelSpec) -> Result<(), TunnelError> {
        if self.fail_close {
            return Err(TunnelError::Transport {
                spec: spec.clone(),
                fault: TunnelFault::Close,
            });
        }
        self.record(TunnelEvent::Closed(spec.via_port));
        Ok(())
    }

    fn revision(&self, route_ref: &NetworkRouteRef) -> Result<NetworkRouteRevision, TunnelError> {
        let value = self
            .revisions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(route_ref.as_str())
            .copied()
            .ok_or(TunnelError::RevisionUnavailable)?;
        Ok(NetworkRouteRevision::new(value))
    }
}

// ---------------------------------------------------------------------------------------------
// 目录替身
// ---------------------------------------------------------------------------------------------

/// 路由目录：一条路由要么直连，要么走一条隧道 spec。
///
/// `local_endpoint` 用 `Mutex<Option<String>>` 而不是构造期定死，是因为「隧道开起来了、
/// 端点却拿不到」正是要证的那种半开现场 —— 它只能在隧道**开起来之后**被抽掉。
struct ScriptedRouteCatalog {
    spec: Option<TunnelSpec>,
    direct: String,
    local_endpoint: Mutex<Option<String>>,
}

impl ScriptedRouteCatalog {
    fn direct() -> Arc<Self> {
        Arc::new(Self {
            spec: None,
            direct: "db.internal:5432".to_owned(),
            local_endpoint: Mutex::new(None),
        })
    }

    fn tunneled(local_endpoint: &str) -> Arc<Self> {
        Arc::new(Self {
            spec: Some(spec(22)),
            direct: "db.internal:5432".to_owned(),
            local_endpoint: Mutex::new(Some(local_endpoint.to_owned())),
        })
    }

    fn endpoint(&self) -> Option<String> {
        self.local_endpoint
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn withdraw_endpoint(&self) {
        *self
            .local_endpoint
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

impl RouteCatalog for ScriptedRouteCatalog {
    fn tunnel_spec(&self, _route_ref: &NetworkRouteRef) -> Option<TunnelSpec> {
        self.spec.clone()
    }

    fn direct_endpoint(&self, _route_ref: &NetworkRouteRef) -> Option<String> {
        Some(self.direct.clone())
    }

    fn tunneled_endpoint(&self, _spec: &TunnelSpec) -> Option<String> {
        self.endpoint()
    }
}

// ---------------------------------------------------------------------------------------------
// 装配
// ---------------------------------------------------------------------------------------------

fn spec(via_port: u16) -> TunnelSpec {
    TunnelSpec::new(NetworkRouteRef::new("route-a"), "jump.internal", via_port)
}

fn ctx() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("org-1"),
        PrincipalId::new("principal-1"),
        None,
        ClientInstanceId::new("client-1"),
        RequestId::new("request-1"),
        None,
    )
}

fn binding(spec: &TunnelSpec) -> TunnelBinding {
    TunnelBinding {
        spec: spec.clone(),
        // `ref_count` 是观测快照，端口释放以调用为准，所以这里随便填一个数也合法。
        ref_count: 99,
    }
}

fn provider(
    catalog: &Arc<ScriptedRouteCatalog>,
    port: Arc<RecordingTunnelPort>,
) -> RouteNetworkProvider {
    RouteNetworkProvider::new(Arc::clone(catalog) as Arc<dyn RouteCatalog>, port)
}

async fn ensure(
    provider: &RouteNetworkProvider,
    spec: TunnelSpec,
) -> Result<TunnelBinding, PortError> {
    provider.ensure_tunnel(&ctx(), spec).await
}

async fn release(provider: &RouteNetworkProvider, spec: &TunnelSpec) -> Result<(), PortError> {
    provider.release_tunnel(&binding(spec)).await
}

// ---------------------------------------------------------------------------------------------
// 断言类一：并发幂等（同一 spec 反复 ensure 不得重复开隧道）
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn twenty_concurrent_ensures_of_one_tunnel_open_it_exactly_once() {
    let port = Arc::new(RecordingTunnelPort::new());
    let provider = Arc::new(provider(
        &ScriptedRouteCatalog::tunneled("127.0.0.1:53124"),
        Arc::clone(&port),
    ));

    let gate = Arc::new(tokio::sync::Barrier::new(CONCURRENCY));
    let mut tasks = Vec::with_capacity(CONCURRENCY);
    for _ in 0..CONCURRENCY {
        let provider = Arc::clone(&provider);
        let gate = Arc::clone(&gate);
        tasks.push(tokio::spawn(async move {
            gate.wait().await;
            ensure(&provider, spec(22)).await
        }));
    }
    let bindings = join_all(&mut tasks).await;

    assert!(
        bindings.iter().all(Result::is_ok),
        "{CONCURRENCY} 份共享引用必须全部拿到"
    );
    // 共享身份 = spec 全等值：二十次 ensure 只开一次。
    assert_eq!(
        port.opened(),
        1,
        "同一 spec 被并发 ensure 二十次，物理开隧道只能有一次"
    );
    assert_eq!(port.closed(), 0, "还有二十份引用在手上，不该拆");
    assert_eq!(
        provider.reference_count(&spec(22)),
        Some(CONCURRENCY as u32),
        "二十份共享引用都必须记在同一条条目上"
    );

    for task in tasks {
        task.abort();
    }
}

// ---------------------------------------------------------------------------------------------
// 断言类二：失败回滚（开了之后的后置步骤失败 ⇒ 立刻还回去；开隧道本身失败 ⇒ 不留痕）
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_tunnel_that_opens_without_a_local_endpoint_is_rolled_back() {
    let catalog = ScriptedRouteCatalog::tunneled("127.0.0.1:53124");
    let port = Arc::new(RecordingTunnelPort::new());
    let provider = provider(&catalog, Arc::clone(&port));
    catalog.withdraw_endpoint();

    assert!(matches!(
        ensure(&provider, spec(22)).await,
        Err(PortError::BackendUnavailable(_))
    ));

    // 半开状态是最贵的那种错：隧道开着、引用挂着、路由却拿不到。
    assert_eq!(port.opened(), 1, "隧道确实开起来了，所以这条现场是真的");
    assert_eq!(port.closed(), 1, "后置步骤失败就必须原样还回去，归零即真拆");
    assert_eq!(provider.reference_count(&spec(22)), None, "不许留残引用");
    assert!(matches!(provider.state(&spec(22)), TunnelState::Absent));
}

#[tokio::test]
async fn a_failed_open_leaves_no_trace_and_a_later_release_closes_nothing() {
    let port = RecordingTunnelPort::failing_open();
    let provider = provider(
        &ScriptedRouteCatalog::tunneled("127.0.0.1:53124"),
        Arc::clone(&port),
    );

    assert!(matches!(
        ensure(&provider, spec(22)).await,
        Err(PortError::BackendUnavailable(_))
    ));
    assert_eq!(port.opened(), 0, "没建成的隧道不算开过");

    // 失败之后调用方多半还会走一遍释放流程：那一次释放必须是什么都不发生，
    // 而不是「去关一条从来没开成的隧道」。
    assert!(release(&provider, &spec(22)).await.is_ok());
    assert_eq!(port.closed(), 0);
    assert_eq!(provider.reference_count(&spec(22)), None);
    assert!(matches!(provider.state(&spec(22)), TunnelState::Absent));
}

#[tokio::test]
async fn a_close_that_is_not_confirmed_keeps_the_entry_and_stays_idempotent() {
    let port = RecordingTunnelPort::failing_close();
    let provider = provider(
        &ScriptedRouteCatalog::tunneled("127.0.0.1:53124"),
        Arc::clone(&port),
    );

    ensure(&provider, spec(22)).await.expect("隧道应当开得起来");
    assert!(release(&provider, &spec(22)).await.is_ok());
    assert!(release(&provider, &spec(22)).await.is_ok());

    // 关闭未证实：台账按设计**保留**条目等后续观察，而不是假装预算/隧道已经回收。
    assert!(matches!(
        provider.state(&spec(22)),
        TunnelState::Unconfirmed
    ));
    assert_eq!(port.closed(), 0, "没能确认的关闭不许被记成「关过」");
}

#[tokio::test]
async fn a_spec_that_needs_no_tunnel_is_answered_without_touching_the_table() {
    let port = Arc::new(RecordingTunnelPort::new());
    let provider = provider(
        &ScriptedRouteCatalog::tunneled("127.0.0.1:53124"),
        Arc::clone(&port),
    );

    // `via_port == 0` = 不走隧道。这是端口必须原样透传、不许硬造一条固定资源的情形
    // （§9.4：单连接、无状态的生命周期不许被伪装成可池化的固定资源）。
    let handed_back = ensure(&provider, spec(0)).await.expect("免隧道不是错误");

    assert_eq!(handed_back.ref_count, 0);
    assert_eq!(port.opened(), 0);
    assert_eq!(provider.reference_count(&spec(0)), None);
}

// ---------------------------------------------------------------------------------------------
// 断言类三：重复释放幂等
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn releasing_one_binding_twenty_concurrent_times_closes_at_most_once() {
    let port = Arc::new(RecordingTunnelPort::new());
    let provider = Arc::new(provider(
        &ScriptedRouteCatalog::tunneled("127.0.0.1:53124"),
        Arc::clone(&port),
    ));
    let held = ensure(&provider, spec(22)).await.expect("隧道应当开得起来");

    let gate = Arc::new(tokio::sync::Barrier::new(CONCURRENCY));
    let mut tasks = Vec::with_capacity(CONCURRENCY);
    for _ in 0..CONCURRENCY {
        let provider = Arc::clone(&provider);
        let binding = held.clone();
        let gate = Arc::clone(&gate);
        tasks.push(tokio::spawn(async move {
            gate.wait().await;
            // 端口契约写明「重复释放是幂等的」，所以每一次都必须成功返回。
            provider.release_tunnel(&binding).await
        }));
    }
    let outcomes = join_all(&mut tasks).await;

    assert!(outcomes.iter().all(Result::is_ok));
    assert_eq!(port.closed(), 1, "同一份引用还二十次，物理关闭只能有一次");
    assert_eq!(provider.reference_count(&spec(22)), None);
    assert!(matches!(provider.state(&spec(22)), TunnelState::Absent));
}

#[tokio::test]
async fn releasing_a_tunnel_that_was_never_taken_is_a_no_op_success() {
    let port = Arc::new(RecordingTunnelPort::new());
    let provider = provider(
        &ScriptedRouteCatalog::tunneled("127.0.0.1:53124"),
        Arc::clone(&port),
    );

    assert!(release(&provider, &spec(22)).await.is_ok());
    assert!(release(&provider, &spec(22)).await.is_ok());

    assert_eq!(port.closed(), 0, "没人持有过的东西不许被关");
    assert_eq!(port.opened(), 0);
}

// ---------------------------------------------------------------------------------------------
// 断言类四：引用不下溢
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn twenty_concurrent_repeats_of_one_holder_are_accepted_exactly_once() {
    let port = Arc::new(RecordingTunnelPort::new());
    let provider = Arc::new(provider(
        &ScriptedRouteCatalog::tunneled("127.0.0.1:53124"),
        Arc::clone(&port),
    ));
    ensure(&provider, spec(22)).await.expect("隧道应当开得起来");

    // 同一条 spec 被并发释放二十次，但**只有一份**引用在手上。
    // 台账的 `take_reference` 带 `debug_assert!(refs > 0)`：任何多减一次都会在这里炸，
    // 所以「没炸 + 只关一次」合起来就是引用不下溢的证据。
    let gate = Arc::new(tokio::sync::Barrier::new(CONCURRENCY));
    let mut tasks = Vec::with_capacity(CONCURRENCY);
    for _ in 0..CONCURRENCY {
        let provider = Arc::clone(&provider);
        let gate = Arc::clone(&gate);
        tasks.push(tokio::spawn(async move {
            gate.wait().await;
            provider.release_tunnel(&binding(&spec(22))).await
        }));
    }
    let outcomes = join_all(&mut tasks).await;

    assert!(outcomes.iter().all(Result::is_ok));
    assert_eq!(port.closed(), 1);
    assert_eq!(provider.reference_count(&spec(22)), None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_storm_of_shared_holders_returns_the_reference_count_to_zero_exactly() {
    let port = Arc::new(RecordingTunnelPort::new());
    let provider = Arc::new(provider(
        &ScriptedRouteCatalog::tunneled("127.0.0.1:53124"),
        Arc::clone(&port),
    ));

    // 先把所有 ensure 压到同一时刻，再统一放行 release：这样「开一次、关一次、
    // 引用归零」是确定性的，而不是靠运气撞出来的。
    let enter = Arc::new(tokio::sync::Barrier::new(CONCURRENCY));
    let leave = Arc::new(tokio::sync::Barrier::new(CONCURRENCY));
    let mut tasks = Vec::with_capacity(CONCURRENCY);
    for _ in 0..CONCURRENCY {
        let provider = Arc::clone(&provider);
        let enter = Arc::clone(&enter);
        let leave = Arc::clone(&leave);
        tasks.push(tokio::spawn(async move {
            enter.wait().await;
            let held = ensure(&provider, spec(22)).await.expect("应当拿到共享引用");
            leave.wait().await;
            provider.release_tunnel(&held).await
        }));
    }
    let outcomes = join_all(&mut tasks).await;

    assert!(outcomes.iter().all(Result::is_ok));
    assert_eq!(port.opened(), 1);
    assert_eq!(port.closed(), 1);
    assert_eq!(
        provider.reference_count(&spec(22)),
        None,
        "二十进二十出之后不该剩任何引用 —— 剩了就是有人替它续了命"
    );
    assert!(matches!(provider.state(&spec(22)), TunnelState::Absent));
}

// ---------------------------------------------------------------------------------------------
// 路由解析：隧道引用怎么变成端点
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_direct_route_resolves_to_its_own_endpoint_without_a_tunnel() {
    let port = Arc::new(RecordingTunnelPort::new());
    port.set_revision(&NetworkRouteRef::new("route-a"), 7);
    let provider = provider(&ScriptedRouteCatalog::direct(), Arc::clone(&port));

    let plan = resolve(&provider).await.expect("直连路由应当解析成功");

    assert!(!plan.tunneled);
    assert_eq!(plan.endpoint, "db.internal:5432");
    assert_eq!(
        plan.route_revision.get(),
        7,
        "PoolKey 的 networkRouteRevision 分量就取自这里"
    );
    assert_eq!(port.events(), Vec::new(), "直连路由不该碰隧道端口");
}

#[tokio::test]
async fn a_tunneled_route_stays_refused_until_this_provider_holds_the_reference() {
    let port = Arc::new(RecordingTunnelPort::new());
    port.set_revision(&NetworkRouteRef::new("route-a"), 9);
    let provider = provider(
        &ScriptedRouteCatalog::tunneled("127.0.0.1:53124"),
        Arc::clone(&port),
    );

    // 还没有任何引用就发 `tunneled: true`，等于把调用方指向一个随时会被拆掉的隧道。
    assert!(matches!(
        resolve(&provider).await,
        Err(PortError::BackendUnavailable(_))
    ));

    ensure(&provider, spec(22)).await.expect("应当拿到共享引用");
    let plan = resolve(&provider).await.expect("有活引用就应当解析成功");

    assert!(plan.tunneled);
    assert_eq!(plan.endpoint, "127.0.0.1:53124");
    assert_eq!(plan.route_revision.get(), 9);
}

#[tokio::test]
async fn the_reference_withdrawn_last_is_the_one_that_tears_the_tunnel_down() {
    let catalog = ScriptedRouteCatalog::tunneled("127.0.0.1:53124");
    let port = Arc::new(RecordingTunnelPort::new());
    port.set_revision(&NetworkRouteRef::new("route-a"), 3);
    let provider = provider(&catalog, Arc::clone(&port));

    let first = ensure(&provider, spec(22)).await.expect("第一份引用");
    let second = ensure(&provider, spec(22)).await.expect("第二份引用");
    assert_eq!(port.opened(), 1, "第二份引用共享第一条隧道");
    assert!(resolve(&provider).await.is_ok(), "还有引用，路由就仍然可用");

    release(&provider, &first.spec).await.expect("归还第一份");
    assert_eq!(port.closed(), 0, "还剩一份引用在手上");
    assert!(
        resolve(&provider).await.is_ok(),
        "另一处还在用，这条共享隧道不许被拆掉"
    );

    release(&provider, &second.spec).await.expect("归还第二份");
    assert_eq!(port.closed(), 1, "最后一份归零才真拆");
    assert!(matches!(
        resolve(&provider).await,
        Err(PortError::BackendUnavailable(_))
    ));
}

#[tokio::test]
async fn an_unknown_route_revision_is_refused_rather_than_guessed() {
    let port = Arc::new(RecordingTunnelPort::new());
    let provider = provider(&ScriptedRouteCatalog::direct(), Arc::clone(&port));

    assert!(matches!(
        resolve(&provider).await,
        Err(PortError::NotFound(_))
    ));
    assert!(matches!(
        provider.revision(NetworkRouteRef::new("route-a")),
        Err(PortError::NotFound(_))
    ));
}

// ---------------------------------------------------------------------------------------------

async fn resolve(provider: &RouteNetworkProvider) -> Result<RoutePlan, PortError> {
    provider
        .resolve_route(&ctx(), NetworkRouteRef::new("route-a"))
        .await
}

/// 逐个 join 到底：不中途早退，任务全部跑到结束。
async fn join_all<T>(tasks: &mut [tokio::task::JoinHandle<T>]) -> Vec<T> {
    let mut out = Vec::with_capacity(tasks.len());
    for task in tasks.iter_mut() {
        match task.await {
            Ok(value) => out.push(value),
            Err(join_error) => panic!("任务 join 失败：{join_error}"),
        }
    }
    out
}

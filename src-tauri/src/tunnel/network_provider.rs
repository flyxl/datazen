//! 桌面宿主侧的 [`NetworkProvider`] 装配：把**物理开法**接回**引用账**。
//!
//! # 这一层补的是哪个洞
//!
//! Runtime 资源路径通过 `request.tunnel_spec` 描述隧道路由，而物理 SSH / 代理 / WebSocket
//! 隧道仍由桌面宿主打开。本适配器把 Runtime 的路由与引用契约接到宿主的物理隧道实现。
//!
//! # 三层的分工
//!
//! | 层 | 谁 | 负责什么 |
//! |---|---|---|
//! | 共享账 | `runtime::tunnel::RouteNetworkProvider` | 谁共享、引用多少、归零才拆 |
//! | 物理端口 | 本文件的 [`DesktopTunnelHost`]（`TunnelTransport`） | 真的把 SSH/代理/WS 隧道起起来 |
//! | 路由目录 | 本文件的 [`DesktopTunnelHost`]（`RouteCatalog`） | 路由 → spec / 端点 |
//!
//! 分工的必然性来自两个冻结事实：`TunnelSpec` 只带
//! `route_ref / via_host / via_port`（**没有** SSH 凭据、代理地址、WebSocket 路径），
//! 而 `TunnelHandle` 是不透明的 `Arc<()>`（**拿不到**本地端口）。所以
//! 「怎么开」只能问宿主，「开成什么样」只能由宿主另行报出 —— 不能从句柄里反推。
//!
//! # 同步端口遇上异步开法
//!
//! [`TunnelTransport::open`] 是**同步**方法（台账 `acquire` 同步，
//! `ResourceManager::acquire` 也同步，它是在异步 `acquire_resource` 内部被调用的），
//! 而 SSH 握手 / 代理 `CONNECT` / WebSocket 升级全是 `async`。桥法只有一个约束：
//! 必须借**应用那个**运行时来 await（见 [`start_tunnel_blocking`]），因为隧道把
//! accept 循环 `tokio::spawn` 出去，自建一个运行时会让转发任务随运行时一起死。
//!
//! # 不做的事
//!
//! * **不记任何整数计数。** 本文件不存「开过几次 / 还剩几个」。引用账只有一份，在
//!   `TunnelLedger` 里；本文件只按 [`TunnelSpec`] 存**活着的隧道对象**，
//!   对象被摘走就是物理拆除（`Drop`）。
//! * **不建过渡兼容层。** 隧道物理开法由本适配器统一提供，不维护并行的会话级入口。
//! * **不静默降级。** 调用方要了隧道、而路由给出的配置里 `tunnel_kind` 却是 `None`
//!   时，本文件报开隧道**失败**，而不是悄悄让连接直连真实库主机。

use std::sync::{Arc, Mutex, MutexGuard};

use datazen_modules::tunnel::KnownHostsRepository;
use datazen_platform_api::id::{NetworkRouteRef, NetworkRouteRevision};
use datazen_platform_api::ports::network::{NetworkProvider, TunnelSpec};
use datazen_runtime::tunnel::{
    RouteCatalog, RouteNetworkProvider, TunnelError, TunnelFault, TunnelHandle, TunnelTransport,
};
use tracing::{debug, warn};

use crate::tunnel::{start_for_connection, Tunnel};
use datazen_driver_api::{ConnectionConfig, DriverError};

/// 路由 → 可启动隧道的连接配置。宿主实现细节里**唯一**需要注入的东西。
///
/// 三个方法**全是同步的**，且**不许**在实现里阻塞等 I/O —— 它们跑在
/// `TunnelTransport::open` 这个同步端口的调用栈上（见模块头的「同步端口遇上异步开法」）。
/// 存档隧道要靠内存快照回答：快照的刷新由实现方在别处做。
///
/// [`TunnelSpec`] 里只有 `route_ref / via_host / via_port`：`route_ref` 是**存档隧道的
/// id**（与 `config.tunnel_id` → `TunnelRepository::get` 一一对应），
/// `via_host` / `via_port` 是跳转腿，**只进共享身份与池键**，不参与开法。
/// 真正的 SSH 账号、代理地址、WebSocket 路径只能由本端口去查存档补齐。
pub trait DesktopTunnelSource: Send + Sync + 'static {
    /// 这条路由对应的连接配置，隧道段（`tunnel_kind` + `ssh_tunnel` /
    /// `http_proxy_tunnel` / `websocket_tunnel`）必须已经填好。
    ///
    /// Err 的字符串会进日志，**不得**携带凭据。
    fn connection_config(&self, route_ref: &NetworkRouteRef) -> Result<ConnectionConfig, String>;

    /// 这条路由的**共享身份**，跳转腿（`via_host` / `via_port`）照实给。
    ///
    /// `None` = 这条路由不走隧道，走 [`Self::direct_endpoint`]。给了 `Some` 就必须
    /// 给真的跳转腿：**端口不许填 0**（0 是账里的「这条不需要隧道」哨兵，填 0 等于
    /// 谎报直连），也不许拿 `route_ref` 之类的东西凑数 —— 共享身份要能区分
    /// 「同一条隧道」与「同一台机器上的另一条隧道」。
    fn tunnel_spec(&self, route_ref: &NetworkRouteRef) -> Option<TunnelSpec>;

    /// 不走隧道的路由端点（`host:port`）。返回 `None` 表示这条路由没有直连形态。
    fn direct_endpoint(&self, route_ref: &NetworkRouteRef) -> Option<String>;

    /// 这条路由的修订号。路由表任何一次改动都必须让它变。
    ///
    /// 它会进 `PoolKey` 的 `networkRouteRevision` 分量：值不变就意味着**还在复用同一条
    /// 路由**，变了就必须重新决议。宿主没有「全局路由表修订号」这种东西，所以它由本端口
    /// 的实现方如实给出 —— 拿不到就返回 `Err`，**不许**编一个恒定值冒充稳定。
    fn revision(&self, route_ref: &NetworkRouteRef) -> Result<NetworkRouteRevision, String>;
}

/// 按 [`TunnelSpec`] 存活的隧道对象。
///
/// **不是账**：没有引用计数，也不参与「该不该关」的判断 —— 谁来关、什么时候关，
/// 全部由上游的引用账说了算。本表只是「已开的隧道对象挂在哪儿」。
struct Roster<T> {
    entries: Vec<(TunnelSpec, T)>,
}

impl<T> Roster<T> {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// 全等值定位。`TunnelSpec` 只 `PartialEq`（无 `Ord` / `Hash`），所以只能线性找 ——
    /// 这正是它做共享键的代价，也是它小的代价。
    fn position(&self, spec: &TunnelSpec) -> Option<usize> {
        self.entries.iter().position(|(held, _)| held == spec)
    }

    fn value(&self, spec: &TunnelSpec) -> Option<&T> {
        self.position(spec)
            .and_then(|index| self.entries.get(index))
            .map(|(_, value)| value)
    }

    fn record(&mut self, spec: &TunnelSpec, value: T) {
        match self.position(spec) {
            Some(index) => self.entries[index] = (spec.clone(), value),
            None => self.entries.push((spec.clone(), value)),
        }
    }

    /// 摘走并**返回**。返回值即所有权移交：调用方一放手，对象析构，物理隧道随之拆除。
    fn withdraw(&mut self, spec: &TunnelSpec) -> Option<T> {
        self.position(spec)
            .map(|index| self.entries.remove(index).1)
    }
}

/// 端口实现方按自己的形状报错；这里只做一次窄化，避免每个调用点各写一遍。
fn open_fault(spec: &TunnelSpec, detail: impl std::fmt::Display) -> TunnelError {
    warn!(
        target: "datazen::tunnel",
        route = %spec.route_ref,
        via = %spec.via_host,
        viaPort = spec.via_port,
        "桌面隧道开启失败：{detail}"
    );
    TunnelError::Transport {
        spec: spec.clone(),
        fault: TunnelFault::Open,
    }
}

/// 在**应用长驻运行时**上把隧道开起来，然后同步返回。
///
/// 同步端口（[`TunnelTransport::open`]）撞上异步开法时，这一段是唯一的桥。
/// 三条约束决定了它的写法：
///
/// 1. **必须用调用方的运行时。** `HttpProxyTunnel::start` 把 accept 循环
///    `tokio::spawn` 出去；自建一个 `current_thread` 运行时再丢掉，转发任务当场死，
///    隧道变哑（`http_proxy.rs` 里那段「转发死了隧道还像活着」的告警正是这件事）。
/// 2. **先问 flavor 再决定怎么等。** `block_in_place` 在 `current_thread` 运行时上会
///    panic，所以那种运行时直接报**开隧道失败**，不靠 panic 说话。
/// 3. **不在运行时里就报失败**，而不是自己造一个运行时 —— 同第 1 条。
fn start_tunnel_blocking(
    spec: &TunnelSpec,
    config: ConnectionConfig,
    known_hosts: Arc<dyn KnownHostsRepository>,
) -> Result<Option<Tunnel>, TunnelError> {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return Err(open_fault(
            spec,
            "当前线程不在 tokio 运行时上，隧道无法在同步端口上开启",
        ));
    };
    if !matches!(
        handle.runtime_flavor(),
        tokio::runtime::RuntimeFlavor::MultiThread
    ) {
        return Err(open_fault(
            spec,
            "隧道需要多线程 tokio 运行时才能在同步端口上开启",
        ));
    }

    let start = async move { start_for_connection(config, known_hosts).await };
    // `block_in_place` 把当前 worker 让给调度器去跑别的任务，所以这段握手等待
    // 不会让运行时少一个执行线程。
    //
    // `start_for_connection` 一并回一份改写过 `host`/`port` 的连接配置；端口这一层
    // 不消费它 —— 谁来改写连接配置是接线点（`session_backend::open`）的事，不是端口的事。
    tokio::task::block_in_place(move || handle.block_on(start))
        .map(|(_, tunnel)| tunnel)
        .map_err(|error| open_fault(spec, error))
}

/// 宿主侧实现：同一个对象同时充当物理端口与路由目录。
///
/// 之所以合一：端口刚把隧道挂上表，目录就得立刻能报出它的本地端口
/// （`RouteNetworkProvider::ensure_tunnel` 紧接着就问一次），两份状态就必然有
/// 「挂上了还没报出来」的窗口。
pub struct DesktopTunnelHost {
    source: Arc<dyn DesktopTunnelSource>,
    /// 已知主机的**存储端口**实例（宿主注入）：桌面读 `{appData}/ssh_known_hosts.json`。
    known_hosts: Arc<dyn KnownHostsRepository>,
    /// 存活的隧道。`std::sync::Mutex` 而非 `tokio::sync::Mutex`：
    /// 临界区里没有任何 `.await`，开隧道那段耗时全部在锁**之外**。
    live: Mutex<Roster<Tunnel>>,
}

impl DesktopTunnelHost {
    pub fn new(
        source: Arc<dyn DesktopTunnelSource>,
        known_hosts: Arc<dyn KnownHostsRepository>,
    ) -> Self {
        Self {
            source,
            known_hosts,
            live: Mutex::new(Roster::new()),
        }
    }

    fn live(&self) -> MutexGuard<'_, Roster<Tunnel>> {
        self.live
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl TunnelTransport for DesktopTunnelHost {
    fn open(&self, spec: &TunnelSpec) -> Result<Option<TunnelHandle>, TunnelError> {
        // 已在表里：上游账本会拦掉重复开，但端口自己也不该把第二条隧道挂到同一个 spec 上。
        if self.live().value(spec).is_some() {
            debug!(
                target: "datazen::tunnel",
                via = %spec.via_host,
                viaPort = spec.via_port,
                "隧道已在表内，不重复开启"
            );
            return Ok(Some(TunnelHandle::new()));
        }

        let config = self
            .source
            .connection_config(&spec.route_ref)
            .map_err(|detail| open_fault(spec, detail))?;

        let tunnel = start_tunnel_blocking(spec, config, self.known_hosts.clone())?;

        let Some(tunnel) = tunnel else {
            // 调用方拿着 `via_port != 0` 要隧道，而存档配置里 `tunnel_kind` 是 `None`。
            // 报失败而不是放行：放行等于让这次连接直连真实库主机，绕过跳转腿。
            return Err(open_fault(
                spec,
                DriverError::InvalidConfig(
                    "调用方要求隧道，但存档隧道没有可用的 tunnelKind".into(),
                ),
            ));
        };

        let local_port = tunnel.local_port();
        self.live().record(spec, tunnel);
        debug!(
            target: "datazen::tunnel",
            via = %spec.via_host,
            viaPort = spec.via_port,
            localPort = local_port,
            "桌面隧道已开启"
        );
        Ok(Some(TunnelHandle::new()))
    }

    fn close(&self, spec: &TunnelSpec) -> Result<(), TunnelError> {
        // 摘走即拆除：本地监听的析构就是物理关闭点。
        match self.live().withdraw(spec) {
            Some(tunnel) => {
                debug!(
                    target: "datazen::tunnel",
                    via = %spec.via_host,
                    viaPort = spec.via_port,
                    localPort = tunnel.local_port(),
                    "桌面隧道已拆除"
                );
                Ok(())
            }
            None => {
                // 上游账本在归零时才会发 close，所以「表里没有」意味着它已经走了。
                // 报成功：重复关闭不该被当成错误，更不该造出一条假的失败现场。
                debug!(
                    target: "datazen::tunnel",
                    via = %spec.via_host,
                    viaPort = spec.via_port,
                    "桌面隧道本就不在表内，关闭按幂等放过"
                );
                Ok(())
            }
        }
    }

    fn revision(&self, route_ref: &NetworkRouteRef) -> Result<NetworkRouteRevision, TunnelError> {
        self.source.revision(route_ref).map_err(|detail| {
            warn!(
                target: "datazen::tunnel",
                route = %route_ref,
                "路由修订号不可用：{detail}"
            );
            TunnelError::RevisionUnavailable
        })
    }
}

impl RouteCatalog for DesktopTunnelHost {
    fn tunnel_spec(&self, route_ref: &NetworkRouteRef) -> Option<TunnelSpec> {
        self.source.tunnel_spec(route_ref)
    }

    fn direct_endpoint(&self, route_ref: &NetworkRouteRef) -> Option<String> {
        self.source.direct_endpoint(route_ref)
    }

    fn tunneled_endpoint(&self, spec: &TunnelSpec) -> Option<String> {
        let local_port = {
            let live = self.live();
            live.value(spec).map(Tunnel::local_port)
        };
        local_port.map(local_endpoint)
    }
}

/// 驱动永远拨回环：`127.0.0.1:<local_port>`。
fn local_endpoint(local_port: u16) -> String {
    format!("127.0.0.1:{local_port}")
}

/// 装配出桌面宿主该用的那个 [`NetworkProvider`]。
///
/// 拿到它之后，`ensure_tunnel` / `resolve_route` / `release_tunnel` 就有了实现。
/// 但**接线点不在本文件** —— `ResourceManager::acquire` 只在
/// `request.tunnel_spec` 为 `Some` 时才问账要引用，而
/// `platform::session_backend::open()` 目前把存档里的原始 `connection_config`
/// 原样交给 `acquire_resource`，从不填 `tunnel_spec`。接线属于
/// `src-tauri/src/platform/`（平台轨），本轨不越界改它。
pub fn desktop_network_provider(
    source: Arc<dyn DesktopTunnelSource>,
    known_hosts: Arc<dyn KnownHostsRepository>,
) -> Arc<dyn NetworkProvider> {
    let host = Arc::new(DesktopTunnelHost::new(source, known_hosts));
    let catalog: Arc<dyn RouteCatalog> = host.clone();
    Arc::new(RouteNetworkProvider::new(catalog, host))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shared_spec_is_located_by_full_value_equality() {
        let mut roster = Roster::<&'static str>::new();
        roster.record(&spec(22), "first");
        assert_eq!(roster.position(&spec(22)), Some(0));
        // 只差一跳端口就是**另一条**隧道 —— 共享身份是 spec 全等值。
        assert_eq!(roster.position(&spec(2222)), None);
    }

    #[test]
    fn recording_the_same_spec_twice_replaces_rather_than_duplicates() {
        let mut roster = Roster::<&'static str>::new();
        roster.record(&spec(22), "first");
        roster.record(&spec(22), "second");

        assert_eq!(roster.entries.len(), 1, "同一条隧道不该在表里出现两次");
        assert_eq!(roster.value(&spec(22)), Some(&"second"));
    }

    #[test]
    fn withdrawing_an_absent_spec_is_a_no_op() {
        let mut roster = Roster::<&'static str>::new();
        roster.record(&spec(22), "held");
        assert_eq!(roster.withdraw(&spec(22)), Some("held"));
        assert_eq!(roster.withdraw(&spec(22)), None);
        assert_eq!(roster.withdraw(&spec(2222)), None);
    }

    #[test]
    fn the_local_endpoint_always_points_at_loopback() {
        assert_eq!(local_endpoint(53124), "127.0.0.1:53124");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_tunnel_is_opened_on_the_caller_runtime_so_its_forwarder_outlives_the_call() {
        // 夹具隧道真的 `tokio::spawn` 出去一个 accept 循环；自建运行时会把它弄死。
        // 这里验证的是「桥用的确实是调用方的运行时」这条约束本身。
        let probe = tokio::spawn(async {
            tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .map(|listener| listener.local_addr())
        });
        let bound = probe.await;
        assert!(
            bound
                .expect("bind task runs on the calling runtime")
                .is_ok(),
            "accept 循环应当挂在调用方的运行时上"
        );
    }

    fn spec(via_port: u16) -> TunnelSpec {
        TunnelSpec::new(NetworkRouteRef::new("tunnel-1"), "jump.internal", via_port)
    }
}

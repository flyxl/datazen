//! 路由侧 [`NetworkProvider`] 实现：隧道引用在端口内共享、归还、回滚。
//!
//! 名字表达的是**职责**（按路由目录翻译隧道引用与端点），不是**形态**：本内核
//! 不依赖任何具体形态，谁来接线是宿主侧的事。
//!
//! # 为什么有本文件
//!
//! [`NetworkProvider`]（`datazen_platform_api::ports::network`，**冻结上游**）的四个方法里，
//! [`ensure_tunnel`] / [`release_tunnel`] 承诺「同一路由/隧道被多处使用时共享同一份引用计数」
//! 与「引用计数归零才真正拆除、重复释放是幂等的」。此前各连接路径分别持有隧道句柄，
//! 没有通过 `NetworkProvider` 统一落实这份共享引用契约。本文件提供 Runtime 侧的共享账，
//! 物理隧道仍由宿主的 [`TunnelTransport`] 打开和关闭。
//!
//! 本模块**不认识** SSH / HTTP 代理 / WebSocket 任何一种隧道：怎么开、开成什么样，
//! 是 [`TunnelTransport`] 实现方（宿主侧）的事；本模块只管「共享身份 + 引用生命周期」。
//!
//! # 三条不变式
//!
//! 1. **共享身份 = `TunnelSpec` 全等值**，不是路由、不是 `PoolKey`。同一 spec 被两处
//!    `ensure_tunnel` 时，**第二次不再开隧道**，只把已有条目加一份引用。
//! 2. **开隧道失败不留痕**。台账 [`TunnelLedger::acquire`] 在 `transport.open` 失败时
//!    既不建条目也不加引用；本文件因此**不会**把失败登记进名册。
//! 3. **重复释放是幂等的**。名册摘空之后再来一次 `release_tunnel`，直接成功返回，
//!    既不减引用，也不二次发 `transport.close`。
//!
//! # 「哪一条引用」怎么认：端口没有 consumer 身份，于是自造一张名册
//!
//! [`TunnelBinding`] 只有 `{ spec, ref_count }`，[`RequestContext`] 的六个字段里
//! **没有一个**是可用的隧道租约标识（`client_instance_id` 是「客户端实例」，不是会话；
//! `request_id` 是请求关联号，同一请求可能 `ensure_tunnel` 两次）。所以每次
//! `ensure_tunnel` 现场铸一个只此一份的 [`LeaseId`] 当**持有者身份**，
//! 记进名册；`release_tunnel` 从名册里摘走**一份**身份，交给台账
//! [`TunnelLedger::return_resource`] 归零。
//!
//! 名册是**身份表，不是计数表**：它是 `spec -> Vec<LeaseId>`，长度会变、值互不相同，
//! 全模块**不读它的长度做任何判断**（`TunnelSpec` 没有 `Ord` / `Hash`，只能线性查找）。
//! 系统里唯一参与「能不能释放」的整数仍然只有台账的 `refs`——这一点由
//! `tunnel::single_counter_audit` 那道闸门保证，本文件已登记进它的审计表。
//!
//! # 仍然不做的事（不是遗漏，是边界）
//!
//! * **不新增**任何 `PortError` 变体（上游冻结）：[`TunnelError`] 逐条映射到既有的十二个变体，
//!   文案只取 [`TunnelError::reason`] 的稳定字面码，`Debug` 不回显 `spec`。
//! * **不把**「单连接无状态驱动」包装成固定资源：释放时归还的是**隧道引用**，
//!   物理资源许可由 `resource` 层自己的账管，本文件碰不到也不假装管。
//! * **不改** `RequestContext`，也**不**要求调用方在请求里带 consumer id。
//! * **不改**既有稳定字面码：`desktopRouteUnknown` / `desktopTunnelReferenceNotHeld` /
//!   `desktopTunnelEndpointUnavailable` / `desktopTunnelTeardownUnconfirmed` 与
//!   `LeaseId` 前缀 `desktop-tunnel/` 都是**已对外可见**的值（进错误消息、进租约文本、
//!   进 trace reason），本轨道是纯重命名，故逐字保留；形态名是否要从字面码里退场，
//!   属于行为变更，另行裁定。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use datazen_platform_api::context::RequestContext;
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{NetworkRouteRef, NetworkRouteRevision};
use datazen_platform_api::ports::network::{NetworkProvider, RoutePlan, TunnelBinding, TunnelSpec};

use super::error::TunnelError;
use super::ledger::TunnelLedger;
use super::transport::TunnelTransport;
use super::TunnelRelease;
use super::TunnelState;
use crate::connection::types::LeaseId;

/// 路由目录：把 `NetworkRouteRef` 翻译成「走不走隧道、隧道建好后走哪个端点」。
///
/// **刻意不叫「存储」**：它是一份只读的映射，问的是「这条路由对应什么」，
/// 不承担任何生命周期（开不开、什么时候关都是 [`RouteNetworkProvider`] 与
/// [`TunnelTransport`] 的事）。
pub trait RouteCatalog: Send + Sync + 'static {
    /// 这条路由要不要走隧道。`None` = 直连（`via_port == 0` 之类）。
    fn tunnel_spec(&self, route_ref: &NetworkRouteRef) -> Option<TunnelSpec>;

    /// 直连端点（`host:port`）。仅在 [`tunnel_spec`] 返回 `None` 时被问。
    fn direct_endpoint(&self, route_ref: &NetworkRouteRef) -> Option<String>;

    /// 隧道**建好之后**的本地端点（`127.0.0.1:<本地端口>`）。
    ///
    /// 由宿主实现回答，因为只有宿主知道自己的监听端口；[`TunnelHandle`] 是不透明句柄，
    /// 拿不到端点。返回 `None` 意味着「隧道开着但路由不可用」，本文件会**立即回滚**。
    fn tunneled_endpoint(&self, spec: &TunnelSpec) -> Option<String>;
}

/// 唯一计数铁律下的受保护状态：台账 + 持有者名册，**同一个锁**。
///
/// 两者必须原子地一起动（开了引用就得记名字、还了引用就得摘名字），所以合成一个
/// 临界区；分成两把锁会造出「有引用没有名字」或「有名字没有引用」的窗口。
struct TunnelBooks {
    ledger: TunnelLedger,
    /// `spec -> 尚未归还的持有者身份`。**身份表，不是计数表**（见模块头）。
    roster: Vec<(TunnelSpec, Vec<LeaseId>)>,
}

impl TunnelBooks {
    fn position(&self, spec: &TunnelSpec) -> Option<usize> {
        self.roster.iter().position(|(held, _)| held == spec)
    }

    fn record(&mut self, spec: &TunnelSpec, holder: LeaseId) {
        match self.position(spec) {
            Some(index) => self.roster[index].1.push(holder),
            None => self.roster.push((spec.clone(), vec![holder])),
        }
    }

    /// 摘走**一份**身份；这一份用完即弃，名册空了就整桶删掉。
    fn withdraw(&mut self, spec: &TunnelSpec) -> Option<LeaseId> {
        let index = self.position(spec)?;
        let holder = self.roster[index].1.pop();
        if self.roster[index].1.is_empty() {
            self.roster.remove(index);
        }
        holder
    }
}

/// 路由 [`NetworkProvider`]：把路由目录翻译成隧道引用与端点。
///
/// 线程模型：所有状态在一把 [`Mutex`] 后面；四个端口方法里**没有 `.await`**，
/// 所以锁不会跨挂起点被持有（`.await` 之前必须放锁，见各方法内的作用域）。
pub struct RouteNetworkProvider {
    catalog: Arc<dyn RouteCatalog>,
    transport: Arc<dyn TunnelTransport>,
    books: Mutex<TunnelBooks>,
    /// 持有者身份的唯一来源。**只当身份发生器用**：取值从不参与任何判断，
    /// 因此它不是第二本账（对比 [`TunnelLedger`] 的 `refs` 是唯一计数）。
    tickets: AtomicU64,
}

impl RouteNetworkProvider {
    pub fn new(catalog: Arc<dyn RouteCatalog>, transport: Arc<dyn TunnelTransport>) -> Self {
        Self {
            catalog,
            transport: Arc::clone(&transport),
            books: Mutex::new(TunnelBooks {
                ledger: TunnelLedger::new(transport),
                roster: Vec::new(),
            }),
            tickets: AtomicU64::new(1),
        }
    }

    fn lock(&self) -> MutexGuard<'_, TunnelBooks> {
        self.books
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn mint_holder(&self) -> LeaseId {
        let ticket = self.tickets.fetch_add(1, Ordering::Relaxed);
        // 字面码 `desktop-tunnel/` 是**已对外可见的稳定标识**（会进 `LeaseId` 文本），
        // 本轨道是纯重命名，故逐字保留；形态名是否要从字面码里退场，另行裁定。
        LeaseId::new(format!("desktop-tunnel/{ticket}"))
    }

    /// 归还**一份**引用。名册里没有身份 = 这份引用早就还过了 = 幂等空转。
    fn give_back(&self, spec: &TunnelSpec) -> Option<TunnelRelease> {
        let mut books = self.lock();
        let holder = books.withdraw(spec)?;
        // 台账这一层理论上找不到刚记进去的身份（同一把锁、同一临界区），
        // 真出现 `None` 就当作「这份引用已经不在了」，与重复释放同义。
        books.ledger.return_resource(&holder)
    }

    fn revision_of(&self, route_ref: &NetworkRouteRef) -> Result<NetworkRouteRevision, PortError> {
        self.transport
            .revision(route_ref)
            .map_err(|error| tunnel_error_to_port(error))
    }

    fn holds_live_reference(&self, spec: &TunnelSpec) -> bool {
        matches!(self.lock().ledger.state(spec), TunnelState::Live)
    }

    /// 观测面：当前共享引用数（台账 `refs` 的投影，不是本模块自存的第二份）。
    pub fn reference_count(&self, spec: &TunnelSpec) -> Option<u32> {
        self.lock().ledger.ref_count(spec)
    }

    /// 观测面：这条隧道现在的状态（[`TunnelState::Absent`] = 台账里没有这条 spec）。
    pub fn state(&self, spec: &TunnelSpec) -> TunnelState {
        self.lock().ledger.state(spec)
    }
}

/// [`TunnelError`] → **既有** [`PortError`] 变体。不新增形状（上游冻结），
/// 文案只取稳定字面码：隧道错误不携带凭据，但 `spec` 的 `Debug` 也一并不进日志。
fn tunnel_error_to_port(error: TunnelError) -> PortError {
    match error {
        // 这一条本不该出现：每次 `ensure_tunnel` 都铸新身份，同一身份不可能重复持有。
        // 真出现了说明名册被外部改坏，重试无意义，因此不当成幂等冲突悄悄吞掉。
        TunnelError::AlreadyHeld { .. } => PortError::IdempotencyConflict,
        // 条目还在但状态不可共享（正在关闭 / 上一次关闭未证实）：这份申请拿到的是
        // 一个已经不可依赖的隧道，按 §4.3 fence 语义拒。
        TunnelError::NotShareable { .. } => PortError::StaleClaim,
        TunnelError::RevisionUnavailable => PortError::NotFound(error.reason().to_owned()),
        TunnelError::Transport { .. } => PortError::BackendUnavailable(error.reason().to_owned()),
    }
}

#[async_trait]
impl NetworkProvider for RouteNetworkProvider {
    async fn resolve_route(
        &self,
        _ctx: &RequestContext,
        route_ref: NetworkRouteRef,
    ) -> Result<RoutePlan, PortError> {
        let route_revision = self.revision_of(&route_ref)?;
        let Some(spec) = self.catalog.tunnel_spec(&route_ref) else {
            let endpoint = self
                .catalog
                .direct_endpoint(&route_ref)
                .ok_or_else(|| PortError::NotFound("desktopRouteUnknown".to_owned()))?;
            return Ok(RoutePlan::new(endpoint, false, route_revision));
        };

        // 不变式：`tunneled: true` 只在「本 provider 手上有这条 spec 的活引用」时发出去。
        // 否则调用方拿到的端点会指向一个随时可能被拆掉的隧道，影响仍在使用它的其他会话。
        if !self.holds_live_reference(&spec) {
            return Err(PortError::BackendUnavailable(
                "desktopTunnelReferenceNotHeld".to_owned(),
            ));
        }
        let endpoint = self.catalog.tunneled_endpoint(&spec).ok_or_else(|| {
            PortError::BackendUnavailable("desktopTunnelEndpointUnavailable".to_owned())
        })?;
        Ok(RoutePlan::new(endpoint, true, route_revision))
    }

    fn revision(&self, route_ref: NetworkRouteRef) -> Result<NetworkRouteRevision, PortError> {
        self.revision_of(&route_ref)
    }

    async fn ensure_tunnel(
        &self,
        _ctx: &RequestContext,
        spec: TunnelSpec,
    ) -> Result<TunnelBinding, PortError> {
        let holder = self.mint_holder();
        // 锁只包住台账这一次调用：开隧道是宿主端口里的阻塞动作，绝不能持锁跨越它。
        let acquired = self
            .lock()
            .ledger
            .acquire(Some(&spec), &holder)
            .map_err(tunnel_error_to_port)?;

        // `None` = 这一条 spec 根本不必开隧道（`via_port == 0`，或端口当场答复「不用」）。
        // 没有引用可还，也就没有名册要记——不记才是最诚实的记法。
        let Some(lease) = acquired else {
            return Ok(TunnelBinding { spec, ref_count: 0 });
        };

        {
            let mut books = self.lock();
            books.record(&spec, holder);
        }

        // 后置校验：隧道开着不等于路由可用。拿不到本地端点就是**半开状态**，
        // 这时把刚拿到的那一份原样还掉（归零就真拆），不留残引用、不留半开条目。
        if self.catalog.tunneled_endpoint(&spec).is_none() {
            self.give_back(&spec);
            return Err(PortError::BackendUnavailable(
                "desktopTunnelEndpointUnavailable".to_owned(),
            ));
        }

        Ok(lease.binding)
    }

    async fn release_tunnel(&self, binding: &TunnelBinding) -> Result<(), PortError> {
        // 端口只认「释放一次引用」这一件事，且明写重复释放是幂等的。
        // 名册里已经摘空就什么都不做：既不会再减一次引用（`refs` 不会低于 0），
        // 也不会二次发 `transport.close`。
        match self.give_back(&binding.spec) {
            None => {}
            Some(release) if matches!(release.state, TunnelState::Unconfirmed) => {
                // 关闭未证实：台账按设计**保留**条目等后续观察，本层只登记。
                // 这里回 `Err` 反而危险——它会诱使调用方重试释放，而重试在台账里
                // 只是一次无副作用的早退，并不会真的把隧道拆掉。
                tracing::warn!(
                    target: "datazen_runtime::tunnel",
                    reason = "desktopTunnelTeardownUnconfirmed",
                    "隧道关闭未证实：台账条目保留，等待后续观察"
                );
            }
            Some(_) => {}
        }
        Ok(())
    }
}

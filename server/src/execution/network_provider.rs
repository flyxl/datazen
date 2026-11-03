//! `NetworkProvider` 的服务端适配器（WR-D4 执行接线）。
//!
//! 端口语义（`datazen::execution::application::network`）：桌面与 server 共用
//! 一个端口；server 的 SSRF 白名单是「端点校验」的**部署侧强化**，不是端口的
//! 第二套语义。本适配器的裁决：
//!
//! - 路由表与隧道引用计数都是**进程内存形态**（单实例 v1；多 worker 形态的
//!   路由仲裁是后续切片）。路由经 `register_route` 显式登记（宿主装配时
//!   注入），未登记的 `route_ref` 一律 `NotFound`。
//! - **出站白名单 fail-closed**：构造时注入允许的端点主机清单；`resolve_route`
//!   解析出的端点主机不在清单内 → `PortError::BackendUnavailable`（端口词汇
//!   无独立「禁止」变体，登记为映射裁决：消息明说 egress 被白名单拦截，
//!   不含机密）。**空清单 = 全部拒绝**——没有白名单的部署不出网。
//! - 隧道引用计数按**整个 `TunnelSpec`** 键控（跳板不同不复用——与端口的
//!   语义钉一致）；`release_tunnel` 幂等（计数归零后继续释放是 no-op）。
//! - 路由版本号：每次对同一 `route_ref` 的重新登记 +1（凭据/跳板变更让
//!   `PoolKey.networkRouteRevision` 前移，旧物理连接退出池）。

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use datazen::execution::application::network::{
    NetworkProvider, RoutePlan, TunnelBinding, TunnelSpec,
};
use datazen::shared::context::RequestContext;
use datazen::shared::error::PortError;
use datazen::shared::id::{NetworkRouteRef, NetworkRouteRevision};

#[derive(Debug, Clone)]
struct RegisteredRoute {
    endpoint: String,
    tunneled: bool,
    revision: u64,
}

#[derive(Default)]
struct Inner {
    routes: HashMap<NetworkRouteRef, RegisteredRoute>,
    /// 整个 `TunnelSpec` 键控的引用计数（归零即出表）。键是 spec 的规范
    /// 字符串投影（`TunnelSpec` 只实现 `Eq` 不实现 `Hash`）。
    tunnels: HashMap<String, u32>,
}

/// `TunnelSpec` 的规范键：路由引用 | 跳板 | 端口。跳板主机词表不含 `|`
/// （主机名/IP 的合法字符集外字符；与 §6.2 隔离键的输入契约同款登记）。
fn spec_key(spec: &TunnelSpec) -> String {
    format!(
        "{}|{}|{}",
        spec.route_ref.as_str(),
        spec.via_host,
        spec.via_port
    )
}

/// 出站白名单 + 内存路由表的服务端适配器。一个进程一份。
pub struct ServerNetworkProvider {
    /// 允许出站的端点主机（精确匹配，大小写不敏感）。空 = 全拒。
    allowed_hosts: Vec<String>,
    inner: Mutex<Inner>,
}

impl std::fmt::Debug for ServerNetworkProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerNetworkProvider")
            .field("allowed_hosts", &self.allowed_hosts)
            .field("inner", &"route table + tunnel refcounts")
            .finish()
    }
}

impl ServerNetworkProvider {
    /// 构造。`allowed_hosts` 是出站白名单（主机名/IP，不含端口；比较大小写
    /// 不敏感）。**空清单 = 全部出站拒绝**（fail-closed）。
    pub fn new(allowed_hosts: Vec<String>) -> Self {
        let allowed_hosts = allowed_hosts
            .into_iter()
            .map(|host| host.to_ascii_lowercase())
            .collect();
        Self {
            allowed_hosts,
            inner: Mutex::new(Inner::default()),
        }
    }

    /// 登记一条路由（宿主装配/管理面注入）。同一 `route_ref` 重新登记使
    /// 版本号 +1（旧物理资源按 PoolKey 语义退出池）。返回当前版本。
    pub fn register_route(
        &self,
        route_ref: NetworkRouteRef,
        endpoint: impl Into<String>,
        tunneled: bool,
    ) -> NetworkRouteRevision {
        let mut inner = self.inner.lock().expect("route table lock");
        let entry = inner
            .routes
            .entry(route_ref)
            .or_insert_with(|| RegisteredRoute {
                endpoint: String::new(),
                tunneled: false,
                revision: 0,
            });
        entry.endpoint = endpoint.into();
        entry.tunneled = tunneled;
        entry.revision += 1;
        NetworkRouteRevision::new(entry.revision)
    }

    /// 端点主机提取：`scheme://[userinfo@]host[:port][/path]` 的 host 段。
    /// IPv6 裸地址的 `:` 分隔是已知简化（部署清单里用括号形登记）。
    fn endpoint_host(endpoint: &str) -> Option<String> {
        let rest = endpoint.split_once("://")?.1;
        let rest = rest.split_once('@').map_or(rest, |(_, tail)| tail);
        let host_port = rest.split(['/']).next()?;
        let host = match host_port.rsplit_once(':') {
            // 端口段必须是纯数字才当端口（保护裸 IPv6）。
            Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() => {
                host
            }
            _ => host_port,
        };
        let host = host.trim_start_matches('[').trim_end_matches(']');
        (!host.is_empty()).then(|| host.to_ascii_lowercase())
    }

    /// 出站白名单判定（宿主 EgressGate 闸门复用；`pub(crate)` 供
    /// bootstrap 的驱动出口闸注入——driver_exec.rs 账目表「未接线 = 不设防」
    /// 的接线件）。
    pub(crate) fn whitelist_check(&self, endpoint: &str) -> Result<(), PortError> {
        let host = Self::endpoint_host(endpoint).ok_or_else(|| {
            PortError::BackendUnavailable(format!("route endpoint is not parseable: {endpoint}"))
        })?;
        if self.allowed_hosts.iter().any(|allowed| allowed == &host) {
            return Ok(());
        }
        Err(PortError::BackendUnavailable(format!(
            "egress to {host} is blocked by the server egress whitelist"
        )))
    }
}

#[async_trait]
impl NetworkProvider for ServerNetworkProvider {
    async fn resolve_route(
        &self,
        _ctx: &RequestContext,
        route_ref: NetworkRouteRef,
    ) -> Result<RoutePlan, PortError> {
        let (endpoint, tunneled, revision) = {
            let inner = self.inner.lock().expect("route table lock");
            let route = inner
                .routes
                .get(&route_ref)
                .ok_or_else(|| PortError::NotFound("network route".into()))?;
            (route.endpoint.clone(), route.tunneled, route.revision)
        };
        self.whitelist_check(&endpoint)?;
        Ok(RoutePlan::new(
            endpoint,
            tunneled,
            NetworkRouteRevision::new(revision),
        ))
    }

    fn revision(&self, route_ref: NetworkRouteRef) -> Result<NetworkRouteRevision, PortError> {
        let inner = self.inner.lock().expect("route table lock");
        inner
            .routes
            .get(&route_ref)
            .map(|route| NetworkRouteRevision::new(route.revision))
            .ok_or_else(|| PortError::NotFound("network route".into()))
    }

    async fn ensure_tunnel(
        &self,
        _ctx: &RequestContext,
        spec: TunnelSpec,
    ) -> Result<TunnelBinding, PortError> {
        // 出站白名单同样约束隧道跳板（跳板是出站端点的一种）。
        self.whitelist_check(&format!("ssh://{}", spec.via_host))?;
        let mut inner = self.inner.lock().expect("tunnel table lock");
        let count = inner.tunnels.entry(spec_key(&spec)).or_insert(0);
        *count += 1;
        Ok(TunnelBinding {
            spec,
            ref_count: *count,
        })
    }

    async fn release_tunnel(&self, binding: &TunnelBinding) -> Result<(), PortError> {
        let mut inner = self.inner.lock().expect("tunnel table lock");
        if let Some(count) = inner.tunnels.get_mut(&spec_key(&binding.spec)) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                inner.tunnels.remove(&spec_key(&binding.spec));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen::shared::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};

    fn context() -> RequestContext {
        RequestContext::new(
            OrganizationId::new("org-1"),
            PrincipalId::new("user-1"),
            None,
            ClientInstanceId::new("client-1"),
            RequestId::new("req-1"),
            None,
        )
    }

    fn make_provider(hosts: &[&str]) -> ServerNetworkProvider {
        ServerNetworkProvider::new(hosts.iter().map(|h| h.to_string()).collect())
    }

    #[tokio::test]
    async fn resolve_route_follows_the_registered_plan() {
        let provider = make_provider(&["db.internal"]);
        let route = NetworkRouteRef::new("route://prod/main");
        provider.register_route(route.clone(), "postgresql://db.internal:5432/app", false);
        let plan = provider
            .resolve_route(&context(), route)
            .await
            .expect("plan");
        assert_eq!(plan.endpoint, "postgresql://db.internal:5432/app");
        assert!(!plan.tunneled);
        assert_eq!(plan.route_revision, NetworkRouteRevision::new(1));
    }

    #[tokio::test]
    async fn reregistering_a_route_advances_its_revision() {
        let provider = make_provider(&["db.internal"]);
        let route = NetworkRouteRef::new("route://prod/main");
        provider.register_route(route.clone(), "postgresql://db.internal:5432/app", false);
        provider.register_route(route.clone(), "postgresql://db2.internal:5432/app", true);
        assert_eq!(
            provider.revision(route).expect("revision"),
            NetworkRouteRevision::new(2)
        );
    }

    #[tokio::test]
    async fn egress_whitelist_blocks_unknown_hosts_and_fail_closed_on_empty() {
        let provider = make_provider(&["db.internal"]);
        let route = NetworkRouteRef::new("route://prod/main");
        provider.register_route(route.clone(), "postgresql://db.internal:5432/app", false);
        provider
            .resolve_route(&context(), route.clone())
            .await
            .expect("whitelisted host passes");
        provider.register_route(route.clone(), "postgresql://evil.example:5432/app", false);
        let blocked = provider.resolve_route(&context(), route.clone()).await;
        assert!(
            matches!(&blocked, Err(PortError::BackendUnavailable(message)) if message.contains("whitelist")),
            "未登记主机被拦：{blocked:?}"
        );
        // 空清单 = 全拒（fail-closed）。
        let locked_down = make_provider(&[]);
        locked_down.register_route(route.clone(), "postgresql://db.internal:5432/app", false);
        assert!(locked_down.resolve_route(&context(), route).await.is_err());
    }

    #[tokio::test]
    async fn tunnel_refcounts_are_keyed_by_the_whole_spec_and_release_is_idempotent() {
        let provider = make_provider(&["jump.internal"]);
        let ctx = context();
        let spec = TunnelSpec::new(NetworkRouteRef::new("route://prod"), "jump.internal", 22);
        let first = provider
            .ensure_tunnel(&ctx, spec.clone())
            .await
            .expect("bind");
        let second = provider
            .ensure_tunnel(&ctx, spec.clone())
            .await
            .expect("bind again");
        assert_eq!(second.ref_count, 2, "同规格共享同一条隧道");
        provider.release_tunnel(&first).await.expect("release");
        let still = provider
            .ensure_tunnel(&ctx, spec.clone())
            .await
            .expect("rebind");
        assert_eq!(still.ref_count, 2);
        provider.release_tunnel(&still).await.expect("release");
        provider
            .release_tunnel(&still)
            .await
            .expect("release is idempotent");
        let fresh = provider
            .ensure_tunnel(&ctx, spec)
            .await
            .expect("rebind from zero");
        assert_eq!(fresh.ref_count, 1, "归零后真正拆除");
    }

    #[tokio::test]
    async fn unregistered_routes_are_not_found() {
        let provider = make_provider(&["db.internal"]);
        let missing = provider
            .resolve_route(&context(), NetworkRouteRef::new("route://none"))
            .await;
        assert!(matches!(missing, Err(PortError::NotFound(_))));
        assert!(provider
            .revision(NetworkRouteRef::new("route://none"))
            .is_err());
    }
}

//! `NetworkProvider`：网络出口端口。
//!
//! 词汇表（§4.4）：`NetworkRouteRef`、`NetworkRouteRevision`、`RoutePlan`、`TunnelSpec`、
//! `TunnelBinding`。
//!
//! 桌面与 server 共用一个端口，返回**端点字符串**；server 的 SSRF 白名单是「端点校验」的
//! 部署侧强化，不是端口的第二套语义。共享隧道引用计数在端口内维护。

use async_trait::async_trait;

use crate::context::RequestContext;
use crate::error::PortError;
use crate::id::{NetworkRouteRef, NetworkRouteRevision};

/// 路由方案。落地成机器无关的端点描述，不假设是 socket。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutePlan {
    /// 机器无关的端点串。`scheme` 决定后续解析方式，端口不做协议适配。
    pub endpoint: String,
    /// 是否经由隧道到达。
    pub tunneled: bool,
    /// 本次路由方案绑定的路由版本。`PoolKey` 的 `networkRouteRevision` 分量。
    pub route_revision: NetworkRouteRevision,
}

impl RoutePlan {
    pub fn new(
        endpoint: impl Into<String>,
        tunneled: bool,
        route_revision: NetworkRouteRevision,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            tunneled,
            route_revision,
        }
    }
}

/// 隧道规格：如何到端点（跳板、直连、SOCKS 等）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelSpec {
    pub route_ref: NetworkRouteRef,
    pub via_host: String,
    pub via_port: u16,
}

impl TunnelSpec {
    pub fn new(route_ref: NetworkRouteRef, via_host: impl Into<String>, via_port: u16) -> Self {
        Self {
            route_ref,
            via_host: via_host.into(),
            via_port,
        }
    }
}

/// 已建立的隧道绑定。**同一 `TunnelSpec` 共享引用计数**：
/// 计数降到 0 时隧道才真正拆除。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelBinding {
    pub spec: TunnelSpec,
    /// 引用计数快照。仅供观测，不用于判断能否释放（释放以 `release_tunnel` 的调用为准）。
    pub ref_count: u32,
}

#[async_trait]
pub trait NetworkProvider: Send + Sync + 'static {
    /// 解析路由。未配置路由的连接由驱动按普通 DNS 直连，本方法不参与。
    async fn resolve_route(
        &self,
        ctx: &RequestContext,
        route_ref: NetworkRouteRef,
    ) -> Result<RoutePlan, PortError>;

    /// 路由版本。`PoolKey` 的 `networkRouteRevision` 分量用它。
    fn revision(&self, route_ref: NetworkRouteRef) -> Result<NetworkRouteRevision, PortError>;

    /// 共享引用：同一路由/隧道被多处使用时返回同一 binding 的引用计数句柄。
    async fn ensure_tunnel(
        &self,
        ctx: &RequestContext,
        spec: TunnelSpec,
    ) -> Result<TunnelBinding, PortError>;

    /// 释放一次引用。引用计数归零才真正拆除；重复释放是幂等的。
    async fn release_tunnel(&self, binding: &TunnelBinding) -> Result<(), PortError>;
}

/// 便捷：`RoutePlan` 的端点解析辅助，driver 用它区分「协议前缀://主机:端口」。
pub fn endpoint_scheme(endpoint: &str) -> Option<&str> {
    endpoint.split_once("://").map(|(scheme, _)| scheme)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::Counter;

    #[test]
    fn tunnel_sharing_is_keyed_by_the_whole_spec_not_just_the_route() {
        let route = NetworkRouteRef::new("route://bastion/prod");
        let a = TunnelSpec::new(route.clone(), "jump.internal", 22);
        let b = TunnelSpec::new(route, "jump.internal", 22);
        let c = TunnelSpec::new(
            NetworkRouteRef::new("route://bastion/prod"),
            "other.internal",
            22,
        );
        assert_eq!(a, b, "同规格必须共享同一条隧道");
        assert_ne!(a, c, "跳板不同不得复用同一条隧道");
    }

    #[test]
    fn binding_exposes_a_refcount_for_observability() {
        let spec = TunnelSpec::new(
            NetworkRouteRef::new("route://bastion/prod"),
            "jump.internal",
            22,
        );
        let binding = TunnelBinding {
            spec: spec.clone(),
            ref_count: 2,
        };
        assert_eq!(binding.spec, spec);
        assert_eq!(binding.ref_count, 2);
    }

    #[test]
    fn route_plan_carries_the_revision_used_by_the_pool_key() {
        let plan = RoutePlan::new(
            "postgres://db.internal:5432",
            false,
            NetworkRouteRevision::new(7),
        );
        assert_eq!(plan.route_revision.counter(), Counter::new(7));
        assert!(!plan.tunneled);
        assert!(
            RoutePlan::new(
                "socks5://127.0.0.1:1080",
                true,
                NetworkRouteRevision::new(0)
            )
            .tunneled
        );
    }

    #[test]
    fn endpoint_scheme_is_extracted_without_touching_the_rest() {
        assert_eq!(
            endpoint_scheme("postgres://db.internal:5432"),
            Some("postgres")
        );
        assert_eq!(endpoint_scheme("socks5://127.0.0.1:1080"), Some("socks5"));
        assert_eq!(endpoint_scheme("db.internal:5432"), None);
    }

    #[test]
    fn provider_never_sees_a_credential() {
        // 端口签名里没有任何凭据参数：网络出口与秘密材料彻底分离。
        // `NetworkRouteRef` 走 opaque newtype，Debug 输出被整体抹去。
        let route = NetworkRouteRef::new("route://bastion/prod");
        assert_eq!(route.as_str(), "route://bastion/prod");
        assert_eq!(format!("{route:?}"), "NetworkRouteRef(<redacted>)");
    }
}

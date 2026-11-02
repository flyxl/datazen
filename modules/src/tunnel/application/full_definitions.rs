//! 用例：全量名单（`get_tunnels` 流的业务核）。
//!
//! 端口只有两档：名单（摘要，**不带密钥**）与单条取用（全量）。这个载荷是
//! **全量**且形状冻结，因此由两档组合而成：先取名单，再逐条取用。名单与
//! 取用之间消失的条目**跳过**而不是报错——两档之间没有事务，另一个写入方
//! 完全可能先删一步。
//!
//! 归位自桌面命令壳 `commands/tunnel.rs::get_tunnels` 的内联编排（WR-A2：
//! 宿主 command 无业务编排）。IPC 载荷不变；端口失败原文透传，宿主照旧以
//! 命令名上下文映射。生产代码没有 `get_tunnels` 的调用方（所有列表路径都
//! 走 `get_tunnel_summaries`），但命令的形状与载荷不变，组合语义仍需唯一
//! 权威——包括「消失条目跳过」这条取舍，由本文件的测试钉住。

use datazen_driver_api::SavedTunnel;

use crate::shared::error::PortError;
use crate::shared::scope::OrganizationScope;
use crate::tunnel::domain::TunnelRepository;

/// 全量名单：名单档 × 单条取用。
///
/// 返回顺序跟随名单档；名单之后消失的条目跳过（见模块文档）。
pub async fn list_full_definitions(
    tunnels: &dyn TunnelRepository,
    scope: &OrganizationScope,
) -> Result<Vec<SavedTunnel>, PortError> {
    let summaries = tunnels.list(scope).await?;
    let mut full = Vec::with_capacity(summaries.len());
    for summary in summaries {
        if let Some(tunnel) = tunnels.get(scope, &summary.id).await? {
            full.push(tunnel);
        }
    }
    Ok(full)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::id::OrganizationId;
    use crate::tunnel::test_support::MemoryTunnelStore;
    use async_trait::async_trait;
    use datazen_driver_api::{SavedTunnelSummary, TunnelKind, TunnelUsage};

    fn scope() -> OrganizationScope {
        OrganizationScope::new(OrganizationId::new("org-1"))
    }

    fn saved(id: &str, name: &str) -> SavedTunnel {
        SavedTunnel {
            id: id.into(),
            name: name.into(),
            kind: TunnelKind::Ssh,
            ssh: None,
            http_proxy: None,
            websocket: None,
        }
    }

    /// 名单里有、取用时已被别的写入方删掉的条目要**跳过**。
    ///
    /// [`MemoryTunnelStore`] 的两档永远一致（同一个 `Vec`），变不出「名单里
    /// 有、取用时没有」的局面——所以包一层，让 `get` 对指定 id 返回 `None`。
    /// 被钉的是用例的取舍（跳过而不是报错），不是替身的巧合。
    struct VanishingTunnelStore {
        inner: MemoryTunnelStore,
        vanished: std::sync::Mutex<Vec<String>>,
    }

    impl VanishingTunnelStore {
        fn new() -> Self {
            Self {
                inner: MemoryTunnelStore::new(),
                vanished: std::sync::Mutex::new(Vec::new()),
            }
        }

        /// 把 `id` 标记为「名单之后消失」：`get` 将返回 `None`。
        fn vanish(&self, id: &str) {
            self.vanished.lock().expect("vanished lock").push(id.into());
        }
    }

    #[async_trait]
    impl TunnelRepository for VanishingTunnelStore {
        async fn list(
            &self,
            scope: &OrganizationScope,
        ) -> Result<Vec<SavedTunnelSummary>, PortError> {
            self.inner.list(scope).await
        }

        async fn get(
            &self,
            scope: &OrganizationScope,
            tunnel_id: &str,
        ) -> Result<Option<SavedTunnel>, PortError> {
            if self
                .vanished
                .lock()
                .expect("vanished lock")
                .iter()
                .any(|id| id == tunnel_id)
            {
                return Ok(None);
            }
            self.inner.get(scope, tunnel_id).await
        }

        async fn save(
            &self,
            scope: &OrganizationScope,
            tunnel: SavedTunnel,
        ) -> Result<(), PortError> {
            self.inner.save(scope, tunnel).await
        }

        async fn delete(
            &self,
            scope: &OrganizationScope,
            tunnel_id: &str,
        ) -> Result<(), PortError> {
            self.inner.delete(scope, tunnel_id).await
        }

        async fn usage(
            &self,
            scope: &OrganizationScope,
            tunnel_id: &str,
        ) -> Result<TunnelUsage, PortError> {
            self.inner.usage(scope, tunnel_id).await
        }
    }

    /// 两档组合：返回的是全量定义（含配置块），顺序跟随名单档。
    #[tokio::test]
    async fn combines_the_two_tiers_in_list_order() {
        let store = MemoryTunnelStore::new();
        let mut first = saved("t1", "Prod Bastion");
        first.http_proxy = Some(datazen_driver_api::HttpProxyTunnelConfig {
            enabled: true,
            host: "proxy.corp".into(),
            port: 8080,
            scheme: "http".into(),
            username: None,
            password: None,
            headers: None,
            connect_timeout_secs: 30,
        });
        store.save_tunnel(first);
        store.save_tunnel(saved("t2", "Other"));

        let full = list_full_definitions(&store, &scope())
            .await
            .expect("combining the tiers must succeed");
        assert_eq!(
            full.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            vec!["t1", "t2"],
            "the full list follows the summary tier's order"
        );
        assert!(
            full[0].http_proxy.is_some(),
            "the full tier carries the configuration blocks, not just summaries"
        );
        // 全量路径不脱敏：配置块里的主机名原样随行（与 get_tunnels 载荷一致）。
        assert_eq!(
            full[0].http_proxy.as_ref().expect("proxy config").host,
            "proxy.corp"
        );
    }

    /// 名单与取用之间消失的条目跳过而不是报错。
    #[tokio::test]
    async fn skips_entries_that_vanish_between_the_tiers() {
        let store = VanishingTunnelStore::new();
        store.inner.save_tunnel(saved("t1", "Prod Bastion"));
        store.inner.save_tunnel(saved("t2", "Doomed"));
        // 名单里有 t2，但取用时它已被别的写入方删掉。
        store.vanish("t2");

        let full = list_full_definitions(&store, &scope())
            .await
            .expect("a vanished entry must be skipped, not surfaced as an error");
        assert_eq!(
            full.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            vec!["t1"],
            "the vanished entry is skipped; the survivor comes through in full"
        );
    }

    /// 端口失败原文透传：组合层不吞错误、不改文案。
    #[tokio::test]
    async fn propagates_port_errors_untouched() {
        // MemoryTunnelStore 从不失败；用一个最小的失败替身钉透传语义。
        struct FailingStore;
        #[async_trait]
        impl TunnelRepository for FailingStore {
            async fn list(
                &self,
                _scope: &OrganizationScope,
            ) -> Result<Vec<SavedTunnelSummary>, PortError> {
                Err(PortError::BackendUnavailable("store exploded".into()))
            }
            async fn get(
                &self,
                _scope: &OrganizationScope,
                _tunnel_id: &str,
            ) -> Result<Option<SavedTunnel>, PortError> {
                Ok(None)
            }
            async fn save(
                &self,
                _scope: &OrganizationScope,
                _tunnel: SavedTunnel,
            ) -> Result<(), PortError> {
                Ok(())
            }
            async fn delete(
                &self,
                _scope: &OrganizationScope,
                _tunnel_id: &str,
            ) -> Result<(), PortError> {
                Ok(())
            }
            async fn usage(
                &self,
                _scope: &OrganizationScope,
                _tunnel_id: &str,
            ) -> Result<TunnelUsage, PortError> {
                Ok(TunnelUsage {
                    connection_ids: Vec::new(),
                    connection_names: Vec::new(),
                })
            }
        }

        let err = list_full_definitions(&FailingStore, &scope())
            .await
            .expect_err("a port failure must propagate");
        assert_eq!(
            err.to_string(),
            PortError::BackendUnavailable("store exploded".into()).to_string(),
            "the combination layer must not rewrite the port's error text"
        );
    }
}

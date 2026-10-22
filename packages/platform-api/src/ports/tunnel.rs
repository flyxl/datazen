//! `TunnelRepository`：隧道定义的持久化端口。
//!
//! 隧道定义是**独立聚合**：连接侧只存 `tunnelId`，解引用发生在**建连时**而不是
//! 打开表单时（[隧道 §4.1](../../backend/tunnel.md#41-引用而非拷贝)）。因此本端口
//! **不接受 connection id**——没有「按连接取隧道」这种查询，连接与隧道之间只有
//! 一条由调用方自己维护的引用。
//!
//! # 两档：名单不带密钥，单条取用才给全量
//!
//! [`TunnelRepository::list`] 返回**摘要**（`id` / `name` / `kind`），**不带**任何
//! 口令、密钥或 token；[`TunnelRepository::get`] 才返回全量定义。这不是风格选择：
//! 名单路径要进选择器与 IPC 载荷，把明文凭据混进去等于把它下发到渲染进程
//! （[隧道 §4.2](../../backend/tunnel.md#42-加密与最小暴露)）。
//!
//! # 组织限定
//!
//! 每个方法都带 [`OrganizationScope`]：越组织访问按**「不存在」**处理（`list` →
//! 空列表、`get` → `None`），**不**返回「无权限」——后者会泄漏「存在但无权」
//! （CM-05）。桌面是单租户：它只承认自己那一个组织，别的组织一律按「不存在」
//! 处理。
//!
//! # 词汇表
//!
//! 本端口**不另造**行类型：隧道的配置形状（`SshTunnelConfig` /
//! `HttpProxyTunnelConfig` / `WebSocketTunnelConfig` / `TunnelKind`）本来就住在
//! 驱动契约 crate `datazen-driver-api`——那是**每个形态都已经链接**的那一份。
//! 摘要类型 [`SavedTunnelSummary`] 与引用投影 [`TunnelUsage`] 同样已经在那里：
//! 再造第二份摘要类型只会在两处之间慢慢漂移，而 IPC 载荷必须逐字不变。
//!
//! # 落盘加密不在端口层
//!
//! 端口是**内存内明文**语义：主钥匙串在宿主手里，加密与落盘格式由实现方负责
//! （桌面：与连接密码同源的 AES-256-GCM）。端口不重试、不鉴权、不缓存。

use async_trait::async_trait;
use datazen_driver_api::{SavedTunnel, SavedTunnelSummary, TunnelUsage};

use crate::error::PortError;
use crate::ports::scope::OrganizationScope;

/// 隧道定义的持久化端口。
///
/// 实现的**存储形态**自由：桌面落 `{appData}/tunnels.json`（加密），服务端可以落
/// 服务端存储目录或组织级存储——端口只承诺「按组织取回 / 写回隧道定义」。
#[async_trait]
pub trait TunnelRepository: Send + Sync + 'static {
    /// 名单：**只带摘要**，不含任何口令 / 密钥 / token。
    ///
    /// 越组织返回空列表而不是错误（CM-05）。
    async fn list(&self, scope: &OrganizationScope) -> Result<Vec<SavedTunnelSummary>, PortError>;

    /// 单条取用：全量定义（**含明文凭据**，只在宿主内存与建连路径里流转）。
    ///
    /// 缺失或越组织一律返回 `None`——**不是** `NotFound`。
    async fn get(
        &self,
        scope: &OrganizationScope,
        tunnel_id: &str,
    ) -> Result<Option<SavedTunnel>, PortError>;

    /// 保存（新建 / 更新同一条定义）。
    ///
    /// 越组织的写入按「不存在」处理：本端口不替调用方做越权判定，只承诺
    /// **不把数据写进别的组织的命名空间**。
    async fn save(&self, scope: &OrganizationScope, tunnel: SavedTunnel) -> Result<(), PortError>;

    /// 删除一条定义。**不做引用检查**——删除前的引用方查询走 [`Self::usage`]。
    async fn delete(&self, scope: &OrganizationScope, tunnel_id: &str) -> Result<(), PortError>;

    /// **反向投影**：哪些连接引用了这条隧道。
    ///
    /// `connection_ids` 与 `connection_names` 位置对齐。它与 [`Self::get`] 分开，
    /// 是因为「这条隧道被谁用着」是**连接侧**的事实，不属于隧道定义本身。
    async fn usage(
        &self,
        scope: &OrganizationScope,
        tunnel_id: &str,
    ) -> Result<TunnelUsage, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::OrganizationId;

    fn scope() -> OrganizationScope {
        OrganizationScope::new(OrganizationId::new("org-1"))
    }

    #[test]
    fn the_list_tier_carries_no_secret_field() {
        // 摘要类型是**驱动契约 crate** 里那一份：名单进选择器与 IPC 载荷，
        // 它不得带任何凭据字段（driver-api 侧另有单测逐字段断言）。
        let summary = SavedTunnelSummary {
            id: "t1".into(),
            name: "Prod Bastion".into(),
            kind: datazen_driver_api::TunnelKind::Ssh,
        };
        let json = serde_json::to_value(&summary).expect("serialize summary");
        // 字段集就是三个：`kind` 的**取值**（`ssh` / `httpProxy`）不是配置块。
        let mut fields = json
            .as_object()
            .expect("object")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        fields.sort();
        assert_eq!(
            fields,
            vec!["id", "kind", "name"],
            "名单档只有三个字段: {json}"
        );
        for forbidden in [
            "password",
            "passphrase",
            "authToken",
            "ssh",
            "httpProxy",
            "websocket",
        ] {
            assert!(
                json.get(forbidden).is_none(),
                "名单摘要泄漏了 {forbidden}: {json}"
            );
        }
    }

    #[test]
    fn usage_projection_carries_connection_references_only() {
        let usage = TunnelUsage {
            connection_ids: vec!["c1".into()],
            connection_names: vec!["Alpha".into()],
        };
        let json = serde_json::to_string(&usage).expect("serialize usage");
        assert!(json.contains("c1"));
        assert!(!json.contains("password"));
    }

    #[test]
    fn scope_is_the_only_organization_input() {
        // 端口方法不接受 connection id：隧道定义是独立聚合。
        let scope = scope();
        assert_eq!(scope.organization_id(), &OrganizationId::new("org-1"));
    }
}

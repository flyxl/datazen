//! 宿主侧注入点（WR-F5）：宿主（web 形态即服务端宿主）装配 [`crate::tunnel`] 端口面的**来源三件套**。
//!
//! 隧道机制（SSH / HTTP CONNECT / WebSocket 本地腿、probe、加密）已在
//! `modules` 的 tunnel feature 共用 —— 桌面与 web 形态跑同一份实现（见
//! `apps/desktop/src-tauri/src/tunnel/mod.rs` 的桌面装配）。本模块只为 web
//! 宿主定义**注入来源**：存储根、凭据、监听点。仅注入来源，不加机制。
//!
//! # 三件套
//!
//! 1. **存储根**（[`HostInMemoryTunnelRoot`] + known-hosts 半边
//!    [`HostInMemoryKnownHosts`]）：对应桌面 `{appData}/tunnels.json`（加密落盘）
//!    的 web 半边。
//!    **裁决：内存语义，不选 OPFS。** web 宿主（服务端）的持久化由服务端存储
//!    目录负责（server storage，本 wave 白名单外，后续装配时换实现）；浏览器
//!    侧根本没有这个注入点的执行位，OPFS 既够不着也会造出第二份真相。内存根
//!    保持端口「可装配、可同语义测试」，落盘交给装配方换实现 —— 端口契约
//!    （两档名单/单条、组织限定、落盘加密在实现方）不变。
//! 2. **凭据**（[`HostTunnelCredentialSource`]）：隧道定义在端口层是**内存内
//!    明文**（`TunnelRepository` 的既有语义），落盘加密与密钥管理由实现方
//!    负责。web 形态的凭据以 **SecretRef**（`{domain, key}` 引用，永不传值）
//!    进出；安全域解析器未接线前，默认实现
//!    [`UnresolvedHostCredentials`] 抛**命名缺口** —— 沿 WR-F1#2 的登记语义
//!    （显式缺口，绝不静默空答）。
//! 3. **监听点**（[`HostListenPoint`]）：本地腿 `127.0.0.1:<port>` 的绑定方。
//!    web 形态裁决：**监听点恒在服务端**（`ServerLoopback`）—— 浏览器不开
//!    本地套接字；驱动经服务端 host 拨 `127.0.0.1:<local_port>`，与桌面
//!    「驱动永远连 127.0.0.1:<local_port>」的隧道不变式同形。
//!
//! 本模块不读宿主状态、不落盘、不做鉴权（tunnel 模块既有纪律）。

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::shared::error::PortError;
use crate::shared::scope::OrganizationScope;
use crate::tunnel::domain::{KnownHostEntry, KnownHostsRepository, TunnelRepository};
use datazen_driver_api::{SavedTunnel, SavedTunnelSummary, TunnelUsage};

// ---------------------------------------------------------------------------
// 来源 1：存储根（内存语义，裁决见模块文档）
// ---------------------------------------------------------------------------

/// web 宿主的内存隧道存储根：[`TunnelRepository`] 的 web 默认实现。
///
/// 与桌面 `DesktopRepository` 同一套端口契约：`list` 只给摘要（无凭据字段）、
/// `get` 给全量、**越组织按「不存在」**（CM-05：list → 空表、get → None）。
/// upsert 语义与桌面一致（按 id 去重）。落盘（服务端存储目录 + AES-256-GCM
/// 同源加密）由装配方以实现替换本类型，端口不动。
#[derive(Default)]
pub struct HostInMemoryTunnelRoot {
    /// 组织限定的命名空间：`(org id, tunnel definition)`。web 宿主是多租户
    /// 侧（桌面单租户），命名空间由作用域决定而非实现方猜。
    tunnels: Mutex<Vec<(String, SavedTunnel)>>,
}

impl HostInMemoryTunnelRoot {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl TunnelRepository for HostInMemoryTunnelRoot {
    async fn list(&self, scope: &OrganizationScope) -> Result<Vec<SavedTunnelSummary>, PortError> {
        let tunnels = self.tunnels.lock().map_err(|_| {
            PortError::BackendUnavailable("tunnel root lock poisoned".into())
        })?;
        // 摘要投影用 domain 的共享 [`crate::tunnel::domain::summarize`] ——
        // web 装配钉的映射就是实现方生产路径的那一份，不另造第二份。
        Ok(tunnels
            .iter()
            .filter(|(org, _)| org == scope.organization_id().as_str())
            .map(|(_, tunnel)| crate::tunnel::domain::summarize(tunnel))
            .collect())
    }

    async fn get(
        &self,
        scope: &OrganizationScope,
        tunnel_id: &str,
    ) -> Result<Option<SavedTunnel>, PortError> {
        let tunnels = self.tunnels.lock().map_err(|_| {
            PortError::BackendUnavailable("tunnel root lock poisoned".into())
        })?;
        Ok(tunnels
            .iter()
            .filter(|(org, _)| org == scope.organization_id().as_str())
            .find(|(_, t)| t.id == tunnel_id)
            .map(|(_, t)| t.clone()))
    }

    async fn save(&self, scope: &OrganizationScope, tunnel: SavedTunnel) -> Result<(), PortError> {
        let mut tunnels = self.tunnels.lock().map_err(|_| {
            PortError::BackendUnavailable("tunnel root lock poisoned".into())
        })?;
        let org = scope.organization_id().as_str().to_string();
        match tunnels
            .iter()
            .position(|(o, t)| o == &org && t.id == tunnel.id)
        {
            Some(pos) => tunnels[pos] = (org, tunnel),
            None => tunnels.push((org, tunnel)),
        }
        Ok(())
    }

    async fn delete(&self, scope: &OrganizationScope, tunnel_id: &str) -> Result<(), PortError> {
        let mut tunnels = self.tunnels.lock().map_err(|_| {
            PortError::BackendUnavailable("tunnel root lock poisoned".into())
        })?;
        let org = scope.organization_id().as_str();
        tunnels.retain(|(o, t)| !(o == org && t.id == tunnel_id));
        Ok(())
    }

    async fn usage(
        &self,
        _scope: &OrganizationScope,
        _tunnel_id: &str,
    ) -> Result<TunnelUsage, PortError> {
        // 引用投影是连接侧的事实；内存根没有连接聚合可投影，语义同「无人引用」。
        Ok(TunnelUsage {
            connection_ids: Vec::new(),
            connection_names: Vec::new(),
        })
    }
}

/// web 宿主的内存 known-hosts 半边（SSH TOFU）。空集合 = 首次使用即信任，
/// 与桌面 `{appData}/ssh_known_hosts.json` 的端口语义一致。
#[derive(Default)]
pub struct HostInMemoryKnownHosts {
    entries: Mutex<HashMap<String, KnownHostEntry>>,
}

impl HostInMemoryKnownHosts {
    pub fn new() -> Self {
        Self::default()
    }
}

impl KnownHostsRepository for HostInMemoryKnownHosts {
    fn load(&self) -> HashMap<String, KnownHostEntry> {
        self.entries
            .lock()
            .map(|entries| entries.clone())
            .unwrap_or_default()
    }

    fn save(&self, entries: &HashMap<String, KnownHostEntry>) -> Result<(), PortError> {
        let mut guard = self
            .entries
            .lock()
            .map_err(|_| PortError::BackendUnavailable("known hosts lock poisoned".into()))?;
        *guard = entries.clone();
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 来源 2：凭据（SecretRef 引用语义；缺口沿 WR-F1#2 登记）
// ---------------------------------------------------------------------------

/// 凭据引用（SecretRef）：web 形态的凭据跨边界**只以引用流转**，密钥本体
/// 由服务端安全域持有 —— 与 WR-F1#2 的 `SecretRef` 同一语义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSecretRef {
    pub domain: String,
    pub key: String,
}

impl HostSecretRef {
    /// 解析半边：坏引用在这里被拒绝（装配错误，不是密钥缺失）。
    pub fn parse(domain: &str, key: &str) -> Result<Self, String> {
        if domain.is_empty() || key.is_empty() {
            return Err(
                "HostSecretRef: expected non-empty {domain, key} — credentials cross the \
                 boundary as references, never as values."
                    .into(),
            );
        }
        Ok(Self {
            domain: domain.to_string(),
            key: key.to_string(),
        })
    }
}

/// WR-F5 命名缺口：web 的安全域（服务端密钥管理）解析器尚未接线 —— 沿
/// WR-F1#2 的登记语义：显式缺口、使用时失败、绝不静默空答。
pub const HOST_SECRET_RESOLVER_GAP: &str =
    "HostTunnelCredentialSource has no secret resolver yet: the security-domain route for \
     server-side secret management is not wired. Wiring gap WR-F5#1 (沿 WR-F1#2 登记语义).";

/// 隧道凭据的 web 注入面：按引用取值。桌面形没有这个端口 —— 桌面凭据从
/// 加密落盘的隧道定义里直接解出；web 形态密钥本体在服务端安全域，隧道定义
/// 与建连路径只携引用。
#[async_trait]
pub trait HostTunnelCredentialSource: Send + Sync + 'static {
    async fn resolve(&self, credential: &HostSecretRef) -> Result<Option<String>, PortError>;
}

/// 默认实现：命名缺口。任何解析请求都如实失败并点名缺口 —— 不返回看似
/// 成功的空值，也不把引用当明文用。
pub struct UnresolvedHostCredentials;

#[async_trait]
impl HostTunnelCredentialSource for UnresolvedHostCredentials {
    async fn resolve(&self, credential: &HostSecretRef) -> Result<Option<String>, PortError> {
        Err(PortError::BackendUnavailable(format!(
            "{HOST_SECRET_RESOLVER_GAP} (ref: {}/{}; backend unavailable is the port-level \
             fact: the security-domain backend route is the missing dependency)",
            credential.domain, credential.key
        )))
    }
}

// ---------------------------------------------------------------------------
// 来源 3：监听点（恒在服务端）
// ---------------------------------------------------------------------------

/// 本地腿监听点的 web 形态。不变式：浏览器永不绑定本地套接字；驱动侧的
/// 「连 127.0.0.1:<local_port>」由服务端 host 兑现。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostListenPoint {
    /// 服务端回环：`bind_host` 恒为 `127.0.0.1`，端口由服务端分配或指定。
    ServerLoopback { port: u16 },
}

impl HostListenPoint {
    pub const LOOPBACK_HOST: &str = "127.0.0.1";

    /// 监听点地址。唯一形态就是服务端回环 —— 浏览器侧根本没有第二变体，
    /// 这是类型层面的裁决而非运行时开关。
    pub fn bind_target(&self) -> (String, u16) {
        match self {
            Self::ServerLoopback { port } => (Self::LOOPBACK_HOST.to_string(), *port),
        }
    }
}

// ---------------------------------------------------------------------------
// 注入三件套的装配束
// ---------------------------------------------------------------------------

/// web 宿主注入束：存储根 + known-hosts + 凭据来源 + 监听点。web 宿主装配
/// 时铸一次，隧道用例（probe / 建连）与桌面共用同一批函数签名。
pub struct HostTunnelSources {
    pub storage_root: std::sync::Arc<dyn TunnelRepository>,
    pub known_hosts: std::sync::Arc<dyn KnownHostsRepository>,
    pub credentials: std::sync::Arc<dyn HostTunnelCredentialSource>,
    pub listen_point: HostListenPoint,
}

impl HostTunnelSources {
    /// 缺省装配：内存根 + 命名缺口凭据 + 服务端回环监听点。
    pub fn in_memory(listen_port: u16) -> Self {
        Self {
            storage_root: std::sync::Arc::new(HostInMemoryTunnelRoot::new()),
            known_hosts: std::sync::Arc::new(HostInMemoryKnownHosts::new()),
            credentials: std::sync::Arc::new(UnresolvedHostCredentials),
            listen_point: HostListenPoint::ServerLoopback { port: listen_port },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::id::OrganizationId;
    use datazen_driver_api::{SshTunnelConfig, TunnelKind};

    // 与桌面 `tunnel::mod::tests` 同语义：base_config / 显式 kind 优先。
    fn ssh_tunnel(id: &str) -> SavedTunnel {
        SavedTunnel {
            id: id.into(),
            name: "Prod Bastion".into(),
            kind: TunnelKind::Ssh,
            ssh: Some(SshTunnelConfig {
                enabled: true,
                host: "jump".into(),
                port: 22,
                username: "u".into(),
                auth_method: "password".into(),
                password: Some("secret".into()),
                private_key_path: None,
                passphrase: None,
                jump: None,
            }),
            http_proxy: None,
            websocket: None,
        }
    }

    fn scope() -> OrganizationScope {
        OrganizationScope::new(OrganizationId::new("org-1"))
    }

    /// 桌面同语义 1（两档 + upsert）：`list` 只给摘要（无凭据字段），
    /// `get` 才给全量；同 id 保存是更新不是第二条。
    #[tokio::test]
    async fn memory_storage_root_keeps_the_two_tier_contract_and_upserts() {
        let root = HostInMemoryTunnelRoot::new();

        root.save(&scope(), ssh_tunnel("t1")).await.unwrap();
        // upsert：同 id 第二次保存覆盖。
        let mut renamed = ssh_tunnel("t1");
        renamed.name = "Renamed".into();
        root.save(&scope(), renamed).await.unwrap();

        let summaries = root.list(&scope()).await.unwrap();
        assert_eq!(summaries.len(), 1, "upsert must not duplicate");
        assert_eq!(summaries[0].id, "t1");
        assert_eq!(summaries[0].name, "Renamed");

        let full = root.get(&scope(), "t1").await.unwrap().expect("full definition");
        assert_eq!(full.ssh.as_ref().unwrap().password.as_deref(), Some("secret"));
        // 缺失/越组织一律 None（不是 NotFound）。
        assert!(root.get(&scope(), "ghost").await.unwrap().is_none());
        let other = OrganizationScope::new(OrganizationId::new("org-2"));
        assert!(root.get(&other, "t1").await.unwrap().is_none(), "CM-05: cross-org get is None");
        assert!(
            root.list(&other).await.unwrap().is_empty(),
            "CM-05: cross-org list is empty"
        );

        // 名单档无凭据字段（与桌面 tunnels.json 的最小暴露语义一致）。
        let json = serde_json::to_value(&summaries).unwrap();
        let rendered = serde_json::to_string(&json).unwrap();
        assert!(!rendered.contains("secret"), "list tier leaked a credential: {rendered}");
    }

    /// 桌面同语义 2（凭据最小暴露 + 缺口登记）：凭据解析是命名缺口
    /// （WR-F1#2 语义），引用在解析半边被校验；监听点恒为服务端回环。
    #[tokio::test]
    async fn credentials_are_references_and_resolution_names_the_gap() {
        // 引用半边：空域/空键在装配边界被拒。
        assert!(HostSecretRef::parse("tunnel", "bastion-pass").is_ok());
        assert!(HostSecretRef::parse("", "k").is_err());
        assert!(HostSecretRef::parse("tunnel", "").is_err());

        let sources = HostTunnelSources::in_memory(0);
        let err = sources
            .credentials
            .resolve(&HostSecretRef::parse("tunnel", "bastion-pass").unwrap())
            .await
            .expect_err("unresolved credentials must fail, never answer empty");
        let rendered = err.to_string();
        assert!(
            rendered.contains("WR-F5#1"),
            "gap must be named: {rendered}"
        );
        // 错误只携引用坐标（tunnel/bastion-pass），端口面上从未存在密钥本体
        // 可言 —— 取值半边连一次明文候选都不会产生。
        assert!(rendered.contains("tunnel/bastion-pass"), "{rendered}");

        // 监听点：唯一形态是服务端回环 —— 浏览器不开本地套接字。
        let sources = HostTunnelSources::in_memory(15432);
        assert_eq!(
            sources.listen_point.bind_target(),
            ("127.0.0.1".to_string(), 15432)
        );
        assert_eq!(HostListenPoint::LOOPBACK_HOST, "127.0.0.1");
    }
}

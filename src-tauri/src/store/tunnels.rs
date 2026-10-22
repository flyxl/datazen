//! Independent tunnel definitions (`tunnels.json`).
//!
//! Connections reference a tunnel via `ConnectionConfig.tunnel_id`.
//! Sensitive fields (SSH password/passphrase, HTTP proxy password, WS auth token)
//! are encrypted at rest with the same AES-256-GCM key as connection passwords.
//!
//! # 这一层是**存储实现**，不是端口
//!
//! 端口是 [`datazen_platform_api::ports::TunnelRepository`]（契约 crate 里那份，
//! 跨形态共享）：**名单**走摘要（不带密钥），**单条取用**才给全量。本文件提供它的
//! 桌面实现——加密与落盘格式**留在这里**（主钥匙串在宿主手里），端口层是
//! 「内存内明文」语义。
//!
//! 越组织访问按「不存在」处理（空列表 / `None` / 写入报 `NotFound`），
//! **不**返回「无权限」——那会泄漏「存在但无权」（CM-05）。桌面是单租户：
//! 它只承认自己那一个组织。

use async_trait::async_trait;
use datazen_driver_api::{SavedTunnel, SavedTunnelSummary, TunnelUsage};
use datazen_platform_api::error::PortError;
use datazen_platform_api::ports::scope::OrganizationScope;
use datazen_platform_api::ports::tunnel::TunnelRepository;

use super::{DesktopRepository, DesktopRepositoryError};

impl DesktopRepository {
    pub(super) async fn load_tunnels_from_disk(
        &self,
    ) -> Result<Vec<SavedTunnel>, DesktopRepositoryError> {
        let path = self.data_dir.join("tunnels.json");
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| DesktopRepositoryError::ReadError(e.to_string()))?;

        let mut tunnels: Vec<SavedTunnel> = serde_json::from_str(&content)
            .map_err(|e| DesktopRepositoryError::ParseError(e.to_string()))?;

        for t in &mut tunnels {
            self.decrypt_tunnel_secrets(t);
        }

        Ok(tunnels)
    }

    fn decrypt_tunnel_secrets(&self, t: &mut SavedTunnel) {
        if let Some(ref mut ssh) = t.ssh {
            if let Some(enc) = &ssh.password {
                match self.decrypt(enc) {
                    Ok(plain) => ssh.password = Some(plain),
                    Err(e) => {
                        tracing::warn!(tunnel = %t.name, error = %e, "Failed to decrypt SSH password");
                        ssh.password = None;
                    }
                }
            }
            if let Some(enc) = &ssh.passphrase {
                match self.decrypt(enc) {
                    Ok(plain) => ssh.passphrase = Some(plain),
                    Err(e) => {
                        tracing::warn!(tunnel = %t.name, error = %e, "Failed to decrypt SSH passphrase");
                        ssh.passphrase = None;
                    }
                }
            }
            if let Some(ref mut jump) = ssh.jump {
                // Nested ProxyJump hop — only one level of secrets for now.
                if let Some(enc) = &jump.password {
                    match self.decrypt(enc) {
                        Ok(plain) => jump.password = Some(plain),
                        Err(_) => jump.password = None,
                    }
                }
                if let Some(enc) = &jump.passphrase {
                    match self.decrypt(enc) {
                        Ok(plain) => jump.passphrase = Some(plain),
                        Err(_) => jump.passphrase = None,
                    }
                }
            }
        }
        if let Some(ref mut http) = t.http_proxy {
            if let Some(enc) = &http.password {
                match self.decrypt(enc) {
                    Ok(plain) => http.password = Some(plain),
                    Err(e) => {
                        tracing::warn!(tunnel = %t.name, error = %e, "Failed to decrypt HTTP proxy password");
                        http.password = None;
                    }
                }
            }
        }
        if let Some(ref mut ws) = t.websocket {
            if let Some(enc) = &ws.auth_token {
                match self.decrypt(enc) {
                    Ok(plain) => ws.auth_token = Some(plain),
                    Err(e) => {
                        tracing::warn!(tunnel = %t.name, error = %e, "Failed to decrypt WS auth token");
                        ws.auth_token = None;
                    }
                }
            }
        }
    }

    fn encrypt_tunnel_secrets(
        &self,
        t: &SavedTunnel,
    ) -> Result<SavedTunnel, DesktopRepositoryError> {
        let mut out = t.clone();
        if let Some(ref mut ssh) = out.ssh {
            if let Some(pw) = &ssh.password {
                if !pw.is_empty() {
                    ssh.password = Some(self.encrypt(pw)?);
                }
            }
            if let Some(pp) = &ssh.passphrase {
                if !pp.is_empty() {
                    ssh.passphrase = Some(self.encrypt(pp)?);
                }
            }
            if let Some(ref mut jump) = ssh.jump {
                if let Some(pw) = &jump.password {
                    if !pw.is_empty() {
                        jump.password = Some(self.encrypt(pw)?);
                    }
                }
                if let Some(pp) = &jump.passphrase {
                    if !pp.is_empty() {
                        jump.passphrase = Some(self.encrypt(pp)?);
                    }
                }
            }
        }
        if let Some(ref mut http) = out.http_proxy {
            if let Some(pw) = &http.password {
                if !pw.is_empty() {
                    http.password = Some(self.encrypt(pw)?);
                }
            }
        }
        if let Some(ref mut ws) = out.websocket {
            if let Some(tok) = &ws.auth_token {
                if !tok.is_empty() {
                    ws.auth_token = Some(self.encrypt(tok)?);
                }
            }
        }
        Ok(out)
    }

    async fn persist_tunnels(&self, tunnels: &[SavedTunnel]) -> Result<(), DesktopRepositoryError> {
        let mut to_disk = Vec::with_capacity(tunnels.len());
        for t in tunnels {
            to_disk.push(self.encrypt_tunnel_secrets(t)?);
        }
        self.save_json_file("tunnels.json", &to_disk).await
    }

    /// 名单（全量，含明文凭据）。
    ///
    /// **端口化之后，生产代码不再走这里**：名单走
    /// [`TunnelRepository::list`]（摘要，不带密钥），单条取用走
    /// [`TunnelRepository::get`]。本方法只作为存储实现与测试夹具保留。
    pub(crate) async fn get_tunnels(&self) -> Vec<SavedTunnel> {
        let cache = self.cache.read().await;
        cache.tunnels.clone()
    }

    /// 单条取用（全量，含明文凭据）。
    pub(crate) async fn get_tunnel(&self, id: &str) -> Option<SavedTunnel> {
        let cache = self.cache.read().await;
        cache.tunnels.iter().find(|t| t.id == id).cloned()
    }

    /// 保存（新建 / 更新）：先更新内存缓存，再加密落盘。
    pub(crate) async fn save_tunnel(
        &self,
        tunnel: SavedTunnel,
    ) -> Result<(), DesktopRepositoryError> {
        let _guard = self.write_lock.lock().await;

        {
            let mut cache = self.cache.write().await;
            if let Some(pos) = cache.tunnels.iter().position(|t| t.id == tunnel.id) {
                cache.tunnels[pos] = tunnel;
            } else {
                cache.tunnels.push(tunnel);
            }
        }

        let snapshot = {
            let cache = self.cache.read().await;
            cache.tunnels.clone()
        };

        // Already holding write_lock — encrypt + atomic write without re-locking.
        let mut to_disk = Vec::with_capacity(snapshot.len());
        for t in &snapshot {
            to_disk.push(self.encrypt_tunnel_secrets(t)?);
        }
        let content = serde_json::to_string_pretty(&to_disk)
            .map_err(|e| DesktopRepositoryError::ParseError(e.to_string()))?;
        let path = self.data_dir.join("tunnels.json");
        Self::write_file_atomic(&path, content).await
    }

    /// 删除：先改内存缓存，再加密落盘。**不做引用检查**。
    pub(crate) async fn delete_tunnel(&self, id: &str) -> Result<(), DesktopRepositoryError> {
        {
            let mut cache = self.cache.write().await;
            cache.tunnels.retain(|t| t.id != id);
        }
        let snapshot = {
            let cache = self.cache.read().await;
            cache.tunnels.clone()
        };
        self.persist_tunnels(&snapshot).await
    }
}

/// 存储错误 → 端口错误。
///
/// **原文保留**：端口把失败压成 `PortError::BackendUnavailable(原文)`，而这里塞进去的
/// 是 `DesktopRepositoryError` 的完整 `Display`（例如 `Write error: ...`）。命令层再把它原样
/// 还回去，因此 IPC 载荷与端口化之前**逐字相同**。
fn store_error(error: DesktopRepositoryError) -> PortError {
    PortError::BackendUnavailable(error.to_string())
}

/// 桌面形态的隧道定义仓储。
///
/// 桌面是**单租户**：它只承认自己那一个组织，别的作用域一律按「不存在」处理
/// （CM-05）。加密与落盘格式由 [`DesktopRepository`] 承担（与连接密码同源的 AES-256-GCM），
/// 端口层是「内存内明文」语义。
#[async_trait]
impl TunnelRepository for DesktopRepository {
    /// 名单：**只带摘要**，不含任何口令 / 密钥 / token。
    async fn list(&self, scope: &OrganizationScope) -> Result<Vec<SavedTunnelSummary>, PortError> {
        if !crate::platform::is_local_scope(scope) {
            return Ok(Vec::new());
        }
        Ok(self
            .get_tunnels()
            .await
            .into_iter()
            .map(|tunnel| SavedTunnelSummary {
                id: tunnel.id,
                name: tunnel.name,
                kind: tunnel.kind,
            })
            .collect())
    }

    /// 单条取用：全量定义（含明文凭据，只在宿主内存与建连路径里流转）。
    async fn get(
        &self,
        scope: &OrganizationScope,
        tunnel_id: &str,
    ) -> Result<Option<SavedTunnel>, PortError> {
        if !crate::platform::is_local_scope(scope) {
            return Ok(None);
        }
        Ok(self.get_tunnel(tunnel_id).await)
    }

    /// 保存（新建 / 更新）。越组织的写入按「不存在」处理。
    async fn save(&self, scope: &OrganizationScope, tunnel: SavedTunnel) -> Result<(), PortError> {
        if !crate::platform::is_local_scope(scope) {
            return Err(PortError::NotFound("tunnel".into()));
        }
        self.save_tunnel(tunnel).await.map_err(store_error)
    }

    /// 删除。越组织的写入按「不存在」处理。
    async fn delete(&self, scope: &OrganizationScope, tunnel_id: &str) -> Result<(), PortError> {
        if !crate::platform::is_local_scope(scope) {
            return Err(PortError::NotFound("tunnel".into()));
        }
        self.delete_tunnel(tunnel_id).await.map_err(store_error)
    }

    /// 反向投影：哪些连接引用了这条隧道。
    ///
    /// `connection_ids` 与 `connection_names` 位置对齐。空 id 不做扫面：
    /// `materialize_tunnel_refs` 把 `""` 当「无隧道引用」。
    async fn usage(
        &self,
        scope: &OrganizationScope,
        tunnel_id: &str,
    ) -> Result<TunnelUsage, PortError> {
        let mut connection_ids = Vec::new();
        let mut connection_names = Vec::new();
        if !crate::platform::is_local_scope(scope) {
            return Ok(TunnelUsage {
                connection_ids,
                connection_names,
            });
        }
        // An empty query must not sweep in connections whose reference is itself
        // empty: `materialize_tunnel_refs` treats "" as "no tunnel reference".
        if !tunnel_id.is_empty() {
            for conn in self.get_connections().await {
                if conn.tunnel_id.as_deref() == Some(tunnel_id) {
                    connection_ids.push(conn.id);
                    connection_names.push(conn.name);
                }
            }
        }
        Ok(TunnelUsage {
            connection_ids,
            connection_names,
        })
    }
}

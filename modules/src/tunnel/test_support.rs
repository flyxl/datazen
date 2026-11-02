//! 隧道域测试共享替身：内存版 [`TunnelRepository`] 与 known-hosts。
//!
//! 与 `ssh.rs` 的 `test_fixture`（真 SSH 堡垒机）分工：本模块是**存储替身**，
//! 只为让领域测试不落盘。`list` / `usage` 走 [`crate::tunnel::domain`]
//! 的两条共享投影——替身自己不另造映射，被测的映射就永远是那唯一一份。
//! 末尾的 [`rebindable`] 是机制测试共用的端口释放探针。

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use datazen_driver_api::{ConnectionConfig, SavedTunnel, SavedTunnelSummary, TunnelUsage};

use crate::shared::error::PortError;
use crate::shared::scope::OrganizationScope;
use crate::tunnel::domain::{
    summarize, usage_from_connections, KnownHostEntry, KnownHostsRepository, TunnelRepository,
};

/// 内存隧道仓储：测试里充当 `TunnelRepository` 的实现方。
///
/// `list` / `usage` 直接调用共享投影（[`summarize`] / [`usage_from_connections`]），
/// 因此领域测试钉的映射就是实现方生产路径会用的那一份。
pub(crate) struct MemoryTunnelStore {
    tunnels: Mutex<Vec<SavedTunnel>>,
    connections: Mutex<Vec<ConnectionConfig>>,
}

impl MemoryTunnelStore {
    pub(crate) fn new() -> Self {
        Self {
            tunnels: Mutex::new(Vec::new()),
            connections: Mutex::new(Vec::new()),
        }
    }

    /// 保存（新建 / 更新同一条定义），按 id 去重——与桌面仓储的 upsert 语义一致。
    pub(crate) fn save_tunnel(&self, tunnel: SavedTunnel) {
        let mut tunnels = self.tunnels.lock().expect("tunnels lock");
        match tunnels.iter().position(|t| t.id == tunnel.id) {
            Some(pos) => tunnels[pos] = tunnel,
            None => tunnels.push(tunnel),
        }
    }

    /// 追加一条连接定义（反向投影的输入侧）。
    pub(crate) fn add_connection(&self, conn: ConnectionConfig) {
        self.connections
            .lock()
            .expect("connections lock")
            .push(conn);
    }
}

#[async_trait]
impl TunnelRepository for MemoryTunnelStore {
    async fn list(&self, _scope: &OrganizationScope) -> Result<Vec<SavedTunnelSummary>, PortError> {
        let tunnels = self.tunnels.lock().expect("tunnels lock");
        Ok(tunnels.iter().map(summarize).collect())
    }

    async fn get(
        &self,
        _scope: &OrganizationScope,
        tunnel_id: &str,
    ) -> Result<Option<SavedTunnel>, PortError> {
        let tunnels = self.tunnels.lock().expect("tunnels lock");
        Ok(tunnels.iter().find(|t| t.id == tunnel_id).cloned())
    }

    async fn save(&self, _scope: &OrganizationScope, tunnel: SavedTunnel) -> Result<(), PortError> {
        self.save_tunnel(tunnel);
        Ok(())
    }

    async fn delete(&self, _scope: &OrganizationScope, tunnel_id: &str) -> Result<(), PortError> {
        let mut tunnels = self.tunnels.lock().expect("tunnels lock");
        tunnels.retain(|t| t.id != tunnel_id);
        Ok(())
    }

    async fn usage(
        &self,
        _scope: &OrganizationScope,
        tunnel_id: &str,
    ) -> Result<TunnelUsage, PortError> {
        let connections = self.connections.lock().expect("connections lock");
        Ok(usage_from_connections(connections.iter(), tunnel_id))
    }
}

/// 内存 known-hosts：SSH 隧道测试的 TOFU 存储（空集合即「首次使用即信任」）。
#[derive(Default)]
pub(crate) struct MemoryKnownHosts {
    entries: Mutex<HashMap<String, KnownHostEntry>>,
}

impl KnownHostsRepository for MemoryKnownHosts {
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

/// 一个本地端口在隧道 `Drop` 之后是否可以立刻重新绑定。
///
/// 数据路径测试（`HttpProxyTunnel` / `WebSocketTunnel`）用它钉「拆隧道必须
/// 释放 127.0.0.1 上的本地端口」——释放是异步完成的，所以轮询而不是断言一次。
pub(crate) async fn rebindable(port: u16) -> bool {
    for _ in 0..100 {
        if tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    false
}

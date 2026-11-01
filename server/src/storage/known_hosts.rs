//! `KnownHostsRepository` 的服务端适配器（WR-D2 第二切片）。
//!
//! 端口是**同步**的（SSH 主机密钥校验发生在 russh 的 `check_server_key`
//! 回调里）且无鉴权维度：组织隔离由宿主按「每组织一个仓储实例」实现——
//! 构造参数 `organization_id` 是实例的隔离边界，端口形状不动（见端口
//! 模块头的两处刻意差异）。
//!
//! 降级语义属于端口契约：`load` 读不到**一律按空集合**（坏库不得阻断
//! 隧道启动）；`save` 是**全表替换**（单事务 DELETE + INSERT），失败
//! 如实报 `BackendUnavailable`——「指纹没记下来，下次连接会被当成首次
//! 使用」是必须可见的事实。

use std::collections::HashMap;

use datazen::tunnel::domain::{KnownHostEntry, KnownHostsRepository};
use datazen::shared::error::PortError;

use super::AdminStore;

pub struct ServerKnownHosts {
    store: std::sync::Arc<AdminStore>,
    organization_id: String,
}

impl ServerKnownHosts {
    /// 每组织一个实例；同一 [`AdminStore`] 上的多个实例互不可见。
    pub fn new(
        store: std::sync::Arc<AdminStore>,
        organization_id: impl Into<String>,
    ) -> Self {
        Self {
            store,
            organization_id: organization_id.into(),
        }
    }
}

impl KnownHostsRepository for ServerKnownHosts {
    fn load(&self) -> HashMap<String, KnownHostEntry> {
        let conn = match self.store.lock_for_adapter() {
            Ok(conn) => conn,
            Err(source) => {
                tracing::warn!(%source, "known hosts load failed; treating as empty (TOFU)");
                return HashMap::new();
            }
        };
        let mut stmt = match conn.prepare(
            "SELECT host_key, fingerprint, algorithm, public_key
             FROM known_hosts WHERE organization_id = ?1",
        ) {
            Ok(stmt) => stmt,
            Err(source) => {
                tracing::warn!(%source, "known hosts query failed; treating as empty (TOFU)");
                return HashMap::new();
            }
        };
        let rows = match stmt.query_map([&self.organization_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                KnownHostEntry {
                    fingerprint: row.get(1)?,
                    algorithm: row.get(2)?,
                    public_key: row.get(3)?,
                },
            ))
        }) {
            Ok(rows) => rows,
            Err(source) => {
                tracing::warn!(%source, "known hosts read failed; treating as empty (TOFU)");
                return HashMap::new();
            }
        };
        let mut entries = HashMap::new();
        for row in rows.flatten() {
            entries.insert(row.0, row.1);
        }
        entries
    }

    fn save(&self, entries: &HashMap<String, KnownHostEntry>) -> Result<(), PortError> {
        let mut conn = self
            .store
            .lock_for_adapter()
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))?;
        let tx = conn
            .transaction()
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))?;
        tx.execute(
            "DELETE FROM known_hosts WHERE organization_id = ?1",
            [&self.organization_id],
        )
        .map_err(|source| PortError::BackendUnavailable(source.to_string()))?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO known_hosts
                       (organization_id, host_key, fingerprint, algorithm, public_key)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(|source| PortError::BackendUnavailable(source.to_string()))?;
            for (host_key, entry) in entries {
                stmt.execute(rusqlite::params![
                    self.organization_id,
                    host_key,
                    entry.fingerprint,
                    entry.algorithm,
                    entry.public_key,
                ])
                .map_err(|source| PortError::BackendUnavailable(source.to_string()))?;
            }
        }
        tx.commit()
            .map_err(|source| PortError::BackendUnavailable(source.to_string()))
    }
}

use datazen_driver_api::ConnectionConfig;
use datazen_platform_api::error::PortError;
use datazen_platform_api::ports::scope::OrganizationScope;
use datazen_platform_api::ports::ConnectionCatalog;

use super::{DesktopRepository, DesktopRepositoryError};

impl DesktopRepository {
    pub(super) async fn load_connections_from_disk(
        &self,
    ) -> Result<Vec<ConnectionConfig>, DesktopRepositoryError> {
        let path = self.data_dir.join("connections.json");
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| DesktopRepositoryError::ReadError(e.to_string()))?;

        let mut connections: Vec<ConnectionConfig> = serde_json::from_str(&content)
            .map_err(|e| DesktopRepositoryError::ParseError(e.to_string()))?;

        for conn in &mut connections {
            if let Some(enc) = &conn.password {
                match self.decrypt(enc) {
                    Ok(plain) => conn.password = Some(plain),
                    Err(e) => {
                        tracing::warn!(conn_name = %conn.name, error = %e, "Failed to decrypt password, clearing");
                        conn.password = None;
                    }
                }
            }
            if let Some(ref mut ssh) = conn.ssh_tunnel {
                if let Some(enc) = &ssh.password {
                    match self.decrypt(enc) {
                        Ok(plain) => ssh.password = Some(plain),
                        Err(e) => {
                            tracing::warn!(
                                conn_name = %conn.name,
                                error = %e,
                                "Failed to decrypt SSH password, clearing"
                            );
                            ssh.password = None;
                        }
                    }
                }
                if let Some(enc) = &ssh.passphrase {
                    match self.decrypt(enc) {
                        Ok(plain) => ssh.passphrase = Some(plain),
                        Err(e) => {
                            tracing::warn!(
                                conn_name = %conn.name,
                                error = %e,
                                "Failed to decrypt SSH passphrase, clearing"
                            );
                            ssh.passphrase = None;
                        }
                    }
                }
            }
        }

        Ok(connections)
    }

    async fn persist_connections(
        &self,
        connections: &[ConnectionConfig],
    ) -> Result<(), DesktopRepositoryError> {
        let mut to_disk = Vec::with_capacity(connections.len());

        for conn in connections {
            let mut c = conn.clone();
            if let Some(pw) = &c.password {
                c.password = Some(self.encrypt(pw)?);
            }
            if let Some(ref mut ssh) = c.ssh_tunnel {
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
            }
            to_disk.push(c);
        }

        let metadata = self.cache.read().await.platform_profiles.clone();
        let records = super::platform_profiles::encode_records(to_disk, &metadata)?;
        self.save_json_file("connections.json", &records).await
    }

    /// Persist while the caller already holds `write_lock`. Must NOT go
    /// through `save_json_file` — it re-acquires the same tokio Mutex and
    /// would self-deadlock. Writes atomically via `write_file_atomic`.
    pub(super) async fn persist_connections_locked(
        &self,
        connections: &[ConnectionConfig],
    ) -> Result<(), DesktopRepositoryError> {
        let mut to_disk = Vec::with_capacity(connections.len());

        for conn in connections {
            let mut c = conn.clone();
            if let Some(pw) = &c.password {
                c.password = Some(self.encrypt(pw)?);
            }
            if let Some(ref mut ssh) = c.ssh_tunnel {
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
            }
            to_disk.push(c);
        }

        let metadata = self.cache.read().await.platform_profiles.clone();
        let records = super::platform_profiles::encode_records(to_disk, &metadata)?;
        let content = serde_json::to_string_pretty(&records)
            .map_err(|e| DesktopRepositoryError::ParseError(e.to_string()))?;
        let path = self.data_dir.join("connections.json");
        Self::write_file_atomic(&path, content).await
    }

    pub async fn get_connections(&self) -> Vec<ConnectionConfig> {
        let cache = self.cache.read().await;
        cache.connections.clone()
    }

    pub async fn get_connection(&self, id: &str) -> Option<ConnectionConfig> {
        let cache = self.cache.read().await;
        cache.connections.iter().find(|c| c.id == id).cloned()
    }

    pub async fn save_connection(
        &self,
        config: ConnectionConfig,
    ) -> Result<(), DesktopRepositoryError> {
        // Serialize the whole update→snapshot→persist sequence under the
        // store-wide write lock. Snapshotting before acquiring the lock let
        // an older copy overwrite a newer file when concurrent saves
        // interleaved (lost update — CI flake where 16 concurrent saves left
        // only 13 entries on disk).
        let _guard = self.write_lock.lock().await;

        {
            let mut cache = self.cache.write().await;
            let previous = cache
                .connections
                .iter()
                .find(|c| c.id == config.id)
                .cloned();
            let metadata = cache
                .platform_profiles
                .entry(config.id.clone())
                .or_default();
            if let Some(previous) = previous {
                metadata.config_revision =
                    metadata.config_revision.checked_add(1).ok_or_else(|| {
                        DesktopRepositoryError::WriteError("revision exhausted".into())
                    })?;
                if previous.password != config.password
                    || serde_json::to_value(&previous.ssh_tunnel).ok()
                        != serde_json::to_value(&config.ssh_tunnel).ok()
                    || previous.options != config.options
                {
                    metadata.credential_revision =
                        metadata.credential_revision.checked_add(1).ok_or_else(|| {
                            DesktopRepositoryError::WriteError("revision exhausted".into())
                        })?;
                }
            }
            if let Some(pos) = cache.connections.iter().position(|c| c.id == config.id) {
                cache.connections[pos] = config.clone();
            } else {
                cache.connections.push(config.clone());
            }
        }

        let snapshot = {
            let cache = self.cache.read().await;
            cache.connections.clone()
        };

        // Already holding write_lock — use the non-locking persist variant.
        self.persist_connections_locked(&snapshot).await
    }

    /// Update connection activity metadata without advancing the Runtime
    /// configuration revision. Activity timestamps are observational metadata;
    /// treating them as a connection change invalidates every live session
    /// immediately after `connect` returns.
    pub async fn touch_last_connected_at(&self, id: &str) -> Result<(), DesktopRepositoryError> {
        let _guard = self.write_lock.lock().await;
        {
            let mut cache = self.cache.write().await;
            if let Some(config) = cache.connections.iter_mut().find(|config| config.id == id) {
                config.last_connected_at = Some(chrono::Utc::now().to_rfc3339());
            } else {
                return Ok(());
            }
        }
        let snapshot = {
            let cache = self.cache.read().await;
            cache.connections.clone()
        };
        self.persist_connections_locked(&snapshot).await
    }

    pub async fn delete_connection(&self, id: &str) -> Result<(), DesktopRepositoryError> {
        {
            let mut cache = self.cache.write().await;
            cache.connections.retain(|c| c.id != id);
        }

        let snapshot = {
            let cache = self.cache.read().await;
            cache.connections.clone()
        };

        self.persist_connections(&snapshot).await
    }

    /// Returns the union of persisted custom groups and groups found on connections.
    pub async fn get_groups(&self) -> Vec<String> {
        let cache = self.cache.read().await;
        let mut set = std::collections::BTreeSet::new();
        for g in &cache.groups {
            set.insert(g.clone());
        }
        for c in &cache.connections {
            if let Some(g) = &c.group {
                set.insert(g.clone());
            }
        }
        set.into_iter().collect()
    }

    pub async fn save_groups(&self, groups: Vec<String>) -> Result<(), DesktopRepositoryError> {
        {
            let mut cache = self.cache.write().await;
            cache.groups = groups;
        }
        let snapshot = {
            let cache = self.cache.read().await;
            cache.groups.clone()
        };
        self.save_json_file("groups.json", &snapshot).await
    }

    /// Reorder connections to match the given list of IDs.
    /// IDs not present in the current list are ignored; connections
    /// whose IDs are not in `ordered_ids` are appended at the end.
    pub async fn reorder_connections(
        &self,
        ordered_ids: Vec<String>,
    ) -> Result<(), DesktopRepositoryError> {
        {
            let mut cache = self.cache.write().await;
            let mut by_id: std::collections::HashMap<String, ConnectionConfig> = cache
                .connections
                .drain(..)
                .map(|c| (c.id.clone(), c))
                .collect();
            for id in &ordered_ids {
                if let Some(c) = by_id.remove(id) {
                    cache.connections.push(c);
                }
            }
            // Append any remaining connections not mentioned in ordered_ids.
            let mut remaining: Vec<ConnectionConfig> = by_id.into_values().collect();
            remaining.sort_by(|a, b| a.name.cmp(&b.name));
            cache.connections.extend(remaining);
        }
        let snapshot = {
            let cache = self.cache.read().await;
            cache.connections.clone()
        };
        self.persist_connections(&snapshot).await
    }

    pub fn decrypt_password(&self, encrypted: &str) -> Result<String, DesktopRepositoryError> {
        self.decrypt(encrypted)
    }
}

fn catalog_error(error: DesktopRepositoryError) -> PortError {
    PortError::BackendUnavailable(error.to_string())
}

/// 桌面形态的连接目录仓储（§5.1.2 第六步：命令不再直读 `DesktopRepository`）。
///
/// 桌面是**单租户**：它只承认自己那一个组织，别的作用域一律按「不存在」处理
/// （CM-05）。读写、加密与落盘都在 [`DesktopRepository`] 内完成（口令与 SSH / 代理
/// 凭据同源 AES-256-GCM），端口层是「内存内明文」语义。
///
/// **为什么不另造投影类型**：连接表单要求「打开即逐字回显」，IPC 载荷就是
/// [`ConnectionConfig`] 本身；再造一份投影只会两处双写、慢慢漂移（见端口模块头）。
#[async_trait::async_trait]
impl ConnectionCatalog for DesktopRepository {
    async fn list(&self, scope: &OrganizationScope) -> Result<Vec<ConnectionConfig>, PortError> {
        if !crate::platform::is_local_scope(scope) {
            return Ok(Vec::new());
        }
        Ok(self.get_connections().await)
    }

    async fn get(
        &self,
        scope: &OrganizationScope,
        connection_id: &str,
    ) -> Result<Option<ConnectionConfig>, PortError> {
        if !crate::platform::is_local_scope(scope) {
            return Ok(None);
        }
        Ok(self.get_connection(connection_id).await)
    }

    async fn save(
        &self,
        scope: &OrganizationScope,
        config: ConnectionConfig,
    ) -> Result<(), PortError> {
        if !crate::platform::is_local_scope(scope) {
            return Err(PortError::NotFound("connection".into()));
        }
        self.save_connection(config).await.map_err(catalog_error)
    }

    async fn delete(
        &self,
        scope: &OrganizationScope,
        connection_id: &str,
    ) -> Result<(), PortError> {
        if !crate::platform::is_local_scope(scope) {
            return Err(PortError::NotFound("connection".into()));
        }
        self.delete_connection(connection_id)
            .await
            .map_err(catalog_error)
    }

    async fn reorder(
        &self,
        scope: &OrganizationScope,
        ordered_ids: Vec<String>,
    ) -> Result<(), PortError> {
        if !crate::platform::is_local_scope(scope) {
            return Err(PortError::NotFound("connection".into()));
        }
        self.reorder_connections(ordered_ids)
            .await
            .map_err(catalog_error)
    }

    async fn touch_last_connected_at(
        &self,
        scope: &OrganizationScope,
        connection_id: &str,
    ) -> Result<(), PortError> {
        if !crate::platform::is_local_scope(scope) {
            return Err(PortError::NotFound("connection".into()));
        }
        self.touch_last_connected_at(connection_id)
            .await
            .map_err(catalog_error)
    }

    async fn groups(&self, scope: &OrganizationScope) -> Result<Vec<String>, PortError> {
        if !crate::platform::is_local_scope(scope) {
            return Ok(Vec::new());
        }
        Ok(self.get_groups().await)
    }

    async fn save_groups(
        &self,
        scope: &OrganizationScope,
        groups: Vec<String>,
    ) -> Result<(), PortError> {
        if !crate::platform::is_local_scope(scope) {
            return Err(PortError::NotFound("connection group".into()));
        }
        self.save_groups(groups).await.map_err(catalog_error)
    }
}

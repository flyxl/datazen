use crate::db::ConnectionConfig;

use super::{Store, StoreError};

impl Store {
    pub(super) async fn load_connections_from_disk(
        &self,
    ) -> Result<Vec<ConnectionConfig>, StoreError> {
        let path = self.data_dir.join("connections.json");
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| StoreError::ReadError(e.to_string()))?;

        let mut connections: Vec<ConnectionConfig> =
            serde_json::from_str(&content).map_err(|e| StoreError::ParseError(e.to_string()))?;

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
    ) -> Result<(), StoreError> {
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

        self.save_json_file("connections.json", &to_disk).await
    }

    /// Persist while the caller already holds `write_lock`. Must NOT go
    /// through `save_json_file` — it re-acquires the same tokio Mutex and
    /// would self-deadlock. Writes atomically via `write_file_atomic`.
    async fn persist_connections_locked(
        &self,
        connections: &[ConnectionConfig],
    ) -> Result<(), StoreError> {
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

        let content = serde_json::to_string_pretty(&to_disk)
            .map_err(|e| StoreError::ParseError(e.to_string()))?;
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

    pub async fn save_connection(&self, config: ConnectionConfig) -> Result<(), StoreError> {
        // Serialize the whole update→snapshot→persist sequence under the
        // store-wide write lock. Snapshotting before acquiring the lock let
        // an older copy overwrite a newer file when concurrent saves
        // interleaved (lost update — CI flake where 16 concurrent saves left
        // only 13 entries on disk).
        let _guard = self.write_lock.lock().await;

        {
            let mut cache = self.cache.write().await;
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

    pub async fn delete_connection(&self, id: &str) -> Result<(), StoreError> {
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

    pub async fn save_groups(&self, groups: Vec<String>) -> Result<(), StoreError> {
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
    pub async fn reorder_connections(&self, ordered_ids: Vec<String>) -> Result<(), StoreError> {
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

    pub fn decrypt_password(&self, encrypted: &str) -> Result<String, StoreError> {
        self.decrypt(encrypted)
    }
}

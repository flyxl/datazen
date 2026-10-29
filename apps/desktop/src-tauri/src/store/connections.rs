use datazen_driver_api::ConnectionConfig;

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
        // Serialize the whole delete→snapshot→persist sequence under the
        // store-wide write lock, mirroring save_connection: without it a
        // concurrent save/touch could persist a snapshot that still contains
        // the deleted row after this delete already wrote the file (lost
        // delete — the same stale-snapshot race the save path fixed).
        let _guard = self.write_lock.lock().await;

        {
            let mut cache = self.cache.write().await;
            cache.connections.retain(|c| c.id != id);
        }

        let snapshot = {
            let cache = self.cache.read().await;
            cache.connections.clone()
        };

        // Already holding write_lock — use the non-locking persist variant.
        self.persist_connections_locked(&snapshot).await
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
        // Same doctrine as save_connection: the reorder→snapshot→persist
        // sequence must be atomic against every other connection writer.
        let _guard = self.write_lock.lock().await;

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
        // Already holding write_lock — use the non-locking persist variant.
        self.persist_connections_locked(&snapshot).await
    }

    pub fn decrypt_password(&self, encrypted: &str) -> Result<String, DesktopRepositoryError> {
        self.decrypt(encrypted)
    }
}

//! Manages live connections and coordinates with the driver registry.

use crate::db::{ConnectionConfig, ConnectionHandle, DatabaseDriver, DatabaseType, DriverError, ServerInfo};
use crate::db::registry::DriverRegistry;
use crate::ssh_tunnel::SshTunnel;
use crate::store::Store;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

struct ActiveConnection {
    handle: ConnectionHandle,
    config: ConnectionConfig,
    #[allow(dead_code)]
    created_at: Instant,
    last_used: Instant,
    _tunnel: Option<SshTunnel>,
}

/// Coordinates configuration lookup, driver selection, and pooling handles.
pub struct ConnectionManager {
    registry: Arc<DriverRegistry>,
    connections: Arc<RwLock<HashMap<String, ActiveConnection>>>,
    /// Maps runtime connectionId → persistent configId so we can reconnect
    /// after idle eviction using the latest config from the Store.
    config_id_map: Arc<RwLock<HashMap<String, String>>>,
    store: Arc<Store>,
    idle_timeout: Duration,
    /// Per-config_id locks to prevent concurrent connect attempts for the same
    /// configuration. Second+ callers wait and reuse the first caller's result.
    connect_locks: std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("Configuration not found: {0}")]
    ConfigNotFound(String),

    #[error("Connection not found: {0}")]
    ConnectionNotFound(String),

    #[error("Driver not found for type: {0}")]
    DriverNotFound(DatabaseType),

    #[error("Driver error: {0}")]
    DriverError(#[from] DriverError),
}

impl ConnectionManager {
    pub fn new(registry: Arc<DriverRegistry>, store: Arc<Store>) -> Self {
        Self {
            registry,
            connections: Arc::new(RwLock::new(HashMap::new())),
            config_id_map: Arc::new(RwLock::new(HashMap::new())),
            store,
            idle_timeout: Duration::from_secs(1800),
            connect_locks: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub async fn connect(&self, config_id: &str) -> Result<String, ConnectionError> {
        let (driver, handle, mut effective_config, tunnel) =
            self.establish_connection(config_id).await?;
        let connection_id = handle.id.clone();

        if effective_config.server_version.is_none() {
            if let Ok(info) = driver.get_server_info(&handle).await {
                effective_config.server_version = Some(info.server_version.clone());
                // Persist version to the stored config
                if let Some(mut stored) = self.store.get_connection(config_id).await {
                    stored.server_version = Some(info.server_version);
                    let _ = self.store.save_connection(stored).await;
                }
            }
        }

        self.config_id_map.write().await
            .insert(connection_id.clone(), config_id.to_string());

        let mut connections = self.connections.write().await;
        connections.insert(
            connection_id.clone(),
            ActiveConnection {
                handle,
                config: effective_config,
                created_at: Instant::now(),
                last_used: Instant::now(),
                _tunnel: tunnel,
            },
        );

        Ok(connection_id)
    }

    /// Open a driver connection without registering in the UI session map.
    pub(crate) async fn establish_connection(
        &self,
        config_id: &str,
    ) -> Result<
        (
            Arc<dyn DatabaseDriver>,
            ConnectionHandle,
            ConnectionConfig,
            Option<SshTunnel>,
        ),
        ConnectionError,
    > {
        let config = self
            .store
            .get_connection(config_id)
            .await
            .ok_or_else(|| ConnectionError::ConfigNotFound(config_id.to_string()))?;

        let (effective_config, tunnel) = self.maybe_start_tunnel(config).await?;

        let driver = self
            .driver_for_type(&effective_config.database_type)
            .await?;

        let handle = driver.connect(&effective_config).await?;

        Ok((driver, handle, effective_config, tunnel))
    }

    pub(crate) async fn driver_for_type(
        &self,
        database_type: &DatabaseType,
    ) -> Result<Arc<dyn DatabaseDriver>, ConnectionError> {
        self.registry
            .get(database_type)
            .await
            .ok_or_else(|| ConnectionError::DriverNotFound(database_type.clone()))
    }

    #[cfg(test)]
    pub(crate) async fn ui_session_map_len(&self) -> usize {
        self.config_id_map.read().await.len()
    }

    /// Return an existing connection for the given config_id, or create a new one.
    /// Concurrent callers for the same config_id are serialised: the first caller
    /// performs the actual connect; subsequent callers wait then reuse its result.
    pub async fn get_or_connect(&self, config_id: &str) -> Result<String, ConnectionError> {
        let lock = {
            let mut locks = self.connect_locks.lock().unwrap();
            locks
                .entry(config_id.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };

        let _guard = lock.lock().await;

        {
            let map = self.config_id_map.read().await;
            let connections = self.connections.read().await;
            for (conn_id, cfg_id) in map.iter() {
                if cfg_id == config_id && connections.contains_key(conn_id) {
                    return Ok(conn_id.clone());
                }
            }
        }
        self.connect(config_id).await
    }

    pub async fn disconnect(&self, connection_id: &str) -> Result<(), ConnectionError> {
        self.config_id_map.write().await.remove(connection_id);

        let mut connections = self.connections.write().await;

        if let Some(active) = connections.remove(connection_id) {
            if let Some(driver) = self.registry.get(&active.config.database_type).await {
                let _ = driver.disconnect(active.handle).await;
            }
        }

        Ok(())
    }

    pub async fn get_connection(
        &self,
        connection_id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), ConnectionError> {
        {
            let mut connections = self.connections.write().await;
            if let Some(active) = connections.get_mut(connection_id) {
                active.last_used = Instant::now();

                let driver = self
                    .registry
                    .get(&active.config.database_type)
                    .await
                    .ok_or_else(|| ConnectionError::DriverNotFound(active.config.database_type.clone()))?;

                return Ok((driver, active.handle.clone()));
            }
        }

        // Connection was evicted — reconnect using configId from persistent Store
        self.reconnect(connection_id).await
    }

    /// Transparently re-establish an evicted connection.
    /// Reads the latest config from the persistent Store via `config_id_map`.
    async fn reconnect(
        &self,
        connection_id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), ConnectionError> {
        let config_id = {
            let map = self.config_id_map.read().await;
            map.get(connection_id).cloned()
        };

        let config_id = config_id
            .ok_or_else(|| ConnectionError::ConnectionNotFound(connection_id.to_string()))?;

        let config = self
            .store
            .get_connection(&config_id)
            .await
            .ok_or_else(|| ConnectionError::ConfigNotFound(config_id.clone()))?;

        tracing::info!(%connection_id, %config_id, name = %config.name, "Auto-reconnecting evicted connection");

        let (effective_config, tunnel) = self.maybe_start_tunnel(config).await?;

        let driver = self
            .registry
            .get(&effective_config.database_type)
            .await
            .ok_or(ConnectionError::DriverNotFound(effective_config.database_type.clone()))?;

        let handle = driver.connect(&effective_config).await?;

        let mut connections = self.connections.write().await;
        connections.insert(
            connection_id.to_string(),
            ActiveConnection {
                handle: handle.clone(),
                config: effective_config,
                created_at: Instant::now(),
                last_used: Instant::now(),
                _tunnel: tunnel,
            },
        );

        tracing::info!(%connection_id, "Auto-reconnect succeeded");
        Ok((driver, handle))
    }

    pub async fn get_connection_config(
        &self,
        connection_id: &str,
    ) -> Result<ConnectionConfig, ConnectionError> {
        let connections = self.connections.read().await;
        let active = connections
            .get(connection_id)
            .ok_or_else(|| ConnectionError::ConnectionNotFound(connection_id.to_string()))?;
        Ok(active.config.clone())
    }

    /// Update the active logical database for a live session (after `use_database`).
    /// Keeps schema-cache keys and metadata lookups aligned with the session.
    pub async fn set_active_database(
        &self,
        connection_id: &str,
        database: &str,
    ) -> Result<(), ConnectionError> {
        let mut connections = self.connections.write().await;
        let active = connections
            .get_mut(connection_id)
            .ok_or_else(|| ConnectionError::ConnectionNotFound(connection_id.to_string()))?;
        active.config.database = Some(database.to_string());
        active.last_used = Instant::now();
        Ok(())
    }

    pub async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, ConnectionError> {
        let (effective_config, _tunnel) = self.maybe_start_tunnel(config.clone()).await?;

        let driver = self
            .registry
            .get(&effective_config.database_type)
            .await
            .ok_or_else(|| ConnectionError::DriverNotFound(effective_config.database_type.clone()))?;

        driver
            .test_connection(&effective_config)
            .await
            .map_err(ConnectionError::DriverError)
    }

    /// Lightweight touch: refreshes `last_used` so idle cleanup doesn't evict.
    pub async fn ping(&self, connection_id: &str) -> bool {
        let mut connections = self.connections.write().await;
        if let Some(active) = connections.get_mut(connection_id) {
            active.last_used = Instant::now();
            true
        } else {
            false
        }
    }

    pub async fn cleanup_idle_connections(&self) {
        let mut connections = self.connections.write().await;
        let now = Instant::now();

        let to_remove: Vec<String> = connections
            .iter()
            .filter(|(_, conn)| now.duration_since(conn.last_used) > self.idle_timeout)
            .map(|(id, _)| id.clone())
            .collect();

        for id in &to_remove {
            if let Some(active) = connections.remove(id) {
                tracing::info!(connection_id = %id, name = %active.config.name,
                    "Evicting idle connection (config_id mapping preserved for auto-reconnect)");
                if let Some(driver) = self.registry.get(&active.config.database_type).await {
                    let _ = driver.disconnect(active.handle).await;
                }
            }
        }
        // Note: config_id_map is NOT cleared here — it stays so reconnect() works
    }

    pub fn start_cleanup_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(300));
            loop {
                ticker.tick().await;
                self.cleanup_idle_connections().await;
            }
        });
    }

    pub async fn shutdown(&self) {
        self.config_id_map.write().await.clear();

        let mut connections = self.connections.write().await;

        for (_, active) in connections.drain() {
            if let Some(driver) = self.registry.get(&active.config.database_type).await {
                let _ = driver.disconnect(active.handle).await;
            }
        }
    }

    async fn maybe_start_tunnel(
        &self,
        config: ConnectionConfig,
    ) -> Result<(ConnectionConfig, Option<SshTunnel>), ConnectionError> {
        let ssh = match &config.ssh_tunnel {
            Some(s) if s.enabled => s,
            _ => return Ok((config, None)),
        };

        let remote_host = config.host.as_deref().ok_or_else(|| {
            ConnectionError::DriverError(DriverError::InvalidConfig(
                "SSH tunnel requires a database host".into(),
            ))
        })?;
        let remote_port = config.port.ok_or_else(|| {
            ConnectionError::DriverError(DriverError::InvalidConfig(
                "SSH tunnel requires a database port".into(),
            ))
        })?;

        tracing::info!(
            ssh_host = %ssh.host,
            ssh_port = ssh.port,
            remote = %format!("{remote_host}:{remote_port}"),
            "Starting SSH tunnel"
        );

        let known_hosts_path = self.store.data_dir().join("ssh_known_hosts.json");
        let tunnel = SshTunnel::start(ssh, remote_host, remote_port, &known_hosts_path)
            .await
            .map_err(ConnectionError::DriverError)?;

        let mut tunneled = config;
        tunneled.host = Some("127.0.0.1".to_string());
        tunneled.port = Some(tunnel.local_port());
        tunneled.ssh_tunnel = None;

        Ok((tunneled, Some(tunnel)))
    }
}

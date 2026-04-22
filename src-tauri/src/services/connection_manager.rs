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
    store: Arc<Store>,
    idle_timeout: Duration,
}

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("Configuration not found: {0}")]
    ConfigNotFound(String),

    #[error("Connection not found: {0}")]
    ConnectionNotFound(String),

    #[error("Driver not found for type: {0:?}")]
    DriverNotFound(DatabaseType),

    #[error("Driver error: {0}")]
    DriverError(#[from] DriverError),
}

impl ConnectionManager {
    pub fn new(registry: Arc<DriverRegistry>, store: Arc<Store>) -> Self {
        Self {
            registry,
            connections: Arc::new(RwLock::new(HashMap::new())),
            store,
            idle_timeout: Duration::from_secs(1800),
        }
    }

    pub async fn connect(&self, config_id: &str) -> Result<String, ConnectionError> {
        let config = self
            .store
            .get_connection(config_id)
            .await
            .ok_or_else(|| ConnectionError::ConfigNotFound(config_id.to_string()))?;

        // If SSH tunnel is configured, establish it and rewrite host/port
        let (effective_config, tunnel) = self.maybe_start_tunnel(config).await?;

        let driver = self
            .registry
            .get(&effective_config.database_type)
            .await
            .ok_or(ConnectionError::DriverNotFound(effective_config.database_type.clone()))?;

        let handle = driver.connect(&effective_config).await?;
        let connection_id = handle.id.clone();

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

    pub async fn disconnect(&self, connection_id: &str) -> Result<(), ConnectionError> {
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
        let mut connections = self.connections.write().await;

        let active = connections
            .get_mut(connection_id)
            .ok_or_else(|| ConnectionError::ConnectionNotFound(connection_id.to_string()))?;

        active.last_used = Instant::now();

        let driver = self
            .registry
            .get(&active.config.database_type)
            .await
            .ok_or_else(|| ConnectionError::DriverNotFound(active.config.database_type.clone()))?;

        Ok((driver, active.handle.clone()))
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

    pub async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, ConnectionError> {
        // If SSH tunnel is configured, establish a temporary tunnel for testing
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
        // _tunnel is dropped here, closing the temporary SSH session
    }

    pub async fn cleanup_idle_connections(&self) {
        let mut connections = self.connections.write().await;
        let now = Instant::now();

        let to_remove: Vec<String> = connections
            .iter()
            .filter(|(_, conn)| now.duration_since(conn.last_used) > self.idle_timeout)
            .map(|(id, _)| id.clone())
            .collect();

        for id in to_remove {
            if let Some(active) = connections.remove(&id) {
                if let Some(driver) = self.registry.get(&active.config.database_type).await {
                    let _ = driver.disconnect(active.handle).await;
                }
            }
        }
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
        let mut connections = self.connections.write().await;

        for (_, active) in connections.drain() {
            if let Some(driver) = self.registry.get(&active.config.database_type).await {
                let _ = driver.disconnect(active.handle).await;
            }
        }
    }

    /// If SSH tunnel is enabled in the config, start the tunnel and return
    /// a modified config pointing to `127.0.0.1:<local_port>`.
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

        let tunnel = SshTunnel::start(ssh, remote_host, remote_port)
            .await
            .map_err(ConnectionError::DriverError)?;

        let mut tunneled = config;
        tunneled.host = Some("127.0.0.1".to_string());
        tunneled.port = Some(tunnel.local_port());
        // Clear SSH config from the effective config so drivers don't see it
        tunneled.ssh_tunnel = None;

        Ok((tunneled, Some(tunnel)))
    }
}

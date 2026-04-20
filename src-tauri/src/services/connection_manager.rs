//! Manages live connections and coordinates with the driver registry.

use crate::db::{ConnectionConfig, ConnectionHandle, DatabaseDriver, DatabaseType, DriverError, ServerInfo};
use crate::db::registry::DriverRegistry;
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

        let driver = self
            .registry
            .get(&config.database_type)
            .await
            .ok_or(ConnectionError::DriverNotFound(config.database_type.clone()))?;

        let handle = driver.connect(&config).await?;
        let connection_id = handle.id.clone();

        let mut connections = self.connections.write().await;
        connections.insert(
            connection_id.clone(),
            ActiveConnection {
                handle,
                config,
                created_at: Instant::now(),
                last_used: Instant::now(),
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
        let driver = self
            .registry
            .get(&config.database_type)
            .await
            .ok_or_else(|| ConnectionError::DriverNotFound(config.database_type.clone()))?;

        driver
            .test_connection(config)
            .await
            .map_err(ConnectionError::DriverError)
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
}

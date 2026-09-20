//! Connection manager (sessions + tunnels).

use crate::db::registry::DriverRegistry;
use crate::db::{
    ConnectionConfig, ConnectionHandle, DatabaseDriver, DatabaseType, DriverError, ServerInfo,
};
use crate::tunnel::Tunnel as SshTunnel;
use crate::store::Store;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

struct ActiveSession {
    handle: ConnectionHandle,
    config: ConnectionConfig,
    #[allow(dead_code)]
    created_at: Instant,
    last_used: Instant,
    _tunnel: Option<SshTunnel>,
}

pub struct ConnectionManager {
    registry: Arc<DriverRegistry>,
    connections: Arc<RwLock<HashMap<String, ActiveSession>>>,
    session_owner_map: Arc<RwLock<HashMap<String, String>>>,
    ref_counts: Arc<RwLock<HashMap<String, usize>>>,
    store: Arc<Store>,
    idle_timeout: Duration,
    connect_locks: std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error(
        "Connection config '{0}' not found (connectionId refers to a persisted \
         connection configuration; no such configuration is stored)"
    )]
    ConnectionConfigNotFound(String),

    #[error(
        "DB session '{0}' not found (a dbSessionId is a runtime session id; \
         maybe you passed a connectionId where a dbSessionId was expected)"
    )]
    DbSessionNotFound(String),

    #[error("Driver not found for type: {0}")]
    DriverNotFound(DatabaseType),

    #[error("Driver error: {0}")]
    DriverError(#[from] DriverError),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl ConnectionManager {
    pub fn new(registry: Arc<DriverRegistry>, store: Arc<Store>) -> Self {
        Self {
            registry,
            connections: Arc::new(RwLock::new(HashMap::new())),
            session_owner_map: Arc::new(RwLock::new(HashMap::new())),
            ref_counts: Arc::new(RwLock::new(HashMap::new())),
            store,
            idle_timeout: Duration::from_secs(1800),
            connect_locks: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub async fn connect_dedicated(
        &self,
        connection_id: &str,
        database: Option<&str>,
    ) -> Result<String, ConnectionError> {
        let db_session_id = self.connect_with_config(connection_id, database).await?;
        let mut refs = self.ref_counts.write().await;
        *refs.entry(db_session_id.clone()).or_insert(0) += 1;
        tracing::debug!(db_session_id = %db_session_id, refs = refs[&db_session_id], "dedicated session ref acquired");
        Ok(db_session_id)
    }

    async fn connect_with_config(
        &self,
        connection_id: &str,
        database_override: Option<&str>,
    ) -> Result<String, ConnectionError> {
        let (driver, handle, mut effective_config, tunnel) = self
            .establish_connection(connection_id, database_override)
            .await?;
        let db_session_id = handle.id.clone();

        if effective_config.server_version.is_none() {
            if let Ok(info) = driver.get_server_info(&handle).await {
                effective_config.server_version = Some(info.server_version.clone());
                if let Some(mut stored) = self.store.get_connection(connection_id).await {
                    stored.server_version = Some(info.server_version);
                    let _ = self.store.save_connection(stored).await;
                }
            }
        }

        self.session_owner_map
            .write()
            .await
            .insert(db_session_id.clone(), connection_id.to_string());

        let mut connections = self.connections.write().await;
        connections.insert(
            db_session_id.clone(),
            ActiveSession {
                handle,
                config: effective_config,
                created_at: Instant::now(),
                last_used: Instant::now(),
                _tunnel: tunnel,
            },
        );

        Ok(db_session_id)
    }

    pub async fn connect(&self, connection_id: &str) -> Result<String, ConnectionError> {
        self.connect_with_config(connection_id, None).await
    }

    pub(crate) async fn establish_connection(
        &self,
        connection_id: &str,
        database_override: Option<&str>,
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
            .get_connection(connection_id)
            .await
            .ok_or_else(|| ConnectionError::ConnectionConfigNotFound(connection_id.to_string()))?;

        let (mut effective_config, tunnel) = self.maybe_start_tunnel(config).await?;
        if let Some(db) = database_override.map(str::trim).filter(|s| !s.is_empty()) {
            effective_config.database = Some(db.to_string());
        }
        let pool_size = crate::store::clamp_connection_pool_size(
            self.store.get_settings().await.connection_pool_size,
        );
        effective_config.max_pool_size = pool_size;

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

    pub async fn owner_connection_id(&self, db_session_id: &str) -> Option<String> {
        self.session_owner_map
            .read()
            .await
            .get(db_session_id)
            .cloned()
    }

    #[cfg(test)]
    pub(crate) async fn session_owner_map_len(&self) -> usize {
        self.session_owner_map.read().await.len()
    }


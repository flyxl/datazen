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

    pub async fn get_or_connect_session(
        &self,
        connection_id: &str,
    ) -> Result<String, ConnectionError> {
        let lock = {
            let mut locks = self
                .connect_locks
                .lock()
                .map_err(|e| ConnectionError::Internal(format!("connect lock poisoned: {e}")))?;
            locks
                .entry(connection_id.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };

        let _guard = lock.lock().await;

        {
            let owner_map = self.session_owner_map.read().await;
            let connections = self.connections.read().await;
            for (session_id, owner_connection_id) in owner_map.iter() {
                if owner_connection_id == connection_id && connections.contains_key(session_id) {
                    let mut refs = self.ref_counts.write().await;
                    *refs.entry(session_id.clone()).or_insert(0) += 1;
                    tracing::debug!(db_session_id = %session_id, refs = refs[session_id], "session ref acquired (reuse)");
                    return Ok(session_id.clone());
                }
            }
        }
        let db_session_id = self.connect(connection_id).await?;
        let mut refs = self.ref_counts.write().await;
        *refs.entry(db_session_id.clone()).or_insert(0) += 1;
        tracing::debug!(db_session_id = %db_session_id, refs = refs[&db_session_id], "session ref acquired (new)");
        Ok(db_session_id)
    }

    pub async fn release(&self, db_session_id: &str) -> Result<bool, ConnectionError> {
        let should_disconnect = {
            let mut refs = self.ref_counts.write().await;
            if let Some(count) = refs.get_mut(db_session_id) {
                *count = count.saturating_sub(1);
                tracing::debug!(db_session_id = %db_session_id, refs = *count, "session ref released");
                if *count == 0 {
                    refs.remove(db_session_id);
                    true
                } else {
                    false
                }
            } else {
                true
            }
        };
        if should_disconnect {
            self.disconnect(db_session_id).await?;
        }
        Ok(should_disconnect)
    }

    pub async fn disconnect(&self, db_session_id: &str) -> Result<(), ConnectionError> {
        self.ref_counts.write().await.remove(db_session_id);
        self.session_owner_map.write().await.remove(db_session_id);

        let mut connections = self.connections.write().await;

        if let Some(active) = connections.remove(db_session_id) {
            if let Some(driver) = self.registry.get(&active.config.database_type).await {
                let _ = driver.disconnect(active.handle).await;
            }
        }

        Ok(())
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) async fn ref_count(&self, db_session_id: &str) -> usize {
        self.ref_counts
            .read()
            .await
            .get(db_session_id)
            .copied()
            .unwrap_or(0)
    }

    pub async fn get_session(
        &self,
        db_session_id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), ConnectionError> {
        {
            let mut connections = self.connections.write().await;
            if let Some(active) = connections.get_mut(db_session_id) {
                active.last_used = Instant::now();

                let driver = self
                    .registry
                    .get(&active.config.database_type)
                    .await
                    .ok_or_else(|| {
                        ConnectionError::DriverNotFound(active.config.database_type.clone())
                    })?;

                return Ok((driver, active.handle.clone()));
            }
        }

        self.reconnect(db_session_id).await
    }

    pub async fn resolve_session_for_connection(
        &self,
        connection_id: &str,
    ) -> Result<(String, Arc<dyn DatabaseDriver>, ConnectionHandle), ConnectionError> {
        let db_session_id = self.get_or_connect_session(connection_id).await?;
        let (driver, handle) = self.get_session(&db_session_id).await?;
        Ok((db_session_id, driver, handle))
    }

    async fn reconnect(
        &self,
        db_session_id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), ConnectionError> {
        let owner_connection_id = {
            let map = self.session_owner_map.read().await;
            map.get(db_session_id).cloned()
        };

        let connection_id = owner_connection_id
            .ok_or_else(|| ConnectionError::DbSessionNotFound(db_session_id.to_string()))?;

        let config = self
            .store
            .get_connection(&connection_id)
            .await
            .ok_or_else(|| ConnectionError::ConnectionConfigNotFound(connection_id.clone()))?;

        tracing::info!(db_session_id = %db_session_id, %connection_id, name = %config.name, "Auto-reconnecting evicted session");

        let (effective_config, tunnel) = self.maybe_start_tunnel(config).await?;

        let driver = self
            .registry
            .get(&effective_config.database_type)
            .await
            .ok_or(ConnectionError::DriverNotFound(
                effective_config.database_type.clone(),
            ))?;

        let mut handle = driver.connect(&effective_config).await?;
        handle.id = db_session_id.to_string();

        let mut connections = self.connections.write().await;
        connections.insert(
            db_session_id.to_string(),
            ActiveSession {
                handle: handle.clone(),
                config: effective_config,
                created_at: Instant::now(),
                last_used: Instant::now(),
                _tunnel: tunnel,
            },
        );

        tracing::info!(db_session_id = %db_session_id, "Auto-reconnect succeeded");
        Ok((driver, handle))
    }

    pub async fn get_session_config(
        &self,
        db_session_id: &str,
    ) -> Result<ConnectionConfig, ConnectionError> {
        let connections = self.connections.read().await;
        let active = connections
            .get(db_session_id)
            .ok_or_else(|| ConnectionError::DbSessionNotFound(db_session_id.to_string()))?;
        Ok(active.config.clone())
    }

    pub async fn set_active_database(
        &self,
        db_session_id: &str,
        database: &str,
    ) -> Result<(), ConnectionError> {
        let mut connections = self.connections.write().await;
        let active = connections
            .get_mut(db_session_id)
            .ok_or_else(|| ConnectionError::DbSessionNotFound(db_session_id.to_string()))?;
        active.config.database = Some(database.to_string());
        Ok(())
    }

    pub async fn list_sessions(&self) -> Vec<String> {
        self.connections.read().await.keys().cloned().collect()
    }

    pub async fn get_server_info(
        &self,
        db_session_id: &str,
    ) -> Result<ServerInfo, ConnectionError> {
        let (driver, handle) = self.get_session(db_session_id).await?;
        driver
            .get_server_info(&handle)
            .await
            .map_err(ConnectionError::DriverError)
    }

    async fn maybe_start_tunnel(
        &self,
        config: ConnectionConfig,
    ) -> Result<(ConnectionConfig, Option<SshTunnel>), ConnectionError> {
        let known_hosts_path = self.store.data_dir().join("ssh_known_hosts.json");
        crate::tunnel::start_for_connection(config, &known_hosts_path)
            .await
            .map_err(ConnectionError::DriverError)
    }

    pub async fn cleanup_idle_connections(&self) {
        let now = Instant::now();
        let mut to_remove = Vec::new();
        {
            let connections = self.connections.read().await;
            let refs = self.ref_counts.read().await;
            for (id, active) in connections.iter() {
                let refs_n = refs.get(id).copied().unwrap_or(0);
                if refs_n == 0 && now.duration_since(active.last_used) > self.idle_timeout {
                    to_remove.push(id.clone());
                }
            }
        }
        for id in to_remove {
            tracing::info!(db_session_id = %id, "Evicting idle session");
            let _ = self.disconnect(&id).await;
        }
    }

    pub fn spawn_idle_cleanup(self: &Arc<Self>) {
        let mgr = Arc::clone(self);
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(60));
            loop {
                ticker.tick().await;
                mgr.cleanup_idle_connections().await;
            }
        });
    }

    #[cfg(test)]
    pub(crate) async fn insert_test_session(
        &self,
        db_session_id: &str,
        connection_id: &str,
        config: ConnectionConfig,
        handle: ConnectionHandle,
    ) {
        self.session_owner_map
            .write()
            .await
            .insert(db_session_id.to_string(), connection_id.to_string());
        self.connections.write().await.insert(
            db_session_id.to_string(),
            ActiveSession {
                handle,
                config,
                created_at: Instant::now(),
                last_used: Instant::now(),
                _tunnel: None,
            },
        );
    }

    #[cfg(test)]
    pub(crate) async fn expire_test_session(&self, db_session_id: &str) {
        let mut connections = self.connections.write().await;
        if let Some(active) = connections.get_mut(db_session_id) {
            active.last_used = Instant::now()
                .checked_sub(self.idle_timeout + Duration::from_secs(1))
                .unwrap_or_else(Instant::now);
        }
    }
}

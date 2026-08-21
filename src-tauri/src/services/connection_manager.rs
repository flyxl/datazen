//! Manages live connections and coordinates with the driver registry.

use crate::db::registry::DriverRegistry;
use crate::db::{
    ConnectionConfig, ConnectionHandle, DatabaseDriver, DatabaseType, DriverError, ServerInfo,
};
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
    /// Reference counts per runtime connectionId. Each `get_or_connect` caller
    /// increments; `release` decrements; session is torn down only at zero.
    ref_counts: Arc<RwLock<HashMap<String, usize>>>,
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
            ref_counts: Arc::new(RwLock::new(HashMap::new())),
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

        self.config_id_map
            .write()
            .await
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

        let (mut effective_config, tunnel) = self.maybe_start_tunnel(config).await?;
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

    /// Resolve the persistent `config_id` for a given runtime `connection_id`.
    /// Returns `None` if the mapping does not exist (e.g. connection was never registered).
    pub async fn resolve_config_id(&self, connection_id: &str) -> Option<String> {
        self.config_id_map.read().await.get(connection_id).cloned()
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
                    let mut refs = self.ref_counts.write().await;
                    *refs.entry(conn_id.clone()).or_insert(0) += 1;
                    tracing::debug!(%conn_id, refs = refs[conn_id], "session ref acquired (reuse)");
                    return Ok(conn_id.clone());
                }
            }
        }
        let conn_id = self.connect(config_id).await?;
        let mut refs = self.ref_counts.write().await;
        *refs.entry(conn_id.clone()).or_insert(0) += 1;
        tracing::debug!(%conn_id, refs = refs[&conn_id], "session ref acquired (new)");
        Ok(conn_id)
    }

    /// Decrement the reference count for a session. Only tears down the
    /// underlying driver connection when the count reaches zero.
    pub async fn release(&self, connection_id: &str) -> Result<bool, ConnectionError> {
        let should_disconnect = {
            let mut refs = self.ref_counts.write().await;
            if let Some(count) = refs.get_mut(connection_id) {
                *count = count.saturating_sub(1);
                tracing::debug!(%connection_id, refs = *count, "session ref released");
                if *count == 0 {
                    refs.remove(connection_id);
                    true
                } else {
                    false
                }
            } else {
                true
            }
        };
        if should_disconnect {
            self.disconnect(connection_id).await?;
        }
        Ok(should_disconnect)
    }

    /// Force-disconnect regardless of reference count (e.g. from sidebar).
    pub async fn disconnect(&self, connection_id: &str) -> Result<(), ConnectionError> {
        self.ref_counts.write().await.remove(connection_id);
        self.config_id_map.write().await.remove(connection_id);

        let mut connections = self.connections.write().await;

        if let Some(active) = connections.remove(connection_id) {
            if let Some(driver) = self.registry.get(&active.config.database_type).await {
                let _ = driver.disconnect(active.handle).await;
            }
        }

        Ok(())
    }

    /// Current reference count for a session (0 if not tracked).
    #[cfg(test)]
    pub(crate) async fn ref_count(&self, connection_id: &str) -> usize {
        self.ref_counts
            .read()
            .await
            .get(connection_id)
            .copied()
            .unwrap_or(0)
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
                    .ok_or_else(|| {
                        ConnectionError::DriverNotFound(active.config.database_type.clone())
                    })?;

                return Ok((driver, active.handle.clone()));
            }
        }

        // Connection was evicted — reconnect using configId from persistent Store
        self.reconnect(connection_id).await
    }

    /// Resolve a session from either a runtime connection id or a persistent config id.
    ///
    /// - Runtime id already live (or reconnectable via `config_id_map`) → returned as-is.
    /// - Otherwise treat `id` as a config id and `get_or_connect`, then return that runtime id.
    pub async fn resolve_session(
        &self,
        id: &str,
    ) -> Result<(String, Arc<dyn DatabaseDriver>, ConnectionHandle), ConnectionError> {
        if let Ok((driver, handle)) = self.get_connection(id).await {
            return Ok((id.to_string(), driver, handle));
        }
        let runtime_id = self.get_or_connect(id).await?;
        let (driver, handle) = self.get_connection(&runtime_id).await?;
        Ok((runtime_id, driver, handle))
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
            .ok_or(ConnectionError::DriverNotFound(
                effective_config.database_type.clone(),
            ))?;

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

    pub async fn test_connection(
        &self,
        config: &ConnectionConfig,
    ) -> Result<ServerInfo, ConnectionError> {
        let (effective_config, _tunnel) = self.maybe_start_tunnel(config.clone()).await?;

        let driver = self
            .registry
            .get(&effective_config.database_type)
            .await
            .ok_or_else(|| {
                ConnectionError::DriverNotFound(effective_config.database_type.clone())
            })?;

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
        self.ref_counts.write().await.clear();
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

    #[cfg(test)]
    pub(crate) async fn insert_test_session(
        &self,
        runtime_id: &str,
        config_id: &str,
        config: ConnectionConfig,
        handle: ConnectionHandle,
    ) {
        self.config_id_map
            .write()
            .await
            .insert(runtime_id.to_string(), config_id.to_string());
        self.connections.write().await.insert(
            runtime_id.to_string(),
            ActiveConnection {
                handle,
                config,
                created_at: Instant::now(),
                last_used: Instant::now(),
                _tunnel: None,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::registry::DriverRegistry;
    use crate::db::{ConnectionConfig, SslMode};
    use crate::store::Store;
    use crate::testing::mock_driver::{MockDriver, MockDriverOptions};
    use async_trait::async_trait;

    struct StubDriver(String);

    #[async_trait]
    impl DatabaseDriver for StubDriver {
        fn driver_type(&self) -> DatabaseType {
            self.0.clone()
        }

        async fn connect(
            &self,
            _config: &ConnectionConfig,
        ) -> Result<ConnectionHandle, DriverError> {
            Err(DriverError::QueryFailed("stub".into()))
        }

        async fn test_connection(
            &self,
            _config: &ConnectionConfig,
        ) -> Result<ServerInfo, DriverError> {
            Err(DriverError::QueryFailed("stub".into()))
        }

        async fn disconnect(&self, _handle: ConnectionHandle) -> Result<(), DriverError> {
            Ok(())
        }

        async fn get_databases(
            &self,
            _handle: &ConnectionHandle,
        ) -> Result<Vec<String>, DriverError> {
            Ok(vec![])
        }

        async fn get_tables(
            &self,
            _handle: &ConnectionHandle,
            _database: &str,
        ) -> Result<Vec<crate::db::TableInfo>, DriverError> {
            Ok(vec![])
        }

        async fn get_table_schema(
            &self,
            _handle: &ConnectionHandle,
            _table: &str,
        ) -> Result<crate::db::TableSchema, DriverError> {
            Err(DriverError::QueryFailed("stub".into()))
        }

        async fn query(
            &self,
            _handle: &ConnectionHandle,
            _sql: &str,
        ) -> Result<crate::db::QueryResult, DriverError> {
            Err(DriverError::QueryFailed("stub".into()))
        }

        async fn query_multi(
            &self,
            _handle: &ConnectionHandle,
            _sql: &str,
            _limit: Option<u32>,
        ) -> Result<crate::db::MultiQueryResult, DriverError> {
            Err(DriverError::QueryFailed("stub".into()))
        }

        async fn query_with_params(
            &self,
            _handle: &ConnectionHandle,
            _sql: &str,
            _params: &[crate::db::Value],
        ) -> Result<crate::db::QueryResult, DriverError> {
            Err(DriverError::QueryFailed("stub".into()))
        }

        async fn execute(
            &self,
            _handle: &ConnectionHandle,
            _sql: &str,
        ) -> Result<u64, DriverError> {
            Ok(0)
        }

        async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
            Ok(())
        }
    }

    async fn test_manager_stub() -> ConnectionManager {
        std::env::set_var("DATAZEN_KEYRING", "file");
        let dir = tempfile::tempdir().unwrap();
        // Keep tempdir alive for the store path lifetime of this test helper call site.
        let path = dir.path().to_path_buf();
        std::mem::forget(dir);
        let store = Arc::new(Store::init_with_path(&path).await.unwrap());
        let registry = Arc::new(DriverRegistry::new());
        registry
            .register_test_driver("postgresql", Arc::new(StubDriver("postgresql".into())))
            .await;
        ConnectionManager::new(registry, store)
    }

    fn test_config(config_id: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: config_id.to_string(),
            name: "test".into(),
            database_type: "postgresql".into(),
            host: Some("127.0.0.1".into()),
            port: Some(5432),
            database: Some("db".into()),
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::Prefer,
            connection_timeout: 30,
            max_pool_size: 10,
            ssh_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
            pinned: false,
        }
    }

    #[tokio::test]
    async fn resolve_session_passthrough_runtime_id() {
        let mgr = test_manager_stub().await;
        let handle = ConnectionHandle {
            id: "rt-1".into(),
            pool_id: "pool-1".into(),
        };
        mgr.insert_test_session("rt-1", "cfg-1", test_config("cfg-1"), handle.clone())
            .await;

        let (runtime_id, driver, returned) = mgr.resolve_session("rt-1").await.unwrap();
        assert_eq!(runtime_id, "rt-1");
        assert_eq!(returned.id, "rt-1");
        assert_eq!(driver.driver_type(), "postgresql");
    }

    #[tokio::test]
    async fn resolve_session_config_id_reuses_existing_runtime() {
        let mgr = test_manager_stub().await;
        let handle = ConnectionHandle {
            id: "rt-2".into(),
            pool_id: "pool-2".into(),
        };
        mgr.insert_test_session("rt-2", "cfg-2", test_config("cfg-2"), handle)
            .await;

        let (runtime_id, _driver, returned) = mgr.resolve_session("cfg-2").await.unwrap();
        assert_eq!(runtime_id, "rt-2");
        assert_eq!(returned.id, "rt-2");
    }

    async fn test_manager() -> (
        crate::testing::FileKeyringGuard,
        Arc<ConnectionManager>,
        Arc<Store>,
        Arc<MockDriver>,
    ) {
        let keyring = crate::testing::FileKeyringGuard::set();
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::init_with_path(dir.path()).await.unwrap());
        let registry = Arc::new(DriverRegistry::new());
        let mock = MockDriver::new(
            "postgres",
            MockDriverOptions {
                server_version: "PostgreSQL 16".into(),
                ..Default::default()
            },
        );
        registry
            .register_test_driver("postgres", mock.clone())
            .await;
        let mgr = Arc::new(ConnectionManager::new(registry, store.clone()));
        (keyring, mgr, store, mock)
    }

    fn sample_config(id: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: id.into(),
            name: "Test".into(),
            database_type: "postgres".into(),
            host: Some("localhost".into()),
            port: Some(5432),
            database: Some("app".into()),
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::Prefer,
            connection_timeout: 30,
            max_pool_size: 10,
            ssh_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
            pinned: false,
        }
    }

    #[tokio::test]
    async fn connect_registers_session_and_returns_runtime_id() {
        let (_keyring, mgr, store, _) = test_manager().await;
        store.save_connection(sample_config("cfg-1")).await.unwrap();
        let conn_id = mgr.connect("cfg-1").await.unwrap();
        assert!(conn_id.starts_with("mock-cfg-1"));
        assert_eq!(mgr.ui_session_map_len().await, 1);
    }

    #[tokio::test]
    async fn get_or_connect_reuses_existing_session() {
        let (_keyring, mgr, store, mock) = test_manager().await;
        store.save_connection(sample_config("cfg-1")).await.unwrap();
        let first = mgr.get_or_connect("cfg-1").await.unwrap();
        let second = mgr.get_or_connect("cfg-1").await.unwrap();
        assert_eq!(first, second);
        assert_eq!(mock.get_columns_calls(), 0);
    }

    #[tokio::test]
    async fn get_connection_returns_driver_and_updates_last_used() {
        let (_keyring, mgr, store, _) = test_manager().await;
        store.save_connection(sample_config("cfg-1")).await.unwrap();
        let conn_id = mgr.connect("cfg-1").await.unwrap();
        let (driver, handle) = mgr.get_connection(&conn_id).await.unwrap();
        assert_eq!(driver.driver_type(), "postgres");
        assert_eq!(handle.id, conn_id);
    }

    #[tokio::test]
    async fn disconnect_removes_session() {
        let (_keyring, mgr, store, _) = test_manager().await;
        store.save_connection(sample_config("cfg-1")).await.unwrap();
        let conn_id = mgr.connect("cfg-1").await.unwrap();
        mgr.disconnect(&conn_id).await.unwrap();
        assert_eq!(mgr.ui_session_map_len().await, 0);
    }

    #[tokio::test]
    async fn ping_returns_true_for_active_connection() {
        let (_keyring, mgr, store, _) = test_manager().await;
        store.save_connection(sample_config("cfg-1")).await.unwrap();
        let conn_id = mgr.connect("cfg-1").await.unwrap();
        assert!(mgr.ping(&conn_id).await);
        assert!(!mgr.ping("missing").await);
    }

    #[tokio::test]
    async fn get_connection_config_returns_stored_config() {
        let (_keyring, mgr, store, _) = test_manager().await;
        store.save_connection(sample_config("cfg-1")).await.unwrap();
        let conn_id = mgr.connect("cfg-1").await.unwrap();
        let cfg = mgr.get_connection_config(&conn_id).await.unwrap();
        assert_eq!(cfg.id, "cfg-1");
        assert_eq!(cfg.name, "Test");
    }

    #[tokio::test]
    async fn test_connection_uses_driver() {
        let (_keyring, mgr, _, _) = test_manager().await;
        let info = mgr.test_connection(&sample_config("cfg-1")).await.unwrap();
        assert_eq!(info.server_version, "PostgreSQL 16");
    }

    #[tokio::test]
    async fn connect_errors_when_config_missing() {
        let (_keyring, mgr, _, _) = test_manager().await;
        let err = mgr.connect("missing").await.unwrap_err();
        assert!(matches!(err, ConnectionError::ConfigNotFound(_)));
    }

    #[tokio::test]
    async fn driver_not_found_when_type_unregistered() {
        let (_keyring, mgr, store, _) = test_manager().await;
        let mut cfg = sample_config("cfg-x");
        cfg.database_type = "unknown-db".into();
        store.save_connection(cfg).await.unwrap();
        let err = mgr.connect("cfg-x").await.unwrap_err();
        assert!(matches!(err, ConnectionError::DriverNotFound(_)));
    }

    #[tokio::test]
    async fn shutdown_disconnects_all() {
        let (_keyring, mgr, store, _) = test_manager().await;
        store.save_connection(sample_config("cfg-1")).await.unwrap();
        let _ = mgr.connect("cfg-1").await.unwrap();
        mgr.shutdown().await;
        assert_eq!(mgr.ui_session_map_len().await, 0);
    }
}

//! Manages live database sessions and coordinates with the driver registry.
//!
//! ## ID terminology
//!
//! Two distinct kinds of identifiers flow through this module:
//!
//! - **`connection_id`** — the id of a *persisted connection configuration*
//!   ([`ConnectionConfig::id`] as stored in the [`Store`]). It is stable across
//!   app restarts and is what users pick in the UI sidebar.
//! - **`db_session_id`** — the *runtime database session* handle id generated
//!   by the driver once a connection has been established
//!   ([`ConnectionHandle::id`]). It only exists while a live session is pooled.
//!
//! Flow: `connect(connection_id)` loads the persisted config, asks the driver
//! to establish a session, and returns the resulting `db_session_id`. Callers
//! use that `db_session_id` for all subsequent operations. The
//! [`ConnectionManager::session_owner_map`] records `db_session_id →
//! connection_id` ownership; when a session is evicted for being idle, the map
//! is kept so [`ConnectionManager::get_session`] can transparently rebuild the
//! driver session from the *latest* persisted config while reusing the exact
//! same `db_session_id`.

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

/// A live driver session plus the effective config it was established with.
struct ActiveSession {
    handle: ConnectionHandle,
    config: ConnectionConfig,
    #[allow(dead_code)]
    created_at: Instant,
    last_used: Instant,
    _tunnel: Option<SshTunnel>,
}

/// Coordinates connection-config lookup, driver selection, and session pooling.
///
/// Keys and maps, by ID kind:
///
/// - `connections`: `db_session_id` → live [`ActiveSession`].
/// - `session_owner_map`: `db_session_id` → owning `connection_id` (the
///   persisted connection config it was created from). Survives idle eviction
///   so evicted sessions can be rebuilt under their original `db_session_id`.
/// - `ref_counts`: `db_session_id` → number of active borrowers.
pub struct ConnectionManager {
    registry: Arc<DriverRegistry>,
    connections: Arc<RwLock<HashMap<String, ActiveSession>>>,
    /// Maps `db_session_id` → its owning `connection_id` (persisted connection
    /// config id), so we can reconnect after idle eviction using the latest
    /// config from the Store.
    session_owner_map: Arc<RwLock<HashMap<String, String>>>,
    /// Reference counts per `db_session_id`. Each `get_or_connect_session`
    /// caller increments; `release` decrements; session is torn down only at
    /// zero.
    ref_counts: Arc<RwLock<HashMap<String, usize>>>,
    store: Arc<Store>,
    idle_timeout: Duration,
    /// Per-`connection_id` locks to prevent concurrent connect attempts for the
    /// same persisted configuration. Second+ callers wait and reuse the first
    /// caller's result.
    connect_locks: std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

#[derive(Debug, Error)]
pub enum ConnectionError {
    /// The persisted connection configuration (`connection_id`) does not exist.
    #[error(
        "Connection config '{0}' not found (connectionId refers to a persisted \
         connection configuration; no such configuration is stored)"
    )]
    ConnectionConfigNotFound(String),

    /// No live (or rebuildable) runtime session for this `db_session_id`.
    #[error(
        "DB session '{0}' not found (a dbSessionId is a runtime session id; \
         maybe you passed a connectionId where a dbSessionId was expected)"
    )]
    DbSessionNotFound(String),

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
            session_owner_map: Arc::new(RwLock::new(HashMap::new())),
            ref_counts: Arc::new(RwLock::new(HashMap::new())),
            store,
            idle_timeout: Duration::from_secs(1800),
            connect_locks: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Establish a database session for the persisted connection configuration
    /// `connection_id` and register it.
    ///
    /// Returns the runtime `db_session_id` (the driver-generated session
    /// handle id) that callers must use for subsequent operations.
    /// Establish a **new** runtime session for `connection_id`, optionally
    /// pinning the initial catalog/database, without reusing an existing session.
    /// Used by Transfer / Sync / Schema Diff sub-windows so they do not share
    /// live session state (active database, transactions) with the main workspace.
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

    /// Open a driver session for the persisted connection configuration
    /// `connection_id` without registering it in the UI session map.
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

    /// Resolve the owning `connection_id` (persisted connection config id)
    /// for a runtime `db_session_id`.
    /// Returns `None` if the mapping does not exist (e.g. no session was ever
    /// established for it via this manager).
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

    /// Return an existing live `db_session_id` for the given persisted
    /// `connection_id`, or establish a new session for it.
    /// Concurrent callers for the same `connection_id` are serialised: the
    /// first caller performs the actual connect; subsequent callers wait then
    /// reuse its result.
    pub async fn get_or_connect_session(
        &self,
        connection_id: &str,
    ) -> Result<String, ConnectionError> {
        let lock = {
            let mut locks = self.connect_locks.lock().unwrap();
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

    /// Decrement the reference count for a `db_session_id`. Only tears down
    /// the underlying driver session when the count reaches zero.
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

    /// Force-disconnect the session `db_session_id` regardless of reference
    /// count (e.g. from sidebar). Also drops its owner mapping.
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

    /// Current reference count for a `db_session_id` (0 if not tracked).
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

    /// Return the live driver session for `db_session_id`, refreshing its
    /// last-used timestamp.
    ///
    /// If the session was evicted (idle cleanup), it is transparently
    /// re-established under the SAME `db_session_id`: the surviving
    /// `session_owner_map` yields the owning `connection_id`, whose latest
    /// persisted config is used to rebuild the driver session.
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

        // Session evicted — rebuild it under the same db_session_id using the
        // owning connection_id's latest persisted config.
        self.reconnect(db_session_id).await
    }

    /// **MCP / Workflow / db_tools only.** Resolve a session from an id that may
    /// be either kind, trying **`db_session_id` first**, then falling back to
    /// `connection_id`.
    ///
    /// - If `id` matches a live runtime session (or one rebuildable via
    ///   `session_owner_map`), it is treated as a `db_session_id` and returned
    ///   as-is.
    /// - Otherwise `id` is treated as a persisted `connection_id`:
    ///   `get_or_connect_session` ensures a session exists for it and that
    ///   (possibly newly created) `db_session_id` is returned.
    ///
    /// GUI IPC paths must call [`get_session`](Self::get_session) directly and
    /// pass a real `db_session_id`; they must not use this dual-mode helper.
    pub async fn resolve_session_for_mcp(
        &self,
        id: &str,
    ) -> Result<(String, Arc<dyn DatabaseDriver>, ConnectionHandle), ConnectionError> {
        if let Ok((driver, handle)) = self.get_session(id).await {
            return Ok((id.to_string(), driver, handle));
        }
        let db_session_id = self.get_or_connect_session(id).await?;
        let (driver, handle) = self.get_session(&db_session_id).await?;
        Ok((db_session_id, driver, handle))
    }

    /// Transparently re-establish an evicted session under the SAME
    /// `db_session_id`. The surviving `session_owner_map` provides the owning
    /// `connection_id`; the latest config for it is read from the persistent
    /// Store so config edits made while idle are picked up.
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

        let handle = driver.connect(&effective_config).await?;

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

    /// Effective config of the live session `db_session_id`.
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

    /// Update the active logical database for a live session (after `use_database`).
    /// Keeps schema-cache keys and metadata lookups aligned with the session.
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

    /// Lightweight touch: refreshes a session's `last_used` so idle cleanup
    /// doesn't evict it.
    pub async fn ping(&self, db_session_id: &str) -> bool {
        let mut connections = self.connections.write().await;
        if let Some(active) = connections.get_mut(db_session_id) {
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
                tracing::info!(db_session_id = %id, name = %active.config.name,
                    "Evicting idle db session (session_owner_map entry kept for auto-reconnect)");
                if let Some(driver) = self.registry.get(&active.config.database_type).await {
                    let _ = driver.disconnect(active.handle).await;
                }
            }
        }
        // Note: session_owner_map is NOT cleared here — it stays so reconnect()
        // can rebuild evicted sessions under their original db_session_id
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
        self.session_owner_map.write().await.clear();

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

    /// Register a live session directly, bypassing driver connect.
    /// `db_session_id` is the runtime session handle id; `connection_id` is
    /// the persisted connection config id that owns it.
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

    /// Backdate a session's `last_used` beyond the idle timeout so a follow-up
    /// [`ConnectionManager::cleanup_idle_connections`] evicts it.
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

    /// A persisted connection config whose id (`connection_id`) is `connection_id`.
    fn test_config(connection_id: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: connection_id.to_string(),
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
    async fn resolve_session_passthrough_db_session_id() {
        let mgr = test_manager_stub().await;
        let handle = ConnectionHandle {
            id: "rt-1".into(),
            pool_id: "pool-1".into(),
        };
        // Live db session "rt-1" owned by persisted connection config "cfg-1".
        mgr.insert_test_session("rt-1", "cfg-1", test_config("cfg-1"), handle.clone())
            .await;

        let (db_session_id, driver, returned) = mgr.resolve_session_for_mcp("rt-1").await.unwrap();
        assert_eq!(db_session_id, "rt-1");
        assert_eq!(returned.id, "rt-1");
        assert_eq!(driver.driver_type(), "postgresql");
    }

    #[tokio::test]
    async fn resolve_session_connection_id_reuses_existing_runtime() {
        let mgr = test_manager_stub().await;
        let handle = ConnectionHandle {
            id: "rt-2".into(),
            pool_id: "pool-2".into(),
        };
        mgr.insert_test_session("rt-2", "cfg-2", test_config("cfg-2"), handle)
            .await;

        // Input is the connection_id of a config whose session already lives:
        // it must resolve to that existing db_session_id.
        let (db_session_id, _driver, returned) = mgr.resolve_session_for_mcp("cfg-2").await.unwrap();
        assert_eq!(db_session_id, "rt-2");
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
    async fn connect_registers_session_and_returns_db_session_id() {
        let (_keyring, mgr, store, _) = test_manager().await;
        store.save_connection(sample_config("cfg-1")).await.unwrap();
        let db_session_id = mgr.connect("cfg-1").await.unwrap();
        // db_session_id comes from the driver's runtime handle.
        assert!(db_session_id.starts_with("mock-cfg-1"));
        assert_eq!(mgr.session_owner_map_len().await, 1);
        assert_eq!(
            mgr.owner_connection_id(&db_session_id).await.as_deref(),
            Some("cfg-1")
        );
    }

    #[tokio::test]
    async fn get_or_connect_session_reuses_existing_session() {
        let (_keyring, mgr, store, mock) = test_manager().await;
        store.save_connection(sample_config("cfg-1")).await.unwrap();
        let first = mgr.get_or_connect_session("cfg-1").await.unwrap();
        let second = mgr.get_or_connect_session("cfg-1").await.unwrap();
        assert_eq!(first, second);
        assert_eq!(mock.get_columns_calls(), 0);
    }

    #[tokio::test]
    async fn get_session_returns_driver_and_updates_last_used() {
        let (_keyring, mgr, store, _) = test_manager().await;
        store.save_connection(sample_config("cfg-1")).await.unwrap();
        let db_session_id = mgr.connect("cfg-1").await.unwrap();
        let (driver, handle) = mgr.get_session(&db_session_id).await.unwrap();
        assert_eq!(driver.driver_type(), "postgres");
        assert_eq!(handle.id, db_session_id);
    }

    #[tokio::test]
    async fn disconnect_removes_session() {
        let (_keyring, mgr, store, _) = test_manager().await;
        store.save_connection(sample_config("cfg-1")).await.unwrap();
        let db_session_id = mgr.connect("cfg-1").await.unwrap();
        mgr.disconnect(&db_session_id).await.unwrap();
        assert_eq!(mgr.session_owner_map_len().await, 0);
    }

    #[tokio::test]
    async fn ping_returns_true_for_active_session() {
        let (_keyring, mgr, store, _) = test_manager().await;
        store.save_connection(sample_config("cfg-1")).await.unwrap();
        let db_session_id = mgr.connect("cfg-1").await.unwrap();
        assert!(mgr.ping(&db_session_id).await);
        assert!(!mgr.ping("missing").await);
    }

    #[tokio::test]
    async fn get_session_config_returns_stored_config() {
        let (_keyring, mgr, store, _) = test_manager().await;
        store.save_connection(sample_config("cfg-1")).await.unwrap();
        let db_session_id = mgr.connect("cfg-1").await.unwrap();
        let cfg = mgr.get_session_config(&db_session_id).await.unwrap();
        // The session's effective config keeps the owning connection_id.
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
    async fn connect_errors_when_connection_config_missing() {
        let (_keyring, mgr, _, _) = test_manager().await;
        let err = mgr.connect("missing").await.unwrap_err();
        assert!(matches!(err, ConnectionError::ConnectionConfigNotFound(_)));
        // The message must make clear which kind of id was not found.
        assert!(err
            .to_string()
            .contains("Connection config 'missing' not found"));
    }

    /// Invariant: an idle-evicted session is auto-reconnected through
    /// `session_owner_map` and keeps its original `db_session_id`.
    #[tokio::test]
    async fn evicted_session_auto_reconnects_preserving_db_session_id() {
        let (_keyring, mgr, store, _) = test_manager().await;
        store.save_connection(sample_config("cfg-1")).await.unwrap();
        let db_session_id = mgr.get_or_connect_session("cfg-1").await.unwrap();
        assert_eq!(
            mgr.owner_connection_id(&db_session_id).await.as_deref(),
            Some("cfg-1")
        );

        // Force idle eviction of the live session.
        mgr.expire_test_session(&db_session_id).await;
        mgr.cleanup_idle_connections().await;
        assert!(!mgr.ping(&db_session_id).await);
        // The ownership mapping survives eviction — this is what enables
        // auto-reconnect under the same db_session_id.
        assert_eq!(
            mgr.owner_connection_id(&db_session_id).await.as_deref(),
            Some("cfg-1")
        );

        // Auto-reconnect rebuilds the session from the latest persisted config
        // and reuses the exact same db_session_id.
        let (driver, handle) = mgr.get_session(&db_session_id).await.unwrap();
        assert_eq!(driver.driver_type(), "postgres");
        assert_eq!(handle.id, db_session_id);
        assert!(mgr.ping(&db_session_id).await);
        assert_eq!(
            mgr.owner_connection_id(&db_session_id).await.as_deref(),
            Some("cfg-1")
        );
    }

    /// Invariant: `resolve_session_for_mcp` tries the id as a **db_session_id first**
    /// and only falls back to treating it as a connection_id. When one string
    /// is both a live db_session_id and a persisted connection_id, the runtime
    /// session wins and no new session is created.
    #[tokio::test]
    async fn resolve_session_prefers_db_session_id_over_connection_id() {
        let (_keyring, mgr, store, _) = test_manager().await;
        // "dual" is BOTH a persisted connection_id ...
        store.save_connection(sample_config("dual")).await.unwrap();
        // ... and the db_session_id of a live session owned by "owner-a".
        mgr.insert_test_session(
            "dual",
            "owner-a",
            sample_config("dual"),
            ConnectionHandle {
                id: "dual".into(),
                pool_id: "pool-dual".into(),
            },
        )
        .await;

        let before = mgr.session_owner_map_len().await;
        let (db_session_id, _driver, handle) = mgr.resolve_session_for_mcp("dual").await.unwrap();

        // Resolved as the existing db_session_id, NOT as connection_id "dual"
        // (which would have created a fresh "mock-dual…" session).
        assert_eq!(db_session_id, "dual");
        assert_eq!(handle.id, "dual");
        assert_eq!(mgr.session_owner_map_len().await, before);
        assert_eq!(
            mgr.owner_connection_id(&db_session_id).await.as_deref(),
            Some("owner-a")
        );
    }

    /// Invariant (fallback leg): an id that is no live db_session_id is treated
    /// as a connection_id; a new session is created and its db_session_id
    /// returned with the owner mapping recorded.
    #[tokio::test]
    async fn resolve_session_falls_back_to_connection_id_and_creates_session() {
        let (_keyring, mgr, store, _) = test_manager().await;
        store
            .save_connection(sample_config("cfg-fb"))
            .await
            .unwrap();

        let (db_session_id, driver, handle) = mgr.resolve_session_for_mcp("cfg-fb").await.unwrap();
        assert_ne!(db_session_id, "cfg-fb");
        assert!(db_session_id.starts_with("mock-cfg-fb"));
        assert_eq!(handle.id, db_session_id);
        assert_eq!(driver.driver_type(), "postgres");
        assert_eq!(
            mgr.owner_connection_id(&db_session_id).await.as_deref(),
            Some("cfg-fb")
        );
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
        assert_eq!(mgr.session_owner_map_len().await, 0);
    }
}

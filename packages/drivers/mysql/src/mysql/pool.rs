//! Pool lifecycle and the shared out-of-band control connection.

use super::*;

impl MysqlDriver {
    /// Identity of the shared control connection.
    ///
    /// The `KILL QUERY` itself is database-agnostic — it names a thread id on the
    /// whole instance — but the **pool is not**: a `MySqlPool` reconnects with the
    /// connect options it was built from, so the database is part of its identity.
    /// Keying without it would let a pool built for one database serve a resource on
    /// another, and a database that has since been dropped would take every later
    /// acquisition down with it (the PostgreSQL driver learned this from the desktop
    /// E2E, which re-creates the worker database before every spec file).
    ///
    /// The password is excluded (a secret has no business in a map key); the
    /// connection is dropped once the last resource of this identity is released.
    pub(crate) fn control_pool_key(config: &ConnectionConfig) -> String {
        format!(
            "myctl|{}|{}|{}|{}|{:?}|{}",
            config.id,
            config.host.as_deref().unwrap_or(DEFAULT_HOST),
            config.port.unwrap_or(DEFAULT_PORT),
            config.username.as_deref().unwrap_or(""),
            config.ssl_mode,
            config
                .database
                .as_deref()
                .map(str::trim)
                .unwrap_or_default(),
        )
    }

    /// Get-or-create the shared control connection for this connection identity,
    /// adding one reference for the resource being opened.
    ///
    /// Held under the write lock across the connect on purpose: two resources
    /// racing for the same identity must not open two control connections, and the
    /// wait is bounded by one connection attempt.
    pub(crate) async fn acquire_control_pool(
        &self,
        config: &ConnectionConfig,
        timeout: Duration,
    ) -> Result<(), DriverError> {
        let key = Self::control_pool_key(config);
        let mut pools = self.control_pools.write().await;
        if let Some(entry) = pools.get_mut(&key) {
            entry.refs = entry.refs.saturating_add(1);
            return Ok(());
        }
        match Self::open_pool(
            build_mysql_options(config)?,
            timeout,
            CONTROL_POOL_CONNECTIONS,
            0,
        )
        .await
        {
            Ok(pool) => {
                pools.insert(key, ControlPoolEntry { pool, refs: 1 });
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// Drop one reference to the shared control connection of `pool_id`, closing
    /// the physical connection once the last resource is gone.
    ///
    /// Also forgets which identity the resource belonged to, so a double release
    /// cannot decrement a stranger's count.
    pub(crate) async fn release_control_pool(&self, pool_id: &str) {
        let key = self.control_keys.write().await.remove(pool_id);
        let Some(key) = key else {
            return;
        };
        let mut pools = self.control_pools.write().await;
        let Some(entry) = pools.get_mut(&key) else {
            return;
        };
        entry.refs = entry.refs.saturating_sub(1);
        if entry.refs == 0 {
            if let Some(entry) = pools.remove(&key) {
                entry.pool.close().await;
            }
        }
    }

    /// The shared control connection serving `handle`'s resource.
    ///
    /// Resolved through the resource's identity, not through the pool itself,
    /// because one control connection serves every resource of a (connection
    /// identity, database). `None` means this resource has no usable control
    /// connection, which is a supported state — acquisition never depends on it, so
    /// `KILL QUERY` and the transaction-evidence probe degrade instead of refusing
    /// the resource.
    pub(crate) async fn control_pool_for(&self, handle: &ConnectionHandle) -> Option<MySqlPool> {
        let key = self
            .control_keys
            .read()
            .await
            .get(&handle.pool_id)
            .cloned()?;
        self.control_pools
            .read()
            .await
            .get(&key)
            .map(|entry| entry.pool.clone())
    }

    pub fn new(is_mariadb: bool) -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
            active_databases: RwLock::new(HashMap::new()),
            transactions: Mutex::new(HashMap::new()),
            fixed_sessions: RwLock::new(HashMap::new()),
            fixed_states: RwLock::new(HashMap::new()),
            query_executions: Mutex::new(HashMap::new()),
            control_pools: RwLock::new(HashMap::new()),
            control_keys: RwLock::new(HashMap::new()),
            is_mariadb,
        }
    }

    pub(crate) fn get_pool<'a>(
        pools: &'a HashMap<String, MySqlPool>,
        handle: &ConnectionHandle,
    ) -> Result<&'a MySqlPool, DriverError> {
        pools
            .get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))
    }
}

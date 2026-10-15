//! Connection pool and connect-option helpers for PostgreSQL driver.

use crate::postgres::PostgresDriver;
use crate::resource::capabilities::CONTROL_POOL_CONNECTIONS;
use datazen_driver_api::*;
use sqlx::postgres::{PgPoolOptions, PgSslMode};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::time::Duration;

/// Upper bound on cached per-database pools for one handle. Browsing many
/// databases must not translate into unbounded server connections.
const MAX_DATABASE_POOLS_PER_HANDLE: usize = 8;

/// The socket PostgreSQL dials when the config names no host. One constant
/// feeds both [`build_pg_options`] and `DatabaseDriver::default_host`, so the
/// host can never compare endpoints against a value the driver does not dial.
pub(crate) const DEFAULT_HOST: &str = "localhost";

/// The port PostgreSQL dials when the config names none. See [`DEFAULT_HOST`].
pub(crate) const DEFAULT_PORT: u16 = 5432;

/// Connection ceiling for a cached foreign-database pool. Schema reads are
/// short and serialized by the caller, so a small pool is enough.
const DATABASE_POOL_MAX_CONNECTIONS: u32 = 2;

pub(crate) fn build_pg_options(
    config: &ConnectionConfig,
) -> Result<sqlx::postgres::PgConnectOptions, DriverError> {
    use sqlx::ConnectOptions;
    let mut opts = sqlx::postgres::PgConnectOptions::new()
        .host(config.host.as_deref().unwrap_or(DEFAULT_HOST))
        .port(config.port.unwrap_or(DEFAULT_PORT))
        .database(PostgresDriver::resolve_connect_database(config));

    if let Some(username) = &config.username {
        opts = opts.username(username);
    }
    if let Some(password) = config.password.as_deref().filter(|p| !p.trim().is_empty()) {
        opts = opts.password(password);
    }

    let pg_ssl = match config.ssl_mode {
        SslMode::Disable => PgSslMode::Disable,
        SslMode::Prefer => PgSslMode::Prefer,
        SslMode::Require => PgSslMode::Require,
        SslMode::VerifyCa => PgSslMode::VerifyCa,
        SslMode::VerifyFull => PgSslMode::VerifyFull,
    };
    opts = opts.ssl_mode(pg_ssl);

    if let Some(schema) = config.schema.as_deref().filter(|s| !s.trim().is_empty()) {
        let clean = schema.trim().replace('"', "\"\"");
        opts = opts.options([("search_path".to_string(), format!("\"{clean}\",public"))]);
    }

    opts = opts.log_statements(tracing::log::LevelFilter::Trace);
    Ok(opts)
}

impl PostgresDriver {
    pub(crate) fn get_pool<'a>(
        pools: &'a HashMap<String, PgPool>,
        handle: &ConnectionHandle,
    ) -> Result<&'a PgPool, DriverError> {
        pools
            .get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))
    }

    /// Trim and validate a database name before it is used to open a pool.
    pub(crate) fn validate_database_name(database: &str) -> Result<String, DriverError> {
        let trimmed = database.trim();
        if trimmed.is_empty() {
            return Err(DriverError::InvalidConfig(
                "Database name must not be empty".into(),
            ));
        }
        if trimmed.contains('\0') {
            return Err(DriverError::InvalidConfig(
                "Database name contains invalid characters".into(),
            ));
        }
        Ok(trimmed.to_string())
    }

    /// Database used when connecting: config value, or default `postgres` when empty.
    pub(crate) fn resolve_connect_database(config: &ConnectionConfig) -> &str {
        config
            .database
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("postgres")
    }

    /// Identity of the shared cancel-only control connection.
    ///
    /// The cancel itself is database-agnostic — `pg_cancel_backend` names a pid on
    /// the whole instance — but the **pool is not**: a `PgPool` reconnects with the
    /// connect options it was built from, so the database is part of its identity.
    /// Keying without it let a pool built for one database serve a resource on
    /// another, and in the desktop E2E, where the worker database behind a
    /// connection is dropped and re-created before every spec file, the reused pool
    /// then tried to reconnect to a database that no longer existed and took the
    /// whole acquisition down with it (measured: the two spec files that follow the
    /// first one failed in their `before all` hook).
    ///
    /// So: one control connection per (identity, database), shared by every resource
    /// that resolves to the same server and database — not one per resource, which
    /// is the duplicate the resource budget used to charge for. It stays out of the
    /// declared cost of a resource (see `resource/capabilities.rs`).
    ///
    /// The password is excluded too (a secret has no business in a map key); the
    /// config that carries it is stored per resource and the connection is
    /// dropped once the last resource of this identity is released.
    pub(crate) fn control_pool_key(config: &ConnectionConfig) -> String {
        format!(
            "pgctl|{}|{}|{}|{}|{:?}|{}",
            config.id,
            config.host.as_deref().unwrap_or(DEFAULT_HOST),
            config.port.unwrap_or(DEFAULT_PORT),
            config.username.as_deref().unwrap_or(""),
            config.ssl_mode,
            Self::resolve_connect_database(config),
        )
    }

    pub(crate) async fn open_pool(
        opts: sqlx::postgres::PgConnectOptions,
        timeout: Duration,
        max_connections: u32,
        min_connections: u32,
    ) -> Result<PgPool, DriverError> {
        let mut builder = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(timeout);
        if min_connections > 0 {
            builder = builder.min_connections(min_connections);
        }
        builder
            .connect_with(opts)
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))
    }

    pub(crate) async fn fetch_tables_from_pool(
        pool: &PgPool,
        schema_filter: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let filter_clause = match schema_filter {
            Some(s) if !s.trim().is_empty() => {
                let escaped = s.replace('\'', "''");
                format!("AND n.nspname = '{escaped}'")
            }
            _ => String::new(),
        };

        let sql = format!(
            r#"
            SELECT n.nspname AS table_schema, c.relname AS table_name,
                   CASE c.relkind
                     WHEN 'v' THEN 'VIEW'
                     WHEN 'm' THEN 'VIEW'
                     ELSE 'BASE TABLE'
                   END AS table_type
            FROM pg_catalog.pg_class c
            JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
            WHERE c.relkind IN ('r', 'v', 'm', 'f', 'p')
              AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
              AND NOT pg_catalog.pg_is_other_temp_schema(n.oid)
              AND (pg_catalog.pg_my_temp_schema() = 0 OR n.oid <> pg_catalog.pg_my_temp_schema())
              {filter_clause}
            UNION ALL
            SELECT n.nspname AS table_schema, '' AS table_name, 'SCHEMA_MARKER' AS table_type
            FROM pg_catalog.pg_namespace n
            WHERE n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
              AND NOT pg_catalog.pg_is_other_temp_schema(n.oid)
              AND (pg_catalog.pg_my_temp_schema() = 0 OR n.oid <> pg_catalog.pg_my_temp_schema())
              {filter_clause}
              AND NOT EXISTS (
                SELECT 1 FROM pg_catalog.pg_class c
                WHERE c.relnamespace = n.oid
                  AND c.relkind IN ('r', 'v', 'm', 'f', 'p')
              )
            ORDER BY table_schema, table_name
            "#
        );

        let rows = sqlx::query(&sql)
            .fetch_all(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        Ok(rows
            .iter()
            .map(|r| {
                let tt: String = r.get("table_type");
                let name: String = r.get("table_name");
                TableInfo {
                    schema: r.get("table_schema"),
                    name,
                    table_type: match tt.as_str() {
                        "VIEW" => TableType::View,
                        "SCHEMA_MARKER" => TableType::SystemTable,
                        _ => TableType::Table,
                    },
                    row_count: None,
                }
            })
            .collect())
    }

    /// Whether `database` is the database this handle's primary pool is
    /// connected to. Read-only metadata recorded at connect time — it is never
    /// mutated to "switch" a session.
    pub(crate) async fn is_active_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> bool {
        self.active_databases
            .read()
            .await
            .get(&handle.pool_id)
            .map(String::as_str)
            == Some(database)
    }

    /// Select one gate per original pool, including callers waiting to acquire it.
    /// Never remove an entry while a waiter retains its Arc: that would let a
    /// new caller create a second gate for the same physical resource. Expired
    /// weak entries are reclaimed on the next lookup, after the last caller
    /// (including a cancelled close) drops its Arc.
    async fn pool_lifecycle_gate(&self, pool_id: &str) -> std::sync::Arc<tokio::sync::RwLock<()>> {
        let mut gates = self.pool_lifecycle.lock().await;
        gates.retain(|_, gate| gate.strong_count() > 0);
        if let Some(gate) = gates.get(pool_id).and_then(std::sync::Weak::upgrade) {
            return gate;
        }
        let gate = std::sync::Arc::new(tokio::sync::RwLock::new(()));
        gates.insert(pool_id.to_string(), std::sync::Arc::downgrade(&gate));
        gate
    }

    /// Resolve the pool that serves `database` for this handle.
    ///
    /// The handle's own database (and a blank name, which callers use to mean
    /// "the current one") maps to the primary pool. Any other database gets a
    /// dedicated cached pool, because PostgreSQL resolves unqualified relations
    /// against the *session's* catalog and offers no way to reach another one.
    pub(crate) async fn pool_for_target(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<PgPool, DriverError> {
        let lifecycle = self.pool_lifecycle_gate(&handle.pool_id).await;
        // Keep the read side across resolution, I/O and cache registration.
        // Disconnect's write side must see every creation admitted before it.
        let _creation = lifecycle.read().await;
        let db = database.trim();
        if db.is_empty() || self.is_active_database(handle, db).await {
            let pools = self.pools.read().await;
            return Self::get_pool(&pools, handle).cloned();
        }
        let db = Self::validate_database_name(db)?;

        let key = (handle.pool_id.clone(), db.clone());
        let now = self
            .database_pool_clock
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        {
            let mut cache = self.database_pools.write().await;
            if let Some(entry) = cache.get_mut(&key) {
                entry.last_used = now;
                return Ok(entry.pool.clone());
            }
        }

        let pool = self
            .pool_for_named_database(handle, &db, DATABASE_POOL_MAX_CONNECTIONS, 0)
            .await?;

        let evicted = {
            let mut cache = self.database_pools.write().await;
            // Another caller may have won the race; prefer the cached pool and
            // drop the one we just opened.
            if let Some(entry) = cache.get_mut(&key) {
                entry.last_used = now;
                vec![pool.clone()]
            } else {
                cache.insert(
                    key,
                    crate::postgres::DatabasePoolEntry {
                        pool: pool.clone(),
                        last_used: now,
                    },
                );
                Self::evict_database_pools(&mut cache, &handle.pool_id)
            }
        };
        for stale in evicted {
            stale.close().await;
        }
        Ok(pool)
    }

    /// Drop the least-recently-used foreign-database pools beyond the per-handle
    /// cap, returning them so the caller can close them outside the lock.
    fn evict_database_pools(
        cache: &mut HashMap<(String, String), crate::postgres::DatabasePoolEntry>,
        pool_id: &str,
    ) -> Vec<PgPool> {
        let mut owned: Vec<((String, String), u64)> = cache
            .iter()
            .filter(|((owner, _), _)| owner == pool_id)
            .map(|(key, entry)| (key.clone(), entry.last_used))
            .collect();
        if owned.len() <= MAX_DATABASE_POOLS_PER_HANDLE {
            return Vec::new();
        }
        owned.sort_by_key(|(_, last_used)| *last_used);
        let excess = owned.len() - MAX_DATABASE_POOLS_PER_HANDLE;
        owned
            .into_iter()
            .take(excess)
            .filter_map(|(key, _)| cache.remove(&key).map(|entry| entry.pool))
            .collect()
    }

    /// Close and forget the cached pool for one database (right-click
    /// "close database connection"). Returns whether a pool was open.
    pub(crate) async fn close_database_pool(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<bool, DriverError> {
        let lifecycle = self.pool_lifecycle_gate(&handle.pool_id).await;
        let _close = lifecycle.write().await;
        let db = database.trim();
        if db.is_empty() || self.is_active_database(handle, db).await {
            return Err(DriverError::InvalidConfig(
                "cannot close the connection's own database".into(),
            ));
        }
        let key = (handle.pool_id.clone(), db.to_string());
        let pool = self
            .database_pools
            .read()
            .await
            .get(&key)
            .map(|entry| entry.pool.clone());
        match pool {
            Some(pool) => {
                // A cancelled close remains registered for disconnect/retry.
                pool.close().await;
                self.database_pools.write().await.remove(&key);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Every foreign-database pool still cached for this handle.
    ///
    /// The handle's own database is deliberately excluded. It is the session's
    /// primary pool, so it is already implied by the connection being up;
    /// [`Self::close_database_pool`] refuses to release it individually; and
    /// [`Self::resolve_connect_database`] falls back to the literal `postgres`
    /// when the config names no database — so reporting it made every freshly
    /// connected session mark a database the user never opened. What is left is
    /// exactly the set `close_database` can act on.
    pub(crate) async fn open_databases_impl(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<Vec<String>, DriverError> {
        let cache = self.database_pools.read().await;
        let mut open: Vec<String> = cache
            .keys()
            .filter(|(owner, _)| owner == &handle.pool_id)
            .map(|(_, database)| database.clone())
            .collect();
        open.sort();
        open.dedup();
        Ok(open)
    }

    /// Open a pool for `database` using the handle's stored connect template.
    pub(crate) async fn pool_for_named_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        max_connections: u32,
        min_connections: u32,
    ) -> Result<PgPool, DriverError> {
        let configs = self.connect_configs.read().await;
        let config = configs
            .get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        let timeout = Duration::from_secs(config.connection_timeout as u64);
        let opts = build_pg_options(config)?.database(database);
        drop(configs);

        Self::open_pool(opts, timeout, max_connections, min_connections)
            .await
            .map_err(|e| {
                // Surface unknown-database as QueryFailed (parity with MySQL USE failures).
                match e {
                    DriverError::ConnectionFailed(msg) => DriverError::QueryFailed(format!(
                        "Failed to connect to database `{database}`: {msg}"
                    )),
                    other => other,
                }
            })
    }

    pub(crate) async fn test_connection_impl(
        &self,
        config: &ConnectionConfig,
    ) -> Result<ServerInfo, DriverError> {
        let opts = build_pg_options(config)?;
        let timeout = Duration::from_secs(config.connection_timeout as u64);

        let pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(timeout)
            .connect_with(opts)
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;

        let result = sqlx::query("SELECT version()")
            .fetch_one(&pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()));

        pool.close().await;

        let row = result?;
        let version: String = row.try_get(0).unwrap_or_default();

        Ok(ServerInfo {
            server_version: version,
            server_type: "PostgreSQL".to_string(),
        })
    }

    pub(crate) async fn connect_impl(
        &self,
        config: &ConnectionConfig,
    ) -> Result<ConnectionHandle, DriverError> {
        let opts = build_pg_options(config)?;
        let timeout = Duration::from_secs(config.connection_timeout as u64);
        let resolved_db = Self::resolve_connect_database(config).to_string();

        let max = config.effective_max_pool_size();
        let min = 2u32.min(max);
        let pool = Self::open_pool(opts, timeout, max, min).await?;

        let acquire_result: Result<(), DriverError> = async {
            let _c1 = pool
                .acquire()
                .await
                .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
            if max >= 2 {
                let _c2 = pool
                    .acquire()
                    .await
                    .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
            }
            Ok(())
        }
        .await;

        if let Err(e) = acquire_result {
            pool.close().await;
            return Err(e);
        }

        let pool_id = uuid::Uuid::new_v4().to_string();
        let connection_id = uuid::Uuid::new_v4().to_string();

        self.connect_configs
            .write()
            .await
            .insert(pool_id.clone(), config.clone());
        self.active_databases
            .write()
            .await
            .insert(pool_id.clone(), resolved_db);
        let control_opts = build_pg_options(config)?;
        let control_key = Self::control_pool_key(config);
        if let Err(error) = self
            .acquire_control_pool(&control_key, control_opts, timeout)
            .await
        {
            // The cancel-only connection is an auxiliary capability, and the
            // primary path must not depend on it: a resource that cannot cancel is
            // degraded, not unusable. Failing here instead is how a stale control
            // pool (bound to a database that has since been dropped) turned into a
            // refused `DROP TABLE` in the desktop E2E. Resolution is deferred to
            // `control_pool_for`, which answers `None` and lets cancel degrade.
            tracing::warn!(
                %error,
                "cancel-only control connection unavailable; this resource cannot cancel precisely"
            );
        }
        self.pools.write().await.insert(pool_id.clone(), pool);

        Ok(ConnectionHandle {
            id: connection_id,
            pool_id,
        })
    }

    /// Get-or-create the shared cancel-only control connection for
    /// `control_key`, adding one reference for the resource being opened.
    ///
    /// Held under the write lock across the connect on purpose: two resources
    /// racing for the same identity must not open two control connections, and
    /// the wait is bounded by one connection attempt.
    async fn acquire_control_pool(
        &self,
        control_key: &str,
        opts: sqlx::postgres::PgConnectOptions,
        timeout: Duration,
    ) -> Result<(), DriverError> {
        let mut pools = self.control_pools.write().await;
        if let Some(entry) = pools.get_mut(control_key) {
            entry.refs = entry.refs.saturating_add(1);
            return Ok(());
        }
        match Self::open_pool(opts, timeout, CONTROL_POOL_CONNECTIONS, 0).await {
            Ok(pool) => {
                pools.insert(
                    control_key.to_string(),
                    crate::postgres::ControlPoolEntry { pool, refs: 1 },
                );
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// The shared control connection serving `handle`'s resource.
    ///
    /// Resolved through the resource's config, not through `pool_id`, because one
    /// control connection serves every resource of a (connection identity,
    /// database). Callers that need a backend pid (`pg_cancel_backend`) or a
    /// server-global fact go through here; `None` means this resource has no usable
    /// control connection, which is a supported state — acquisition never depends on
    /// it, so cancel degrades instead of refusing the resource.
    pub(crate) async fn control_pool_for(&self, handle: &ConnectionHandle) -> Option<PgPool> {
        let key = {
            let configs = self.connect_configs.read().await;
            configs.get(&handle.pool_id).map(Self::control_pool_key)
        }?;
        self.control_pools
            .read()
            .await
            .get(&key)
            .map(|entry| entry.pool.clone())
    }

    /// Drop one reference to the shared control connection for the resource
    /// `pool_id`, closing the physical connection once the last one is gone.
    ///
    /// Must run **before** `connect_configs` forgets the resource: the identity
    /// key comes from that config.
    async fn release_control_pool(&self, pool_id: &str) {
        let key = {
            let configs = self.connect_configs.read().await;
            configs.get(pool_id).map(Self::control_pool_key)
        };
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

    pub(crate) async fn disconnect_impl(
        &self,
        handle: ConnectionHandle,
    ) -> Result<(), DriverError> {
        let lifecycle = self.pool_lifecycle_gate(&handle.pool_id).await;
        // Tokio's per-pool write gate waits for earlier creators and blocks
        // later ones until every original pool and its config are removed.
        // Cancellation drops the guard; retained registrations permit retry.
        let _disconnect = lifecycle.write().await;
        let removed = self.transactions.lock().await.remove(&handle.id);
        if let Some(pinned) = removed {
            let mut conn = pinned.lock().await;
            let _ = sqlx::query("ROLLBACK").execute(&mut **conn).await;
        }
        // Retain every original pool until all close futures confirm closure.
        // A host timeout may cancel this future at any await; the next close
        // must still find and await those same pools, including foreign catalogs.
        let foreign_pools: Vec<PgPool> = self
            .database_pools
            .read()
            .await
            .iter()
            .filter(|((owner, _), _)| owner == &handle.pool_id)
            .map(|(_, entry)| entry.pool.clone())
            .collect();
        for pool in foreign_pools {
            pool.close().await;
        }
        let pool = self.pools.read().await.get(&handle.pool_id).cloned();
        if let Some(pool) = pool {
            pool.close().await;
        }
        // The control connection is shared by every resource of one connection
        // identity, so this drops a reference rather than closing it: the last
        // resource out closes it. Runs while `connect_configs` still holds the
        // config the identity key is derived from.
        self.release_control_pool(&handle.pool_id).await;
        self.database_pools
            .write()
            .await
            .retain(|(owner, _), _| owner != &handle.pool_id);
        self.pools.write().await.remove(&handle.pool_id);
        self.fixed_sessions.write().await.remove(&handle.id);
        self.fixed_contexts.write().await.remove(&handle.id);
        self.active_databases.write().await.remove(&handle.pool_id);
        self.connect_configs.write().await.remove(&handle.pool_id);
        self.query_executions
            .lock()
            .await
            .retain(|_, execution| execution.session_id != handle.id);
        Ok(())
    }

    pub(crate) async fn get_databases_impl(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<Vec<String>, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let rows = sqlx::query(
            "SELECT datname FROM pg_database WHERE datistemplate = false ORDER BY datname",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        Ok(rows.iter().map(|r| r.get::<String, _>(0)).collect())
    }

    pub(crate) async fn get_tables_impl(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::AnySchema)?;
        // A blank or self-referential name means "the database this handle is
        // connected to"; anything else selects (or opens) that database's pool.
        let pool = self.pool_for_target(handle, database).await?;
        Self::fetch_tables_from_pool(&pool, schema).await
    }

    pub(crate) async fn get_server_info_impl(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<ServerInfo, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let row = sqlx::query("SELECT version()")
            .fetch_one(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let version: String = row.try_get(0).unwrap_or_default();
        Ok(ServerInfo {
            server_version: version,
            server_type: "PostgreSQL".to_string(),
        })
    }

    pub(crate) async fn physical_database_identity_impl(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Option<String>, DriverError> {
        let pool = self.pool_for_target(handle, database).await?;
        let Ok(row) = sqlx::query(
            "SELECT system_identifier::text AS system_identifier, current_database() AS database_name FROM pg_control_system()",
        )
        .fetch_one(&pool)
        .await
        else {
            // The postmaster start time identifies one running node, not a
            // PostgreSQL cluster; aliases that route to different replicas
            // could otherwise be incorrectly treated as separate databases.
            return Ok(None);
        };
        let server: String = row
            .try_get("system_identifier")
            .map_err(|error| DriverError::QueryFailed(error.to_string()))?;
        let database: String = row
            .try_get("database_name")
            .map_err(|error| DriverError::QueryFailed(error.to_string()))?;
        Ok(Some(
            serde_json::json!({
                "driver": "postgresql",
                "server": server,
                "database": database,
            })
            .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::DatabaseDriver;

    fn config(host: Option<&str>, port: Option<u16>) -> ConnectionConfig {
        ConnectionConfig {
            id: "cfg".into(),
            name: "n".into(),
            database_type: "postgresql".into(),
            host: host.map(str::to_string),
            port,
            database: Some("app".into()),
            schema: None,
            username: None,
            password: None,
            ssl_mode: Default::default(),
            connection_timeout: 5,
            max_pool_size: 10,
            ssh_tunnel: None,
            tunnel_kind: None,
            tunnel_id: None,
            http_proxy_tunnel: None,
            websocket_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
            pinned: false,
        }
    }

    /// 反漂移闸：`default_host()`/`default_port()` 是宿主做端点物理身份摘要时
    /// 唯一的「默认 host/port」来源。它一旦与 `build_pg_options` 实际拨号的值分家，
    /// 省略 host 的连接与显式写全 host 的连接就会算出两个不同的 service_key，
    /// 自覆盖在 admission 静默漏判。所以这里不测常量本身，只测「声明 == 实拨」。
    #[test]
    fn the_declared_defaults_are_exactly_what_connect_dials() {
        let driver = PostgresDriver::new();
        let declared_host = driver
            .default_host()
            .expect("postgres has an implicit host");
        let declared_port = driver
            .default_port()
            .expect("postgres has an implicit port");

        let dialled = build_pg_options(&config(None, None)).expect("options build");
        assert_eq!(
            dialled.get_host(),
            declared_host,
            "default_host() must be the host connect actually dials"
        );
        assert_eq!(
            dialled.get_port(),
            declared_port,
            "default_port() must be the port connect actually dials"
        );

        // 常量与声明同源：改一处不改另一处，测试立刻红。
        assert_eq!(declared_host, DEFAULT_HOST);
        assert_eq!(declared_port, DEFAULT_PORT);

        // 配置里写了值就用配置的值，默认不覆盖。
        let explicit =
            build_pg_options(&config(Some("db-a.example.com"), Some(6432))).expect("options build");
        assert_eq!(explicit.get_host(), "db-a.example.com");
        assert_eq!(explicit.get_port(), 6432);
    }
}

//! Connection pool and connect-option helpers for PostgreSQL driver.

use crate::postgres::PostgresDriver;
use datazen_driver_api::*;
use sqlx::postgres::{PgPoolOptions, PgSslMode};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::time::Duration;

/// Upper bound on cached per-database pools for one handle. Browsing many
/// databases must not translate into unbounded server connections.
const MAX_DATABASE_POOLS_PER_HANDLE: usize = 8;

/// Connection ceiling for a cached foreign-database pool. Schema reads are
/// short and serialized by the caller, so a small pool is enough.
const DATABASE_POOL_MAX_CONNECTIONS: u32 = 2;

pub(crate) fn build_pg_options(
    config: &ConnectionConfig,
) -> Result<sqlx::postgres::PgConnectOptions, DriverError> {
    use sqlx::ConnectOptions;
    let mut opts = sqlx::postgres::PgConnectOptions::new()
        .host(config.host.as_deref().unwrap_or("localhost"))
        .port(config.port.unwrap_or(5432))
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
                format!("AND n.nspname IN ('{escaped}', 'public')")
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
        let db = database.trim();
        if db.is_empty() || self.is_active_database(handle, db).await {
            return Err(DriverError::InvalidConfig(
                "cannot close the connection's own database".into(),
            ));
        }
        let removed = self
            .database_pools
            .write()
            .await
            .remove(&(handle.pool_id.clone(), db.to_string()));
        match removed {
            Some(entry) => {
                entry.pool.close().await;
                Ok(true)
            }
            None => Ok(false),
        }
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
        let control_pool = match Self::open_pool(control_opts, timeout, 1, 0).await {
            Ok(p) => p,
            Err(e) => {
                pool.close().await;
                return Err(e);
            }
        };
        self.pools.write().await.insert(pool_id.clone(), pool);
        self.control_pools
            .write()
            .await
            .insert(pool_id.clone(), control_pool);

        Ok(ConnectionHandle {
            id: connection_id,
            pool_id,
        })
    }

    pub(crate) async fn disconnect_impl(
        &self,
        handle: ConnectionHandle,
    ) -> Result<(), DriverError> {
        if let Some(mut conn) = self.transactions.lock().await.remove(&handle.id) {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        }
        self.active_databases.write().await.remove(&handle.pool_id);
        self.connect_configs.write().await.remove(&handle.pool_id);
        let foreign_pools: Vec<PgPool> = {
            let mut cache = self.database_pools.write().await;
            let keys: Vec<(String, String)> = cache
                .keys()
                .filter(|(owner, _)| owner == &handle.pool_id)
                .cloned()
                .collect();
            keys.into_iter()
                .filter_map(|key| cache.remove(&key).map(|entry| entry.pool))
                .collect()
        };
        for pool in foreign_pools {
            pool.close().await;
        }
        self.query_executions
            .lock()
            .await
            .retain(|_, execution| execution.session_id != handle.id);
        if let Some(pool) = self.pools.write().await.remove(&handle.pool_id) {
            pool.close().await;
        }
        if let Some(pool) = self.control_pools.write().await.remove(&handle.pool_id) {
            pool.close().await;
        }
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
}

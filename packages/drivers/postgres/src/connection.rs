//! Connection pool and connect-option helpers for PostgreSQL driver.

use crate::postgres::PostgresDriver;
use datazen_driver_api::*;
use sqlx::postgres::{PgPoolOptions, PgSslMode};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::time::Duration;

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

    /// Trim and validate a database name for `use_database` / reconnect.
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

    pub(crate) async fn fetch_tables_from_pool(pool: &PgPool) -> Result<Vec<TableInfo>, DriverError> {
        let rows = sqlx::query(
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
            UNION ALL
            SELECT n.nspname AS table_schema, '' AS table_name, 'SCHEMA_MARKER' AS table_type
            FROM pg_catalog.pg_namespace n
            WHERE n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
              AND NOT pg_catalog.pg_is_other_temp_schema(n.oid)
              AND (pg_catalog.pg_my_temp_schema() = 0 OR n.oid <> pg_catalog.pg_my_temp_schema())
              AND NOT EXISTS (
                SELECT 1 FROM pg_catalog.pg_class c
                WHERE c.relnamespace = n.oid
                  AND c.relkind IN ('r', 'v', 'm', 'f', 'p')
              )
            ORDER BY table_schema, table_name
            "#,
        )
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

pub(crate) async fn test_connection_impl(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
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

pub(crate) async fn connect_impl(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
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

pub(crate) async fn disconnect_impl(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        if let Some(mut conn) = self.transactions.lock().await.remove(&handle.id) {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        }
        self.active_databases.write().await.remove(&handle.pool_id);
        self.connect_configs.write().await.remove(&handle.pool_id);
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

pub(crate) async fn get_databases_impl(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
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
    ) -> Result<Vec<TableInfo>, DriverError> {
        let db = database.trim();

        // Empty name → list tables on the currently connected database.
        if db.is_empty() {
            let pools = self.pools.read().await;
            let pool = Self::get_pool(&pools, handle)?;
            return Self::fetch_tables_from_pool(pool).await;
        }

        let active = self
            .active_databases
            .read()
            .await
            .get(&handle.pool_id)
            .cloned();

        if active.as_deref() == Some(db) {
            let pools = self.pools.read().await;
            let pool = Self::get_pool(&pools, handle)?;
            return Self::fetch_tables_from_pool(pool).await;
        }

        // information_schema is per-database in Postgres — open a temporary pool
        // for the named catalog without permanently switching the handle.
        let temp = self.pool_for_named_database(handle, db, 1, 0).await?;
        let result = Self::fetch_tables_from_pool(&temp).await;
        temp.close().await;
        result
    }

pub(crate) async fn use_database_impl(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<(), DriverError> {
        let trimmed = Self::validate_database_name(database)?;

        {
            let active = self.active_databases.read().await;
            if active.get(&handle.pool_id).map(String::as_str) == Some(trimmed.as_str()) {
                return Ok(());
            }
        }

        if self.transactions.lock().await.contains_key(&handle.id) {
            return Err(DriverError::TransactionError(
                "Cannot switch database while a transaction is open".into(),
            ));
        }

        // Postgres cannot USE like MySQL — reconnect the handle's pool to the target DB.
        // Missing connect template / pool → ConnectionFailed (same shape as get_pool).
        let max = {
            let configs = self.connect_configs.read().await;
            configs
                .get(&handle.pool_id)
                .map(|c| c.effective_max_pool_size())
                .unwrap_or(10)
        };
        let min = 2u32.min(max);
        let new_pool = self
            .pool_for_named_database(handle, &trimmed, max, min)
            .await?;
        let new_control_pool = match self.pool_for_named_database(handle, &trimmed, 1, 0).await {
            Ok(pool) => pool,
            Err(error) => {
                new_pool.close().await;
                return Err(error);
            }
        };

        let old = {
            let mut pools = self.pools.write().await;
            pools.insert(handle.pool_id.clone(), new_pool)
        };
        let old_control = {
            let mut pools = self.control_pools.write().await;
            pools.insert(handle.pool_id.clone(), new_control_pool)
        };
        self.active_databases
            .write()
            .await
            .insert(handle.pool_id.clone(), trimmed);

        if let Some(old) = old {
            old.close().await;
        }
        if let Some(old_control) = old_control {
            old_control.close().await;
        }
        Ok(())
    }

pub(crate) async fn get_server_info_impl(&self, handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
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

//! PostgreSQL driver backed by sqlx PgPool.

use crate::structure::{caps_for_version, plan_structure_changes_with_caps};
use async_trait::async_trait;
use datazen_driver_api::*;
use rust_decimal::prelude::ToPrimitive;
use sqlx::pool::PoolConnection;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Column, Executor, PgPool, Postgres, Row};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

const JS_MAX_SAFE_INT: i64 = 9_007_199_254_740_991;
const JS_MIN_SAFE_INT: i64 = -9_007_199_254_740_991;

pub struct PostgresDriver {
    pools: RwLock<HashMap<String, PgPool>>,
    /// Template connection config (host/user/pass/timeout) for reconnecting to other databases.
    /// Postgres has no session `USE`; switching means a new pool with the target database name.
    connect_configs: RwLock<HashMap<String, ConnectionConfig>>,
    /// Database the handle's pool is currently connected to, keyed by pool_id.
    active_databases: RwLock<HashMap<String, String>>,
    /// Open transactions: connection held for the lifetime of BEGIN…COMMIT/ROLLBACK, keyed by handle.id.
    transactions: Mutex<HashMap<String, PoolConnection<Postgres>>>,
}

impl PostgresDriver {
    pub fn new() -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
            connect_configs: RwLock::new(HashMap::new()),
            active_databases: RwLock::new(HashMap::new()),
            transactions: Mutex::new(HashMap::new()),
        }
    }

    fn get_pool<'a>(
        pools: &'a HashMap<String, PgPool>,
        handle: &ConnectionHandle,
    ) -> Result<&'a PgPool, DriverError> {
        pools
            .get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))
    }

    /// Trim and validate a database name for `use_database` / reconnect.
    fn validate_database_name(database: &str) -> Result<String, DriverError> {
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
    fn resolve_connect_database(config: &ConnectionConfig) -> &str {
        config
            .database
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("postgres")
    }

    async fn open_pool(
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

    async fn fetch_tables_from_pool(pool: &PgPool) -> Result<Vec<TableInfo>, DriverError> {
        let rows = sqlx::query(
            r#"
            SELECT table_schema, table_name, table_type
            FROM information_schema.tables
            WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
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
                TableInfo {
                    schema: r.get("table_schema"),
                    name: r.get("table_name"),
                    table_type: match tt.as_str() {
                        "VIEW" => TableType::View,
                        _ => TableType::Table,
                    },
                    row_count: None,
                }
            })
            .collect())
    }

    /// Open a pool for `database` using the handle's stored connect template.
    async fn pool_for_named_database(
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

    fn safe_integer(v: i64) -> Value {
        if v > JS_MAX_SAFE_INT || v < JS_MIN_SAFE_INT {
            Value::String(v.to_string())
        } else {
            Value::Integer(v)
        }
    }

    /// Bind `Value` params into a sqlx Postgres query (`$1`, `$2`, … placeholders).
    fn bind_values<'q>(
        mut query: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
        params: &'q [Value],
    ) -> sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments> {
        for p in params {
            query = match p {
                Value::Null => query.bind(Option::<String>::None),
                Value::Bool(b) => query.bind(*b),
                Value::Integer(i) => query.bind(*i),
                Value::Float(f) => query.bind(*f),
                Value::String(s) | Value::Timestamp(s) => query.bind(s.as_str()),
                Value::Bytes(b) => query.bind(b.as_slice()),
                Value::Json(j) => query.bind(j),
            };
        }
        query
    }

    fn columns_of_row(row: &sqlx::postgres::PgRow) -> Vec<ColumnInfo> {
        row.columns()
            .iter()
            .map(|c| ColumnInfo {
                name: c.name().to_string(),
                data_type: c.type_info().to_string(),
                nullable: true,
            })
            .collect()
    }

    async fn describe_columns<'e, E>(executor: E, sql: &str) -> Vec<ColumnInfo>
    where
        E: Executor<'e, Database = Postgres>,
    {
        match executor.describe(sql).await {
            Ok(desc) => desc
                .columns()
                .iter()
                .enumerate()
                .map(|(i, c)| ColumnInfo {
                    name: c.name().to_string(),
                    data_type: c.type_info().to_string(),
                    nullable: desc.nullable(i).unwrap_or(true),
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    fn decode_rows(rows: &[sqlx::postgres::PgRow]) -> (Vec<ColumnInfo>, Vec<Vec<Option<Value>>>) {
        let columns: Vec<ColumnInfo> = rows.first().map(Self::columns_of_row).unwrap_or_default();

        let result_rows: Vec<Vec<Option<Value>>> = rows
            .iter()
            .map(|row| {
                row.columns()
                    .iter()
                    .enumerate()
                    .map(|(i, col)| {
                        let type_name = col.type_info().to_string().to_uppercase();
                        match type_name.as_str() {
                            "INT8" | "BIGINT" | "BIGSERIAL" => {
                                row.try_get::<i64, _>(i).ok().map(Self::safe_integer)
                            }
                            "INT4" | "INT" | "INTEGER" | "SERIAL" => row
                                .try_get::<i32, _>(i)
                                .ok()
                                .map(|v| Value::Integer(v as i64))
                                .or_else(|| row.try_get::<i64, _>(i).ok().map(Self::safe_integer)),
                            "INT2" | "SMALLINT" | "SMALLSERIAL" => row
                                .try_get::<i16, _>(i)
                                .ok()
                                .map(|v| Value::Integer(v as i64))
                                .or_else(|| {
                                    row.try_get::<i32, _>(i)
                                        .ok()
                                        .map(|v| Value::Integer(v as i64))
                                }),
                            "FLOAT4" | "REAL" => row
                                .try_get::<f32, _>(i)
                                .ok()
                                .map(|v| Value::Float(v as f64))
                                .or_else(|| row.try_get::<f64, _>(i).ok().map(Value::Float)),
                            "FLOAT8" | "DOUBLE PRECISION" => {
                                row.try_get::<f64, _>(i).ok().map(Value::Float)
                            }
                            "NUMERIC" | "DECIMAL" => row
                                .try_get::<rust_decimal::Decimal, _>(i)
                                .ok()
                                .map(|d| {
                                    if d.scale() == 0 {
                                        if let Some(n) = d.to_i64() {
                                            return Self::safe_integer(n);
                                        }
                                    }
                                    d.to_f64()
                                        .map(Value::Float)
                                        .unwrap_or_else(|| Value::String(d.to_string()))
                                })
                                .or_else(|| row.try_get::<f64, _>(i).ok().map(Value::Float))
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            "BOOL" | "BOOLEAN" => row.try_get::<bool, _>(i).ok().map(Value::Bool),
                            "DATE" => row
                                .try_get::<chrono::NaiveDate, _>(i)
                                .ok()
                                .map(|d| Value::String(d.format("%Y-%m-%d").to_string()))
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            "TIME" | "TIME WITHOUT TIME ZONE" => row
                                .try_get::<chrono::NaiveTime, _>(i)
                                .ok()
                                .map(|t| Value::String(t.format("%H:%M:%S").to_string()))
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            "TIMETZ" | "TIME WITH TIME ZONE" => {
                                row.try_get::<String, _>(i).ok().map(Value::String)
                            }
                            "TIMESTAMP" | "TIMESTAMP WITHOUT TIME ZONE" => row
                                .try_get::<chrono::NaiveDateTime, _>(i)
                                .ok()
                                .map(|dt| Value::String(dt.format("%Y-%m-%d %H:%M:%S").to_string()))
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            "TIMESTAMPTZ" | "TIMESTAMP WITH TIME ZONE" => row
                                .try_get::<chrono::DateTime<chrono::Utc>, _>(i)
                                .ok()
                                .map(|dt| Value::String(dt.to_rfc3339()))
                                .or_else(|| {
                                    row.try_get::<chrono::NaiveDateTime, _>(i).ok().map(|dt| {
                                        Value::String(dt.format("%Y-%m-%d %H:%M:%S").to_string())
                                    })
                                })
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            "UUID" => row
                                .try_get::<uuid::Uuid, _>(i)
                                .ok()
                                .map(|u| Value::String(u.to_string()))
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            "JSON" | "JSONB" => row
                                .try_get::<serde_json::Value, _>(i)
                                .ok()
                                .map(Value::Json)
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            s @ ("INET" | "CIDR" | "MACADDR" | "MACADDR8") => {
                                // These PG network types need ipnetwork/ipnet feature for native decode.
                                // Fallback: read text representation via cast in a sub-query,
                                // or simply try the generic fallback chain.
                                row.try_get::<String, _>(i)
                                    .ok()
                                    .map(Value::String)
                                    .or_else(|| {
                                        // sqlx might not implement Decode<String> for these types,
                                        // so we return a placeholder.
                                        Some(Value::String(format!("<{}>", s.to_lowercase())))
                                    })
                            }
                            "INTERVAL" => row
                                .try_get::<sqlx::postgres::types::PgInterval, _>(i)
                                .ok()
                                .map(|iv| {
                                    let mut parts = Vec::new();
                                    if iv.months != 0 {
                                        let years = iv.months / 12;
                                        let months = iv.months % 12;
                                        if years != 0 {
                                            parts.push(format!("{} years", years));
                                        }
                                        if months != 0 {
                                            parts.push(format!("{} mons", months));
                                        }
                                    }
                                    if iv.days != 0 {
                                        parts.push(format!("{} days", iv.days));
                                    }
                                    if iv.microseconds != 0 {
                                        let total_secs = iv.microseconds / 1_000_000;
                                        let h = total_secs / 3600;
                                        let m = (total_secs % 3600) / 60;
                                        let s = total_secs % 60;
                                        parts.push(format!("{:02}:{:02}:{:02}", h, m, s));
                                    }
                                    Value::String(if parts.is_empty() {
                                        "00:00:00".into()
                                    } else {
                                        parts.join(" ")
                                    })
                                })
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            "BYTEA" => row.try_get::<Vec<u8>, _>(i).ok().map(|bytes| {
                                let hex: String =
                                    bytes.iter().map(|b| format!("{:02x}", b)).collect();
                                Value::String(format!("\\x{}", hex))
                            }),
                            _ => row
                                .try_get::<String, _>(i)
                                .ok()
                                .map(Value::String)
                                .or_else(|| row.try_get::<i64, _>(i).ok().map(Self::safe_integer))
                                .or_else(|| row.try_get::<f64, _>(i).ok().map(Value::Float))
                                .or_else(|| row.try_get::<bool, _>(i).ok().map(Value::Bool)),
                        }
                    })
                    .collect()
            })
            .collect();

        (columns, result_rows)
    }

    fn extract_pg_plan_metrics(plan_json: &serde_json::Value) -> (Option<f64>, Option<i64>) {
        let plan = plan_json
            .as_array()
            .and_then(|rows| rows.first())
            .and_then(|row| row.get("Plan"));
        let Some(plan) = plan else {
            return (None, None);
        };
        let total_cost = plan.get("Total Cost").and_then(|v| v.as_f64());
        let estimated_rows = plan.get("Plan Rows").and_then(|v| v.as_i64());
        (total_cost, estimated_rows)
    }

    async fn stream_one_statement<'e, E>(
        executor: E,
        stmt: &str,
        limit: Option<u32>,
        index: usize,
        on_event: &QueryStreamCallback,
    ) -> Result<(), DriverError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        use futures_util::TryStreamExt;
        let (effective_sql, applied_limit) = apply_select_limit(stmt, limit);
        let stmt_start = Instant::now();
        if is_pg_result_query(&effective_sql) {
            let mut stream = sqlx::query(effective_sql.as_str()).fetch(executor);
            let mut batcher =
                QueryRowBatcher::new(Arc::clone(on_event), index, stmt.to_string(), applied_limit);
            while let Some(row) = stream
                .try_next()
                .await
                .map_err(|e| DriverError::QueryFailed(format!("[{stmt}] {e}")))?
            {
                if !batcher.started() {
                    batcher.start(Self::columns_of_row(&row));
                }
                let (_, mut decoded) = Self::decode_rows(std::slice::from_ref(&row));
                if !batcher.push(decoded.pop().unwrap_or_default()) {
                    break;
                }
            }
            batcher.finish(stmt_start.elapsed().as_millis() as u64, None);
        } else {
            let result = sqlx::query(effective_sql.as_str())
                .execute(executor)
                .await
                .map_err(|e| DriverError::QueryFailed(format!("[{stmt}] {e}")))?;
            emit_execute_statement(
                on_event,
                index,
                stmt.to_string(),
                result.rows_affected(),
                stmt_start.elapsed().as_millis() as u64,
            );
        }
        Ok(())
    }
}

fn build_pg_options(
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
    if let Some(password) = &config.password {
        opts = opts.password(password);
    }

    opts = opts.log_statements(tracing::log::LevelFilter::Trace);
    Ok(opts)
}

#[async_trait]
impl DatabaseDriver for PostgresDriver {
    fn driver_type(&self) -> DatabaseType {
        "postgresql".to_string()
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let opts = build_pg_options(config)?;
        let timeout = Duration::from_secs(config.connection_timeout as u64);

        let pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(timeout)
            .connect_with(opts)
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;

        let row = sqlx::query("SELECT version()")
            .fetch_one(&pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let version: String = row.try_get(0).unwrap_or_default();
        pool.close().await;

        Ok(ServerInfo {
            server_version: version,
            server_type: "PostgreSQL".to_string(),
        })
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let opts = build_pg_options(config)?;
        let timeout = Duration::from_secs(config.connection_timeout as u64);
        let resolved_db = Self::resolve_connect_database(config).to_string();

        let max = config.effective_max_pool_size();
        let min = 2u32.min(max);
        let pool = Self::open_pool(opts, timeout, max, min).await?;

        {
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
        self.pools.write().await.insert(pool_id.clone(), pool);

        Ok(ConnectionHandle {
            id: connection_id,
            pool_id,
        })
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        if let Some(mut conn) = self.transactions.lock().await.remove(&handle.id) {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        }
        self.active_databases.write().await.remove(&handle.pool_id);
        self.connect_configs.write().await.remove(&handle.pool_id);
        if let Some(pool) = self.pools.write().await.remove(&handle.pool_id) {
            pool.close().await;
        }
        Ok(())
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
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

    async fn get_tables(
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

    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let cols = sqlx::query(
            r#"
                    SELECT column_name, data_type, is_nullable, column_default,
                           col_description((quote_ident(table_schema)||'.'||quote_ident(table_name))::regclass, ordinal_position) as comment
                    FROM information_schema.columns
                    WHERE table_name = $1
                    ORDER BY ordinal_position
                    "#,
        )
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        // `quote_ident($1)::regclass` fails when the table is not on search_path.
        // Columns must still load so SQL autocomplete can list fields.
        let pk_rows = sqlx::query(
            r#"
                    SELECT a.attname
                    FROM pg_index i
                    JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
                    JOIN pg_class c ON c.oid = i.indrelid
                    JOIN pg_namespace n ON n.oid = c.relnamespace
                    WHERE c.relname = $1 AND i.indisprimary
                      AND n.nspname = ANY (current_schemas(true))
                    "#,
        )
        .bind(table)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let pk_names: Vec<String> = pk_rows.iter().map(|r| r.get::<String, _>(0)).collect();
        let columns: Vec<ColumnSchema> = cols
            .iter()
            .map(|r| {
                let name: String = r.get("column_name");
                let nullable: String = r.get("is_nullable");
                ColumnSchema {
                    is_primary_key: pk_names.contains(&name),
                    name,
                    data_type: r.get("data_type"),
                    nullable: nullable == "YES",
                    default_value: r.get("column_default"),
                    comment: r.get("comment"),
                    is_auto_increment: false,
                }
            })
            .collect();

        Ok((columns, pk_names))
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let cols = sqlx::query(
            r#"
            SELECT column_name, data_type, is_nullable, column_default,
                   col_description((quote_ident(table_schema)||'.'||quote_ident(table_name))::regclass, ordinal_position) as comment
            FROM information_schema.columns
            WHERE table_name = $1
            ORDER BY ordinal_position
            "#,
        )
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let pk_rows = sqlx::query(
            r#"
            SELECT a.attname
            FROM pg_index i
            JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
            WHERE i.indrelid = quote_ident($1)::regclass AND i.indisprimary
            "#,
        )
        .bind(table)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let pk_names: Vec<String> = pk_rows.iter().map(|r| r.get::<String, _>(0)).collect();

        let columns: Vec<ColumnSchema> = cols
            .iter()
            .map(|r| {
                let name: String = r.get("column_name");
                let nullable: String = r.get("is_nullable");
                ColumnSchema {
                    is_primary_key: pk_names.contains(&name),
                    name,
                    data_type: r.get("data_type"),
                    nullable: nullable == "YES",
                    default_value: r.get("column_default"),
                    comment: r.get("comment"),
                    is_auto_increment: false,
                }
            })
            .collect();

        // ── indexes ──
        let idx_rows = sqlx::query(
            r#"
            SELECT i.relname::text                                AS index_name,
                   array_agg(a.attname::text ORDER BY k.n)        AS columns,
                   ix.indisunique                                  AS is_unique,
                   ix.indisprimary                                 AS is_primary,
                   am.amname::text                                 AS index_type
            FROM pg_index ix
            JOIN pg_class i  ON i.oid  = ix.indexrelid
            JOIN pg_class t  ON t.oid  = ix.indrelid
            JOIN pg_am   am ON am.oid  = i.relam
            JOIN LATERAL unnest(ix.indkey) WITH ORDINALITY AS k(attnum, n) ON true
            JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = k.attnum
            WHERE ix.indrelid = quote_ident($1)::regclass
            GROUP BY i.relname, ix.indisunique, ix.indisprimary, am.amname
            ORDER BY ix.indisprimary DESC, i.relname
            "#,
        )
        .bind(table)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let indexes: Vec<IndexInfo> = idx_rows
            .iter()
            .map(|r| IndexInfo {
                name: r.get("index_name"),
                columns: r.get::<Vec<String>, _>("columns"),
                is_unique: r.get("is_unique"),
                is_primary: r.get("is_primary"),
                index_type: r.get("index_type"),
            })
            .collect();

        // ── foreign keys ──
        let fk_rows = sqlx::query(
            r#"
            SELECT
                tc.constraint_name::text                                             AS fk_name,
                array_agg(kcu.column_name::text ORDER BY kcu.ordinal_position)       AS columns,
                ccu.table_name::text                                                 AS ref_table,
                array_agg(ccu.column_name::text ORDER BY kcu.ordinal_position)       AS ref_columns,
                rc.update_rule::text,
                rc.delete_rule::text
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
              ON kcu.constraint_name = tc.constraint_name
             AND kcu.table_schema   = tc.table_schema
            JOIN information_schema.constraint_column_usage ccu
              ON ccu.constraint_name = tc.constraint_name
             AND ccu.table_schema   = tc.table_schema
            JOIN information_schema.referential_constraints rc
              ON rc.constraint_name = tc.constraint_name
             AND rc.constraint_schema = tc.table_schema
            WHERE tc.constraint_type = 'FOREIGN KEY'
              AND tc.table_name = $1
            GROUP BY tc.constraint_name, ccu.table_name, rc.update_rule, rc.delete_rule
            ORDER BY tc.constraint_name
            "#,
        )
        .bind(table)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let foreign_keys: Vec<ForeignKeyInfo> = fk_rows
            .iter()
            .map(|r| ForeignKeyInfo {
                name: r.get("fk_name"),
                columns: r.get::<Vec<String>, _>("columns"),
                referenced_table: r.get("ref_table"),
                referenced_columns: r.get::<Vec<String>, _>("ref_columns"),
                on_update: r.get("update_rule"),
                on_delete: r.get("delete_rule"),
            })
            .collect();

        Ok(TableSchema {
            table_name: table.to_string(),
            columns,
            primary_keys: pk_names,
            indexes,
            foreign_keys,
        })
    }

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        {
            let mut txs = self.transactions.lock().await;
            if let Some(conn) = txs.get_mut(&handle.id) {
                let start = Instant::now();
                let rows = sqlx::query(sql)
                    .fetch_all(&mut **conn)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                let elapsed = start.elapsed().as_millis() as u64;
                let (mut columns, result_rows) = Self::decode_rows(&rows);
                if columns.is_empty() && result_rows.is_empty() {
                    columns = Self::describe_columns(&mut **conn, sql).await;
                }
                let row_count = result_rows.len() as u64;
                return Ok(QueryResult {
                    columns,
                    rows: result_rows,
                    rows_affected: Some(row_count),
                    execution_time_ms: elapsed,
                });
            }
        }

        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let start = Instant::now();
        let rows = sqlx::query(sql)
            .fetch_all(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let elapsed = start.elapsed().as_millis() as u64;

        let (mut columns, result_rows) = Self::decode_rows(&rows);
        if columns.is_empty() && result_rows.is_empty() {
            let mut conn = pool
                .acquire()
                .await
                .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
            columns = Self::describe_columns(&mut *conn, sql).await;
        }
        let row_count = result_rows.len() as u64;

        Ok(QueryResult {
            columns,
            rows: result_rows,
            rows_affected: Some(row_count),
            execution_time_ms: elapsed,
        })
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        let statements = sql_dump::split_sql_statements(sql);
        if statements.is_empty() {
            return Ok(MultiQueryResult {
                results: Vec::new(),
                total_time_ms: 0,
            });
        }

        let total_start = Instant::now();
        let mut results = Vec::with_capacity(statements.len());

        let mut txs = self.transactions.lock().await;
        if let Some(conn) = txs.get_mut(&handle.id) {
            for stmt in &statements {
                let (effective_sql, applied_limit) = apply_select_limit(stmt, limit);
                let trimmed_upper = effective_sql.trim().to_ascii_uppercase();
                let is_query = trimmed_upper.starts_with("SELECT")
                    || trimmed_upper.starts_with("WITH")
                    || trimmed_upper.starts_with("SHOW")
                    || trimmed_upper.starts_with("EXPLAIN");

                let stmt_start = Instant::now();

                if is_query {
                    let rows = sqlx::query(effective_sql.as_str())
                        .fetch_all(&mut **conn)
                        .await
                        .map_err(|e| DriverError::QueryFailed(format!("[{}] {}", stmt, e)))?;
                    let stmt_ms = stmt_start.elapsed().as_millis() as u64;

                    let (columns, mut result_rows) = Self::decode_rows(&rows);
                    let truncated = if let Some(lim) = applied_limit {
                        let fetched = result_rows.len() as u32;
                        if fetched > lim {
                            result_rows.truncate(lim as usize);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    let row_count = result_rows.len() as u64;

                    results.push(StatementResult {
                        sql: stmt.clone(),
                        columns,
                        rows: result_rows,
                        rows_affected: Some(row_count),
                        execution_time_ms: stmt_ms,
                        truncated,
                    });
                } else {
                    let result = sqlx::query(effective_sql.as_str())
                        .execute(&mut **conn)
                        .await
                        .map_err(|e| DriverError::QueryFailed(format!("[{}] {}", stmt, e)))?;
                    let stmt_ms = stmt_start.elapsed().as_millis() as u64;

                    results.push(StatementResult {
                        sql: stmt.clone(),
                        columns: Vec::new(),
                        rows: Vec::new(),
                        rows_affected: Some(result.rows_affected()),
                        execution_time_ms: stmt_ms,
                        truncated: false,
                    });
                }
            }

            return Ok(MultiQueryResult {
                results,
                total_time_ms: total_start.elapsed().as_millis() as u64,
            });
        }
        drop(txs);

        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        for stmt in &statements {
            let (effective_sql, applied_limit) = apply_select_limit(stmt, limit);
            let trimmed_upper = effective_sql.trim().to_ascii_uppercase();
            let is_query = trimmed_upper.starts_with("SELECT")
                || trimmed_upper.starts_with("WITH")
                || trimmed_upper.starts_with("SHOW")
                || trimmed_upper.starts_with("EXPLAIN");

            let stmt_start = Instant::now();

            if is_query {
                let rows = sqlx::query(effective_sql.as_str())
                    .fetch_all(pool)
                    .await
                    .map_err(|e| DriverError::QueryFailed(format!("[{}] {}", stmt, e)))?;
                let stmt_ms = stmt_start.elapsed().as_millis() as u64;

                let (columns, mut result_rows) = Self::decode_rows(&rows);
                let truncated = if let Some(lim) = applied_limit {
                    let fetched = result_rows.len() as u32;
                    if fetched > lim {
                        result_rows.truncate(lim as usize);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                let row_count = result_rows.len() as u64;

                results.push(StatementResult {
                    sql: stmt.clone(),
                    columns,
                    rows: result_rows,
                    rows_affected: Some(row_count),
                    execution_time_ms: stmt_ms,
                    truncated,
                });
            } else {
                let result = sqlx::query(effective_sql.as_str())
                    .execute(pool)
                    .await
                    .map_err(|e| DriverError::QueryFailed(format!("[{}] {}", stmt, e)))?;
                let stmt_ms = stmt_start.elapsed().as_millis() as u64;

                results.push(StatementResult {
                    sql: stmt.clone(),
                    columns: Vec::new(),
                    rows: Vec::new(),
                    rows_affected: Some(result.rows_affected()),
                    execution_time_ms: stmt_ms,
                    truncated: false,
                });
            }
        }

        Ok(MultiQueryResult {
            results,
            total_time_ms: total_start.elapsed().as_millis() as u64,
        })
    }

    async fn query_stream(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        let statements = sql_dump::split_sql_statements(sql);
        if statements.is_empty() {
            on_event(QueryStreamEvent::Done { total_time_ms: 0 });
            return Ok(());
        }

        let total_start = Instant::now();
        {
            let mut txs = self.transactions.lock().await;
            if let Some(conn) = txs.get_mut(&handle.id) {
                for (index, stmt) in statements.iter().enumerate() {
                    Self::stream_one_statement(&mut **conn, stmt, limit, index, &on_event).await?;
                }
                on_event(QueryStreamEvent::Done {
                    total_time_ms: total_start.elapsed().as_millis() as u64,
                });
                return Ok(());
            }
        }

        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        for (index, stmt) in statements.iter().enumerate() {
            Self::stream_one_statement(pool, stmt, limit, index, &on_event).await?;
        }
        on_event(QueryStreamEvent::Done {
            total_time_ms: total_start.elapsed().as_millis() as u64,
        });
        Ok(())
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        {
            let mut txs = self.transactions.lock().await;
            if let Some(conn) = txs.get_mut(&handle.id) {
                let start = Instant::now();
                let rows = Self::bind_values(sqlx::query(sql), params)
                    .fetch_all(&mut **conn)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                let elapsed = start.elapsed().as_millis() as u64;
                let (columns, result_rows) = Self::decode_rows(&rows);
                let row_count = result_rows.len() as u64;
                return Ok(QueryResult {
                    columns,
                    rows: result_rows,
                    rows_affected: Some(row_count),
                    execution_time_ms: elapsed,
                });
            }
        }

        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let start = Instant::now();
        let rows = Self::bind_values(sqlx::query(sql), params)
            .fetch_all(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let elapsed = start.elapsed().as_millis() as u64;

        let (columns, result_rows) = Self::decode_rows(&rows);
        let row_count = result_rows.len() as u64;

        Ok(QueryResult {
            columns,
            rows: result_rows,
            rows_affected: Some(row_count),
            execution_time_ms: elapsed,
        })
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        {
            let mut txs = self.transactions.lock().await;
            if let Some(conn) = txs.get_mut(&handle.id) {
                let result = sqlx::query(sql)
                    .execute(&mut **conn)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                return Ok(result.rows_affected());
            }
        }

        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let result = sqlx::query(sql)
            .execute(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        Ok(result.rows_affected())
    }

    async fn begin_transaction(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        let mut txs = self.transactions.lock().await;
        if txs.contains_key(&handle.id) {
            return Err(DriverError::TransactionError(
                "A transaction is already open on this connection".into(),
            ));
        }

        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        // Active DB is encoded in the pool itself (use_database swaps pools); no per-conn USE.
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        drop(pools);

        sqlx::query("BEGIN")
            .execute(&mut *conn)
            .await
            .map_err(|e| DriverError::TransactionError(e.to_string()))?;

        txs.insert(handle.id.clone(), conn);
        Ok(TransactionHandle {
            id: format!("pg_tx_{}", uuid::Uuid::new_v4()),
            connection_id: handle.id.clone(),
        })
    }

    async fn commit(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        let mut conn = self
            .transactions
            .lock()
            .await
            .remove(&tx.connection_id)
            .ok_or_else(|| {
                DriverError::TransactionError("Transaction not found or already ended".into())
            })?;

        sqlx::query("COMMIT")
            .execute(&mut *conn)
            .await
            .map_err(|e| DriverError::TransactionError(e.to_string()))?;
        Ok(())
    }

    async fn rollback(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        let mut conn = self
            .transactions
            .lock()
            .await
            .remove(&tx.connection_id)
            .ok_or_else(|| {
                DriverError::TransactionError("Transaction not found or already ended".into())
            })?;

        sqlx::query("ROLLBACK")
            .execute(&mut *conn)
            .await
            .map_err(|e| DriverError::TransactionError(e.to_string()))?;
        Ok(())
    }

    async fn explain(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let json_sql = format!("EXPLAIN (FORMAT JSON) {sql}");
        let json_rows = sqlx::query(&json_sql)
            .fetch_all(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let plan_json = json_rows.first().and_then(|row| {
            row.try_get::<serde_json::Value, _>(0).ok().or_else(|| {
                row.try_get::<String, _>(0)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
            })
        });

        let text_sql = format!("EXPLAIN (FORMAT TEXT) {sql}");
        let text_rows = sqlx::query(&text_sql)
            .fetch_all(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let plan_text: String = text_rows
            .iter()
            .map(|r| r.get::<String, _>(0))
            .collect::<Vec<_>>()
            .join("\n");

        let (total_cost, estimated_rows) = plan_json
            .as_ref()
            .map(Self::extract_pg_plan_metrics)
            .unwrap_or((None, None));

        Ok(ExplainResult {
            plan_text,
            plan_json,
            total_cost,
            estimated_rows,
        })
    }

    async fn cancel_query(&self, handle: &ConnectionHandle) -> Result<(), DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let rows = sqlx::query(
            "SELECT pg_cancel_backend(pid) \
             FROM pg_stat_activity \
             WHERE pid != pg_backend_pid() \
               AND state = 'active' \
               AND datname = current_database()",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        tracing::info!(cancelled = rows.len(), "pg: cancelled active queries");
        Ok(())
    }

    async fn use_database(
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

        let old = {
            let mut pools = self.pools.write().await;
            pools.insert(handle.pool_id.clone(), new_pool)
        };
        self.active_databases
            .write()
            .await
            .insert(handle.pool_id.clone(), trimmed);

        if let Some(old) = old {
            old.close().await;
        }
        Ok(())
    }

    async fn get_server_info(&self, handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
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

    async fn dump_table_ddl(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<String, DriverError> {
        let catalog_result = {
            let pools = self.pools.read().await;
            let pool = Self::get_pool(&pools, handle)?;
            fetch_pg_table_ddl_from_catalog(pool, table, |n| self.quote_ident(n)).await
        };
        match catalog_result {
            Ok(ddl) => Ok(ddl),
            Err(_) => sql_dump::dump_table_ddl_from_schema(self, handle, table).await,
        }
    }

    async fn dump_view_ddl(
        &self,
        handle: &ConnectionHandle,
        view: &str,
    ) -> Result<String, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let row = sqlx::query("SELECT pg_get_viewdef($1::regclass, true) AS def")
            .bind(view)
            .fetch_one(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let def: String = row.try_get("def").unwrap_or_default();
        if def.trim().is_empty() {
            return Err(DriverError::QueryFailed(format!(
                "View definition not found: {view}"
            )));
        }
        Ok(format!(
            "CREATE OR REPLACE VIEW {} AS\n{};\n",
            self.quote_ident(view),
            def.trim().trim_end_matches(';')
        ))
    }

    async fn dump_routines(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<String, DriverError> {
        let result = self
            .query(
                handle,
                "SELECT n.nspname AS schema, p.proname AS name, \
                 pg_get_functiondef(p.oid) AS ddl \
                 FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
                 WHERE n.nspname NOT IN ('pg_catalog','information_schema') \
                   AND p.prokind IN ('f', 'p') \
                 ORDER BY 1, 2",
            )
            .await?;
        Ok(collect_named_ddl_column(&result, "ddl", "ROUTINE"))
    }

    async fn dump_triggers(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<String, DriverError> {
        let result = self
            .query(
                handle,
                "SELECT n.nspname AS schema, t.tgname AS name, \
                 pg_get_triggerdef(t.oid) AS ddl \
                 FROM pg_trigger t \
                 JOIN pg_class c ON c.oid = t.tgrelid \
                 JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE NOT t.tgisinternal \
                   AND n.nspname NOT IN ('pg_catalog','information_schema') \
                 ORDER BY 1, 2",
            )
            .await?;
        Ok(collect_named_ddl_column(&result, "ddl", "TRIGGER"))
    }

    async fn dump_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
    ) -> Result<String, DriverError> {
        self.dump_database_with_progress(handle, database, opts, &mut |_| {})
            .await
    }

    fn new_sql_scanner(&self) -> sql_dump::SqlStatementScanner {
        sql_dump::SqlStatementScanner::new().recognize_delimiter_commands(false)
    }

    async fn dump_database_with_progress(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<String, DriverError> {
        let snapshot = match self.begin_transaction(handle).await {
            Ok(tx) => {
                let _ = self
                    .execute(
                        handle,
                        "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY",
                    )
                    .await;
                Some(tx)
            }
            Err(_) => None,
        };
        let result = async {
            let mut out = String::new();
            if opts.create_database {
                // No `\connect` — restore runs against the existing session.
                out.push_str(&format!(
                    "CREATE DATABASE {};\n",
                    self.quote_ident(database)
                ));
            }
            out.push_str(
                &sql_dump::dump_sql_database_with_progress(
                    self,
                    handle,
                    database,
                    opts,
                    on_progress,
                )
                .await?,
            );
            Ok(out)
        }
        .await;
        if let Some(tx) = snapshot {
            if result.is_ok() {
                let _ = self.commit(tx).await;
            } else {
                let _ = self.rollback(tx).await;
            }
        }
        result
    }

    async fn structure_capabilities(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<StructureCapabilities, DriverError> {
        let info = self.get_server_info(handle).await?;
        Ok(caps_for_version(&info.server_version))
    }

    async fn plan_structure_changes(
        &self,
        handle: &ConnectionHandle,
        request: &StructureChangeRequest,
    ) -> Result<StructureChangePlan, DriverError> {
        let caps = self.structure_capabilities(handle).await?;
        plan_structure_changes_with_caps(&caps, request)
    }
}

fn is_pg_result_query(sql: &str) -> bool {
    let upper = sql.trim().to_ascii_uppercase();
    upper.starts_with("SELECT")
        || upper.starts_with("WITH")
        || upper.starts_with("SHOW")
        || upper.starts_with("EXPLAIN")
}

/// If the statement is a SELECT without an existing LIMIT clause, returns a
/// modified SQL with `LIMIT limit+1` appended (the extra row lets us detect
/// truncation).  If the statement already has a LIMIT, the SQL is unchanged
/// but the cap is still returned so the caller can truncate over-limit results.
fn collect_named_ddl_column(result: &QueryResult, col_name: &str, kind_label: &str) -> String {
    let ddl_idx = result
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(col_name));
    let name_idx = result
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case("name"));
    let Some(ddl_idx) = ddl_idx else {
        return String::new();
    };
    let mut out = String::new();
    for row in &result.rows {
        let Some(Value::String(ddl)) = row.get(ddl_idx).and_then(|v| v.as_ref()) else {
            continue;
        };
        if ddl.trim().is_empty() {
            continue;
        }
        if let Some(name_idx) = name_idx {
            if let Some(Value::String(name)) = row.get(name_idx).and_then(|v| v.as_ref()) {
                out.push_str(&format!("-- {kind_label}: {name}\n"));
            }
        }
        let trimmed = ddl.trim_end();
        out.push_str(trimmed);
        if !trimmed.ends_with(';') {
            out.push(';');
        }
        out.push_str("\n\n");
    }
    out
}

fn apply_select_limit(stmt: &str, limit: Option<u32>) -> (String, Option<u32>) {
    let Some(lim) = limit else {
        return (stmt.to_string(), None);
    };

    let trimmed = stmt.trim();
    let upper = trimmed.to_ascii_uppercase();
    let is_select = upper.starts_with("SELECT") || upper.starts_with("WITH");
    if !is_select {
        return (stmt.to_string(), None);
    }

    if has_top_level_limit(trimmed) {
        return (stmt.to_string(), Some(lim));
    }

    let effective = format!("{} LIMIT {}", trimmed, lim + 1);
    (effective, Some(lim))
}

/// Rough heuristic: scan the SQL outside of string literals, dollar-quotes,
/// and parenthesised sub-expressions for the keyword `LIMIT`.
fn has_top_level_limit(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    let mut depth: i32 = 0; // parenthesis nesting

    while i < len {
        match bytes[i] {
            b'\'' => {
                i += 1;
                while i < len {
                    if bytes[i] == b'\'' {
                        i += 1;
                        if i < len && bytes[i] == b'\'' {
                            i += 1; // escaped quote
                        } else {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            b'"' => {
                i += 1;
                while i < len && bytes[i] != b'"' {
                    i += 1;
                }
                if i < len {
                    i += 1;
                }
            }
            b'$' => {
                if let Some(tag_end) = sql_dump::find_dollar_tag(bytes, i) {
                    let tag = &sql[i..tag_end];
                    i = tag_end;
                    loop {
                        if i >= len {
                            break;
                        }
                        if bytes[i] == b'$' {
                            if sql[i..].starts_with(tag) {
                                i += tag.len();
                                break;
                            }
                        }
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            b'-' if i + 1 < len && bytes[i + 1] == b'-' => {
                i += 2;
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                i += 2;
                let mut cd = 1i32;
                while i + 1 < len && cd > 0 {
                    if bytes[i] == b'/' && bytes[i + 1] == b'*' {
                        cd += 1;
                        i += 2;
                    } else if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        cd -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
            }
            b'L' | b'l' if depth == 0 => {
                if i + 5 <= len
                    && sql[i..i + 5].eq_ignore_ascii_case("LIMIT")
                    && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric())
                    && (i + 5 >= len || !bytes[i + 5].is_ascii_alphanumeric())
                {
                    return true;
                }
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    false
}

/// One column line for CREATE TABLE assembly (catalog-backed DDL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PgColumnDdl {
    pub name: String,
    pub data_type: String,
    pub not_null: bool,
    pub default_expr: Option<String>,
}

/// Build `CREATE TABLE schema.table (...)` from catalog-derived column metadata.
pub(crate) fn build_pg_create_table_ddl(
    qualified_name: &str,
    columns: &[PgColumnDdl],
    pk_columns: &[String],
    quote_ident: &dyn Fn(&str) -> String,
) -> String {
    let mut parts: Vec<String> = columns
        .iter()
        .map(|c| {
            let mut line = format!("  {} {}", quote_ident(&c.name), c.data_type);
            if c.not_null {
                line.push_str(" NOT NULL");
            }
            if let Some(ref def) = c.default_expr {
                if !def.is_empty() {
                    line.push_str(&format!(" DEFAULT {def}"));
                }
            }
            line
        })
        .collect();

    if !pk_columns.is_empty() {
        let pk_list: Vec<String> = pk_columns.iter().map(|n| quote_ident(n)).collect();
        parts.push(format!("  PRIMARY KEY ({})", pk_list.join(", ")));
    }

    format!(
        "CREATE TABLE {qualified_name} (\n{}\n);\n",
        parts.join(",\n")
    )
}

async fn fetch_pg_table_ddl_from_catalog(
    pool: &PgPool,
    table: &str,
    quote_ident: impl Fn(&str) -> String,
) -> Result<String, DriverError> {
    let col_rows = sqlx::query(
        r#"
        SELECT
          quote_ident(n.nspname) || '.' || quote_ident(c.relname) AS qualified_name,
          a.attname,
          format_type(a.atttypid, a.atttypmod) AS col_type,
          a.attnotnull AS not_null,
          pg_get_expr(d.adbin, d.adrelid) AS col_default
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_attribute a ON a.attrelid = c.oid
        LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
        WHERE c.oid = $1::regclass
          AND a.attnum > 0
          AND NOT a.attisdropped
        ORDER BY a.attnum
        "#,
    )
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    if col_rows.is_empty() {
        return Err(DriverError::QueryFailed(format!(
            "Table not found or has no columns: {table}"
        )));
    }

    let qualified_name: String = col_rows[0].get("qualified_name");
    let columns: Vec<PgColumnDdl> = col_rows
        .iter()
        .map(|r| PgColumnDdl {
            name: r.get("attname"),
            data_type: r.get("col_type"),
            not_null: r.get("not_null"),
            default_expr: r.try_get("col_default").ok(),
        })
        .collect();

    let pk_rows = sqlx::query(
        r#"
        SELECT a.attname
        FROM pg_constraint con
        JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = ANY(con.conkey)
        WHERE con.contype = 'p'
          AND con.conrelid = $1::regclass
        ORDER BY array_position(con.conkey, a.attnum)
        "#,
    )
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    let pk_columns: Vec<String> = pk_rows.iter().map(|r| r.get("attname")).collect();

    Ok(build_pg_create_table_ddl(
        &qualified_name,
        &columns,
        &pk_columns,
        &quote_ident,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::DatabaseDriver;

    #[test]
    fn validate_database_name_trims_and_accepts() {
        assert_eq!(
            PostgresDriver::validate_database_name("  mydb  ").unwrap(),
            "mydb"
        );
        assert_eq!(
            PostgresDriver::validate_database_name("postgres").unwrap(),
            "postgres"
        );
    }

    #[test]
    fn validate_database_name_rejects_empty_or_invalid() {
        assert!(matches!(
            PostgresDriver::validate_database_name(""),
            Err(DriverError::InvalidConfig(_))
        ));
        assert!(matches!(
            PostgresDriver::validate_database_name("   "),
            Err(DriverError::InvalidConfig(_))
        ));
        assert!(matches!(
            PostgresDriver::validate_database_name("bad\0name"),
            Err(DriverError::InvalidConfig(_))
        ));
    }

    #[test]
    fn resolve_connect_database_defaults_to_postgres() {
        let mut cfg = ConnectionConfig {
            id: "id".into(),
            name: "n".into(),
            database_type: "postgresql".into(),
            host: Some("localhost".into()),
            port: Some(5432),
            database: None,
            schema: None,
            username: None,
            password: None,
            ssl_mode: Default::default(),
            connection_timeout: 5,
            max_pool_size: 10,
            ssh_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
        };
        assert_eq!(PostgresDriver::resolve_connect_database(&cfg), "postgres");

        cfg.database = Some("  ".into());
        assert_eq!(PostgresDriver::resolve_connect_database(&cfg), "postgres");

        cfg.database = Some("  app_db  ".into());
        assert_eq!(PostgresDriver::resolve_connect_database(&cfg), "app_db");
    }

    #[tokio::test]
    async fn use_database_is_wired() {
        let driver = PostgresDriver::new();
        let handle = ConnectionHandle {
            id: "conn".into(),
            pool_id: "missing-pool".into(),
        };

        let err = driver.use_database(&handle, "").await.unwrap_err();
        assert!(
            matches!(err, DriverError::InvalidConfig(_)),
            "expected InvalidConfig, got {err:?}"
        );

        let err = driver.use_database(&handle, "app_db").await.unwrap_err();
        assert!(
            matches!(err, DriverError::ConnectionFailed(_)),
            "expected ConnectionFailed, got {err:?}"
        );
    }

    #[tokio::test]
    async fn use_database_noop_when_already_active() {
        let driver = PostgresDriver::new();
        let pool_id = "test-pool".to_string();
        driver
            .active_databases
            .write()
            .await
            .insert(pool_id.clone(), "already".to_string());

        let handle = ConnectionHandle {
            id: "conn".into(),
            pool_id,
        };

        // No pool registered — would fail if reconnect were attempted; no-op must short-circuit.
        driver
            .use_database(&handle, "already")
            .await
            .expect("same database should be a no-op");
        driver
            .use_database(&handle, "  already  ")
            .await
            .expect("trimmed match should be a no-op");
    }

    #[tokio::test]
    async fn begin_transaction_requires_pool() {
        let driver = PostgresDriver::new();
        let handle = ConnectionHandle {
            id: "conn".into(),
            pool_id: "missing-pool".into(),
        };
        let err = driver.begin_transaction(&handle).await.unwrap_err();
        assert!(
            matches!(err, DriverError::ConnectionFailed(_)),
            "expected ConnectionFailed, got {err:?}"
        );
    }

    #[tokio::test]
    async fn commit_and_rollback_without_begin_error() {
        let driver = PostgresDriver::new();
        let tx = TransactionHandle {
            id: "pg_tx_missing".into(),
            connection_id: "conn".into(),
        };
        let err = driver.commit(tx).await.unwrap_err();
        assert!(
            matches!(err, DriverError::TransactionError(_)),
            "expected TransactionError, got {err:?}"
        );

        let tx = TransactionHandle {
            id: "pg_tx_missing".into(),
            connection_id: "conn".into(),
        };
        let err = driver.rollback(tx).await.unwrap_err();
        assert!(
            matches!(err, DriverError::TransactionError(_)),
            "expected TransactionError, got {err:?}"
        );
    }

    /// Postgres / sqlx use 1-based `$N` placeholders (not `?`).
    #[test]
    fn postgres_placeholders_are_dollar_n() {
        assert_eq!(
            (1..=3).map(|i| format!("${i}")).collect::<Vec<_>>(),
            vec!["$1", "$2", "$3"]
        );
    }

    #[test]
    fn bind_values_accepts_all_value_variants() {
        let params = [
            Value::Null,
            Value::Bool(true),
            Value::Integer(42),
            Value::Float(1.5),
            Value::String("hi".into()),
            Value::Timestamp("2024-01-01T00:00:00Z".into()),
            Value::Bytes(vec![1, 2, 3]),
            Value::Json(serde_json::json!({"a": 1})),
        ];
        // Compiles and builds a bound query for every Value variant.
        let _q = PostgresDriver::bind_values(
            sqlx::query("SELECT $1, $2, $3, $4, $5, $6, $7, $8"),
            &params,
        );
    }

    #[tokio::test]
    async fn query_with_params_requires_pool() {
        let driver = PostgresDriver::new();
        let handle = ConnectionHandle {
            id: "conn".into(),
            pool_id: "missing-pool".into(),
        };
        let err = driver
            .query_with_params(&handle, "SELECT $1::int", &[Value::Integer(1)])
            .await
            .unwrap_err();
        assert!(
            matches!(err, DriverError::ConnectionFailed(_)),
            "expected ConnectionFailed, got {err:?}"
        );
    }

    #[test]
    fn build_pg_create_table_ddl_includes_types_not_null_default_and_pk() {
        let columns = vec![
            PgColumnDdl {
                name: "id".into(),
                data_type: "integer".into(),
                not_null: true,
                default_expr: None,
            },
            PgColumnDdl {
                name: "email".into(),
                data_type: "character varying(255)".into(),
                not_null: true,
                default_expr: None,
            },
            PgColumnDdl {
                name: "status".into(),
                data_type: "text".into(),
                not_null: false,
                default_expr: Some("'active'::text".into()),
            },
        ];
        let sql =
            build_pg_create_table_ddl("\"public\".\"users\"", &columns, &["id".into()], &|n| {
                format!("\"{n}\"")
            });
        assert!(sql.starts_with("CREATE TABLE \"public\".\"users\" ("));
        assert!(sql.contains("\"id\" integer NOT NULL"));
        assert!(sql.contains("\"email\" character varying(255) NOT NULL"));
        assert!(sql.contains("\"status\" text DEFAULT 'active'::text"));
        assert!(sql.contains("PRIMARY KEY (\"id\")"));
        assert!(sql.ends_with(");\n"));
    }

    #[test]
    fn build_pg_create_table_ddl_omits_pk_when_empty() {
        let columns = vec![PgColumnDdl {
            name: "x".into(),
            data_type: "text".into(),
            not_null: false,
            default_expr: None,
        }];
        let sql = build_pg_create_table_ddl("t", &columns, &[], &|n| n.to_string());
        assert!(!sql.contains("PRIMARY KEY"));
    }

    #[test]
    fn apply_select_limit_is_independent_of_subquery_limit() {
        assert_eq!(
            apply_select_limit("SELECT * FROM t", None),
            ("SELECT * FROM t".into(), None)
        );
        assert_eq!(
            apply_select_limit("SELECT * FROM t", Some(10)),
            ("SELECT * FROM t LIMIT 11".into(), Some(10))
        );
        assert_eq!(
            apply_select_limit("SELECT * FROM t LIMIT 3", Some(10)),
            ("SELECT * FROM t LIMIT 3".into(), Some(10))
        );
        let (sql, cap) = apply_select_limit("SELECT * FROM (SELECT * FROM t LIMIT 5) s", Some(10));
        assert!(sql.ends_with(" LIMIT 11"), "{sql}");
        assert_eq!(cap, Some(10));
        assert_eq!(
            apply_select_limit("INSERT INTO t VALUES (1)", Some(10)),
            ("INSERT INTO t VALUES (1)".into(), None)
        );
    }
}

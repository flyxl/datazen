//! SQLite driver backed by sqlx SqlitePool.

use crate::structure;
use async_trait::async_trait;
use datazen_driver_api::*;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Column, Row, Sqlite, SqlitePool};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub struct SqliteDriver {
    pools: RwLock<HashMap<String, SqlitePool>>,
}

impl SqliteDriver {
    pub fn new() -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
        }
    }

    fn get_pool<'a>(
        pools: &'a HashMap<String, SqlitePool>,
        handle: &ConnectionHandle,
    ) -> Result<&'a SqlitePool, DriverError> {
        pools
            .get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))
    }

    /// Bind `Value` params into a sqlx SQLite query (`?` placeholders).
    fn bind_values<'q>(
        mut query: sqlx::query::Query<'q, Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
        params: &'q [Value],
    ) -> sqlx::query::Query<'q, Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
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

    fn columns_of_row(row: &sqlx::sqlite::SqliteRow) -> Vec<ColumnInfo> {
        row.columns()
            .iter()
            .map(|c| ColumnInfo {
                name: c.name().to_string(),
                data_type: c.type_info().to_string(),
                nullable: true,
            })
            .collect()
    }

    fn decode_rows(rows: &[sqlx::sqlite::SqliteRow]) -> (Vec<ColumnInfo>, Vec<Vec<Option<Value>>>) {
        let columns: Vec<ColumnInfo> = rows.first().map(Self::columns_of_row).unwrap_or_default();

        let result_rows: Vec<Vec<Option<Value>>> =
            rows.iter()
                .map(|row| {
                    row.columns()
                        .iter()
                        .enumerate()
                        .map(|(i, col)| {
                            let type_name = col.type_info().to_string().to_uppercase();
                            match type_name.as_str() {
                                "INTEGER" | "INT" | "BIGINT" | "SMALLINT" | "TINYINT"
                                | "MEDIUMINT" => row.try_get::<i64, _>(i).ok().map(Value::Integer),
                                "REAL" | "DOUBLE" | "FLOAT" | "NUMERIC" | "DECIMAL" => {
                                    row.try_get::<f64, _>(i).ok().map(Value::Float)
                                }
                                "BOOLEAN" => {
                                    row.try_get::<bool, _>(i).ok().map(Value::Bool).or_else(|| {
                                        row.try_get::<i32, _>(i).ok().map(|v| Value::Bool(v != 0))
                                    })
                                }
                                "BLOB" => row.try_get::<Vec<u8>, _>(i).ok().map(|bytes| {
                                    let hex: String =
                                        bytes.iter().map(|b| format!("{:02x}", b)).collect();
                                    Value::String(format!("\\x{}", hex))
                                }),
                                _ => row
                                    .try_get::<String, _>(i)
                                    .ok()
                                    .map(Value::String)
                                    .or_else(|| row.try_get::<i64, _>(i).ok().map(Value::Integer))
                                    .or_else(|| row.try_get::<f64, _>(i).ok().map(Value::Float))
                                    .or_else(|| row.try_get::<bool, _>(i).ok().map(Value::Bool)),
                            }
                        })
                        .collect()
                })
                .collect();

        (columns, result_rows)
    }
}

fn db_path(config: &ConnectionConfig) -> Result<String, DriverError> {
    config
        .database
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| DriverError::ConnectionFailed("SQLite database path is required".into()))
        .map(|s| s.to_string())
}

#[async_trait]
impl DatabaseDriver for SqliteDriver {
    fn migration_renderer(
        &self,
    ) -> Option<std::sync::Arc<dyn datazen_driver_api::MigrationRenderer>> {
        Some(std::sync::Arc::new(super::SqliteMigrationRenderer))
    }

    fn migration_capabilities(
        &self,
    ) -> Option<std::sync::Arc<dyn datazen_driver_api::MigrationCapabilities>> {
        Some(std::sync::Arc::new(super::SqliteMigrationCapabilities))
    }

    fn type_normalizer(&self) -> Option<std::sync::Arc<dyn datazen_driver_api::TypeNormalizer>> {
        Some(std::sync::Arc::new(super::SqliteTypeNormalizer))
    }
    fn driver_type(&self) -> DatabaseType {
        "sqlite".to_string()
    }

    fn sync_family(&self) -> String {
        "sqlite".into()
    }

    /// F7: qualify unqualified table references with the ATTACH alias
    /// (`"alias"."t"`). A DataZen SQLite connection is a single file
    /// (`main`), so this is a no-op unless the caller targets an explicit
    /// non-`main` database — i.e. a session where that database was
    /// `ATTACH`ed under the same alias; see `sql_target::qualify_sql`.
    /// Parse failures pass SQL through unchanged.
    fn qualify_sql_target(
        &self,
        sql: &str,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Option<String> {
        Some(crate::sql_target::qualify_sql(sql, database, schema))
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let path = db_path(config)?;
        let url = format!("sqlite:{}", path);
        let timeout = Duration::from_secs(config.connection_timeout as u64);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .acquire_timeout(timeout)
            .connect(&url)
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;

        let result = sqlx::query("SELECT sqlite_version()")
            .fetch_one(&pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()));

        pool.close().await;

        let row = result?;
        let version: String = row.try_get(0).unwrap_or_default();
        Ok(ServerInfo {
            server_version: version,
            server_type: "SQLite".into(),
        })
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let path = db_path(config)?;
        let url = format!("sqlite:{}", path);
        let timeout = Duration::from_secs(config.connection_timeout as u64);
        let pool_id = format!("sqlite_{}", uuid::Uuid::new_v4());

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .min_connections(1)
            .acquire_timeout(timeout)
            .connect(&url)
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;

        // Enable WAL mode for better concurrent access
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&pool)
            .await
            .ok();

        let mut pools = self.pools.write().await;
        pools.insert(pool_id.clone(), pool);

        Ok(ConnectionHandle {
            id: pool_id.clone(),
            pool_id,
        })
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        let mut pools = self.pools.write().await;
        if let Some(pool) = pools.remove(&handle.pool_id) {
            pool.close().await;
        }
        Ok(())
    }

    async fn get_databases(&self, _handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Ok(vec!["main".into()])
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let rows = sqlx::query(
            r#"
            SELECT name, type FROM sqlite_master
            WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%'
            ORDER BY name
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let tables: Vec<TableInfo> = rows
            .iter()
            .map(|row| {
                let name: String = row.get(0);
                let kind: String = row.get(1);
                let table_type = match kind.as_str() {
                    "view" => TableType::View,
                    _ => TableType::Table,
                };
                TableInfo {
                    name,
                    schema: None,
                    table_type,
                    row_count: None,
                }
            })
            .collect();

        Ok(tables)
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let col_rows = sqlx::query(&format!("PRAGMA table_info({})", self.quote_ident(table)))
            .fetch_all(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let mut columns = Vec::new();
        let mut primary_keys = Vec::new();

        for row in &col_rows {
            let name: String = row.get("name");
            let data_type: String = row.get("type");
            let notnull: bool = row.get::<i32, _>("notnull") != 0;
            let default: Option<String> = row.try_get("dflt_value").ok();
            let pk: bool = row.get::<i32, _>("pk") != 0;

            if pk {
                primary_keys.push(name.clone());
            }

            columns.push(ColumnSchema {
                name,
                data_type,
                nullable: !notnull,
                default_value: default,
                is_primary_key: pk,
                is_auto_increment: false,
                comment: None,
            });
        }

        // Indexes
        let idx_rows = sqlx::query(&format!("PRAGMA index_list({})", self.quote_ident(table)))
            .fetch_all(pool)
            .await
            .unwrap_or_default();

        let mut indexes = Vec::new();
        for idx_row in &idx_rows {
            let idx_name: String = idx_row.get("name");
            let is_unique: bool = idx_row.get::<i32, _>("unique") != 0;

            let info_rows = sqlx::query(&format!(
                "PRAGMA index_info(\"{}\")",
                idx_name.replace('"', "\"\"")
            ))
            .fetch_all(pool)
            .await
            .unwrap_or_default();

            let idx_columns: Vec<String> = info_rows
                .iter()
                .map(|r| r.get::<String, _>("name"))
                .collect();

            let is_primary = idx_name.starts_with("sqlite_autoindex_");
            indexes.push(IndexInfo {
                name: idx_name,
                columns: idx_columns,
                is_unique,
                is_primary,
                index_type: "btree".into(),
            });
        }

        // Foreign keys
        let fk_rows = sqlx::query(&format!(
            "PRAGMA foreign_key_list({})",
            self.quote_ident(table)
        ))
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let mut fk_map: HashMap<i64, ForeignKeyInfo> = HashMap::new();
        for fk_row in &fk_rows {
            let id: i64 = fk_row.get::<i32, _>("id") as i64;
            let from_col: String = fk_row.get("from");
            let to_table: String = fk_row.get("table");
            let to_col: String = fk_row.get("to");
            let on_update: String = fk_row.try_get("on_update").unwrap_or_default();
            let on_delete: String = fk_row.try_get("on_delete").unwrap_or_default();

            fk_map
                .entry(id)
                .and_modify(|fk| {
                    fk.columns.push(from_col.clone());
                    fk.referenced_columns.push(to_col.clone());
                })
                .or_insert_with(|| ForeignKeyInfo {
                    name: format!("fk_{}_{}_{}", table, to_table, id),
                    columns: vec![from_col],
                    referenced_table: to_table,
                    referenced_columns: vec![to_col],
                    on_update,
                    on_delete,
                });
        }

        let foreign_keys: Vec<ForeignKeyInfo> = fk_map.into_values().collect();

        Ok(TableSchema {
            table_name: table.to_string(),
            columns,
            primary_keys,
            indexes,
            foreign_keys,
        })
    }

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let start = Instant::now();
        let rows = sqlx::query(sql)
            .fetch_all(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let (columns, result_rows) = Self::decode_rows(&rows);
        Ok(QueryResult {
            columns,
            rows: result_rows,
            rows_affected: None,
            execution_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let total_start = Instant::now();
        let statements: Vec<&str> = sql
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        let mut results = Vec::new();
        for stmt in statements {
            let start = Instant::now();
            let limited_sql = if let Some(lim) = limit {
                if stmt.to_uppercase().starts_with("SELECT")
                    && !stmt.to_uppercase().contains("LIMIT")
                {
                    format!("{} LIMIT {}", stmt, lim)
                } else {
                    stmt.to_string()
                }
            } else {
                stmt.to_string()
            };

            match sqlx::query(&limited_sql).fetch_all(pool).await {
                Ok(rows) => {
                    let (columns, result_rows) = Self::decode_rows(&rows);
                    let truncated = limit.map_or(false, |l| result_rows.len() >= l as usize);
                    results.push(StatementResult {
                        sql: stmt.to_string(),
                        columns,
                        rows: result_rows,
                        rows_affected: None,
                        execution_time_ms: start.elapsed().as_millis() as u64,
                        truncated,
                    });
                }
                Err(e) => {
                    return Err(DriverError::QueryFailed(format!("[{stmt}] {e}")));
                }
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
        use futures_util::TryStreamExt;
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let total_start = Instant::now();
        let statements: Vec<&str> = sql
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if statements.is_empty() {
            on_event(QueryStreamEvent::Done { total_time_ms: 0 });
            return Ok(());
        }

        for (index, stmt) in statements.iter().enumerate() {
            let (effective_sql, applied_limit) = apply_sqlite_select_limit(stmt, limit);
            let stmt_start = Instant::now();
            let mut stream = sqlx::query(&effective_sql).fetch(pool);
            let mut batcher = QueryRowBatcher::new(
                Arc::clone(&on_event),
                index,
                (*stmt).to_string(),
                applied_limit,
            );
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
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let start = Instant::now();
        let rows = Self::bind_values(sqlx::query(sql), params)
            .fetch_all(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let (columns, result_rows) = Self::decode_rows(&rows);
        Ok(QueryResult {
            columns,
            rows: result_rows,
            rows_affected: None,
            execution_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
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
        self.execute(handle, "BEGIN").await?;
        Ok(TransactionHandle {
            id: format!("sqlite_tx_{}", uuid::Uuid::new_v4()),
            connection_id: handle.id.clone(),
        })
    }

    async fn commit(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        // SQLite connect() uses the same value for handle.id and handle.pool_id.
        let handle = ConnectionHandle {
            id: tx.connection_id.clone(),
            pool_id: tx.connection_id,
        };
        self.execute(&handle, "COMMIT").await?;
        Ok(())
    }

    async fn rollback(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        let handle = ConnectionHandle {
            id: tx.connection_id.clone(),
            pool_id: tx.connection_id,
        };
        self.execute(&handle, "ROLLBACK").await?;
        Ok(())
    }

    async fn explain(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        let explain_sql = format!("EXPLAIN QUERY PLAN {}", sql);
        let result = self.query(handle, &explain_sql).await?;

        let plan_lines: Vec<String> = result
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .filter_map(|v| {
                        v.as_ref().map(|val| match val {
                            Value::String(s) => s.clone(),
                            Value::Integer(n) => n.to_string(),
                            _ => format!("{:?}", val),
                        })
                    })
                    .collect::<Vec<_>>()
                    .join(" | ")
            })
            .collect();

        Ok(ExplainResult {
            plan_text: plan_lines.join("\n"),
            plan_json: None,
            plan_tree: None,
            total_cost: None,
            estimated_rows: None,
        })
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        tracing::debug!("sqlite: cancel_query is a no-op (single-connection, in-process)");
        Ok(())
    }

    async fn get_server_info(&self, handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let row = sqlx::query("SELECT sqlite_version()")
            .fetch_one(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let version: String = row.try_get(0).unwrap_or_default();
        Ok(ServerInfo {
            server_version: version,
            server_type: "SQLite".to_string(),
        })
    }

    async fn structure_capabilities(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<StructureCapabilities, DriverError> {
        Ok(structure::capabilities(&self.driver_type()))
    }

    async fn plan_structure_changes(
        &self,
        _handle: &ConnectionHandle,
        request: &StructureChangeRequest,
    ) -> Result<StructureChangePlan, DriverError> {
        let caps = structure::capabilities(&self.driver_type());
        structure::plan_changes(request, &caps)
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        let mut cmds = vec![
            query_command_definition(),
            execute_command_definition(),
            query_stream_command_definition(),
        ];
        cmds.extend(schema_catalog_command_definitions());
        cmds.extend(schema_object_command_definitions());
        cmds.extend(crate::adb::adb_command_definitions());
        cmds
    }

    async fn execute_command(
        &self,
        handle: &ConnectionHandle,
        command: &str,
        input: serde_json::Value,
    ) -> Result<CommandResult, DriverError> {
        match execute_standard_sql_command(self, handle, command, input.clone()).await {
            Err(DriverError::Unsupported(_)) => {}
            other => return other,
        }
        if let Some(result) =
            try_execute_schema_catalog_command(self, handle, command, input.clone()).await?
        {
            return Ok(result);
        }
        if is_schema_object_command(command) {
            return execute_schema_object_command(
                self,
                &self.driver_type(),
                handle,
                command,
                input,
            )
            .await;
        }
        // ADB helpers are unbound: they never touch a connection pool, so the
        // (possibly empty) handle is irrelevant to them.
        if crate::adb::is_adb_command(command) {
            return crate::adb::execute_adb_command(command, &input).await;
        }
        Err(DriverError::Unsupported(format!(
            "unsupported driver command: {command}"
        )))
    }
}

fn apply_sqlite_select_limit(stmt: &str, limit: Option<u32>) -> (String, Option<u32>) {
    let Some(lim) = limit else {
        return (stmt.to_string(), None);
    };
    let trimmed = stmt.trim();
    let upper = trimmed.to_ascii_uppercase();
    let is_select = upper.starts_with("SELECT") || upper.starts_with("WITH");
    if !is_select {
        return (stmt.to_string(), None);
    }
    if upper.split_whitespace().any(|w| w == "LIMIT") {
        return (stmt.to_string(), Some(lim));
    }
    (format!("{trimmed} LIMIT {}", lim + 1), Some(lim))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_sqlite_select_limit_none_does_not_rewrite() {
        let (sql, cap) = apply_sqlite_select_limit("SELECT * FROM t", None);
        assert_eq!(sql, "SELECT * FROM t");
        assert_eq!(cap, None);
    }

    #[test]
    fn apply_sqlite_select_limit_appends_plus_one() {
        let (sql, cap) = apply_sqlite_select_limit("SELECT * FROM t", Some(10));
        assert_eq!(sql, "SELECT * FROM t LIMIT 11");
        assert_eq!(cap, Some(10));
    }

    #[test]
    fn apply_sqlite_select_limit_keeps_existing_limit_and_skips_dml() {
        let (sql, cap) = apply_sqlite_select_limit("SELECT * FROM t LIMIT 3", Some(10));
        assert_eq!(sql, "SELECT * FROM t LIMIT 3");
        assert_eq!(cap, Some(10));
        let (sql, cap) = apply_sqlite_select_limit("INSERT INTO t VALUES (1)", Some(10));
        assert_eq!(sql, "INSERT INTO t VALUES (1)");
        assert_eq!(cap, None);
        let (sql, cap) = apply_sqlite_select_limit("WITH x AS (SELECT 1) SELECT * FROM x", Some(2));
        assert_eq!(sql, "WITH x AS (SELECT 1) SELECT * FROM x LIMIT 3");
        assert_eq!(cap, Some(2));
    }

    fn collect_events() -> (
        QueryStreamCallback,
        std::sync::Arc<std::sync::Mutex<Vec<QueryStreamEvent>>>,
    ) {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let events_cb = std::sync::Arc::clone(&events);
        (
            std::sync::Arc::new(move |ev| {
                events_cb.lock().unwrap().push(ev);
            }),
            events,
        )
    }

    fn stream_row_count(events: &[QueryStreamEvent]) -> usize {
        events
            .iter()
            .filter_map(|e| match e {
                QueryStreamEvent::Rows { rows, .. } => Some(rows.len()),
                _ => None,
            })
            .sum()
    }

    #[tokio::test]
    async fn query_stream_limits_and_batches_independently() {
        let dir =
            std::env::temp_dir().join(format!("datazen-sqlite-stream-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("stream.db");
        let path_str = path.to_string_lossy().to_string();
        std::fs::File::create(&path).unwrap();

        let driver = SqliteDriver::new();
        let handle = driver.connect(&test_config(&path_str)).await.unwrap();
        driver
            .execute(&handle, "CREATE TABLE t (id INTEGER PRIMARY KEY)")
            .await
            .unwrap();
        driver
            .execute(
                &handle,
                "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x < 600)
                 INSERT INTO t SELECT x FROM c",
            )
            .await
            .unwrap();

        let (cb, events) = collect_events();
        driver
            .query_stream(&handle, "SELECT id FROM t ORDER BY id", Some(7), cb)
            .await
            .unwrap();
        {
            let events = events.lock().unwrap();
            assert_eq!(stream_row_count(&events), 7);
            assert!(events.iter().any(|e| matches!(
                e,
                QueryStreamEvent::StatementEnd {
                    truncated: true,
                    ..
                }
            )));
        }

        let (cb, events) = collect_events();
        driver
            .query_stream(&handle, "SELECT id FROM t ORDER BY id", None, cb)
            .await
            .unwrap();
        {
            let events = events.lock().unwrap();
            assert_eq!(stream_row_count(&events), 600);
            let chunks = events
                .iter()
                .filter(|e| matches!(e, QueryStreamEvent::Rows { .. }))
                .count();
            assert!(chunks >= 2);
        }

        let (cb, events) = collect_events();
        driver.query_stream(&handle, "   ", None, cb).await.unwrap();
        assert!(matches!(
            events.lock().unwrap().as_slice(),
            [QueryStreamEvent::Done { total_time_ms: 0 }]
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn test_config(path: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: "sqlite-test".into(),
            name: "test".into(),
            database_type: "sqlite".into(),
            host: None,
            port: None,
            database: Some(path.into()),
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
    async fn commit_and_rollback_end_transaction() {
        let dir = std::env::temp_dir().join(format!("datazen-sqlite-tx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tx.db");
        let path_str = path.to_string_lossy().to_string();
        // Ensure the file exists so sqlx can open it without mode=rwc quirks.
        std::fs::File::create(&path).unwrap();

        let driver = SqliteDriver::new();
        let handle = driver.connect(&test_config(&path_str)).await.unwrap();

        driver
            .execute(&handle, "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)")
            .await
            .unwrap();

        let tx = driver.begin_transaction(&handle).await.unwrap();
        driver
            .execute(&handle, "INSERT INTO t (v) VALUES ('a')")
            .await
            .unwrap();
        driver.rollback(tx).await.unwrap();

        let after_rollback = driver
            .query(&handle, "SELECT COUNT(*) FROM t")
            .await
            .unwrap();
        match &after_rollback.rows[0][0] {
            Some(Value::Integer(0)) => {}
            other => panic!("rollback should discard insert, got {other:?}"),
        }

        let tx = driver.begin_transaction(&handle).await.unwrap();
        driver
            .execute(&handle, "INSERT INTO t (v) VALUES ('b')")
            .await
            .unwrap();
        driver.commit(tx).await.unwrap();

        let after_commit = driver
            .query(&handle, "SELECT COUNT(*) FROM t")
            .await
            .unwrap();
        match &after_commit.rows[0][0] {
            Some(Value::Integer(1)) => {}
            other => panic!("commit should persist insert, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
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
        let _q = SqliteDriver::bind_values(sqlx::query("SELECT ?, ?, ?, ?, ?, ?, ?, ?"), &params);
    }

    #[tokio::test]
    async fn query_with_params_binds_values() {
        let dir =
            std::env::temp_dir().join(format!("datazen-sqlite-params-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("params.db");
        let path_str = path.to_string_lossy().to_string();
        std::fs::File::create(&path).unwrap();

        let driver = SqliteDriver::new();
        let handle = driver.connect(&test_config(&path_str)).await.unwrap();

        driver
            .execute(
                &handle,
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, n INTEGER)",
            )
            .await
            .unwrap();
        driver
            .execute(
                &handle,
                "INSERT INTO t (name, n) VALUES ('alice', 1), ('bob', 2)",
            )
            .await
            .unwrap();

        let result = driver
            .query_with_params(
                &handle,
                "SELECT name FROM t WHERE n = ? AND name = ?",
                &[Value::Integer(2), Value::String("bob".into())],
            )
            .await
            .unwrap();

        assert_eq!(result.rows.len(), 1);
        match &result.rows[0][0] {
            Some(Value::String(s)) if s == "bob" => {}
            other => panic!("expected bob, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}

//! DuckDB driver — embedded OLAP database (file or in-memory).

use async_trait::async_trait;
use datazen_driver_api::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::sync::RwLock;

pub struct DuckDbDriver {
    pools: RwLock<HashMap<String, Arc<Mutex<::duckdb::Connection>>>>,
}

impl DuckDbDriver {
    pub fn new() -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
        }
    }

    fn get<'a>(
        pools: &'a HashMap<String, Arc<Mutex<::duckdb::Connection>>>,
        handle: &ConnectionHandle,
    ) -> Result<Arc<Mutex<::duckdb::Connection>>, DriverError> {
        pools
            .get(&handle.pool_id)
            .cloned()
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))
    }

    fn value_from_ref(v: ::duckdb::types::ValueRef<'_>) -> Option<Value> {
        use ::duckdb::types::ValueRef as VR;
        match v {
            VR::Null => Some(Value::Null),
            VR::Boolean(b) => Some(Value::Bool(b)),
            VR::TinyInt(i) => Some(Value::Integer(i as i64)),
            VR::SmallInt(i) => Some(Value::Integer(i as i64)),
            VR::Int(i) => Some(Value::Integer(i as i64)),
            VR::BigInt(i) => Some(Value::Integer(i)),
            VR::HugeInt(i) => Some(Value::Integer(i as i64)),
            VR::UTinyInt(i) => Some(Value::Integer(i as i64)),
            VR::USmallInt(i) => Some(Value::Integer(i as i64)),
            VR::UInt(i) => Some(Value::Integer(i as i64)),
            VR::UBigInt(i) => Some(Value::Integer(i as i64)),
            VR::Float(f) => Some(Value::Float(f as f64)),
            VR::Double(f) => Some(Value::Float(f)),
            VR::Decimal(d) => Some(Value::String(format!("{d:?}"))),
            VR::Text(t) => Some(Value::String(String::from_utf8_lossy(t).into_owned())),
            VR::Blob(b) | VR::Geometry(b) => Some(Value::String(format!(
                "\\x{}",
                b.iter().map(|x| format!("{x:02x}")).collect::<String>()
            ))),
            VR::Timestamp(_, i) | VR::Time64(_, i) => Some(Value::Timestamp(i.to_string())),
            VR::Date32(i) => Some(Value::Timestamp(i.to_string())),
            _ => Some(Value::String(format!("{v:?}"))),
        }
    }

    fn columns_from_schema(stmt: &::duckdb::Statement<'_>) -> Vec<ColumnInfo> {
        stmt.schema()
            .fields()
            .iter()
            .map(|f| ColumnInfo {
                name: f.name().to_string(),
                data_type: format!("{:?}", f.data_type()),
                nullable: true,
            })
            .collect()
    }

    /// DuckDB `PRAGMA table_info` flags may arrive as bool or integer.
    fn pragma_flag(row: &::duckdb::Row<'_>, idx: usize) -> bool {
        if let Ok(b) = row.get::<_, bool>(idx) {
            return b;
        }
        row.get::<_, i64>(idx).map(|i| i != 0).unwrap_or(false)
    }

    async fn with_conn<T, F>(&self, handle: &ConnectionHandle, f: F) -> Result<T, DriverError>
    where
        T: Send + 'static,
        F: FnOnce(&mut ::duckdb::Connection) -> Result<T, DriverError> + Send + 'static,
    {
        let pools = self.pools.read().await;
        let conn = Self::get(&pools, handle)?;
        drop(pools);
        let conn = conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.blocking_lock();
            f(&mut guard)
        })
        .await
        .map_err(|e| DriverError::QueryFailed(format!("DuckDB task failed: {e}")))?
    }
}

#[async_trait]
impl DatabaseDriver for DuckDbDriver {
    fn driver_type(&self) -> DatabaseType {
        "duckdb".to_string()
    }

    fn supports_explain(&self) -> bool {
        true
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let path = config.database.clone().unwrap_or_default();
        let conn = ::duckdb::Connection::open(if path.is_empty() { ":memory:" } else { &path })
            .map_err(|e| DriverError::ConnectionFailed(format!("DuckDB open failed: {e}")))?;
        let version = conn
            .query_row("SELECT version()", [], |row| row.get::<_, String>(0))
            .unwrap_or_default();
        Ok(ServerInfo {
            server_version: version,
            server_type: "duckdb".to_string(),
        })
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let path = config.database.clone().unwrap_or_default();
        let pool_id = format!("duckdb_{}", uuid::Uuid::new_v4());
        let conn = ::duckdb::Connection::open(if path.is_empty() { ":memory:" } else { &path })
            .map_err(|e| DriverError::ConnectionFailed(format!("DuckDB open failed: {e}")))?;
        self.pools
            .write()
            .await
            .insert(pool_id.clone(), Arc::new(Mutex::new(conn)));
        Ok(ConnectionHandle {
            id: pool_id.clone(),
            pool_id,
        })
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        self.pools.write().await.remove(&handle.pool_id);
        Ok(())
    }

    async fn get_databases(&self, _handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Ok(vec!["main".to_string(), "temp".to_string()])
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        self.with_conn(handle, |conn| {
            let mut stmt = conn
                .prepare("SELECT table_name, table_type FROM information_schema.tables ORDER BY table_name")
                .map_err(|e| DriverError::QueryFailed(format!("DuckDB prepare failed: {e}")))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1).unwrap_or_default(),
                    ))
                })
                .map_err(|e| DriverError::QueryFailed(format!("DuckDB query failed: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| DriverError::QueryFailed(format!("DuckDB read failed: {e}")))?;
            Ok(rows
                .into_iter()
                .map(|(name, kind)| TableInfo {
                    name,
                    schema: None,
                    table_type: if kind.eq_ignore_ascii_case("VIEW") {
                        TableType::View
                    } else {
                        TableType::Table
                    },
                    row_count: None,
                })
                .collect())
        })
        .await
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        let table = table.to_string();
        self.with_conn(handle, move |conn| {
            let mut stmt = conn
                .prepare(&format!(
                    "PRAGMA table_info('{}')",
                    table.replace('\'', "''")
                ))
                .map_err(|e| DriverError::QueryFailed(format!("DuckDB prepare failed: {e}")))?;
            let rows = stmt
                .query_map([], |row| {
                    let not_null = Self::pragma_flag(row, 3);
                    Ok(ColumnSchema {
                        name: row.get::<_, String>(1)?,
                        data_type: row.get::<_, String>(2).unwrap_or_default(),
                        nullable: !not_null,
                        default_value: row.get::<_, Option<String>>(4).ok().flatten(),
                        comment: None,
                        is_primary_key: Self::pragma_flag(row, 5),
                        is_auto_increment: false,
                    })
                })
                .map_err(|e| DriverError::QueryFailed(format!("DuckDB query failed: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| DriverError::QueryFailed(format!("DuckDB read failed: {e}")))?;
            let primary_keys: Vec<String> = rows
                .iter()
                .filter(|c| c.is_primary_key)
                .map(|c| c.name.clone())
                .collect();
            Ok(TableSchema {
                table_name: table.clone(),
                columns: rows,
                primary_keys,
                indexes: Vec::new(),
                foreign_keys: Vec::new(),
            })
        })
        .await
    }

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        let sql = sql.to_string();
        self.with_conn(handle, move |conn| {
            let start = Instant::now();
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| DriverError::QueryFailed(format!("DuckDB prepare failed: {e}")))?;
            // duckdb-rs requires the statement to be executed before schema() is available.
            let mut rows = stmt
                .query([])
                .map_err(|e| DriverError::QueryFailed(format!("DuckDB query failed: {e}")))?;
            let columns = rows
                .as_ref()
                .map(Self::columns_from_schema)
                .unwrap_or_default();
            let mut result_rows = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| DriverError::QueryFailed(format!("DuckDB read failed: {e}")))?
            {
                let vals: Vec<Option<Value>> = (0..columns.len())
                    .map(|i| {
                        row.get_ref::<usize>(i)
                            .ok()
                            .and_then(|v| Self::value_from_ref(v))
                    })
                    .collect();
                result_rows.push(vals);
            }
            Ok(QueryResult {
                columns,
                rows: result_rows,
                rows_affected: None,
                execution_time_ms: start.elapsed().as_millis() as u64,
            })
        })
        .await
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        let sql = sql.to_string();
        self.with_conn(handle, move |conn| {
            let total_start = Instant::now();
            let statements: Vec<String> = sql
                .split(';')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let mut results = Vec::new();
            for stmt in statements {
                let start = Instant::now();
                let limited = if let Some(lim) = limit {
                    if stmt.to_uppercase().starts_with("SELECT")
                        && !stmt.to_uppercase().contains("LIMIT")
                    {
                        format!("{stmt} LIMIT {lim}")
                    } else {
                        stmt.clone()
                    }
                } else {
                    stmt.clone()
                };
                let mut st = conn
                    .prepare(&limited)
                    .map_err(|e| DriverError::QueryFailed(format!("DuckDB prepare failed: {e}")))?;
                let mut rows = st
                    .query([])
                    .map_err(|e| DriverError::QueryFailed(format!("DuckDB query failed: {e}")))?;
                let columns = rows
                    .as_ref()
                    .map(Self::columns_from_schema)
                    .unwrap_or_default();
                let mut result_rows = Vec::new();
                while let Some(row) = rows
                    .next()
                    .map_err(|e| DriverError::QueryFailed(format!("DuckDB read failed: {e}")))?
                {
                    let vals: Vec<Option<Value>> = (0..columns.len())
                        .map(|i| {
                            row.get_ref::<usize>(i)
                                .ok()
                                .and_then(|v| Self::value_from_ref(v))
                        })
                        .collect();
                    result_rows.push(vals);
                }
                results.push(StatementResult {
                    sql: stmt,
                    columns,
                    rows: result_rows,
                    rows_affected: None,
                    execution_time_ms: start.elapsed().as_millis() as u64,
                    truncated: false,
                });
            }
            Ok(MultiQueryResult {
                results,
                total_time_ms: total_start.elapsed().as_millis() as u64,
            })
        })
        .await
    }

    async fn query_stream(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        let sql = sql.to_string();
        self.with_conn(handle, move |conn| {
            let total_start = Instant::now();
            let statements: Vec<String> = sql
                .split(';')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if statements.is_empty() {
                on_event(QueryStreamEvent::Done { total_time_ms: 0 });
                return Ok(());
            }
            for (index, stmt) in statements.iter().enumerate() {
                let start = Instant::now();
                let (effective, applied) = append_select_limit(stmt, limit);
                let mut st = conn
                    .prepare(&effective)
                    .map_err(|e| DriverError::QueryFailed(format!("DuckDB prepare failed: {e}")))?;
                let mut rows = st
                    .query([])
                    .map_err(|e| DriverError::QueryFailed(format!("DuckDB query failed: {e}")))?;
                let columns = rows
                    .as_ref()
                    .map(Self::columns_from_schema)
                    .unwrap_or_default();
                let col_len = columns.len();
                let mut batcher =
                    QueryRowBatcher::new(Arc::clone(&on_event), index, stmt.clone(), applied);
                batcher.start(columns);
                while let Some(row) = rows
                    .next()
                    .map_err(|e| DriverError::QueryFailed(format!("DuckDB read failed: {e}")))?
                {
                    let vals: Vec<Option<Value>> = (0..col_len)
                        .map(|i| {
                            row.get_ref::<usize>(i)
                                .ok()
                                .and_then(|v| Self::value_from_ref(v))
                        })
                        .collect();
                    if !batcher.push(vals) {
                        break;
                    }
                }
                batcher.finish(start.elapsed().as_millis() as u64, None);
            }
            on_event(QueryStreamEvent::Done {
                total_time_ms: total_start.elapsed().as_millis() as u64,
            });
            Ok(())
        })
        .await
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        _params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        self.query(handle, sql).await
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        let sql = sql.to_string();
        self.with_conn(handle, move |conn| {
            conn.execute(&sql, [])
                .map(|n| n as u64)
                .map_err(|e| DriverError::QueryFailed(format!("DuckDB execute failed: {e}")))
        })
        .await
    }

    async fn explain(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        let result = self.query(handle, &format!("EXPLAIN {sql}")).await?;
        Ok(datazen_driver_http_support::explain_result_from_query(
            result,
        ))
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_config() -> ConnectionConfig {
        ConnectionConfig {
            id: "duckdb-smoke".into(),
            name: "duckdb-smoke".into(),
            database_type: "duckdb".into(),
            host: None,
            port: None,
            database: Some(":memory:".into()),
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
        }
    }

    #[tokio::test]
    async fn in_memory_smoke_query_explain_and_primary_keys() {
        let driver = DuckDbDriver::new();
        let handle = driver
            .connect(&memory_config())
            .await
            .expect("connect :memory:");

        driver
            .execute(
                &handle,
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name VARCHAR)",
            )
            .await
            .expect("create table");
        driver
            .execute(&handle, "INSERT INTO items VALUES (1, 'a'), (2, 'b')")
            .await
            .expect("insert");

        let rows = driver
            .query(&handle, "SELECT id, name FROM items ORDER BY id")
            .await
            .expect("query");
        assert_eq!(rows.rows.len(), 2);

        let tables = driver.get_tables(&handle, "main").await.expect("tables");
        assert!(
            tables.iter().any(|t| t.name == "items"),
            "expected items table, got {tables:?}"
        );

        let schema = driver
            .get_table_schema(&handle, "items")
            .await
            .expect("schema");
        assert!(
            !schema.columns.is_empty(),
            "expected columns for items, got {schema:?}"
        );
        assert_eq!(
            schema.primary_keys,
            vec!["id".to_string()],
            "schema={schema:?}"
        );
        assert!(schema
            .columns
            .iter()
            .any(|c| c.name == "id" && c.is_primary_key));

        let (cb, events) = collect_events();
        driver
            .query_stream(&handle, "SELECT id FROM items ORDER BY id", None, cb)
            .await
            .expect("stream");
        let events = events.lock().unwrap();
        assert_eq!(stream_row_count(&events), 2);
        assert!(matches!(events.last(), Some(QueryStreamEvent::Done { .. })));

        let plan = driver
            .explain(&handle, "SELECT * FROM items WHERE id = 1")
            .await
            .expect("explain");
        assert!(
            !plan.plan_text.is_empty(),
            "expected non-empty explain plan"
        );

        driver.disconnect(handle).await.expect("disconnect");
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
    async fn query_stream_empty_sql_emits_done() {
        let driver = DuckDbDriver::new();
        let handle = driver.connect(&memory_config()).await.unwrap();
        let (cb, events) = collect_events();
        driver.query_stream(&handle, "  ", None, cb).await.unwrap();
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            QueryStreamEvent::Done { total_time_ms: 0 }
        ));
    }

    #[tokio::test]
    async fn query_stream_applies_sql_limit_not_batch_size() {
        let driver = DuckDbDriver::new();
        let handle = driver.connect(&memory_config()).await.unwrap();
        let (cb, events) = collect_events();
        driver
            .query_stream(&handle, "SELECT range FROM range(600)", Some(5), cb)
            .await
            .unwrap();
        let events = events.lock().unwrap();
        assert_eq!(stream_row_count(&events), 5);
        assert!(events.iter().any(|e| matches!(
            e,
            QueryStreamEvent::StatementEnd {
                truncated: true,
                ..
            }
        )));
    }

    #[tokio::test]
    async fn query_stream_emits_multiple_row_batches_when_unlimited() {
        let driver = DuckDbDriver::new();
        let handle = driver.connect(&memory_config()).await.unwrap();
        let (cb, events) = collect_events();
        driver
            .query_stream(&handle, "SELECT range FROM range(600)", None, cb)
            .await
            .unwrap();
        let events = events.lock().unwrap();
        assert_eq!(stream_row_count(&events), 600);
        let chunks = events
            .iter()
            .filter(|e| matches!(e, QueryStreamEvent::Rows { .. }))
            .count();
        assert!(chunks >= 2, "expected multiple IPC batches, got {chunks}");
        assert!(events.iter().any(|e| matches!(
            e,
            QueryStreamEvent::StatementEnd {
                truncated: false,
                ..
            }
        )));
    }
}

//! DuckDB driver — embedded OLAP database (file or in-memory).

use super::*;
use async_trait::async_trait;
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
                    Ok(ColumnSchema {
                        name: row.get::<_, String>(1)?,
                        data_type: row.get::<_, String>(2).unwrap_or_default(),
                        nullable: row.get::<_, i64>(3).unwrap_or(1) == 0,
                        default_value: row.get::<_, Option<String>>(4).ok().flatten(),
                        comment: None,
                        is_primary_key: row.get::<_, i64>(5).unwrap_or(0) > 0,
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

    async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError> {
        let sql = sql.to_string();
        self.with_conn(handle, move |conn| {
            let start = Instant::now();
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| DriverError::QueryFailed(format!("DuckDB prepare failed: {e}")))?;
            let columns = Self::columns_from_schema(&stmt);
            let mut rows = stmt
                .query([])
                .map_err(|e| DriverError::QueryFailed(format!("DuckDB query failed: {e}")))?;
            let mut result_rows = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| DriverError::QueryFailed(format!("DuckDB read failed: {e}")))?
            {
                let vals: Vec<Option<Value>> = (0..columns.len())
                    .map(|i| row.get_ref::<usize>(i).ok().and_then(|v| Self::value_from_ref(v)))
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
                    if stmt.to_uppercase().starts_with("SELECT") && !stmt.to_uppercase().contains("LIMIT")
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
                let columns = Self::columns_from_schema(&st);
                let mut rows = st
                    .query([])
                    .map_err(|e| DriverError::QueryFailed(format!("DuckDB query failed: {e}")))?;
                let mut result_rows = Vec::new();
                while let Some(row) = rows
                    .next()
                    .map_err(|e| DriverError::QueryFailed(format!("DuckDB read failed: {e}")))?
                {
                    let vals: Vec<Option<Value>> = (0..columns.len())
                        .map(|i| row.get_ref::<usize>(i).ok().and_then(|v| Self::value_from_ref(v)))
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

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

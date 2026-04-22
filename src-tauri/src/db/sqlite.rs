//! SQLite driver backed by sqlx SqlitePool.

use super::*;
use async_trait::async_trait;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Column, Row, SqlitePool};
use std::collections::HashMap;
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

    fn decode_rows(rows: &[sqlx::sqlite::SqliteRow]) -> (Vec<ColumnInfo>, Vec<Vec<Option<Value>>>) {
        let columns: Vec<ColumnInfo> = if let Some(first) = rows.first() {
            first
                .columns()
                .iter()
                .map(|c| ColumnInfo {
                    name: c.name().to_string(),
                    data_type: c.type_info().to_string(),
                    nullable: true,
                })
                .collect()
        } else {
            Vec::new()
        };

        let result_rows: Vec<Vec<Option<Value>>> = rows
            .iter()
            .map(|row| {
                row.columns()
                    .iter()
                    .enumerate()
                    .map(|(i, col)| {
                        let type_name = col.type_info().to_string().to_uppercase();
                        match type_name.as_str() {
                            "INTEGER" | "INT" | "BIGINT" | "SMALLINT" | "TINYINT" | "MEDIUMINT" => {
                                row.try_get::<i64, _>(i).ok().map(Value::Integer)
                            }
                            "REAL" | "DOUBLE" | "FLOAT" | "NUMERIC" | "DECIMAL" => {
                                row.try_get::<f64, _>(i).ok().map(Value::Float)
                            }
                            "BOOLEAN" => {
                                row.try_get::<bool, _>(i).ok().map(Value::Bool)
                                    .or_else(|| row.try_get::<i32, _>(i).ok().map(|v| Value::Bool(v != 0)))
                            }
                            "BLOB" => {
                                row.try_get::<Vec<u8>, _>(i).ok().map(|bytes| {
                                    let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
                                    Value::String(format!("\\x{}", hex))
                                })
                            }
                            _ => {
                                row.try_get::<String, _>(i)
                                    .ok()
                                    .map(Value::String)
                                    .or_else(|| row.try_get::<i64, _>(i).ok().map(Value::Integer))
                                    .or_else(|| row.try_get::<f64, _>(i).ok().map(Value::Float))
                                    .or_else(|| row.try_get::<bool, _>(i).ok().map(Value::Bool))
                            }
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
    fn driver_type(&self) -> DatabaseType {
        DatabaseType::SQLite
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

        let row = sqlx::query("SELECT sqlite_version()")
            .fetch_one(&pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let version: String = row.try_get(0).unwrap_or_default();
        pool.close().await;
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
            .max_connections(5)
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
        let idx_rows = sqlx::query(&format!(
            "PRAGMA index_list({})", self.quote_ident(table)
        ))
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let mut indexes = Vec::new();
        for idx_row in &idx_rows {
            let idx_name: String = idx_row.get("name");
            let is_unique: bool = idx_row.get::<i32, _>("unique") != 0;

            let info_rows = sqlx::query(&format!("PRAGMA index_info(\"{}\")", idx_name.replace('"', "\"\"")))
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
            "PRAGMA foreign_key_list({})", self.quote_ident(table)
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

    async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError> {
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
                if stmt.to_uppercase().starts_with("SELECT") && !stmt.to_uppercase().contains("LIMIT") {
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
                    results.push(StatementResult {
                        sql: stmt.to_string(),
                        columns: vec![],
                        rows: vec![],
                        rows_affected: None,
                        execution_time_ms: start.elapsed().as_millis() as u64,
                        truncated: false,
                    });
                    tracing::warn!(stmt, error = %e, "sqlite query_multi statement failed");
                }
            }
        }

        Ok(MultiQueryResult {
            results,
            total_time_ms: total_start.elapsed().as_millis() as u64,
        })
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

    async fn commit(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        Ok(())
    }

    async fn rollback(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        Ok(())
    }

    async fn explain(&self, handle: &ConnectionHandle, sql: &str) -> Result<ExplainResult, DriverError> {
        let explain_sql = format!("EXPLAIN QUERY PLAN {}", sql);
        let result = self.query(handle, &explain_sql).await?;

        let plan_lines: Vec<String> = result
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .filter_map(|v| v.as_ref().map(|val| match val {
                        Value::String(s) => s.clone(),
                        Value::Integer(n) => n.to_string(),
                        _ => format!("{:?}", val),
                    }))
                    .collect::<Vec<_>>()
                    .join(" | ")
            })
            .collect();

        Ok(ExplainResult {
            plan_text: plan_lines.join("\n"),
            plan_json: None,
            total_cost: None,
            estimated_rows: None,
        })
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

//! MySQL / MariaDB driver backed by sqlx MySqlPool.

use super::*;
use async_trait::async_trait;
use rust_decimal::prelude::ToPrimitive;
use sqlx::mysql::MySqlPoolOptions;
use sqlx::{Column, MySqlPool, Row};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const JS_MAX_SAFE_INT: i64 = 9_007_199_254_740_991;
const JS_MIN_SAFE_INT: i64 = -9_007_199_254_740_991;

pub struct MysqlDriver {
    pools: RwLock<HashMap<String, MySqlPool>>,
    is_mariadb: bool,
}

impl MysqlDriver {
    pub fn new(is_mariadb: bool) -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
            is_mariadb,
        }
    }

    fn get_pool<'a>(
        pools: &'a HashMap<String, MySqlPool>,
        handle: &ConnectionHandle,
    ) -> Result<&'a MySqlPool, DriverError> {
        pools
            .get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))
    }

    async fn current_database(pool: &MySqlPool) -> Result<String, DriverError> {
        let row = sqlx::query("SELECT DATABASE()")
            .fetch_one(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        Ok(row.try_get::<String, _>(0).unwrap_or_default())
    }

    async fn fetch_columns(
        pool: &MySqlPool,
        table: &str,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        let current_db = Self::current_database(pool).await?;
        Self::fetch_columns_with_db(pool, &current_db, table).await
    }

    async fn fetch_columns_with_db(
        pool: &MySqlPool,
        current_db: &str,
        table: &str,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        let cols = sqlx::query(
            r#"
            SELECT COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT,
                   COLUMN_COMMENT, COLUMN_KEY, EXTRA
            FROM information_schema.COLUMNS
            WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?
            ORDER BY ORDINAL_POSITION
            "#,
        )
        .bind(current_db)
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let mut pk_names: Vec<String> = Vec::new();
        let columns: Vec<ColumnSchema> = cols
            .iter()
            .map(|r| {
                let name: String = r.get("COLUMN_NAME");
                let nullable: String = r.get("IS_NULLABLE");
                let key: String = r.get("COLUMN_KEY");
                let extra: String = r.get("EXTRA");
                let is_pk = key == "PRI";
                if is_pk {
                    pk_names.push(name.clone());
                }
                ColumnSchema {
                    is_primary_key: is_pk,
                    name,
                    data_type: r.get("COLUMN_TYPE"),
                    nullable: nullable == "YES",
                    default_value: r.try_get("COLUMN_DEFAULT").ok(),
                    comment: r.try_get::<String, _>("COLUMN_COMMENT").ok().filter(|s| !s.is_empty()),
                    is_auto_increment: extra.contains("auto_increment"),
                }
            })
            .collect();

        Ok((columns, pk_names))
    }

    fn quote_identifier(name: &str) -> String {
        format!("`{}`", name.replace('`', "``"))
    }

    /// Parse CONSTRAINT ... FOREIGN KEY lines from SHOW CREATE TABLE output.
    fn parse_fk_from_create_table(create_sql: &str) -> Vec<ForeignKeyInfo> {
        let mut fks = Vec::new();
        for line in create_sql.lines() {
            let trimmed = line.trim();
            if !trimmed.contains("FOREIGN KEY") {
                continue;
            }
            // Pattern: CONSTRAINT `name` FOREIGN KEY (`cols`) REFERENCES `table` (`cols`) ...
            let fk_name = Self::extract_backtick_after(trimmed, "CONSTRAINT");
            let fk_cols = Self::extract_backtick_list_after(trimmed, "FOREIGN KEY");
            let ref_table = Self::extract_backtick_after(trimmed, "REFERENCES");
            let ref_cols = Self::extract_backtick_list_after(trimmed, &format!("REFERENCES `{}`", ref_table.replace('`', "``")));

            let on_delete = Self::extract_rule(trimmed, "ON DELETE");
            let on_update = Self::extract_rule(trimmed, "ON UPDATE");

            if !fk_name.is_empty() && !fk_cols.is_empty() {
                fks.push(ForeignKeyInfo {
                    name: fk_name,
                    columns: fk_cols,
                    referenced_table: ref_table,
                    referenced_columns: ref_cols,
                    on_delete,
                    on_update,
                });
            }
        }
        fks.sort_by(|a, b| a.name.cmp(&b.name));
        fks
    }

    /// Extract the first backtick-quoted identifier after a keyword.
    fn extract_backtick_after(s: &str, keyword: &str) -> String {
        if let Some(pos) = s.find(keyword) {
            let after = &s[pos + keyword.len()..];
            if let Some(start) = after.find('`') {
                let inner = &after[start + 1..];
                if let Some(end) = inner.find('`') {
                    return inner[..end].to_string();
                }
            }
        }
        String::new()
    }

    /// Extract a parenthesized list of backtick-quoted identifiers after a keyword.
    fn extract_backtick_list_after(s: &str, keyword: &str) -> Vec<String> {
        if let Some(pos) = s.find(keyword) {
            let after = &s[pos + keyword.len()..];
            if let Some(paren_start) = after.find('(') {
                let inner = &after[paren_start + 1..];
                if let Some(paren_end) = inner.find(')') {
                    let list_str = &inner[..paren_end];
                    return list_str
                        .split(',')
                        .filter_map(|part| {
                            let t = part.trim();
                            if t.starts_with('`') && t.ends_with('`') && t.len() >= 2 {
                                Some(t[1..t.len() - 1].to_string())
                            } else {
                                None
                            }
                        })
                        .collect();
                }
            }
        }
        Vec::new()
    }

    fn extract_rule(s: &str, keyword: &str) -> String {
        if let Some(pos) = s.find(keyword) {
            let after = s[pos + keyword.len()..].trim_start();
            let rule = after.split(|c: char| c == ',' || c == ')' || c == '\n').next().unwrap_or("").trim();
            if !rule.is_empty() {
                return rule.to_uppercase();
            }
        }
        "RESTRICT".to_string()
    }

    fn safe_integer(v: i64) -> Value {
        if v > JS_MAX_SAFE_INT || v < JS_MIN_SAFE_INT {
            Value::String(v.to_string())
        } else {
            Value::Integer(v)
        }
    }

    fn decode_rows(rows: &[sqlx::mysql::MySqlRow]) -> (Vec<ColumnInfo>, Vec<Vec<Option<Value>>>) {
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
                        let debug_name = format!("{:?}", col.type_info());
                        let display_name = col.type_info().to_string();
                        let upper = format!("{} {}", debug_name, display_name).to_uppercase();
                        match upper.as_str() {
                            s if s.contains("BIGINT") || s.contains("INT8") => {
                                row.try_get::<i64, _>(i)
                                    .ok()
                                    .map(Self::safe_integer)
                                    .or_else(|| {
                                        row.try_get::<u64, _>(i)
                                            .ok()
                                            .map(|v| {
                                                if v > JS_MAX_SAFE_INT as u64 {
                                                    Value::String(v.to_string())
                                                } else {
                                                    Value::Integer(v as i64)
                                                }
                                            })
                                    })
                                    .or_else(|| {
                                        row.try_get::<String, _>(i).ok().map(Value::String)
                                    })
                            }
                            s if s.contains("MEDIUMINT") => {
                                // MEDIUMINT: 3 bytes, sqlx reads as i32/u32
                                row.try_get::<i32, _>(i)
                                    .ok()
                                    .map(|v| Value::Integer(v as i64))
                                    .or_else(|| row.try_get::<u32, _>(i).ok().map(|v| Value::Integer(v as i64)))
                                    .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String))
                            }
                            s if s.contains("SMALLINT") => {
                                // SMALLINT: 2 bytes — only use i16/u16
                                row.try_get::<i16, _>(i)
                                    .ok()
                                    .map(|v| Value::Integer(v as i64))
                                    .or_else(|| row.try_get::<u16, _>(i).ok().map(|v| Value::Integer(v as i64)))
                                    .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String))
                            }
                            s if s.contains("TINYINT") => {
                                // TINYINT: 1 byte — only use i8/u8
                                row.try_get::<i8, _>(i)
                                    .ok()
                                    .map(|v| Value::Integer(v as i64))
                                    .or_else(|| row.try_get::<u8, _>(i).ok().map(|v| Value::Integer(v as i64)))
                                    .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String))
                            }
                            s if s.contains("INT") => {
                                // INT: 4 bytes — only use i32/u32
                                row.try_get::<i32, _>(i)
                                    .ok()
                                    .map(|v| Value::Integer(v as i64))
                                    .or_else(|| row.try_get::<u32, _>(i).ok().map(|v| Value::Integer(v as i64)))
                                    .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String))
                            }
                            s if s.contains("DOUBLE") => {
                                // DOUBLE: 8 bytes
                                row.try_get::<f64, _>(i)
                                    .ok()
                                    .map(Value::Float)
                                    .or_else(|| row.try_get::<String, _>(i).ok().and_then(|s| s.parse::<f64>().ok()).map(Value::Float))
                            }
                            s if s.contains("FLOAT") => {
                                // FLOAT: 4 bytes — use f32, then convert to f64
                                row.try_get::<f32, _>(i)
                                    .ok()
                                    .map(|v| Value::Float(v as f64))
                                    .or_else(|| row.try_get::<String, _>(i).ok().and_then(|s| s.parse::<f64>().ok()).map(Value::Float))
                            }
                            s if s.contains("DECIMAL") || s.contains("NUMERIC") => {
                                row.try_get::<rust_decimal::Decimal, _>(i)
                                    .ok()
                                    .map(|d| {
                                        if d.scale() == 0 {
                                            if let Some(n) = d.to_i64() {
                                                return Self::safe_integer(n);
                                            }
                                        }
                                        d.to_f64().map(Value::Float).unwrap_or_else(|| Value::String(d.to_string()))
                                    })
                                    .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String))
                            }
                            s if s.contains("BIT") => {
                                row.try_get::<bool, _>(i)
                                    .ok()
                                    .map(|v| Value::Integer(if v { 1 } else { 0 }))
                                    .or_else(|| row.try_get::<u8, _>(i).ok().map(|v| Value::Integer(v as i64)))
                                    .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String))
                            }
                            s if s.contains("BOOL") || s.contains("BOOLEAN") => {
                                row.try_get::<bool, _>(i).ok().map(Value::Bool)
                            }
                            s if s.contains("DATE") && !s.contains("DATETIME") && !s.contains("TIMESTAMP") => {
                                row.try_get::<chrono::NaiveDate, _>(i)
                                    .ok()
                                    .map(|d| Value::String(d.format("%Y-%m-%d").to_string()))
                                    .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String))
                            }
                            s if s.contains("DATETIME") || s.contains("TIMESTAMP") => {
                                row.try_get::<chrono::NaiveDateTime, _>(i)
                                    .ok()
                                    .map(|dt| Value::String(dt.format("%Y-%m-%d %H:%M:%S").to_string()))
                                    .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String))
                            }
                            s if s.contains("TIME") => {
                                row.try_get::<chrono::NaiveTime, _>(i)
                                    .ok()
                                    .map(|t| Value::String(t.format("%H:%M:%S").to_string()))
                                    .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String))
                            }
                            s if s.contains("YEAR") => {
                                row.try_get::<u16, _>(i)
                                    .ok()
                                    .map(|v| Value::Integer(v as i64))
                                    .or_else(|| row.try_get::<i16, _>(i).ok().map(|v| Value::Integer(v as i64)))
                                    .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String))
                            }
                            s if s.contains("JSON") => {
                                row.try_get::<serde_json::Value, _>(i)
                                    .ok()
                                    .map(Value::Json)
                                    .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String))
                            }
                            _ => {
                                // Only try String for the catch-all; i64/f64 try_get can
                                // panic in sqlx-mysql if column byte-size doesn't match.
                                row.try_get::<String, _>(i).ok().map(Value::String)
                            }
                        }
                    })
                    .collect()
            })
            .collect();

        (columns, result_rows)
    }
}

fn build_mysql_url(config: &ConnectionConfig) -> Result<String, DriverError> {
    let host = config.host.as_deref().unwrap_or("localhost");
    let port = config.port.unwrap_or(3306);
    let user = config.username.as_deref().unwrap_or("root");
    let password = config.password.as_deref().unwrap_or("");
    let database = config.database.as_deref().unwrap_or("");

    let encoded_password = urlencoding::encode(password);

    let url = if database.is_empty() {
        format!("mysql://{}:{}@{}:{}", user, encoded_password, host, port)
    } else {
        format!(
            "mysql://{}:{}@{}:{}/{}",
            user, encoded_password, host, port, database
        )
    };
    Ok(url)
}

#[async_trait]
impl DatabaseDriver for MysqlDriver {
    fn driver_type(&self) -> DatabaseType {
        if self.is_mariadb {
            DatabaseType::MariaDB
        } else {
            DatabaseType::MySQL
        }
    }

    fn quote_char(&self) -> char {
        '`'
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let url = build_mysql_url(config)?;
        let timeout = Duration::from_secs(config.connection_timeout as u64);

        let pool = MySqlPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(timeout)
            .connect(&url)
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;

        let row = sqlx::query("SELECT version()")
            .fetch_one(&pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let version: String = row.try_get(0).unwrap_or_default();
        pool.close().await;

        let server_type = if version.to_lowercase().contains("mariadb") {
            "MariaDB"
        } else {
            "MySQL"
        };

        Ok(ServerInfo {
            server_version: version,
            server_type: server_type.to_string(),
        })
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let url = build_mysql_url(config)?;
        let timeout = Duration::from_secs(config.connection_timeout as u64);

        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(timeout)
            .idle_timeout(Duration::from_secs(600))
            .connect(&url)
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;

        let pool_id = uuid::Uuid::new_v4().to_string();
        let connection_id = uuid::Uuid::new_v4().to_string();

        self.pools.write().await.insert(pool_id.clone(), pool);

        Ok(ConnectionHandle {
            id: connection_id,
            pool_id,
        })
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        if let Some(pool) = self.pools.write().await.remove(&handle.pool_id) {
            pool.close().await;
        }
        Ok(())
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let rows = sqlx::query("SHOW DATABASES")
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
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let rows = sqlx::query(
            r#"
            SELECT TABLE_NAME, TABLE_TYPE
            FROM information_schema.TABLES
            WHERE TABLE_SCHEMA = ?
            ORDER BY TABLE_NAME
            "#,
        )
        .bind(database)
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        Ok(rows
            .iter()
            .map(|r| {
                let tt: String = r.get("TABLE_TYPE");
                TableInfo {
                    schema: Some(database.to_string()),
                    name: r.get("TABLE_NAME"),
                    table_type: match tt.as_str() {
                        "VIEW" => TableType::View,
                        "SYSTEM VIEW" => TableType::SystemTable,
                        _ => TableType::Table,
                    },
                    row_count: None,
                }
            })
            .collect())
    }

    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        Self::fetch_columns(pool, table).await
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        let t0 = std::time::Instant::now();
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let q = Self::quote_identifier(table);

        // All three queries in parallel using fast SHOW commands instead of information_schema
        let (col_rows, idx_rows, create_row) = tokio::try_join!(
            async {
                sqlx::query(&format!("SHOW FULL COLUMNS FROM {}", q))
                    .fetch_all(pool)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))
            },
            async {
                sqlx::query(&format!("SHOW INDEX FROM {}", q))
                    .fetch_all(pool)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))
            },
            async {
                sqlx::query(&format!("SHOW CREATE TABLE {}", q))
                    .fetch_one(pool)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))
            }
        )?;

        tracing::info!(%table, col_rows = col_rows.len(), idx_rows = idx_rows.len(),
            ms = t0.elapsed().as_millis() as u64,
            "mysql get_table_schema: SHOW queries done");

        // ── columns from SHOW FULL COLUMNS ──
        let mut pk_names: Vec<String> = Vec::new();
        let columns: Vec<ColumnSchema> = col_rows
            .iter()
            .map(|r| {
                let name: String = r.get("Field");
                let col_type: String = r.get("Type");
                let nullable: String = r.get("Null");
                let key: String = r.try_get::<String, _>("Key").unwrap_or_default();
                let extra: String = r.try_get::<String, _>("Extra").unwrap_or_default();
                let comment: Option<String> = r.try_get::<String, _>("Comment").ok().filter(|s| !s.is_empty());
                let is_pk = key == "PRI";
                if is_pk {
                    pk_names.push(name.clone());
                }
                ColumnSchema {
                    is_primary_key: is_pk,
                    name,
                    data_type: col_type,
                    nullable: nullable == "YES",
                    default_value: r.try_get("Default").ok(),
                    comment,
                    is_auto_increment: extra.contains("auto_increment"),
                }
            })
            .collect();

        // ── indexes from SHOW INDEX ──
        let mut idx_map: HashMap<String, IndexInfo> = HashMap::new();
        for r in &idx_rows {
            let idx_name: String = r.get("Key_name");
            let col_name: String = r.get("Column_name");
            let non_unique: i64 = r.try_get::<i64, _>("Non_unique").unwrap_or(1);
            let idx_type: String = r.try_get::<String, _>("Index_type").unwrap_or_default();

            let entry = idx_map.entry(idx_name.clone()).or_insert_with(|| IndexInfo {
                name: idx_name.clone(),
                columns: Vec::new(),
                is_unique: non_unique == 0,
                is_primary: idx_name == "PRIMARY",
                index_type: idx_type,
            });
            entry.columns.push(col_name);
        }

        let mut indexes: Vec<IndexInfo> = idx_map.into_values().collect();
        indexes.sort_by(|a, b| {
            b.is_primary.cmp(&a.is_primary).then(a.name.cmp(&b.name))
        });

        // ── foreign keys parsed from SHOW CREATE TABLE output ──
        let create_sql: String = create_row.try_get(1).unwrap_or_default();
        let foreign_keys = Self::parse_fk_from_create_table(&create_sql);

        tracing::info!(%table, cols = columns.len(), indexes = indexes.len(), fks = foreign_keys.len(),
            total_ms = t0.elapsed().as_millis() as u64, "mysql get_table_schema: complete");

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
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let start = Instant::now();
        let rows = sqlx::query(sql)
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

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let statements = split_mysql_statements(sql);
        if statements.is_empty() {
            return Ok(MultiQueryResult {
                results: Vec::new(),
                total_time_ms: 0,
            });
        }

        let total_start = Instant::now();
        let mut results = Vec::with_capacity(statements.len());

        for stmt in &statements {
            let (effective_sql, applied_limit) = apply_mysql_select_limit(stmt, limit);
            let trimmed_upper = effective_sql.trim().to_ascii_uppercase();
            let is_query = trimmed_upper.starts_with("SELECT")
                || trimmed_upper.starts_with("WITH")
                || trimmed_upper.starts_with("SHOW")
                || trimmed_upper.starts_with("DESCRIBE")
                || trimmed_upper.starts_with("DESC")
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
        _handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        Err(DriverError::TransactionError(
            "Transactions not yet implemented".into(),
        ))
    }

    async fn commit(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        Err(DriverError::TransactionError(
            "Transactions not yet implemented".into(),
        ))
    }

    async fn rollback(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        Err(DriverError::TransactionError(
            "Transactions not yet implemented".into(),
        ))
    }

    async fn explain(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let explain_sql = format!("EXPLAIN {sql}");
        let rows = sqlx::query(&explain_sql)
            .fetch_all(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let (columns, result_rows) = Self::decode_rows(&rows);
        let plan_lines: Vec<String> = if columns.is_empty() {
            Vec::new()
        } else {
            result_rows
                .iter()
                .map(|row| {
                    row.iter()
                        .zip(columns.iter())
                        .map(|(val, col)| {
                            let v = match val {
                                Some(Value::String(s)) => s.clone(),
                                Some(Value::Integer(n)) => n.to_string(),
                                Some(Value::Float(f)) => f.to_string(),
                                Some(Value::Bool(b)) => b.to_string(),
                                _ => "NULL".to_string(),
                            };
                            format!("{}: {}", col.name, v)
                        })
                        .collect::<Vec<_>>()
                        .join(" | ")
                })
                .collect()
        };

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

/// Split SQL into statements, respecting strings, comments, and backtick identifiers.
fn split_mysql_statements(input: &str) -> Vec<String> {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut stmts: Vec<String> = Vec::new();
    let mut start = 0;
    let mut i = 0;

    while i < len {
        match bytes[i] {
            b'\'' => {
                i += 1;
                while i < len {
                    if bytes[i] == b'\'' {
                        i += 1;
                        if i < len && bytes[i] == b'\'' {
                            i += 1;
                        } else {
                            break;
                        }
                    } else if bytes[i] == b'\\' {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            b'"' => {
                i += 1;
                while i < len {
                    if bytes[i] == b'"' {
                        i += 1;
                        if i < len && bytes[i] == b'"' {
                            i += 1;
                        } else {
                            break;
                        }
                    } else if bytes[i] == b'\\' {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            b'`' => {
                i += 1;
                while i < len {
                    if bytes[i] == b'`' {
                        i += 1;
                        if i < len && bytes[i] == b'`' {
                            i += 1;
                        } else {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            b'-' if i + 1 < len && bytes[i + 1] == b'-' => {
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'#' => {
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < len {
                    if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
            }
            b';' => {
                let fragment = input[start..i].trim();
                if !fragment.is_empty() {
                    stmts.push(fragment.to_string());
                }
                i += 1;
                start = i;
            }
            _ => {
                i += 1;
            }
        }
    }

    let tail = input[start..].trim();
    if !tail.is_empty() {
        stmts.push(tail.to_string());
    }
    stmts
}

fn apply_mysql_select_limit(stmt: &str, limit: Option<u32>) -> (String, Option<u32>) {
    let Some(lim) = limit else {
        return (stmt.to_string(), None);
    };

    let trimmed = stmt.trim();
    let upper = trimmed.to_ascii_uppercase();
    let is_select = upper.starts_with("SELECT") || upper.starts_with("WITH");
    if !is_select {
        return (stmt.to_string(), None);
    }

    if upper.contains(" LIMIT ") {
        return (stmt.to_string(), None);
    }

    let effective = format!("{} LIMIT {}", trimmed, lim + 1);
    (effective, Some(lim))
}

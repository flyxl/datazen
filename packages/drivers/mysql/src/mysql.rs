//! MySQL / MariaDB driver backed by sqlx MySqlPool.

#[path = "catalog.rs"]
mod catalog;
#[path = "connection.rs"]
mod connection;
#[path = "execution.rs"]
mod execution;
#[path = "sql.rs"]
mod sql;
#[path = "type_decode.rs"]
mod type_decode;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

use crate::structure;
use async_trait::async_trait;
use catalog::{
    dump_mysql_routines, dump_mysql_triggers, extract_named_create_column,
    extract_show_create_table,
};
use connection::build_mysql_options;
use datazen_driver_api::*;
use execution::{build_kill_query_sql, MysqlQueryExecution};
use sql::{apply_mysql_select_limit, split_mysql_statements};
use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use sqlx::pool::PoolConnection;
use sqlx::{MySql, Row};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

pub(crate) use type_decode::{decode_mysql_text, decode_mysql_text_idx, decode_mysql_text_opt};

pub struct MysqlDriver {
    pub(crate) pools: RwLock<HashMap<String, MySqlPool>>,
    /// Active schema selected via `use_database` (or connect config), keyed by pool_id.
    /// Applied with `USE` on each acquired connection so pooled sessions stay consistent.
    pub(crate) active_databases: RwLock<HashMap<String, String>>,
    /// Open transactions: connection held for the lifetime of BEGIN…COMMIT/ROLLBACK, keyed by handle.id.
    pub(crate) transactions: Mutex<HashMap<String, PoolConnection<MySql>>>,
    /// Exact execution target registry. The MySQL thread id never leaves the
    /// driver; the host/UI only sees QueryExecutionId.
    pub(crate) query_executions: Mutex<HashMap<QueryExecutionId, MysqlQueryExecution>>,
    /// Independent control connections used only for KILL QUERY.
    pub(crate) control_pools: RwLock<HashMap<String, MySqlPool>>,
    is_mariadb: bool,
}

impl MysqlDriver {
    pub fn new(is_mariadb: bool) -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
            active_databases: RwLock::new(HashMap::new()),
            transactions: Mutex::new(HashMap::new()),
            query_executions: Mutex::new(HashMap::new()),
            control_pools: RwLock::new(HashMap::new()),
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

    async fn fetch_columns_with_db<'e, E>(
        executor: E,
        current_db: &str,
        table: &str,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError>
    where
        E: sqlx::Executor<'e, Database = sqlx::MySql>,
    {
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
        .fetch_all(executor)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let mut pk_names: Vec<String> = Vec::new();
        let columns: Vec<ColumnSchema> = cols
            .iter()
            .map(|r| {
                let name = decode_mysql_text(r, "COLUMN_NAME");
                let nullable = decode_mysql_text(r, "IS_NULLABLE");
                let key = decode_mysql_text(r, "COLUMN_KEY");
                let extra = decode_mysql_text(r, "EXTRA");
                let is_pk = key == "PRI";
                if is_pk {
                    pk_names.push(name.clone());
                }
                ColumnSchema {
                    is_primary_key: is_pk,
                    name,
                    data_type: decode_mysql_text(r, "COLUMN_TYPE"),
                    nullable: nullable == "YES",
                    default_value: r.try_get("COLUMN_DEFAULT").ok(),
                    comment: {
                        let s = decode_mysql_text(r, "COLUMN_COMMENT");
                        if s.is_empty() {
                            None
                        } else {
                            Some(s)
                        }
                    },
                    is_auto_increment: extra.contains("auto_increment"),
                }
            })
            .collect();

        Ok((columns, pk_names))
    }

    pub(crate) fn quote_identifier(name: &str) -> String {
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
            let ref_cols = Self::extract_backtick_list_after(
                trimmed,
                &format!("REFERENCES `{}`", ref_table.replace('`', "``")),
            );

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
            let rule = after
                .split(|c: char| c == ',' || c == ')' || c == '\n')
                .next()
                .unwrap_or("")
                .trim();
            if !rule.is_empty() {
                return rule.to_uppercase();
            }
        }
        "RESTRICT".to_string()
    }
    fn extract_mysql_plan_metrics(plan_json: &serde_json::Value) -> (Option<f64>, Option<i64>) {
        let query_cost = plan_json
            .get("query_block")
            .and_then(|block| block.get("cost_info"))
            .and_then(|info| info.get("query_cost"))
            .and_then(|value| {
                value
                    .as_f64()
                    .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
            });
        let estimated_rows = plan_json
            .get("query_block")
            .and_then(|block| block.get("table"))
            .and_then(|table| table.get("rows_examined_per_scan"))
            .and_then(|value| {
                value
                    .as_i64()
                    .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
            });
        (query_cost, estimated_rows)
    }

    fn extract_json_from_explain_row(rows: &[sqlx::mysql::MySqlRow]) -> Option<serde_json::Value> {
        rows.first().and_then(|row| {
            row.try_get::<serde_json::Value, _>(0).ok().or_else(|| {
                row.try_get::<String, _>(0)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
            })
        })
    }
}

#[async_trait]
impl DatabaseDriver for MysqlDriver {
    fn migration_renderer(
        &self,
    ) -> Option<std::sync::Arc<dyn datazen_driver_api::MigrationRenderer>> {
        Some(std::sync::Arc::new(super::MysqlMigrationRenderer))
    }

    fn migration_capabilities(
        &self,
    ) -> Option<std::sync::Arc<dyn datazen_driver_api::MigrationCapabilities>> {
        Some(std::sync::Arc::new(super::MysqlMigrationCapabilities))
    }

    fn type_normalizer(&self) -> Option<std::sync::Arc<dyn datazen_driver_api::TypeNormalizer>> {
        Some(std::sync::Arc::new(super::MysqlTypeNormalizer))
    }
    fn driver_type(&self) -> DatabaseType {
        if self.is_mariadb {
            "mariadb".to_string()
        } else {
            "mysql".to_string()
        }
    }

    /// F7: qualify unqualified table references with the target database
    /// (`` `db`.`t` ``), shared by the mysql/mariadb/doris/starrocks/
    /// manticore/ob_oracle variants. Parse failures pass SQL through
    /// unchanged; see `sql_target::qualify_sql`.
    fn qualify_sql_target(
        &self,
        sql: &str,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Option<String> {
        Some(crate::sql_target::qualify_sql(sql, database, schema))
    }

    fn quote_char(&self) -> char {
        '`'
    }

    fn format_sql_literal(&self, value: &Option<super::Value>) -> String {
        match value {
            None | Some(super::Value::Null) => "NULL".to_string(),
            Some(super::Value::Bool(b)) => {
                if *b {
                    "1".to_string()
                } else {
                    "0".to_string()
                }
            }
            Some(super::Value::Integer(i)) => i.to_string(),
            Some(super::Value::Float(f)) => f.to_string(),
            Some(super::Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(super::Value::Bytes(b)) => {
                format!("'{}'", String::from_utf8_lossy(b).replace('\'', "''"))
            }
            Some(super::Value::Timestamp(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(super::Value::Json(j)) => format!("'{}'", j.to_string().replace('\'', "''")),
        }
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let opts = build_mysql_options(config)?;
        let timeout = Duration::from_secs(config.connection_timeout as u64);

        let pool = MySqlPoolOptions::new()
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
        let opts = build_mysql_options(config)?;
        let timeout = Duration::from_secs(config.connection_timeout as u64);

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
        let control_pool = match Self::open_pool(build_mysql_options(config)?, timeout, 1, 0).await
        {
            Ok(p) => p,
            Err(e) => {
                pool.close().await;
                return Err(e);
            }
        };

        if let Some(db) = config
            .database
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            self.active_databases
                .write()
                .await
                .insert(pool_id.clone(), db.to_string());
        }

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

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        if let Some(mut conn) = self.transactions.lock().await.remove(&handle.id) {
            let _ = Self::execute_text_on_conn(&mut conn, "ROLLBACK").await;
        }
        self.active_databases.write().await.remove(&handle.pool_id);
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

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let rows = sqlx::query("SHOW DATABASES")
            .fetch_all(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        Ok(rows.iter().map(|r| decode_mysql_text_idx(r, 0)).collect())
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
                let tt = decode_mysql_text(r, "TABLE_TYPE");
                TableInfo {
                    schema: None,
                    name: decode_mysql_text(r, "TABLE_NAME"),
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
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        self.apply_active_database(handle, &mut conn).await?;

        let current_db = {
            let tracked = self.active_databases.read().await;
            tracked.get(&handle.pool_id).cloned()
        };
        let current_db = match current_db {
            Some(db) if !db.is_empty() => db,
            _ => {
                let row = sqlx::query("SELECT DATABASE()")
                    .fetch_one(&mut *conn)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                row.try_get::<String, _>(0).unwrap_or_default()
            }
        };
        Self::fetch_columns_with_db(&mut *conn, &current_db, table).await
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        let t0 = std::time::Instant::now();
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        self.apply_active_database(handle, &mut conn).await?;

        let q = Self::quote_identifier(table);

        // Sequential on one connection so USE (active database) applies to all SHOW calls.
        let col_rows = sqlx::query(&format!("SHOW FULL COLUMNS FROM {}", q))
            .fetch_all(&mut *conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let idx_rows = sqlx::query(&format!("SHOW INDEX FROM {}", q))
            .fetch_all(&mut *conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let create_row = sqlx::query(&format!("SHOW CREATE TABLE {}", q))
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        tracing::info!(%table, col_rows = col_rows.len(), idx_rows = idx_rows.len(),
            ms = t0.elapsed().as_millis() as u64,
            "mysql get_table_schema: SHOW queries done");

        // ── columns from SHOW FULL COLUMNS ──
        let mut pk_names: Vec<String> = Vec::new();
        let columns: Vec<ColumnSchema> = col_rows
            .iter()
            .map(|r| {
                let name = decode_mysql_text(r, "Field");
                let col_type = decode_mysql_text(r, "Type");
                let nullable = decode_mysql_text(r, "Null");
                let key = decode_mysql_text(r, "Key");
                let extra = decode_mysql_text(r, "Extra");
                let comment: Option<String> = {
                    let s = decode_mysql_text(r, "Comment");
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                };
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
            let idx_name = decode_mysql_text(r, "Key_name");
            let col_name = decode_mysql_text(r, "Column_name");
            let non_unique: i64 = r.try_get::<i64, _>("Non_unique").unwrap_or(1);
            let idx_type = decode_mysql_text(r, "Index_type");

            let entry = idx_map
                .entry(idx_name.clone())
                .or_insert_with(|| IndexInfo {
                    name: idx_name.clone(),
                    columns: Vec::new(),
                    is_unique: non_unique == 0,
                    is_primary: idx_name == "PRIMARY",
                    index_type: idx_type,
                });
            entry.columns.push(col_name);
        }

        let mut indexes: Vec<IndexInfo> = idx_map.into_values().collect();
        indexes.sort_by(|a, b| b.is_primary.cmp(&a.is_primary).then(a.name.cmp(&b.name)));

        // ── foreign keys parsed from SHOW CREATE TABLE output ──
        let create_sql = decode_mysql_text_idx(&create_row, 1);
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
        {
            let mut txs = self.transactions.lock().await;
            if let Some(conn) = txs.get_mut(&handle.id) {
                let start = Instant::now();
                let rows = sqlx::query(sql)
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
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        self.apply_active_database(handle, &mut conn).await?;

        let start = Instant::now();
        let rows = sqlx::query(sql)
            .fetch_all(&mut *conn)
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
        let statements = split_mysql_statements(sql);
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
                    let result = Self::execute_text_on_conn(conn, effective_sql.as_str())
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
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        self.apply_active_database(handle, &mut conn).await?;

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
                    .fetch_all(&mut *conn)
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
                let result = Self::execute_text_on_conn(&mut conn, effective_sql.as_str())
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
        let statements = split_mysql_statements(sql);
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
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        self.apply_active_database(handle, &mut conn).await?;
        for (index, stmt) in statements.iter().enumerate() {
            Self::stream_one_statement(&mut *conn, stmt, limit, index, &on_event).await?;
        }
        on_event(QueryStreamEvent::Done {
            total_time_ms: total_start.elapsed().as_millis() as u64,
        });
        Ok(())
    }

    async fn prepare_query_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        let transactional = self.transactions.lock().await.contains_key(&handle.id);
        let target_pool = self.pools.read().await.get(&handle.pool_id).cloned();
        let control_pool = self
            .control_pools
            .read()
            .await
            .get(&handle.pool_id)
            .cloned();
        let mut executions = self.query_executions.lock().await;
        if executions.contains_key(execution_id) {
            return Err(DriverError::QueryFailed(format!(
                "query execution '{}' is already registered",
                execution_id.as_str()
            )));
        }
        executions.insert(
            execution_id.clone(),
            MysqlQueryExecution {
                session_id: handle.id.clone(),
                target_pool,
                control_pool,
                thread_id: None,
                cancel_requested: false,
                transactional,
            },
        );
        Ok(())
    }

    async fn query_stream_with_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        let statements = split_mysql_statements(sql);
        if statements.is_empty() {
            self.finish_query_execution(handle, execution_id).await?;
            on_event(QueryStreamEvent::Done { total_time_ms: 0 });
            return Ok(());
        }
        self.stream_registered_execution(handle, execution_id, &statements, limit, &on_event)
            .await
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
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        self.apply_active_database(handle, &mut conn).await?;

        let start = Instant::now();
        let rows = Self::bind_values(sqlx::query(sql), params)
            .fetch_all(&mut *conn)
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
                let result = Self::execute_text_on_conn(conn, sql)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                return Ok(result.rows_affected());
            }
        }

        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        self.apply_active_database(handle, &mut conn).await?;

        let result = Self::execute_text_on_conn(&mut conn, sql)
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
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        drop(pools);

        self.apply_active_database(handle, &mut conn).await?;

        Self::execute_text_on_conn(&mut conn, "BEGIN")
            .await
            .map_err(|e| DriverError::TransactionError(e.to_string()))?;

        txs.insert(handle.id.clone(), conn);
        Ok(TransactionHandle {
            id: format!("mysql_tx_{}", uuid::Uuid::new_v4()),
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

        Self::execute_text_on_conn(&mut conn, "COMMIT")
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

        Self::execute_text_on_conn(&mut conn, "ROLLBACK")
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
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        self.apply_active_database(handle, &mut conn).await?;

        let json_sql = format!("EXPLAIN FORMAT=JSON {sql}");
        let json_rows = sqlx::query(&json_sql)
            .fetch_all(&mut *conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let plan_json = Self::extract_json_from_explain_row(&json_rows);

        let text_sql = format!("EXPLAIN {sql}");
        let rows = sqlx::query(&text_sql)
            .fetch_all(&mut *conn)
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
                                Some(Value::Json(j)) => j.to_string(),
                                _ => "NULL".to_string(),
                            };
                            format!("{}: {}", col.name, v)
                        })
                        .collect::<Vec<_>>()
                        .join(" | ")
                })
                .collect()
        };

        let (total_cost, estimated_rows) = plan_json
            .as_ref()
            .map(Self::extract_mysql_plan_metrics)
            .unwrap_or((None, None));

        let plan_tree = plan_json
            .as_ref()
            .and_then(datazen_driver_api::normalize_mysql_explain_plan);

        // Carry the raw classic EXPLAIN result set (columns + rows) so the Host
        // can render it as a DataTable, same as running EXPLAIN in the editor.
        let mut plan_json = plan_json.unwrap_or_else(|| serde_json::json!({}));
        if let Some(obj) = plan_json.as_object_mut() {
            obj.insert(
                "columns".to_string(),
                serde_json::json!(columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>()),
            );
            obj.insert("rows".to_string(), serde_json::json!(result_rows));
        }

        Ok(ExplainResult {
            plan_text: plan_lines.join("\n"),
            plan_json: Some(plan_json),
            plan_tree,
            total_cost,
            estimated_rows,
        })
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Err(DriverError::Unsupported(
            "legacy session-wide query cancellation is disabled; use an execution handle".into(),
        ))
    }

    async fn cancel_query_with_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        // Keep the registry entry locked until KILL QUERY completes. A MySQL
        // thread id belongs to a pooled connection and can be reused; early
        // cleanup could otherwise let a delayed cancel hit a later query.
        let mut executions = self.query_executions.lock().await;
        let (thread_id, control_pool) = {
            let execution = executions.get_mut(execution_id).ok_or_else(|| {
                DriverError::QueryExecutionNotFound(execution_id.as_str().to_string())
            })?;
            if execution.session_id != handle.id {
                return Err(DriverError::QueryExecutionSessionMismatch);
            }
            execution.cancel_requested = true;
            let Some(thread_id) = execution.thread_id else {
                // The stream observes this pending request after acquiring its
                // dedicated connection and before executing user SQL.
                return Ok(());
            };
            (thread_id, execution.control_pool.clone())
        };

        let Some(control_pool) = control_pool else {
            if let Some(execution) = executions.get_mut(execution_id) {
                execution.cancel_requested = false;
            }
            return Err(DriverError::ConnectionFailed(
                "Control pool not found".into(),
            ));
        };
        let mut conn = match control_pool.acquire().await {
            Ok(conn) => conn,
            Err(error) => {
                if let Some(execution) = executions.get_mut(execution_id) {
                    execution.cancel_requested = false;
                }
                return Err(DriverError::ConnectionFailed(error.to_string()));
            }
        };
        let kill_sql = build_kill_query_sql(thread_id);
        match Self::execute_text_on_conn(&mut conn, &kill_sql).await {
            Ok(_) => Ok(()),
            Err(error) => {
                if let Some(execution) = executions.get_mut(execution_id) {
                    execution.cancel_requested = false;
                }
                Err(DriverError::QueryFailed(error.to_string()))
            }
        }
    }

    async fn cleanup_query_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.finish_query_execution(handle, execution_id).await
    }

    fn supports_query_execution_cancel(&self) -> bool {
        true
    }

    // Switch active schema for unqualified names (session + pool-safe USE).

    async fn use_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<(), DriverError> {
        let use_sql = Self::build_use_database_sql(database)?;
        let trimmed = database.trim().to_string();

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

        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let current = Self::current_database(pool).await?;
        if current == trimmed {
            drop(pools);
            self.active_databases
                .write()
                .await
                .insert(handle.pool_id.clone(), trimmed);
            return Ok(());
        }

        // Fail fast if the database does not exist or is inaccessible.
        // Must use text protocol — prepared statements reject USE (MySQL 1295).
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        Self::execute_use_on_conn(&mut conn, &use_sql, &trimmed).await?;
        drop(conn);

        drop(pools);

        self.active_databases
            .write()
            .await
            .insert(handle.pool_id.clone(), trimmed);
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
        let server_type = if self.is_mariadb || version.to_lowercase().contains("mariadb") {
            "MariaDB"
        } else {
            "MySQL"
        };
        Ok(ServerInfo {
            server_version: version,
            server_type: server_type.to_string(),
        })
    }

    async fn dump_table_ddl(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<String, DriverError> {
        let sql = format!("SHOW CREATE TABLE {}", self.quote_ident(table));
        match self.query(handle, &sql).await {
            Ok(result) => {
                if let Some(create) = extract_show_create_table(&result) {
                    let mut ddl = create;
                    let trimmed = ddl.trim_end();
                    if !trimmed.ends_with(';') {
                        ddl = format!("{};\n", trimmed);
                    } else if !ddl.ends_with('\n') {
                        ddl.push('\n');
                    }
                    return Ok(ddl);
                }
                sql_dump::dump_table_ddl_from_schema(self, handle, table).await
            }
            Err(_) => sql_dump::dump_table_ddl_from_schema(self, handle, table).await,
        }
    }

    async fn dump_view_ddl(
        &self,
        handle: &ConnectionHandle,
        view: &str,
    ) -> Result<String, DriverError> {
        let sql = format!("SHOW CREATE VIEW {}", self.quote_ident(view));
        let result = self.query(handle, &sql).await?;
        let create = extract_named_create_column(&result, "Create View").or_else(|| {
            if result.columns.len() >= 2 {
                let row = result.rows.first()?;
                let cell = row.get(1)?.as_ref()?;
                match cell {
                    Value::String(s) if !s.is_empty() => Some(s.clone()),
                    _ => None,
                }
            } else {
                None
            }
        });
        let Some(create) = create else {
            return Err(DriverError::QueryFailed(format!(
                "View definition not found: {view}"
            )));
        };
        let trimmed = create.trim_end();
        if trimmed.ends_with(';') {
            Ok(format!("{trimmed}\n"))
        } else {
            Ok(format!("{trimmed};\n"))
        }
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

    async fn dump_database_with_progress(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<String, DriverError> {
        let snapshot = if opts.single_transaction {
            self.begin_transaction(handle).await.ok()
        } else {
            None
        };
        let result = async {
            let mut out = String::new();
            if opts.create_database {
                let q = self.quote_ident(database);
                out.push_str(&format!(
                    "CREATE DATABASE IF NOT EXISTS {};\nUSE {};\n\n",
                    q, q
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

    async fn dump_routines(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<String, DriverError> {
        Ok(dump_mysql_routines(self, handle).await)
    }

    async fn dump_triggers(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<String, DriverError> {
        Ok(dump_mysql_triggers(self, handle).await)
    }

    async fn structure_capabilities(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<StructureCapabilities, DriverError> {
        Ok(structure::baseline_capabilities(&self.driver_type()))
    }

    async fn plan_structure_changes(
        &self,
        handle: &ConnectionHandle,
        request: &StructureChangeRequest,
    ) -> Result<StructureChangePlan, DriverError> {
        let caps = self.structure_capabilities(handle).await?;
        structure::plan_structure_changes(&caps, request)
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        crate::admin_commands::mysql_admin_command_definitions()
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
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        crate::admin_commands::execute_mysql_admin_command(pool, command, input).await
    }
}

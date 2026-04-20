//! PostgreSQL driver backed by sqlx PgPool.

use super::*;
use async_trait::async_trait;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Column, PgPool, Row};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub struct PostgresDriver {
    pools: RwLock<HashMap<String, PgPool>>,
}

impl PostgresDriver {
    pub fn new() -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
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

    fn decode_rows(rows: &[sqlx::postgres::PgRow]) -> (Vec<ColumnInfo>, Vec<Vec<Option<Value>>>) {
        let columns: Vec<ColumnInfo> = if let Some(first) = rows.first() {
            first
                .columns()
                .iter()
                .map(|c| ColumnInfo {
                    name: c.name().to_string(),
                    data_type: format!("{:?}", c.type_info()),
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
                        let type_name = format!("{:?}", col.type_info());
                        match type_name.to_uppercase().as_str() {
                            s if s.contains("INT8") || s.contains("BIGINT") => {
                                row.try_get::<i64, _>(i).ok().map(Value::Integer)
                            }
                            s if s.contains("INT") => {
                                row.try_get::<i32, _>(i)
                                    .ok()
                                    .map(|v| Value::Integer(v as i64))
                                    .or_else(|| row.try_get::<i64, _>(i).ok().map(Value::Integer))
                                    .or_else(|| row.try_get::<i16, _>(i).ok().map(|v| Value::Integer(v as i64)))
                            }
                            s if s.contains("FLOAT")
                                || s.contains("DOUBLE")
                                || s.contains("NUMERIC")
                                || s.contains("REAL") =>
                            {
                                row.try_get::<f64, _>(i)
                                    .ok()
                                    .or_else(|| row.try_get::<f32, _>(i).ok().map(|v| v as f64))
                                    .map(Value::Float)
                            }
                            s if s.contains("BOOL") => {
                                row.try_get::<bool, _>(i).ok().map(Value::Bool)
                            }
                            _ => row.try_get::<String, _>(i).ok().map(Value::String),
                        }
                    })
                    .collect()
            })
            .collect();

        (columns, result_rows)
    }
}

fn build_pg_options(
    config: &ConnectionConfig,
) -> Result<sqlx::postgres::PgConnectOptions, DriverError> {
    use sqlx::ConnectOptions;
    let mut opts = sqlx::postgres::PgConnectOptions::new()
        .host(config.host.as_deref().unwrap_or("localhost"))
        .port(config.port.unwrap_or(5432))
        .database(config.database.as_deref().unwrap_or("postgres"));

    if let Some(username) = &config.username {
        opts = opts.username(username);
    }
    if let Some(password) = &config.password {
        opts = opts.password(password);
    }

    opts = opts.log_statements(tracing::log::LevelFilter::Debug);
    Ok(opts)
}

#[async_trait]
impl DatabaseDriver for PostgresDriver {
    fn driver_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
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

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(timeout)
            .idle_timeout(Duration::from_secs(600))
            .connect_with(opts)
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
        _database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

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
                   col_description((table_schema||'.'||table_name)::regclass, ordinal_position) as comment
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
            WHERE i.indrelid = $1::regclass AND i.indisprimary
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
            WHERE ix.indrelid = $1::regclass
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

        let statements = split_sql_statements(sql);
        if statements.is_empty() {
            return Ok(MultiQueryResult {
                results: Vec::new(),
                total_time_ms: 0,
            });
        }

        let total_start = Instant::now();
        let mut results = Vec::with_capacity(statements.len());

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

        let explain_sql = format!("EXPLAIN (FORMAT TEXT) {sql}");
        let rows = sqlx::query(&explain_sql)
            .fetch_all(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let plan: String = rows
            .iter()
            .map(|r| r.get::<String, _>(0))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ExplainResult {
            plan_text: plan,
            plan_json: None,
            total_cost: None,
            estimated_rows: None,
        })
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

/// Split a SQL text into individual statements, respecting single-quoted strings,
/// double-quoted identifiers, dollar-quoted strings, `--` line comments, and `/* */`
/// block comments so that semicolons inside those constructs are not treated as
/// statement terminators.
fn split_sql_statements(input: &str) -> Vec<String> {
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
                            i += 1; // escaped ''
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
                while i < len {
                    if bytes[i] == b'"' {
                        i += 1;
                        if i < len && bytes[i] == b'"' {
                            i += 1;
                        } else {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            b'$' => {
                if let Some(tag_end) = find_dollar_tag(bytes, i) {
                    let tag = &input[i..tag_end];
                    i = tag_end;
                    loop {
                        if i >= len {
                            break;
                        }
                        if bytes[i] == b'$' && input[i..].starts_with(tag) {
                            i += tag.len();
                            break;
                        }
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            b'-' if i + 1 < len && bytes[i + 1] == b'-' => {
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                i += 2;
                let mut depth = 1u32;
                while i + 1 < len && depth > 0 {
                    if bytes[i] == b'/' && bytes[i + 1] == b'*' {
                        depth += 1;
                        i += 2;
                    } else if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
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

/// If the statement is a SELECT without an existing LIMIT clause, returns a
/// modified SQL with `LIMIT limit+1` appended (the extra row lets us detect
/// truncation) and `Some(limit)` as the applied cap.  Otherwise returns the
/// original statement unchanged and `None`.
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
        return (stmt.to_string(), None);
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
                if i < len { i += 1; }
            }
            b'$' => {
                if let Some(tag_end) = find_dollar_tag(bytes, i) {
                    let tag = &sql[i..tag_end];
                    i = tag_end;
                    loop {
                        if i >= len { break; }
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
                        cd += 1; i += 2;
                    } else if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        cd -= 1; i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            b'(' => { depth += 1; i += 1; }
            b')' => { depth -= 1; i += 1; }
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
            _ => { i += 1; }
        }
    }

    false
}

/// Try to match a `$tag$` dollar-quote opener starting at position `pos`.
/// Returns `Some(end)` where `end` is the byte index past the closing `$`.
fn find_dollar_tag(bytes: &[u8], pos: usize) -> Option<usize> {
    if pos >= bytes.len() || bytes[pos] != b'$' {
        return None;
    }
    let mut j = pos + 1;
    while j < bytes.len() {
        if bytes[j] == b'$' {
            return Some(j + 1);
        }
        if !bytes[j].is_ascii_alphanumeric() && bytes[j] != b'_' {
            return None;
        }
        j += 1;
    }
    None
}

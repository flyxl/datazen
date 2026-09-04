//! Query execution registration, streaming, and backend PID cancellation.

use crate::postgres::PostgresDriver;
use crate::sql::{apply_select_limit, is_pg_result_query};
use datazen_driver_api::*;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use std::time::Instant;

pub(crate) const PG_BACKEND_PID_SQL: &str = "SELECT pg_backend_pid()";
pub(crate) const PG_CANCEL_BACKEND_SQL: &str = "SELECT pg_cancel_backend($1)";

pub(crate) struct PgQueryExecution {
    pub session_id: String,
    pub target_pool: Option<PgPool>,
    pub control_pool: Option<PgPool>,
    pub backend_pid: Option<i32>,
    pub cancel_requested: bool,
    pub transactional: bool,
}

impl PostgresDriver {
    pub(crate) async fn is_cancel_requested(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<bool, DriverError> {
        let executions = self.query_executions.lock().await;
        let execution = executions.get(execution_id).ok_or_else(|| {
            DriverError::QueryExecutionNotFound(execution_id.as_str().to_string())
        })?;
        if execution.session_id != handle.id {
            return Err(DriverError::QueryExecutionSessionMismatch);
        }
        Ok(execution.cancel_requested)
    }

    pub(crate) async fn bind_backend_pid(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
        backend_pid: i32,
    ) -> Result<bool, DriverError> {
        let mut executions = self.query_executions.lock().await;
        let execution = executions.get_mut(execution_id).ok_or_else(|| {
            DriverError::QueryExecutionNotFound(execution_id.as_str().to_string())
        })?;
        if execution.session_id != handle.id {
            return Err(DriverError::QueryExecutionSessionMismatch);
        }
        execution.backend_pid = Some(backend_pid);
        Ok(execution.cancel_requested)
    }

    pub(crate) async fn finish_query_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        let mut executions = self.query_executions.lock().await;
        if let Some(execution) = executions.get(execution_id) {
            if execution.session_id != handle.id {
                return Err(DriverError::QueryExecutionSessionMismatch);
            }
        }
        executions.remove(execution_id);
        Ok(())
    }

    pub(crate) async fn stream_registered_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
        statements: &[String],
        limit: Option<u32>,
        on_event: &QueryStreamCallback,
    ) -> Result<(), DriverError> {
        let (transactional, target_pool) = {
            let executions = self.query_executions.lock().await;
            let execution = executions.get(execution_id).ok_or_else(|| {
                DriverError::QueryExecutionNotFound(execution_id.as_str().to_string())
            })?;
            if execution.session_id != handle.id {
                return Err(DriverError::QueryExecutionSessionMismatch);
            }
            (execution.transactional, execution.target_pool.clone())
        };

        let result: Result<(), DriverError> = async {
            if transactional {
                let mut txs = self.transactions.lock().await;
                let conn = txs.get_mut(&handle.id).ok_or_else(|| {
                    DriverError::Unsupported(
                        "transaction execution connection is no longer available".into(),
                    )
                })?;
                if self.is_cancel_requested(handle, execution_id).await? {
                    Err(DriverError::QueryCancelled)
                } else {
                    let pid_row = sqlx::query(PG_BACKEND_PID_SQL)
                        .fetch_one(&mut **conn)
                        .await
                        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                    let backend_pid = pid_row
                        .try_get::<i32, _>(0)
                        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                    if self
                        .bind_backend_pid(handle, execution_id, backend_pid)
                        .await?
                    {
                        Err(DriverError::QueryCancelled)
                    } else {
                        let total_start = Instant::now();
                        let mut result = Ok(());
                        for (index, stmt) in statements.iter().enumerate() {
                            if self.is_cancel_requested(handle, execution_id).await? {
                                result = Err(DriverError::QueryCancelled);
                                break;
                            }
                            if let Err(error) = Self::stream_one_statement(
                                &mut **conn,
                                stmt,
                                limit,
                                index,
                                on_event,
                            )
                            .await
                            {
                                result = if self
                                    .is_cancel_requested(handle, execution_id)
                                    .await
                                    .unwrap_or(false)
                                {
                                    Err(DriverError::QueryCancelled)
                                } else {
                                    Err(error)
                                };
                                break;
                            }
                        }
                        if result.is_ok() {
                            on_event(QueryStreamEvent::Done {
                                total_time_ms: total_start.elapsed().as_millis() as u64,
                            });
                        }
                        result
                    }
                }
            } else {
                let pool = target_pool.ok_or_else(|| {
                    DriverError::ConnectionFailed(
                        "Connection pool was not available when query execution was registered"
                            .into(),
                    )
                })?;
                let mut conn = pool
                    .acquire()
                    .await
                    .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;

                if self.is_cancel_requested(handle, execution_id).await? {
                    Err(DriverError::QueryCancelled)
                } else {
                    let pid_row = sqlx::query(PG_BACKEND_PID_SQL)
                        .fetch_one(&mut *conn)
                        .await
                        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                    let backend_pid = pid_row
                        .try_get::<i32, _>(0)
                        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                    if self
                        .bind_backend_pid(handle, execution_id, backend_pid)
                        .await?
                    {
                        Err(DriverError::QueryCancelled)
                    } else {
                        let total_start = Instant::now();
                        let mut result = Ok(());
                        for (index, stmt) in statements.iter().enumerate() {
                            if self.is_cancel_requested(handle, execution_id).await? {
                                result = Err(DriverError::QueryCancelled);
                                break;
                            }
                            if let Err(error) =
                                Self::stream_one_statement(&mut *conn, stmt, limit, index, on_event)
                                    .await
                            {
                                result = if self
                                    .is_cancel_requested(handle, execution_id)
                                    .await
                                    .unwrap_or(false)
                                {
                                    Err(DriverError::QueryCancelled)
                                } else {
                                    Err(error)
                                };
                                break;
                            }
                        }
                        if result.is_ok() {
                            on_event(QueryStreamEvent::Done {
                                total_time_ms: total_start.elapsed().as_millis() as u64,
                            });
                        }
                        result
                    }
                }
            }
        }
        .await;

        let cleanup = self.finish_query_execution(handle, execution_id).await;
        result.and(cleanup)
    }

    pub(crate) async fn stream_one_statement<'e, E>(
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

    pub(crate) async fn prepare_query_execution_impl(
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
            PgQueryExecution {
                session_id: handle.id.clone(),
                target_pool,
                control_pool,
                backend_pid: None,
                cancel_requested: false,
                transactional,
            },
        );
        Ok(())
    }

    pub(crate) async fn cancel_query_with_execution_impl(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        let mut executions = self.query_executions.lock().await;
        let (backend_pid, control_pool) = {
            let execution = executions.get_mut(execution_id).ok_or_else(|| {
                DriverError::QueryExecutionNotFound(execution_id.as_str().to_string())
            })?;
            if execution.session_id != handle.id {
                return Err(DriverError::QueryExecutionSessionMismatch);
            }
            execution.cancel_requested = true;
            let Some(backend_pid) = execution.backend_pid else {
                return Ok(());
            };
            (backend_pid, execution.control_pool.clone())
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
        let cancelled: bool = match sqlx::query(PG_CANCEL_BACKEND_SQL)
            .bind(backend_pid)
            .fetch_one(&mut *conn)
            .await
        {
            Ok(row) => match row.try_get(0) {
                Ok(cancelled) => cancelled,
                Err(error) => {
                    if let Some(execution) = executions.get_mut(execution_id) {
                        execution.cancel_requested = false;
                    }
                    return Err(DriverError::QueryFailed(error.to_string()));
                }
            },
            Err(error) => {
                if let Some(execution) = executions.get_mut(execution_id) {
                    execution.cancel_requested = false;
                }
                return Err(DriverError::QueryFailed(error.to_string()));
            }
        };
        if cancelled {
            Ok(())
        } else {
            if let Some(execution) = executions.get_mut(execution_id) {
                execution.cancel_requested = false;
            }
            Err(DriverError::QueryExecutionNotFound(format!(
                "backend target {backend_pid} is no longer active"
            )))
        }
    }

pub(crate) async fn query_impl(
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

pub(crate) async fn query_multi_impl(
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

pub(crate) async fn query_stream_impl(
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

        let pool = {
            let pools = self.pools.read().await;
            Self::get_pool(&pools, handle)?.clone()
        };
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        for (index, stmt) in statements.iter().enumerate() {
            Self::stream_one_statement(&mut *conn, stmt, limit, index, &on_event).await?;
        }
        on_event(QueryStreamEvent::Done {
            total_time_ms: total_start.elapsed().as_millis() as u64,
        });
        Ok(())
    }

pub(crate) async fn query_with_params_impl(
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

pub(crate) async fn execute_impl(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
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

pub(crate) async fn begin_transaction_impl(
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

pub(crate) async fn commit_impl(&self, tx: TransactionHandle) -> Result<(), DriverError> {
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

pub(crate) async fn rollback_impl(&self, tx: TransactionHandle) -> Result<(), DriverError> {
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

pub(crate) async fn explain_impl(
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

        let plan_tree = plan_json
            .as_ref()
            .and_then(datazen_driver_api::normalize_postgres_explain_plan);

        Ok(ExplainResult {
            plan_text,
            plan_json,
            plan_tree,
            total_cost,
            estimated_rows,
        })
    }
}

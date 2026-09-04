//! Query execution registry, thread binding, and cancellation.

use super::sql::{apply_mysql_select_limit, is_mysql_result_query};
use datazen_driver_api::*;
use sqlx::{MySqlPool, Row};
use std::sync::Arc;
use std::time::Instant;

pub(crate) const MYSQL_CONNECTION_ID_SQL: &str = "SELECT CONNECTION_ID()";

pub(crate) struct MysqlQueryExecution {
    pub(crate) session_id: String,
    pub(crate) target_pool: Option<MySqlPool>,
    pub(crate) control_pool: Option<MySqlPool>,
    pub(crate) thread_id: Option<u64>,
    pub(crate) cancel_requested: bool,
    pub(crate) transactional: bool,
}

pub(crate) fn build_kill_query_sql(thread_id: u64) -> String {
    format!("KILL QUERY {thread_id}")
}

impl super::MysqlDriver {
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

    pub(crate) async fn bind_thread_id(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
        thread_id: u64,
    ) -> Result<bool, DriverError> {
        let mut executions = self.query_executions.lock().await;
        let execution = executions.get_mut(execution_id).ok_or_else(|| {
            DriverError::QueryExecutionNotFound(execution_id.as_str().to_string())
        })?;
        if execution.session_id != handle.id {
            return Err(DriverError::QueryExecutionSessionMismatch);
        }
        execution.thread_id = Some(thread_id);
        Ok(execution.cancel_requested)
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
                    let id_row = sqlx::query(MYSQL_CONNECTION_ID_SQL)
                        .fetch_one(&mut **conn)
                        .await
                        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                    let thread_id = id_row
                        .try_get::<u64, _>(0)
                        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                    if self.bind_thread_id(handle, execution_id, thread_id).await? {
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
                self.apply_active_database(handle, &mut conn).await?;

                if self.is_cancel_requested(handle, execution_id).await? {
                    Err(DriverError::QueryCancelled)
                } else {
                    let id_row = sqlx::query(MYSQL_CONNECTION_ID_SQL)
                        .fetch_one(&mut *conn)
                        .await
                        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                    let thread_id = id_row
                        .try_get::<u64, _>(0)
                        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                    if self.bind_thread_id(handle, execution_id, thread_id).await? {
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
        E: sqlx::Executor<'e, Database = sqlx::MySql>,
    {
        use futures_util::TryStreamExt;
        let (effective_sql, applied_limit) = apply_mysql_select_limit(stmt, limit);
        let stmt_start = Instant::now();
        if is_mysql_result_query(&effective_sql) {
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
            let result = sqlx::Executor::execute(executor, effective_sql.as_str())
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

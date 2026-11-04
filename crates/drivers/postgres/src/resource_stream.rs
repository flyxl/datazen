//! Awaitable SQLx row streaming on one physical resource. Completed
//! statements retain column metadata only; rows exist only in bounded batches.
use crate::sql::{apply_select_limit, is_pg_result_query};
use crate::PostgresDriver;
use datazen_driver_api::resource::{ResourceError, ResultChunk, ResultSink, StatementSummary};
use datazen_driver_api::*;
use futures_util::{StreamExt, TryStreamExt};
use sqlx::{Column, Executor, TypeInfo};

const BATCH_BYTES: usize = 1024 * 1024;
const BATCH_ROWS: usize = 256;

impl PostgresDriver {
    pub(crate) async fn stream_resource_query(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
        sql: &str,
        limit: Option<u32>,
        parameters: Option<&[Value]>,
        sink: &dyn ResultSink,
    ) -> Result<Vec<StatementResult>, ResourceError> {
        let pinned = self.transactions.lock().await.get(&handle.id).cloned();
        if let Some(pinned) = pinned {
            let mut conn = pinned.lock().await;
            let outcome = self
                .stream_resource_on(
                    &mut *conn,
                    handle,
                    execution_id,
                    sql,
                    limit,
                    parameters,
                    sink,
                )
                .await;
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                sqlx::Connection::ping(&mut **conn),
            )
            .await
            {
                Ok(Ok(())) => return outcome,
                _ => {
                    // Tombstone the original handle. No later operation may
                    // check out a replacement connection under this identity.
                    conn.close_on_drop();
                    self.transactions.lock().await.remove(&handle.id);
                    self.pools.write().await.remove(&handle.pool_id);
                    return Err(DriverError::ConnectionFailed(
                        "SessionLost: protocol drain could not be confirmed".into(),
                    )
                    .into());
                }
            }
        }
        if self.fixed_sessions.read().await.contains_key(&handle.id) {
            return Err(DriverError::ConnectionFailed("SessionLost".into()).into());
        }
        let pool = self
            .pools
            .read()
            .await
            .get(&handle.pool_id)
            .cloned()
            .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        self.stream_resource_on(
            &mut conn,
            handle,
            execution_id,
            sql,
            limit,
            parameters,
            sink,
        )
        .await
    }
    async fn stream_resource_on(
        &self,
        conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
        sql: &str,
        limit: Option<u32>,
        parameters: Option<&[Value]>,
        sink: &dyn ResultSink,
    ) -> Result<Vec<StatementResult>, ResourceError> {
        if !self.fixed_sessions.read().await.contains_key(&handle.id) {
            let id: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                .fetch_one(&mut **conn)
                .await
                .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
            self.bind_backend_pid(handle, execution_id, id).await?;
        }
        if parameters.is_some() && sql_dump::split_sql_statements(sql).len() != 1 {
            return Err(DriverError::InvalidConfig(
                "bound parameters require one statement".into(),
            )
            .into());
        }
        let mut results = Vec::new();
        for (statement_index, statement) in
            sql_dump::split_sql_statements(sql).into_iter().enumerate()
        {
            if self.is_cancel_requested(handle, execution_id).await? {
                return Err(DriverError::QueryCancelled.into());
            }
            let (effective_sql, applied_limit) = apply_select_limit(&statement, limit);
            let started = std::time::Instant::now();
            let mut columns = Vec::new();
            let mut batch = Vec::new();
            let mut bytes = 0usize;
            let mut published = 0u64;
            let mut affected = 0u64;
            let mut truncated = false;
            let mut delivery_error = None;
            let mut drain_deadline = None;
            let mut drain_failed = false;
            let mut stream = if let Some(parameters) = parameters {
                #[allow(deprecated)]
                Self::bind_values(sqlx::query(&effective_sql), parameters)
                    .fetch_many(&mut **conn)
                    .boxed()
            } else {
                sqlx::raw_sql(&effective_sql)
                    .fetch_many(&mut **conn)
                    .boxed()
            };
            loop {
                let next = if let Some(deadline) = drain_deadline {
                    match tokio::time::timeout_at(deadline, stream.try_next()).await {
                        Ok(next) => next,
                        Err(_) => {
                            drain_failed = true;
                            break;
                        }
                    }
                } else {
                    stream.try_next().await
                };
                let item = match next {
                    Ok(Some(item)) => item,
                    Ok(None) => break,
                    Err(error) => {
                        if delivery_error.is_none() {
                            delivery_error = Some(ResourceError::Driver(DriverError::QueryFailed(
                                error.to_string(),
                            )));
                        }
                        break;
                    }
                };
                match item {
                    sqlx::Either::Left(result) => affected = result.rows_affected(),
                    sqlx::Either::Right(row) => {
                        if columns.is_empty() {
                            columns = Self::columns_of_row(&row);
                            if let Err(error) = sink
                                .write_output(serde_json::json!({
                                    "kind": "statementColumns",
                                    "statementIndex": statement_index,
                                    "columns": columns,
                                }))
                                .await
                            {
                                delivery_error = Some(error);
                                drain_deadline = Some(
                                    tokio::time::Instant::now() + std::time::Duration::from_secs(5),
                                );
                                let _ = tokio::time::timeout(
                                    std::time::Duration::from_secs(2),
                                    self.cancel_query_with_execution_impl(handle, execution_id),
                                )
                                .await;
                            }
                        }
                        if delivery_error.is_some() {
                            continue;
                        }
                        if applied_limit.is_some_and(|limit| published >= u64::from(limit)) {
                            truncated = true;
                            continue;
                        }
                        let (_, mut decoded) = Self::decode_rows(std::slice::from_ref(&row));
                        let cells = decoded.pop().unwrap_or_default();
                        let size = serde_json::to_vec(&cells)
                            .map_err(|_| {
                                DriverError::QueryFailed("row serialization failed".into())
                            })?
                            .len();
                        if size > BATCH_BYTES {
                            delivery_error = Some(ResourceError::SinkRejected {
                                reason: "a decoded row exceeds the 1 MiB result batch limit".into(),
                            });
                            drain_deadline = Some(
                                tokio::time::Instant::now() + std::time::Duration::from_secs(5),
                            );
                            let _ = tokio::time::timeout(
                                std::time::Duration::from_secs(2),
                                self.cancel_query_with_execution_impl(handle, execution_id),
                            )
                            .await;
                            continue;
                        }
                        if bytes + size > BATCH_BYTES && !batch.is_empty() {
                            if let Err(error) = sink
                                .write(ResultChunk {
                                    statement_index,
                                    sql: statement.clone(),
                                    rows: std::mem::take(&mut batch),
                                    rows_affected: None,
                                })
                                .await
                            {
                                delivery_error = Some(error);
                                drain_deadline = Some(
                                    tokio::time::Instant::now() + std::time::Duration::from_secs(5),
                                );
                                let _ = tokio::time::timeout(
                                    std::time::Duration::from_secs(2),
                                    self.cancel_query_with_execution_impl(handle, execution_id),
                                )
                                .await;
                                continue;
                            }
                            bytes = 0;
                        }
                        bytes += size;
                        batch.push(cells);
                        published += 1;
                        if published == 1 || batch.len() >= BATCH_ROWS {
                            if let Err(error) = sink
                                .write(ResultChunk {
                                    statement_index,
                                    sql: statement.clone(),
                                    rows: std::mem::take(&mut batch),
                                    rows_affected: None,
                                })
                                .await
                            {
                                delivery_error = Some(error);
                                drain_deadline = Some(
                                    tokio::time::Instant::now() + std::time::Duration::from_secs(5),
                                );
                                let _ = tokio::time::timeout(
                                    std::time::Duration::from_secs(2),
                                    self.cancel_query_with_execution_impl(handle, execution_id),
                                )
                                .await;
                            }
                            bytes = 0;
                        }
                    }
                }
            }
            drop(stream);
            if drain_failed {
                conn.close_on_drop();
                self.transactions.lock().await.remove(&handle.id);
                self.pools.write().await.remove(&handle.pool_id);
                return Err(DriverError::ConnectionFailed(
                    "SessionLost: result drain timed out".into(),
                )
                .into());
            }
            if let Some(error) = delivery_error {
                return Err(error);
            }
            if !batch.is_empty() {
                sink.write(ResultChunk {
                    statement_index,
                    sql: statement.clone(),
                    rows: batch,
                    rows_affected: None,
                })
                .await?;
            }
            if columns.is_empty() && is_pg_result_query(&statement) {
                let description = (&mut **conn)
                    .describe(&effective_sql)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                columns = description
                    .columns()
                    .iter()
                    .enumerate()
                    .map(|(index, column)| ColumnInfo {
                        name: column.name().into(),
                        data_type: column.type_info().name().into(),
                        nullable: description.nullable(index).unwrap_or(true),
                    })
                    .collect();
            }
            sink.write_statement_summary(StatementSummary {
                statement_index,
                sql: statement.clone(),
                rows_affected: Some(affected),
            })
            .await?;
            sink.write_output(serde_json::json!({
                "kind": "statementMetadata",
                "statementIndex": statement_index,
                "columns": columns,
                "truncated": truncated,
            }))
            .await?;
            results.push(StatementResult {
                sql: statement,
                columns,
                rows: Vec::new(),
                rows_affected: Some(affected),
                execution_time_ms: started.elapsed().as_millis() as u64,
                truncated,
            });
        }
        Ok(results)
    }
}

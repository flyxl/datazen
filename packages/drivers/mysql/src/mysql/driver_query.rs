//! Query execution: single statements, multi statements, streaming, writes.

use super::*;

impl MysqlDriver {
    pub(crate) async fn query_inner(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        if let Some(state) = self.fixed_states.write().await.get_mut(&handle.id) {
            *state = datazen_driver_api::session::TransactionState::Unknown;
        }
        {
            let pinned = self.transactions.lock().await.get(&handle.id).cloned();

            if let Some(pinned) = pinned {
                let mut conn = pinned.lock().await;
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

    pub(crate) async fn query_multi_inner(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        if let Some(state) = self.fixed_states.write().await.get_mut(&handle.id) {
            *state = datazen_driver_api::session::TransactionState::Unknown;
        }
        let statements = split_mysql_statements(sql);
        if statements.is_empty() {
            return Ok(MultiQueryResult {
                results: Vec::new(),
                total_time_ms: 0,
            });
        }

        let total_start = Instant::now();
        let mut results = Vec::with_capacity(statements.len());

        let pinned = self.transactions.lock().await.get(&handle.id).cloned();

        if let Some(pinned) = pinned {
            let mut conn = pinned.lock().await;
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
                    let result = Self::execute_text_on_conn(&mut *conn, effective_sql.as_str())
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

    pub(crate) async fn query_stream_inner(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        if let Some(state) = self.fixed_states.write().await.get_mut(&handle.id) {
            *state = datazen_driver_api::session::TransactionState::Unknown;
        }
        let statements = split_mysql_statements(sql);
        if statements.is_empty() {
            on_event(QueryStreamEvent::Done { total_time_ms: 0 });
            return Ok(());
        }

        let total_start = Instant::now();
        {
            let pinned = self.transactions.lock().await.get(&handle.id).cloned();

            if let Some(pinned) = pinned {
                let mut conn = pinned.lock().await;
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

    pub(crate) async fn prepare_query_execution_inner(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        let transactional = self.transactions.lock().await.contains_key(&handle.id);
        let target_pool = self.pools.read().await.get(&handle.pool_id).cloned();
        // The control connection is shared per (connection identity, database), so
        // it is reached through this resource's identity rather than `pool_id`.
        let control_pool = self.control_pool_for(handle).await;
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
                cancel_gate: std::sync::Arc::new(tokio::sync::Mutex::new(())),
                session_id: handle.id.clone(),
                target_pool,
                control_pool,
                thread_id: self.fixed_sessions.read().await.get(&handle.id).copied(),
                cancel_requested: false,
                transactional,
            },
        );
        Ok(())
    }

    pub(crate) async fn query_stream_with_execution_inner(
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

    pub(crate) async fn query_with_params_inner(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        if let Some(state) = self.fixed_states.write().await.get_mut(&handle.id) {
            *state = datazen_driver_api::session::TransactionState::Unknown;
        }
        {
            let pinned = self.transactions.lock().await.get(&handle.id).cloned();

            if let Some(pinned) = pinned {
                let mut conn = pinned.lock().await;
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

    pub(crate) fn parameter_placeholder_inner(
        &self,
        _index: usize,
        _data_type: Option<&str>,
    ) -> Result<String, DriverError> {
        Ok("?".to_string())
    }

    pub(crate) async fn execute_with_params_inner(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<u64, DriverError> {
        if let Some(state) = self.fixed_states.write().await.get_mut(&handle.id) {
            *state = datazen_driver_api::session::TransactionState::Unknown;
        }
        {
            let pinned = self.transactions.lock().await.get(&handle.id).cloned();

            if let Some(pinned) = pinned {
                let mut conn = pinned.lock().await;
                let result = Self::bind_values(sqlx::query(sql), params)
                    .execute(&mut **conn)
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

        let result = Self::bind_values(sqlx::query(sql), params)
            .execute(&mut *conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        Ok(result.rows_affected())
    }

    pub(crate) async fn execute_inner(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<u64, DriverError> {
        if let Some(state) = self.fixed_states.write().await.get_mut(&handle.id) {
            *state = datazen_driver_api::session::TransactionState::Unknown;
        }
        {
            let pinned = self.transactions.lock().await.get(&handle.id).cloned();

            if let Some(pinned) = pinned {
                let mut conn = pinned.lock().await;
                let result = Self::execute_text_on_conn(&mut *conn, sql)
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
}

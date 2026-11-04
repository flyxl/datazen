//! Plan rendering and precise query cancellation.

use super::*;

impl MysqlDriver {
    pub(crate) async fn explain_inner(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        if split_mysql_statements(sql).len() != 1 {
            return Err(DriverError::InvalidConfig(
                "Explain requires one statement".into(),
            ));
        }
        // query() routes retained handles through their pinned checkout. Both
        // plan formats therefore see the editor's transaction and temp objects.
        let json_rows = self
            .query(handle, &format!("EXPLAIN FORMAT=JSON {sql}"))
            .await?;
        let plan_json = json_rows
            .rows
            .first()
            .and_then(|row| row.first())
            .and_then(|value| match value {
                Some(Value::Json(json)) => Some(json.clone()),
                Some(Value::String(text)) => serde_json::from_str(text).ok(),
                _ => None,
            });
        let text_rows = self.query(handle, &format!("EXPLAIN {sql}")).await?;
        let columns = text_rows.columns;
        let result_rows = text_rows.rows;
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

    pub(crate) async fn cancel_query_inner(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<(), DriverError> {
        Err(DriverError::Unsupported(
            "legacy session-wide query cancellation is disabled; use an execution handle".into(),
        ))
    }

    pub(crate) async fn cancel_query_with_execution_inner(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        // Keep this execution gated until KILL QUERY completes. A MySQL
        // thread id belongs to a pooled connection and can be reused; early
        // cleanup could otherwise let a delayed cancel hit a later query.
        let gate = {
            let executions = self.query_executions.lock().await;
            let execution = executions.get(execution_id).ok_or_else(|| {
                DriverError::QueryExecutionNotFound(execution_id.as_str().to_string())
            })?;
            if execution.session_id != handle.id {
                return Err(DriverError::QueryExecutionSessionMismatch);
            }
            execution.cancel_gate.clone()
        };
        let _cancel_guard = gate.lock().await;
        let mut executions = self.query_executions.lock().await;
        let (thread_id, control_pool) = {
            let execution = executions.get_mut(execution_id).ok_or_else(|| {
                DriverError::QueryExecutionNotFound(execution_id.as_str().to_string())
            })?;
            if execution.session_id != handle.id {
                return Err(DriverError::QueryExecutionSessionMismatch);
            }
            if !std::sync::Arc::ptr_eq(&execution.cancel_gate, &gate) {
                return Err(DriverError::QueryExecutionNotFound(
                    execution_id.as_str().into(),
                ));
            }
            execution.cancel_requested = true;
            let Some(thread_id) = execution.thread_id else {
                // The stream observes this pending request after acquiring its
                // dedicated connection and before executing user SQL.
                return Ok(());
            };
            (thread_id, execution.control_pool.clone())
        };

        drop(executions);

        let Some(control_pool) = control_pool else {
            if let Some(execution) = self.query_executions.lock().await.get_mut(execution_id) {
                execution.cancel_requested = false;
            }
            return Err(DriverError::ConnectionFailed(
                "Control pool not found".into(),
            ));
        };
        let mut conn = match control_pool.acquire().await {
            Ok(conn) => conn,
            Err(error) => {
                if let Some(execution) = self.query_executions.lock().await.get_mut(execution_id) {
                    execution.cancel_requested = false;
                }
                return Err(DriverError::ConnectionFailed(error.to_string()));
            }
        };
        let kill_sql = build_kill_query_sql(thread_id);
        match Self::execute_text_on_conn(&mut conn, &kill_sql).await {
            Ok(_) => Ok(()),
            Err(error) => {
                if let Some(execution) = self.query_executions.lock().await.get_mut(execution_id) {
                    execution.cancel_requested = false;
                }
                Err(DriverError::QueryFailed(error.to_string()))
            }
        }
    }

    pub(crate) async fn cleanup_query_execution_inner(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.finish_query_execution(handle, execution_id).await
    }

    pub(crate) fn supports_query_execution_cancel_inner(&self) -> bool {
        true
    }
}

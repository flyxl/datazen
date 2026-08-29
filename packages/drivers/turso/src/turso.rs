//! Turso / libSQL driver — sqld HTTP `/v2/pipeline` interface.

use async_trait::async_trait;
use datazen_driver_api::sqlite_structure as sqlite_struct;
use datazen_driver_api::*;
use datazen_driver_http_support::*;
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::RwLock;

pub struct TursoDriver {
    clients: RwLock<HashMap<String, (reqwest::Client, String)>>,
}

impl TursoDriver {
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
        }
    }

    fn get<'a>(
        map: &'a HashMap<String, (reqwest::Client, String)>,
        handle: &ConnectionHandle,
    ) -> Result<&'a (reqwest::Client, String), DriverError> {
        map.get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))
    }

    async fn pipeline(
        client: &reqwest::Client,
        base: &str,
        sql: &str,
    ) -> Result<serde_json::Value, DriverError> {
        let resp = client
            .post(format!("{base}/v2/pipeline"))
            .json(&serde_json::json!({
                "requests": [{ "type": "execute", "stmt": { "sql": sql } }]
            }))
            .send()
            .await
            .map_err(|e| http_error("Turso pipeline request failed", e))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| http_error("Turso read response failed", e))?;
        if !status.is_success() {
            return Err(status_error("Turso query failed", status, &text));
        }
        serde_json::from_str(&text)
            .map_err(|e| DriverError::QueryFailed(format!("Turso JSON parse failed: {e}")))
    }

    fn result_from_json(v: &serde_json::Value) -> QueryResult {
        let first = v
            .get("results")
            .and_then(|r| r.as_array())
            .and_then(|a| a.first())
            .and_then(|r| r.get("response"))
            .and_then(|r| r.get("result"));
        let mut columns = Vec::new();
        let mut rows = Vec::new();
        if let Some(result) = first {
            if let Some(cols) = result.get("cols").and_then(|c| c.as_array()) {
                columns = cols
                    .iter()
                    .filter_map(|c| {
                        Some(ColumnInfo {
                            name: c.get("name")?.as_str()?.to_string(),
                            data_type: c
                                .get("decltype")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .to_string(),
                            nullable: true,
                        })
                    })
                    .collect();
            }
            if let Some(rows_arr) = result.get("rows").and_then(|r| r.as_array()) {
                rows = rows_arr
                    .iter()
                    .map(|row| {
                        row.as_array()
                            .map(|arr| arr.iter().map(turso_cell).collect())
                            .unwrap_or_default()
                    })
                    .collect();
            }
        }
        QueryResult {
            columns,
            rows,
            rows_affected: None,
            execution_time_ms: 0,
        }
    }

    async fn schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        let map = self.clients.read().await;
        let (client, base) = Self::get(&map, handle)?;
        let v = Self::pipeline(
            client,
            base,
            &format!("PRAGMA table_info('{}')", table.replace('\'', "''")),
        )
        .await?;
        let result = v
            .get("results")
            .and_then(|r| r.as_array())
            .and_then(|a| a.first())
            .and_then(|r| r.get("response"))
            .and_then(|r| r.get("result"));
        let mut columns = Vec::new();
        if let Some(r) = result {
            if let Some(rows) = r.get("rows").and_then(|v2| v2.as_array()) {
                for row in rows {
                    let Some(arr) = row.as_array() else {
                        continue;
                    };
                    if arr.len() < 6 {
                        continue;
                    }
                    columns.push(ColumnSchema {
                        name: turso_str(&arr[1]).unwrap_or_default(),
                        data_type: turso_str(&arr[2]).unwrap_or_default(),
                        nullable: turso_int(&arr[3]).map(|v| v == 0).unwrap_or(true),
                        default_value: turso_str(&arr[4]),
                        comment: None,
                        is_primary_key: turso_int(&arr[5]).unwrap_or(0) > 0,
                        is_auto_increment: false,
                    });
                }
            }
        }
        Ok(TableSchema {
            table_name: table.to_string(),
            columns,
            primary_keys: Vec::new(),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
        })
    }
}

fn turso_cell(v: &serde_json::Value) -> Option<Value> {
    match v {
        serde_json::Value::Null => Some(Value::Null),
        serde_json::Value::String(s) => Some(Value::String(s.clone())),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(Value::Integer(i))
            } else {
                n.as_f64().map(Value::Float)
            }
        }
        serde_json::Value::Bool(b) => Some(Value::Bool(*b)),
        other => Some(Value::Json(other.clone())),
    }
}

fn turso_str(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Null => None,
        other => Some(other.to_string()),
    }
}

fn turso_int(v: &serde_json::Value) -> Option<i64> {
    match v {
        serde_json::Value::Number(n) => n.as_i64(),
        serde_json::Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

#[async_trait]
impl DatabaseDriver for TursoDriver {
    fn driver_type(&self) -> DatabaseType {
        "turso".to_string()
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let client = build_http_client(
            config.connection_timeout,
            config.username.as_deref(),
            config.password.as_deref(),
        )?;
        let base = base_url(config)?;
        Self::pipeline(&client, &base, "SELECT 1").await?;
        Ok(ServerInfo {
            server_version: "libSQL".to_string(),
            server_type: "turso".to_string(),
        })
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let client = build_http_client(
            config.connection_timeout,
            config.username.as_deref(),
            config.password.as_deref(),
        )?;
        let base = base_url(config)?;
        let pool_id = format!("turso_{}", uuid::Uuid::new_v4());
        self.clients
            .write()
            .await
            .insert(pool_id.clone(), (client, base));
        Ok(ConnectionHandle {
            id: pool_id.clone(),
            pool_id,
        })
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        self.clients.write().await.remove(&handle.pool_id);
        Ok(())
    }

    async fn get_databases(&self, _handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Ok(vec!["main".to_string()])
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let map = self.clients.read().await;
        let (client, base) = Self::get(&map, handle)?;
        let v = Self::pipeline(
            client,
            base,
            "SELECT name FROM sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .await?;
        let result = v
            .get("results")
            .and_then(|r| r.as_array())
            .and_then(|a| a.first())
            .and_then(|r| r.get("response"))
            .and_then(|r| r.get("result"));
        let mut tables = Vec::new();
        if let Some(r) = result {
            if let Some(rows) = r.get("rows").and_then(|v2| v2.as_array()) {
                for row in rows {
                    if let Some(name) = row.as_array().and_then(|a| a.first()).and_then(turso_str) {
                        tables.push(TableInfo {
                            name,
                            schema: None,
                            table_type: TableType::Table,
                            row_count: None,
                        });
                    }
                }
            }
        }
        Ok(tables)
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        self.schema(handle, table).await
    }

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        let map = self.clients.read().await;
        let (client, base) = Self::get(&map, handle)?;
        let start = Instant::now();
        let v = Self::pipeline(client, base, sql).await?;
        let mut result = Self::result_from_json(&v);
        result.execution_time_ms = start.elapsed().as_millis() as u64;
        Ok(result)
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        _limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        let result = self.query(handle, sql).await?;
        Ok(MultiQueryResult {
            results: vec![StatementResult {
                sql: sql.to_string(),
                columns: result.columns,
                rows: result.rows,
                rows_affected: result.rows_affected,
                execution_time_ms: result.execution_time_ms,
                truncated: false,
            }],
            total_time_ms: result.execution_time_ms,
        })
    }

    async fn query_stream(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        let map = self.clients.read().await;
        let (client, base) = Self::get(&map, handle)?;
        let start = Instant::now();
        let (effective, applied) = append_select_limit(sql, limit);
        let v = Self::pipeline(client, base, &effective).await?;
        let mut result = Self::result_from_json(&v);
        let ms = start.elapsed().as_millis() as u64;
        stream_decoded_rows(
            &on_event,
            0,
            sql.to_string(),
            result.columns,
            std::mem::take(&mut result.rows),
            applied,
            ms,
            result.rows_affected,
        );
        on_event(QueryStreamEvent::Done { total_time_ms: ms });
        Ok(())
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
        let map = self.clients.read().await;
        let (client, base) = Self::get(&map, handle)?;
        let v = Self::pipeline(client, base, sql).await?;
        let first = v
            .get("results")
            .and_then(|r| r.as_array())
            .and_then(|a| a.first())
            .and_then(|r| r.get("response"))
            .and_then(|r| r.get("result"));
        Ok(first
            .and_then(|r| r.get("rows_written"))
            .and_then(|n| n.as_u64())
            .unwrap_or(0))
    }

    fn supports_explain(&self) -> bool {
        true
    }

    async fn explain(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        let result = self
            .query(handle, &format!("EXPLAIN QUERY PLAN {sql}"))
            .await?;
        Ok(explain_result_from_query(result))
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        let mut cmds = vec![
            query_command_definition(),
            execute_command_definition(),
            query_stream_command_definition(),
        ];
        cmds.extend(schema_catalog_command_definitions());
        cmds.extend(schema_object_command_definitions());
        cmds
    }

    async fn execute_command(
        &self,
        handle: &ConnectionHandle,
        command: &str,
        input: serde_json::Value,
    ) -> Result<CommandResult, DriverError> {
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
        execute_standard_sql_command(self, handle, command, input).await
    }

    async fn structure_capabilities(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<StructureCapabilities, DriverError> {
        Ok(sqlite_struct::capabilities(&self.driver_type()))
    }

    async fn plan_structure_changes(
        &self,
        _handle: &ConnectionHandle,
        request: &StructureChangeRequest,
    ) -> Result<StructureChangePlan, DriverError> {
        let caps = sqlite_struct::capabilities(&self.driver_type());
        sqlite_struct::plan_changes(request, &caps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn result_from_json_then_stream_decoded_rows_honors_limit() {
        let v = serde_json::json!({
            "results": [{
                "response": {
                    "result": {
                        "cols": [{"name": "id", "decltype": "INTEGER"}],
                        "rows": [[1], [2], [3]]
                    }
                }
            }]
        });
        let mut result = TursoDriver::result_from_json(&v);
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_cb = Arc::clone(&events);
        let cb: QueryStreamCallback = Arc::new(move |ev| {
            events_cb.lock().unwrap().push(ev);
        });
        stream_decoded_rows(
            &cb,
            0,
            "SELECT 1".into(),
            result.columns,
            std::mem::take(&mut result.rows),
            Some(2),
            1,
            None,
        );
        let events = events.lock().unwrap();
        let rows: usize = events
            .iter()
            .filter_map(|e| match e {
                QueryStreamEvent::Rows { rows, .. } => Some(rows.len()),
                _ => None,
            })
            .sum();
        assert_eq!(rows, 2);
    }
}

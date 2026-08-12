//! Vector store driver — Qdrant REST API.
//!
//! The query editor accepts a JSON command body:
//! `{"collection":"my_collection","query":{"nearest":{"vector":[0.1,0.2],"limit":10}}}`
//! Non-JSON input is rejected with a helpful message.

use async_trait::async_trait;
use datazen_driver_api::*;
use datazen_driver_http_support::*;
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::RwLock;

pub struct VectorDriver {
    clients: RwLock<HashMap<String, (reqwest::Client, String)>>,
}

impl VectorDriver {
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

    async fn get_json(
        client: &reqwest::Client,
        base: &str,
        path: &str,
    ) -> Result<serde_json::Value, DriverError> {
        let resp = client
            .get(format!("{base}{path}"))
            .send()
            .await
            .map_err(|e| http_error("Qdrant request failed", e))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| http_error("Qdrant read response failed", e))?;
        if !status.is_success() {
            return Err(status_error("Qdrant request failed", status, &text));
        }
        serde_json::from_str(&text)
            .map_err(|e| DriverError::QueryFailed(format!("Qdrant JSON parse failed: {e}")))
    }

    fn points_to_result(v: &serde_json::Value) -> QueryResult {
        let mut columns = vec![ColumnInfo {
            name: "id".to_string(),
            data_type: "string".to_string(),
            nullable: false,
        }];
        let mut rows = Vec::new();
        let mut payload_keys: Vec<String> = Vec::new();
        let points = v
            .get("result")
            .and_then(|r| r.get("points"))
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default();
        for point in &points {
            if let Some(payload) = point.get("payload").and_then(|p| p.as_object()) {
                for k in payload.keys() {
                    if !payload_keys.contains(k) {
                        payload_keys.push(k.clone());
                    }
                }
            }
        }
        for k in &payload_keys {
            columns.push(ColumnInfo {
                name: k.clone(),
                data_type: "json".to_string(),
                nullable: true,
            });
        }
        columns.push(ColumnInfo {
            name: "vector".to_string(),
            data_type: "json".to_string(),
            nullable: true,
        });
        for point in &points {
            let mut row = vec![point
                .get("id")
                .map(json_to_value)
                .unwrap_or(Some(Value::Null))];
            let payload = point.get("payload").and_then(|p| p.as_object());
            for k in &payload_keys {
                row.push(
                    payload
                        .and_then(|p| p.get(k))
                        .map(json_to_value)
                        .unwrap_or(Some(Value::Null)),
                );
            }
            row.push(
                point
                    .get("vector")
                    .map(json_to_value)
                    .unwrap_or(Some(Value::Null)),
            );
            rows.push(row);
        }
        QueryResult {
            columns,
            rows,
            rows_affected: None,
            execution_time_ms: 0,
        }
    }
}

#[async_trait]
impl DatabaseDriver for VectorDriver {
    fn driver_type(&self) -> DatabaseType {
        "vector".to_string()
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let client = build_http_client(
            config.connection_timeout,
            config.username.as_deref(),
            config.password.as_deref(),
        )?;
        let base = base_url(config)?;
        let v = Self::get_json(&client, &base, "/collections").await?;
        let version = v
            .get("result")
            .and_then(|r| r.get("collections"))
            .map(|_| "Qdrant".to_string())
            .unwrap_or_default();
        Ok(ServerInfo {
            server_version: version,
            server_type: "vector".to_string(),
        })
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let client = build_http_client(
            config.connection_timeout,
            config.username.as_deref(),
            config.password.as_deref(),
        )?;
        let base = base_url(config)?;
        let pool_id = format!("vector_{}", uuid::Uuid::new_v4());
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
        Ok(vec!["default".to_string()])
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let map = self.clients.read().await;
        let (client, base) = Self::get(&map, handle)?;
        let v = Self::get_json(client, base, "/collections").await?;
        Ok(v.get("result")
            .and_then(|r| r.get("collections"))
            .and_then(|c| c.as_array())
            .into_iter()
            .flatten()
            .filter_map(|c| {
                Some(TableInfo {
                    name: c.get("name")?.as_str()?.to_string(),
                    schema: None,
                    table_type: TableType::Table,
                    row_count: None,
                })
            })
            .collect())
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        let map = self.clients.read().await;
        let (client, base) = Self::get(&map, handle)?;
        let v = Self::get_json(
            client,
            base,
            &format!("/collections/{}", urlencoding::encode(table)),
        )
        .await?;
        let mut columns = Vec::new();
        if let Some(params) = v
            .get("result")
            .and_then(|r| r.get("config"))
            .and_then(|c| c.get("params"))
        {
            if let Some(vectors) = params.get("vectors").and_then(|v2| v2.as_object()) {
                if let Some(size) = vectors.get("size") {
                    columns.push(ColumnSchema {
                        name: "vector".to_string(),
                        data_type: format!("float[{}]", size.as_u64().unwrap_or(0)),
                        nullable: true,
                        default_value: None,
                        comment: None,
                        is_primary_key: false,
                        is_auto_increment: false,
                    });
                }
                if let Some(distance) = vectors.get("distance").and_then(|d| d.as_str()) {
                    columns.push(ColumnSchema {
                        name: "distance".to_string(),
                        data_type: distance.to_string(),
                        nullable: true,
                        default_value: None,
                        comment: None,
                        is_primary_key: false,
                        is_auto_increment: false,
                    });
                }
            }
        }
        columns.push(ColumnSchema {
            name: "id".to_string(),
            data_type: "string".to_string(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: true,
            is_auto_increment: false,
        });
        Ok(TableSchema {
            table_name: table.to_string(),
            columns,
            primary_keys: vec!["id".to_string()],
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
        })
    }

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        let cmd: serde_json::Value = serde_json::from_str(sql.trim())
            .map_err(|_| DriverError::QueryFailed(
                "Vector store queries are JSON commands, e.g. {\"collection\":\"c\",\"query\":{\"nearest\":{\"vector\":[0.1],\"limit\":10}}}".into(),
            ))?;
        let collection = cmd
            .get("collection")
            .and_then(|c| c.as_str())
            .ok_or_else(|| {
                DriverError::QueryFailed("JSON command requires a \"collection\" field".into())
            })?;
        let query = cmd.get("query").ok_or_else(|| {
            DriverError::QueryFailed("JSON command requires a \"query\" field".into())
        })?;
        let map = self.clients.read().await;
        let (client, base) = Self::get(&map, handle)?;
        let start = Instant::now();
        let nearest = query.get("nearest").cloned().unwrap_or(query.clone());
        let resp = client
            .post(format!(
                "{base}/collections/{}/points/search",
                urlencoding::encode(collection)
            ))
            .json(&nearest)
            .send()
            .await
            .map_err(|e| http_error("Qdrant search request failed", e))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| http_error("Qdrant read response failed", e))?;
        if !status.is_success() {
            return Err(status_error("Qdrant search failed", status, &text));
        }
        let v: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
        let mut result = Self::points_to_result(&v);
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
        let mut result = self.query(handle, sql).await?;
        let ms = result.execution_time_ms;
        stream_decoded_rows(
            &on_event,
            0,
            sql.to_string(),
            result.columns,
            std::mem::take(&mut result.rows),
            limit,
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

    async fn execute(&self, _handle: &ConnectionHandle, _sql: &str) -> Result<u64, DriverError> {
        Err(DriverError::QueryFailed(
            "Vector store writes are not supported through the query editor".into(),
        ))
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        query_only_command_definitions(
            "Search a vector collection with a JSON command",
            "JSON command",
        )
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn points_to_result_then_stream_decoded_rows_honors_limit() {
        let v = serde_json::json!({
            "result": {
                "points": [
                    {"id": "a", "payload": {"k": 1}, "vector": [0.1]},
                    {"id": "b", "payload": {"k": 2}, "vector": [0.2]}
                ]
            }
        });
        let mut result = VectorDriver::points_to_result(&v);
        assert_eq!(result.rows.len(), 2);
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_cb = Arc::clone(&events);
        let cb: QueryStreamCallback = Arc::new(move |ev| {
            events_cb.lock().unwrap().push(ev);
        });
        stream_decoded_rows(
            &cb,
            0,
            "{}".into(),
            result.columns,
            std::mem::take(&mut result.rows),
            Some(1),
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
        assert_eq!(rows, 1);
    }
}

//! Elasticsearch driver — REST + SQL translation API (`/_sql`).

use async_trait::async_trait;
use datazen_driver_api::*;
use datazen_driver_http_support::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

pub struct ElasticsearchDriver {
    clients: RwLock<HashMap<String, (reqwest::Client, String)>>,
}

impl ElasticsearchDriver {
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

    async fn sql_json(
        client: &reqwest::Client,
        base: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, DriverError> {
        let resp = client
            .post(format!("{base}/_sql?format=json"))
            .json(body)
            .send()
            .await
            .map_err(|e| http_error("Elasticsearch SQL request failed", e))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| http_error("Elasticsearch read response failed", e))?;
        if !status.is_success() {
            return Err(status_error("Elasticsearch SQL failed", status, &text));
        }
        serde_json::from_str(&text)
            .map_err(|e| DriverError::QueryFailed(format!("Elasticsearch JSON parse failed: {e}")))
    }

    async fn sql(
        client: &reqwest::Client,
        base: &str,
        sql: &str,
    ) -> Result<serde_json::Value, DriverError> {
        Self::sql_json(client, base, &serde_json::json!({ "query": sql })).await
    }

    async fn close_cursor(client: &reqwest::Client, base: &str, cursor: &str) {
        let _ = client
            .post(format!("{base}/_sql/close"))
            .json(&serde_json::json!({ "cursor": cursor }))
            .send()
            .await;
    }

    fn result_from_json(v: &serde_json::Value) -> QueryResult {
        let columns: Vec<ColumnInfo> = v
            .get("columns")
            .and_then(|c| c.as_array())
            .into_iter()
            .flatten()
            .filter_map(|c| {
                Some(ColumnInfo {
                    name: c.get("name")?.as_str()?.to_string(),
                    data_type: c
                        .get("type")
                        .map(|t| t.as_str().unwrap_or("").to_string())
                        .unwrap_or_default(),
                    nullable: true,
                })
            })
            .collect();
        let rows: Vec<Vec<Option<Value>>> = v
            .get("rows")
            .and_then(|r| r.as_array())
            .into_iter()
            .flatten()
            .map(|row| {
                row.as_array()
                    .map(|arr| arr.iter().map(json_to_value).collect())
                    .unwrap_or_default()
            })
            .collect();
        QueryResult {
            columns,
            rows,
            rows_affected: None,
            execution_time_ms: 0,
        }
    }
}

#[async_trait]
impl DatabaseDriver for ElasticsearchDriver {
    fn driver_type(&self) -> DatabaseType {
        "elasticsearch".to_string()
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let client = build_http_client(
            config.connection_timeout,
            config.username.as_deref(),
            config.password.as_deref(),
        )?;
        let base = base_url(config)?;
        let resp = client
            .get(format!("{base}/"))
            .send()
            .await
            .map_err(|e| http_error("Elasticsearch request failed", e))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| http_error("Elasticsearch read response failed", e))?;
        if !status.is_success() {
            return Err(status_error(
                "Elasticsearch connection failed",
                status,
                &text,
            ));
        }
        let version = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                v.get("version")
                    .and_then(|x| x.get("number"))
                    .and_then(|n| n.as_str())
                    .map(String::from)
            })
            .unwrap_or_default()
            .to_string();
        Ok(ServerInfo {
            server_version: version,
            server_type: "elasticsearch".to_string(),
        })
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let client = build_http_client(
            config.connection_timeout,
            config.username.as_deref(),
            config.password.as_deref(),
        )?;
        let base = base_url(config)?;
        let pool_id = format!("es_{}", uuid::Uuid::new_v4());
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
        let resp = client
            .get(format!("{base}/_cat/indices?format=json&h=index"))
            .send()
            .await
            .map_err(|e| http_error("Elasticsearch indices request failed", e))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| http_error("Elasticsearch read response failed", e))?;
        if !status.is_success() {
            return Err(status_error("Elasticsearch indices failed", status, &text));
        }
        let list = serde_json::from_str::<Vec<serde_json::Value>>(&text).unwrap_or_default();
        Ok(list
            .into_iter()
            .filter_map(|v| {
                let name = v.get("index")?.as_str()?.to_string();
                if name.starts_with('.') {
                    return None;
                }
                Some(TableInfo {
                    name,
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
        let resp = client
            .get(format!("{base}/{}/_mapping", urlencoding::encode(table)))
            .send()
            .await
            .map_err(|e| http_error("Elasticsearch mapping request failed", e))?;
        let text = resp
            .text()
            .await
            .map_err(|e| http_error("Elasticsearch read response failed", e))?;
        let mut columns = Vec::new();
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            let props = v
                .pointer(format!("/{}/mappings/properties", table.replace('.', "\\.")).as_str())
                .or_else(|| {
                    v.get(table)
                        .and_then(|t| t.get("mappings"))
                        .and_then(|m| m.get("properties"))
                });
            if let Some(props) = props {
                if let Some(obj) = props.as_object() {
                    for (name, meta) in obj {
                        columns.push(ColumnSchema {
                            name: name.clone(),
                            data_type: meta
                                .get("type")
                                .and_then(|t| t.as_str())
                                .unwrap_or("object")
                                .to_string(),
                            nullable: true,
                            default_value: None,
                            comment: None,
                            is_primary_key: false,
                            is_auto_increment: false,
                        });
                    }
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

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        let map = self.clients.read().await;
        let (client, base) = Self::get(&map, handle)?;
        let start = Instant::now();
        let v = Self::sql(client, base, sql).await?;
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
                rows_affected: None,
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
        let client = client.clone();
        let base = base.clone();
        drop(map);

        let start = Instant::now();
        let fetch_size = match limit {
            Some(n) => QUERY_STREAM_BATCH_SIZE.min((n as usize).saturating_add(1)),
            None => QUERY_STREAM_BATCH_SIZE,
        };
        let mut body = serde_json::json!({ "query": sql, "fetch_size": fetch_size });
        let mut batcher: Option<QueryRowBatcher> = None;
        let mut open_cursor: Option<String> = None;
        loop {
            let v = match Self::sql_json(&client, &base, &body).await {
                Ok(v) => v,
                Err(err) => {
                    if let Some(c) = open_cursor.as_deref() {
                        Self::close_cursor(&client, &base, c).await;
                    }
                    return Err(err);
                }
            };
            if batcher.is_none() {
                let columns: Vec<ColumnInfo> = v
                    .get("columns")
                    .and_then(|c| c.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|c| {
                        Some(ColumnInfo {
                            name: c.get("name")?.as_str()?.to_string(),
                            data_type: c
                                .get("type")
                                .map(|t| t.as_str().unwrap_or("").to_string())
                                .unwrap_or_default(),
                            nullable: true,
                        })
                    })
                    .collect();
                let mut b = QueryRowBatcher::new(Arc::clone(&on_event), 0, sql.to_string(), limit);
                b.start(columns);
                batcher = Some(b);
            }
            let batcher = batcher.as_mut().expect("batcher initialized");
            let mut stop = false;
            if let Some(rows) = v.get("rows").and_then(|r| r.as_array()) {
                for row in rows {
                    let decoded = row
                        .as_array()
                        .map(|arr| arr.iter().map(json_to_value).collect())
                        .unwrap_or_default();
                    if !batcher.push(decoded) {
                        stop = true;
                        break;
                    }
                }
            }
            let next_cursor = v.get("cursor").and_then(|c| c.as_str()).map(str::to_string);
            if stop {
                if let Some(c) = next_cursor.as_deref().or(open_cursor.as_deref()) {
                    Self::close_cursor(&client, &base, c).await;
                }
                break;
            }
            match next_cursor {
                Some(c) => {
                    body = serde_json::json!({ "cursor": c });
                    open_cursor = Some(c);
                }
                None => break,
            }
        }
        let ms = start.elapsed().as_millis() as u64;
        if let Some(b) = batcher {
            b.finish(ms, None);
        } else {
            emit_execute_statement(&on_event, 0, sql.to_string(), 0, ms);
        }
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
        let v = Self::sql(client, base, sql).await?;
        Ok(v.get("rows").and_then(|r| r.as_u64()).unwrap_or(0))
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        let mut cmds = statement_command_definitions(
            "Run an Elasticsearch SQL query",
            "Run an Elasticsearch SQL statement",
            "Elasticsearch SQL",
        );
        cmds.push(query_stream_command_definition());
        cmds
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn collect_events() -> (QueryStreamCallback, Arc<Mutex<Vec<QueryStreamEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_cb = Arc::clone(&events);
        (
            Arc::new(move |ev| {
                events_cb.lock().unwrap().push(ev);
            }),
            events,
        )
    }

    fn http_config(server: &MockServer) -> ConnectionConfig {
        let addr = server.address();
        ConnectionConfig {
            id: "es".into(),
            name: "es".into(),
            database_type: "elasticsearch".into(),
            host: Some(addr.ip().to_string()),
            port: Some(addr.port()),
            database: None,
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::Disable,
            connection_timeout: 5,
            max_pool_size: 4,
            ssh_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
            pinned: false,
        }
    }

    #[tokio::test]
    async fn query_stream_follows_sql_cursor() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/_sql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "columns": [{"name": "id", "type": "integer"}],
                "rows": [[1]],
                "cursor": "next"
            })))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/_sql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rows": [[2]]
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/_sql/close"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server)
            .await;

        let driver = ElasticsearchDriver::new();
        let handle = driver.connect(&http_config(&server)).await.unwrap();
        let (cb, events) = collect_events();
        driver
            .query_stream(&handle, "SELECT 1", None, cb)
            .await
            .unwrap();
        let events = events.lock().unwrap();
        let rows: usize = events
            .iter()
            .filter_map(|e| match e {
                QueryStreamEvent::Rows { rows, .. } => Some(rows.len()),
                _ => None,
            })
            .sum();
        assert_eq!(rows, 2);
        assert!(matches!(events.last(), Some(QueryStreamEvent::Done { .. })));
    }
}

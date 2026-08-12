//! HBase driver — Apache HBase REST (Stargate) JSON API.
//!
//! HBase has no SQL surface. The query editor accepts `scan <table>`.

use async_trait::async_trait;
use datazen_driver_api::*;
use datazen_driver_http_support::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

pub struct HBaseDriver {
    clients: RwLock<HashMap<String, (reqwest::Client, String)>>,
}

impl HBaseDriver {
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
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| http_error("HBase request failed", e))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| http_error("HBase read response failed", e))?;
        if !status.is_success() {
            return Err(status_error("HBase request failed", status, &text));
        }
        serde_json::from_str(&text)
            .map_err(|e| DriverError::QueryFailed(format!("HBase JSON parse failed: {e}")))
    }

    async fn scan(
        client: &reqwest::Client,
        base: &str,
        table: &str,
        limit: u32,
    ) -> Result<QueryResult, DriverError> {
        let resp = client
            .post(format!("{base}/{}/scanner", urlencoding::encode(table)))
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&serde_json::json!({ "batch": limit }))
            .send()
            .await
            .map_err(|e| http_error("HBase scan request failed", e))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(status_error("HBase scan failed", status, &text));
        }
        let scanner_url = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let mut rows = Vec::new();
        if let Some(url) = scanner_url {
            let full = if url.starts_with("http") {
                url
            } else {
                format!("{base}{url}")
            };
            let resp2 = client
                .get(&full)
                .header(reqwest::header::ACCEPT, "application/json")
                .send()
                .await
                .map_err(|e| http_error("HBase scan read failed", e))?;
            if let Ok(v) = resp2.json::<serde_json::Value>().await {
                rows = Self::decode_scan_rows(&v);
            }
            let _ = client.delete(&full).send().await;
        }
        Ok(QueryResult {
            columns: Self::scan_columns(),
            rows,
            rows_affected: None,
            execution_time_ms: 0,
        })
    }

    fn scan_columns() -> Vec<ColumnInfo> {
        vec![
            ColumnInfo {
                name: "row".to_string(),
                data_type: "string".to_string(),
                nullable: false,
            },
            ColumnInfo {
                name: "cell".to_string(),
                data_type: "string".to_string(),
                nullable: true,
            },
        ]
    }

    fn decode_scan_rows(v: &serde_json::Value) -> Vec<Vec<Option<Value>>> {
        v.get("Row")
            .and_then(|r| r.as_array())
            .into_iter()
            .flatten()
            .map(|cell| {
                let key = cell
                    .get("key")
                    .and_then(|k| k.as_str())
                    .map(|s| Value::String(s.to_string()));
                let value = cell
                    .get("Cell")
                    .and_then(|c| c.as_array())
                    .and_then(|a| a.first())
                    .and_then(|c| c.get("$"))
                    .map(|v| {
                        Value::String(
                            v.as_str()
                                .map(str::to_string)
                                .unwrap_or_else(|| v.to_string()),
                        )
                    });
                vec![key, value]
            })
            .collect()
    }

    async fn stream_scan(
        client: &reqwest::Client,
        base: &str,
        table: &str,
        sql: &str,
        limit: Option<u32>,
        on_event: &QueryStreamCallback,
        start: Instant,
    ) -> Result<(), DriverError> {
        let batch = match limit {
            Some(n) => QUERY_STREAM_BATCH_SIZE.min(n.saturating_add(1) as usize) as u32,
            None => QUERY_STREAM_BATCH_SIZE as u32,
        };
        let resp = client
            .post(format!("{base}/{}/scanner", urlencoding::encode(table)))
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&serde_json::json!({ "batch": batch }))
            .send()
            .await
            .map_err(|e| http_error("HBase scan request failed", e))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(status_error("HBase scan failed", status, &text));
        }
        let scanner_url = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let mut batcher = QueryRowBatcher::new(Arc::clone(on_event), 0, sql.to_string(), limit);
        batcher.start(Self::scan_columns());
        if let Some(url) = scanner_url {
            let full = if url.starts_with("http") {
                url
            } else {
                format!("{base}{url}")
            };
            loop {
                let resp2 = client
                    .get(&full)
                    .header(reqwest::header::ACCEPT, "application/json")
                    .send()
                    .await
                    .map_err(|e| http_error("HBase scan read failed", e))?;
                let status = resp2.status();
                if status == reqwest::StatusCode::NO_CONTENT {
                    break;
                }
                if !status.is_success() {
                    let text = resp2.text().await.unwrap_or_default();
                    let _ = client.delete(&full).send().await;
                    return Err(status_error("HBase scan read failed", status, &text));
                }
                let v: serde_json::Value = resp2.json().await.unwrap_or(serde_json::Value::Null);
                let rows = Self::decode_scan_rows(&v);
                if rows.is_empty() {
                    break;
                }
                let mut stop = false;
                for row in rows {
                    if !batcher.push(row) {
                        stop = true;
                        break;
                    }
                }
                if stop {
                    break;
                }
            }
            let _ = client.delete(&full).send().await;
        }
        let ms = start.elapsed().as_millis() as u64;
        batcher.finish(ms, None);
        on_event(QueryStreamEvent::Done { total_time_ms: ms });
        Ok(())
    }
}

#[async_trait]
impl DatabaseDriver for HBaseDriver {
    fn driver_type(&self) -> DatabaseType {
        "hbase".to_string()
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let client = build_http_client(
            config.connection_timeout,
            config.username.as_deref(),
            config.password.as_deref(),
        )?;
        let base = base_url(config)?;
        Self::get_json(&client, &base, "/").await?;
        Ok(ServerInfo {
            server_version: String::new(),
            server_type: "hbase".to_string(),
        })
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let client = build_http_client(
            config.connection_timeout,
            config.username.as_deref(),
            config.password.as_deref(),
        )?;
        let base = base_url(config)?;
        let pool_id = format!("hbase_{}", uuid::Uuid::new_v4());
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
        let v = Self::get_json(client, base, "/").await?;
        Ok(v.get("table")
            .and_then(|t| t.as_array())
            .into_iter()
            .flatten()
            .filter_map(|t| {
                Some(TableInfo {
                    name: t.get("name")?.as_str()?.to_string(),
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
            &format!("/{}/schema", urlencoding::encode(table)),
        )
        .await?;
        let mut columns = Vec::new();
        if let Some(families) = v
            .get("TableSchema")
            .and_then(|s| s.get("ColumnSchema"))
            .and_then(|c| c.as_array())
        {
            for family in families {
                if let Some(name) = family.get("name").and_then(|n| n.as_str()) {
                    columns.push(ColumnSchema {
                        name: name.to_string(),
                        data_type: "family".to_string(),
                        nullable: true,
                        default_value: None,
                        comment: None,
                        is_primary_key: false,
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

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        let trimmed = sql.trim();
        let table = trimmed.strip_prefix("scan ").map(|t| {
            t.trim()
                .trim_matches(|c| c == '"' || c == '\'' || c == '`')
                .to_string()
        });
        let Some(table) = table else {
            return Err(DriverError::QueryFailed(
                "HBase does not support SQL. Use `scan <table>` or the table browser.".into(),
            ));
        };
        let map = self.clients.read().await;
        let (client, base) = Self::get(&map, handle)?;
        let start = Instant::now();
        let mut result = Self::scan(client, base, &table, 100).await?;
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
        let trimmed = sql.trim();
        let table = trimmed.strip_prefix("scan ").map(|t| {
            t.trim()
                .trim_matches(|c| c == '"' || c == '\'' || c == '`')
                .to_string()
        });
        let Some(table) = table else {
            return Err(DriverError::QueryFailed(
                "HBase does not support SQL. Use `scan <table>` or the table browser.".into(),
            ));
        };
        let map = self.clients.read().await;
        let (client, base) = Self::get(&map, handle)?;
        let client = client.clone();
        let base = base.clone();
        drop(map);
        Self::stream_scan(
            &client,
            &base,
            &table,
            sql,
            limit,
            &on_event,
            Instant::now(),
        )
        .await
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
            "HBase writes are not supported through the SQL editor".into(),
        ))
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        query_only_command_definitions("Scan an HBase table. Use `scan <table>`.", "scan <table>")
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_definitions_are_query_only() {
        let ids: Vec<_> = HBaseDriver::new()
            .command_definitions()
            .into_iter()
            .map(|d| d.id)
            .collect();
        assert_eq!(ids, vec!["query"]);
    }

    #[test]
    fn decode_scan_rows_reads_key_and_cell() {
        let v = serde_json::json!({
            "Row": [{
                "key": "rk1",
                "Cell": [{ "$": "v1" }]
            }]
        });
        let rows = HBaseDriver::decode_scan_rows(&v);
        assert_eq!(rows.len(), 1);
        match &rows[0][0] {
            Some(Value::String(s)) => assert_eq!(s, "rk1"),
            other => panic!("expected key rk1, got {other:?}"),
        }
        match &rows[0][1] {
            Some(Value::String(s)) => assert_eq!(s, "v1"),
            other => panic!("expected cell v1, got {other:?}"),
        }
    }

    fn collect_events() -> (
        QueryStreamCallback,
        std::sync::Arc<std::sync::Mutex<Vec<QueryStreamEvent>>>,
    ) {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let events_cb = std::sync::Arc::clone(&events);
        (
            std::sync::Arc::new(move |ev| {
                events_cb.lock().unwrap().push(ev);
            }),
            events,
        )
    }

    fn http_config(server: &wiremock::MockServer) -> ConnectionConfig {
        let addr = server.address();
        ConnectionConfig {
            id: "hb".into(),
            name: "hb".into(),
            database_type: "hbase".into(),
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
        }
    }

    #[tokio::test]
    async fn query_stream_pages_scanner_until_no_content() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let scanner = format!("{}/scanner/1", server.uri());
        Mock::given(method("POST"))
            .and(path("/t/scanner"))
            .respond_with(ResponseTemplate::new(201).insert_header("Location", scanner.as_str()))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/scanner/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "Row": [{"key": "rk", "Cell": [{ "$": "v" }]}]
            })))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/scanner/1"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/scanner/1"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let driver = HBaseDriver::new();
        let handle = driver.connect(&http_config(&server)).await.unwrap();
        let (cb, events) = collect_events();
        driver
            .query_stream(&handle, "scan t", None, cb)
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
        assert_eq!(rows, 1);
        assert!(matches!(events.last(), Some(QueryStreamEvent::Done { .. })));
    }

    #[tokio::test]
    async fn query_stream_rejects_non_scan() {
        let driver = HBaseDriver::new();
        let handle = ConnectionHandle {
            id: "x".into(),
            pool_id: "missing".into(),
        };
        let (cb, _) = collect_events();
        let err = driver
            .query_stream(&handle, "SELECT 1", None, cb)
            .await
            .unwrap_err();
        assert!(matches!(err, DriverError::QueryFailed(_)));
    }
}

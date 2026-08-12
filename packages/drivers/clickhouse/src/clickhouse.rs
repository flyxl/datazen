//! ClickHouse driver — HTTP interface with `default_format=JSON`.
//!
//! Schema metadata is read from the `system` tables.

use async_trait::async_trait;
use datazen_driver_api::*;
use datazen_driver_http_support::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

struct PoolEntry {
    client: reqwest::Client,
    base: String,
    database: Option<String>,
}

pub struct ClickHouseDriver {
    pools: RwLock<HashMap<String, PoolEntry>>,
}

impl ClickHouseDriver {
    pub fn new() -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
        }
    }

    fn get<'a>(
        pools: &'a HashMap<String, PoolEntry>,
        handle: &ConnectionHandle,
    ) -> Result<&'a PoolEntry, DriverError> {
        pools
            .get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))
    }

    fn get_mut<'a>(
        pools: &'a mut HashMap<String, PoolEntry>,
        handle: &ConnectionHandle,
    ) -> Result<&'a mut PoolEntry, DriverError> {
        pools
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))
    }

    async fn http_query(
        client: &reqwest::Client,
        base: &str,
        sql: &str,
        database: Option<&str>,
    ) -> Result<serde_json::Value, DriverError> {
        let url = format!("{base}/");
        let mut req = client
            .post(&url)
            .query(&[("default_format", "JSON"), ("max_result_rows", "100000")]);
        if let Some(db) = database {
            let trimmed = db.trim();
            if !trimmed.is_empty() {
                req = req.query(&[("database", trimmed)]);
            }
        }
        let resp = req
            .body(sql.to_string())
            .send()
            .await
            .map_err(|e| http_error("ClickHouse request failed", e))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| http_error("ClickHouse read response failed", e))?;
        if !status.is_success() {
            return Err(status_error("ClickHouse query failed", status, &text));
        }
        serde_json::from_str(&text)
            .map_err(|e| DriverError::QueryFailed(format!("ClickHouse JSON parse failed: {e}")))
    }

    fn result_from_json(v: &serde_json::Value) -> QueryResult {
        let meta = v
            .get("meta")
            .and_then(|m| m.as_array())
            .cloned()
            .unwrap_or_default();
        let columns: Vec<ColumnInfo> = meta
            .iter()
            .filter_map(|m| {
                Some(ColumnInfo {
                    name: m.get("name")?.as_str()?.to_string(),
                    data_type: m
                        .get("type")
                        .map(|t| t.as_str().unwrap_or("").to_string())
                        .unwrap_or_default(),
                    nullable: true,
                })
            })
            .collect();
        let data = v
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        let rows: Vec<Vec<Option<Value>>> = data
            .iter()
            .map(|row| {
                columns
                    .iter()
                    .map(|c| row.get(c.name.as_str()).and_then(json_to_value))
                    .collect()
            })
            .collect();
        QueryResult {
            columns,
            rows,
            rows_affected: v.get("rows").and_then(|r| r.as_u64()),
            execution_time_ms: 0,
        }
    }

    fn stream_json(
        v: &serde_json::Value,
        sql: &str,
        limit: Option<u32>,
        on_event: &QueryStreamCallback,
        ms: u64,
    ) {
        let columns: Vec<ColumnInfo> = v
            .get("meta")
            .and_then(|m| m.as_array())
            .into_iter()
            .flatten()
            .filter_map(|m| {
                Some(ColumnInfo {
                    name: m.get("name")?.as_str()?.to_string(),
                    data_type: m
                        .get("type")
                        .map(|t| t.as_str().unwrap_or("").to_string())
                        .unwrap_or_default(),
                    nullable: true,
                })
            })
            .collect();
        let mut batcher = QueryRowBatcher::new(Arc::clone(on_event), 0, sql.to_string(), limit);
        batcher.start(columns.clone());
        if let Some(data) = v.get("data").and_then(|d| d.as_array()) {
            for row in data {
                let decoded: Vec<Option<Value>> = columns
                    .iter()
                    .map(|c| row.get(c.name.as_str()).and_then(json_to_value))
                    .collect();
                if !batcher.push(decoded) {
                    break;
                }
            }
        }
        batcher.finish(ms, v.get("rows").and_then(|r| r.as_u64()));
    }
}

#[async_trait]
impl DatabaseDriver for ClickHouseDriver {
    fn driver_type(&self) -> DatabaseType {
        "clickhouse".to_string()
    }

    fn supports_explain(&self) -> bool {
        true
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let client = build_http_client(
            config.connection_timeout,
            config.username.as_deref(),
            config.password.as_deref(),
        )?;
        let base = base_url(config)?;
        let v = Self::http_query(
            &client,
            &base,
            "SELECT version()",
            config.database.as_deref(),
        )
        .await?;
        let version = v
            .get("data")
            .and_then(|d| d.as_array())
            .and_then(|a| a.first())
            .and_then(|r| r.get("version()"))
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string();
        Ok(ServerInfo {
            server_version: version,
            server_type: "clickhouse".to_string(),
        })
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let client = build_http_client(
            config.connection_timeout,
            config.username.as_deref(),
            config.password.as_deref(),
        )?;
        let base = base_url(config)?;
        let pool_id = format!("clickhouse_{}", uuid::Uuid::new_v4());
        self.pools.write().await.insert(
            pool_id.clone(),
            PoolEntry {
                client,
                base,
                database: config.database.clone(),
            },
        );
        Ok(ConnectionHandle {
            id: pool_id.clone(),
            pool_id,
        })
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        self.pools.write().await.remove(&handle.pool_id);
        Ok(())
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        let pools = self.pools.read().await;
        let entry = Self::get(&pools, handle)?;
        let v = Self::http_query(
            &entry.client,
            &entry.base,
            "SELECT name FROM system.databases ORDER BY name",
            None,
        )
        .await?;
        Ok(v.get("data")
            .and_then(|d| d.as_array())
            .into_iter()
            .flatten()
            .filter_map(|r| r.get("name").and_then(|x| x.as_str()).map(String::from))
            .collect())
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let pools = self.pools.read().await;
        let entry = Self::get(&pools, handle)?;
        let db = if database.is_empty() {
            entry
                .database
                .clone()
                .unwrap_or_else(|| "default".to_string())
        } else {
            database.to_string()
        };
        let sql = format!(
            "SELECT name, engine FROM system.tables WHERE database = '{}' AND is_temporary = 0 ORDER BY name",
            db.replace('\'', "''")
        );
        let v = Self::http_query(&entry.client, &entry.base, &sql, Some(&db)).await?;
        Ok(v.get("data")
            .and_then(|d| d.as_array())
            .into_iter()
            .flatten()
            .filter_map(|r| {
                Some(TableInfo {
                    name: r.get("name")?.as_str()?.to_string(),
                    schema: Some(db.clone()),
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
        let pools = self.pools.read().await;
        let entry = Self::get(&pools, handle)?;
        let db = entry
            .database
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let sql = format!(
            "SELECT name, type, default_expression, comment FROM system.columns WHERE database = '{}' AND table = '{}' ORDER BY position",
            db.replace('\'', "''"),
            table.replace('\'', "''")
        );
        let v = Self::http_query(&entry.client, &entry.base, &sql, Some(&db)).await?;
        let columns: Vec<ColumnSchema> = v
            .get("data")
            .and_then(|d| d.as_array())
            .into_iter()
            .flatten()
            .filter_map(|r| {
                Some(ColumnSchema {
                    name: r.get("name")?.as_str()?.to_string(),
                    data_type: r
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string(),
                    nullable: true,
                    default_value: r
                        .get("default_expression")
                        .and_then(|d| d.as_str())
                        .map(String::from),
                    comment: r.get("comment").and_then(|c| c.as_str()).map(String::from),
                    is_primary_key: false,
                    is_auto_increment: false,
                })
            })
            .collect();
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
        let pools = self.pools.read().await;
        let entry = Self::get(&pools, handle)?;
        let start = Instant::now();
        let v =
            Self::http_query(&entry.client, &entry.base, sql, entry.database.as_deref()).await?;
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
        let pools = self.pools.read().await;
        let entry = Self::get(&pools, handle)?;
        let start = Instant::now();
        let (effective, applied) = append_select_limit(sql, limit);
        let v = Self::http_query(
            &entry.client,
            &entry.base,
            &effective,
            entry.database.as_deref(),
        )
        .await?;
        let ms = start.elapsed().as_millis() as u64;
        Self::stream_json(&v, sql, applied, &on_event, ms);
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
        let pools = self.pools.read().await;
        let entry = Self::get(&pools, handle)?;
        let v =
            Self::http_query(&entry.client, &entry.base, sql, entry.database.as_deref()).await?;
        Ok(v.get("rows").and_then(|r| r.as_u64()).unwrap_or(0))
    }

    async fn explain(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        let result = self.query(handle, &format!("EXPLAIN {sql}")).await?;
        Ok(explain_result_from_query(result))
    }

    async fn use_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<(), DriverError> {
        let trimmed = database.trim();
        if trimmed.is_empty() {
            return Err(DriverError::InvalidConfig(
                "Database name must not be empty".into(),
            ));
        }
        if trimmed.contains('\0') {
            return Err(DriverError::InvalidConfig(
                "Database name contains invalid characters".into(),
            ));
        }
        let mut pools = self.pools.write().await;
        let entry = Self::get_mut(&mut pools, handle)?;
        entry.database = Some(trimmed.to_string());
        Ok(())
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

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

    #[test]
    fn stream_json_applies_host_limit_without_holding_multi_result() {
        let v = serde_json::json!({
            "meta": [{"name": "id", "type": "UInt32"}],
            "data": [{"id": 1}, {"id": 2}, {"id": 3}],
            "rows": 3
        });
        let (cb, events) = collect_events();
        ClickHouseDriver::stream_json(&v, "SELECT id FROM t", Some(2), &cb, 9);
        let events = events.lock().unwrap();
        let rows: usize = events
            .iter()
            .filter_map(|e| match e {
                QueryStreamEvent::Rows { rows, .. } => Some(rows.len()),
                _ => None,
            })
            .sum();
        assert_eq!(rows, 2);
        assert!(matches!(
            events.last(),
            Some(QueryStreamEvent::StatementEnd {
                truncated: true,
                ..
            })
        ));
    }

    fn http_config(server: &wiremock::MockServer) -> ConnectionConfig {
        let addr = server.address();
        ConnectionConfig {
            id: "ch".into(),
            name: "ch".into(),
            database_type: "clickhouse".into(),
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
    async fn query_stream_hits_http_and_emits_done() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": [{"name": "id", "type": "UInt8"}],
                "data": [{"id": 1}, {"id": 2}],
                "rows": 2
            })))
            .mount(&server)
            .await;

        let driver = ClickHouseDriver::new();
        let handle = driver.connect(&http_config(&server)).await.unwrap();
        let (cb, events) = collect_events();
        driver
            .query_stream(&handle, "SELECT 1", None, cb)
            .await
            .unwrap();
        let events = events.lock().unwrap();
        assert!(matches!(events.last(), Some(QueryStreamEvent::Done { .. })));
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

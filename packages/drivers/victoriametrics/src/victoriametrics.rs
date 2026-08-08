//! VictoriaMetrics driver — Prometheus HTTP query API.
//!
//! The SQL editor runs PromQL expressions; results come back as instant
//! vectors with metric labels flattened into columns.

use datazen_driver_http_support::*;
use datazen_driver_api::*;
use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::RwLock;

pub struct VictoriaMetricsDriver {
    clients: RwLock<HashMap<String, (reqwest::Client, String)>>,
}

impl VictoriaMetricsDriver {
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
            .map_err(|e| http_error("VictoriaMetrics request failed", e))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| http_error("VictoriaMetrics read response failed", e))?;
        if !status.is_success() {
            return Err(status_error("VictoriaMetrics request failed", status, &text));
        }
        serde_json::from_str(&text)
            .map_err(|e| DriverError::QueryFailed(format!("VictoriaMetrics JSON parse failed: {e}")))
    }

    fn result_from_json(v: &serde_json::Value) -> QueryResult {
        let mut columns = vec![
            ColumnInfo {
                name: "__name__".to_string(),
                data_type: "string".to_string(),
                nullable: true,
            },
            ColumnInfo {
                name: "value".to_string(),
                data_type: "float".to_string(),
                nullable: true,
            },
        ];
        let mut rows = Vec::new();
        let mut seen_labels: Vec<String> = Vec::new();
        if let Some(results) = v.get("data").and_then(|d| d.get("result")).and_then(|r| r.as_array()) {
            for item in results {
                let mut row: Vec<Option<Value>> = Vec::new();
                if let Some(metric) = item.get("metric").and_then(|m| m.as_object()) {
                    for k in metric.keys() {
                        if !seen_labels.contains(k) {
                            seen_labels.push(k.clone());
                            columns.push(ColumnInfo {
                                name: k.clone(),
                                data_type: "string".to_string(),
                                nullable: true,
                            });
                        }
                    }
                    row.push(metric.get("__name__").and_then(|n| n.as_str()).map(|s| Value::String(s.to_string())));
                }
                let value = item
                    .get("value")
                    .and_then(|v2| v2.as_array())
                    .and_then(|a| a.get(1))
                    .and_then(|n| n.as_str())
                    .and_then(|s| s.parse::<f64>().ok())
                    .map(Value::Float)
                    .or_else(|| {
                        item.get("values")
                            .and_then(|v2| v2.as_array())
                            .and_then(|a| a.last())
                            .and_then(|pair| pair.as_array())
                            .and_then(|pair| pair.get(1))
                            .and_then(|n| n.as_str())
                            .and_then(|s| s.parse::<f64>().ok())
                            .map(Value::Float)
                    });
                row.push(value);
                if let Some(metric) = item.get("metric").and_then(|m| m.as_object()) {
                    for k in &seen_labels {
                        if k == "__name__" {
                            continue;
                        }
                        row.push(metric.get(k).and_then(|v2| v2.as_str()).map(|s| Value::String(s.to_string())));
                    }
                }
                rows.push(row);
            }
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
impl DatabaseDriver for VictoriaMetricsDriver {
    fn driver_type(&self) -> DatabaseType {
        "victoriametrics".to_string()
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let client =
            build_http_client(config.connection_timeout, config.username.as_deref(), config.password.as_deref())?;
        let base = base_url(config)?;
        Self::get_json(&client, &base, "/api/v1/query?query=up").await?;
        Ok(ServerInfo {
            server_version: String::new(),
            server_type: "victoriametrics".to_string(),
        })
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let client =
            build_http_client(config.connection_timeout, config.username.as_deref(), config.password.as_deref())?;
        let base = base_url(config)?;
        let pool_id = format!("vm_{}", uuid::Uuid::new_v4());
        self.clients.write().await.insert(pool_id.clone(), (client, base));
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
        let v = Self::get_json(client, base, "/api/v1/label/__name__/values").await?;
        Ok(v.get("data")
            .and_then(|d| d.as_array())
            .into_iter()
            .flatten()
            .filter_map(|n| {
                Some(TableInfo {
                    name: n.as_str()?.to_string(),
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
            &format!(
                "/api/v1/series?match[]={}",
                urlencoding::encode(&format!("{}{{}}", table))
            ),
        )
        .await?;
        let mut labels: Vec<String> = Vec::new();
        if let Some(series) = v.get("data").and_then(|d| d.as_array()) {
            for s in series {
                if let Some(metric) = s.get("metric").and_then(|m| m.as_object()) {
                    for k in metric.keys() {
                        if !labels.contains(k) {
                            labels.push(k.clone());
                        }
                    }
                }
            }
        }
        let columns = labels
            .into_iter()
            .map(|name| ColumnSchema {
                name,
                data_type: "string".to_string(),
                nullable: true,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
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

    async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError> {
        let map = self.clients.read().await;
        let (client, base) = Self::get(&map, handle)?;
        let start = Instant::now();
        let v = Self::get_json(
            client,
            base,
            &format!("/api/v1/query?query={}", urlencoding::encode(sql)),
        )
        .await?;
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
            "VictoriaMetrics is read-only via the HTTP API".into(),
        ))
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

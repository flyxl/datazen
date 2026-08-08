//! HBase driver — Apache HBase REST (Stargate) JSON API.
//!
//! HBase has no SQL surface. The query editor accepts `scan <table>`.

use super::http_support::*;
use super::*;
use async_trait::async_trait;
use std::collections::HashMap;
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
        let columns = vec![
            ColumnInfo { name: "row".to_string(), data_type: "string".to_string(), nullable: false },
            ColumnInfo { name: "cell".to_string(), data_type: "string".to_string(), nullable: true },
        ];
        let mut rows = Vec::new();
        if let Some(url) = scanner_url {
            let full = if url.starts_with("http") { url } else { format!("{base}{url}") };
            let resp2 = client
                .get(full)
                .header(reqwest::header::ACCEPT, "application/json")
                .send()
                .await
                .map_err(|e| http_error("HBase scan read failed", e))?;
            if let Ok(v) = resp2.json::<serde_json::Value>().await {
                if let Some(cells) = v.get("Row").and_then(|r| r.as_array()) {
                    for cell in cells {
                        let key = cell
                            .get("key")
                            .and_then(|k| k.as_str())
                            .map(|s| Value::String(s.to_string()));
                        let value = cell
                            .get("Cell")
                            .and_then(|c| c.as_array())
                            .and_then(|a| a.first())
                            .and_then(|c| c.get("$"))
                            .map(|v| Value::String(v.to_string()));
                        rows.push(vec![key, value]);
                    }
                }
            }
        }
        Ok(QueryResult {
            columns,
            rows,
            rows_affected: None,
            execution_time_ms: 0,
        })
    }
}

#[async_trait]
impl DatabaseDriver for HBaseDriver {
    fn driver_type(&self) -> DatabaseType {
        "hbase".to_string()
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let client =
            build_http_client(config.connection_timeout, config.username.as_deref(), config.password.as_deref())?;
        let base = base_url(config)?;
        Self::get_json(&client, &base, "/").await?;
        Ok(ServerInfo {
            server_version: String::new(),
            server_type: "hbase".to_string(),
        })
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let client =
            build_http_client(config.connection_timeout, config.username.as_deref(), config.password.as_deref())?;
        let base = base_url(config)?;
        let pool_id = format!("hbase_{}", uuid::Uuid::new_v4());
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
        let v = Self::get_json(client, base, &format!("/{}/schema", urlencoding::encode(table))).await?;
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

    async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError> {
        let trimmed = sql.trim();
        let table = trimmed
            .strip_prefix("scan ")
            .map(|t| t.trim().trim_matches(|c| c == '"' || c == '\'' || c == '`').to_string());
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

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

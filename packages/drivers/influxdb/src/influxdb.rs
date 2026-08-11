//! InfluxDB driver — v1 query API over HTTP.

use datazen_driver_http_support::*;
use datazen_driver_api::*;
use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::RwLock;

pub struct InfluxDbDriver {
    clients: RwLock<HashMap<String, (reqwest::Client, String)>>,
}

impl InfluxDbDriver {
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

    async fn query(
        client: &reqwest::Client,
        base: &str,
        database: Option<&str>,
        q: &str,
    ) -> Result<serde_json::Value, DriverError> {
        let mut req = client.get(format!("{base}/query")).query(&[("q", q)]);
        if let Some(db) = database {
            if !db.is_empty() {
                req = req.query(&[("db", db)]);
            }
        }
        let resp = req
            .send()
            .await
            .map_err(|e| http_error("InfluxDB request failed", e))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| http_error("InfluxDB read response failed", e))?;
        if !status.is_success() {
            return Err(status_error("InfluxDB query failed", status, &text));
        }
        serde_json::from_str(&text)
            .map_err(|e| DriverError::QueryFailed(format!("InfluxDB JSON parse failed: {e}")))
    }

    fn result_from_json(v: &serde_json::Value) -> QueryResult {
        let mut columns = Vec::new();
        let mut rows = Vec::new();
        if let Some(results) = v.get("results").and_then(|r| r.as_array()) {
            for series in results
                .iter()
                .filter_map(|r| r.get("series"))
                .filter_map(|s| s.as_array())
                .flatten()
            {
                if let Some(names) = series.get("columns").and_then(|c| c.as_array()) {
                    columns = names
                        .iter()
                        .filter_map(|n| {
                            Some(ColumnInfo {
                                name: n.as_str()?.to_string(),
                                data_type: "string".to_string(),
                                nullable: true,
                            })
                        })
                        .collect();
                }
                if let Some(values) = series.get("values").and_then(|v2| v2.as_array()) {
                    for row in values {
                        if let Some(arr) = row.as_array() {
                            rows.push(arr.iter().map(|x| json_to_value(x)).collect());
                        }
                    }
                }
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
impl DatabaseDriver for InfluxDbDriver {
    fn driver_type(&self) -> DatabaseType {
        "influxdb".to_string()
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let client =
            build_http_client(config.connection_timeout, config.username.as_deref(), config.password.as_deref())?;
        let base = base_url(config)?;
        Self::query(&client, &base, None, "SHOW DATABASES").await?;
        Ok(ServerInfo {
            server_version: String::new(),
            server_type: "influxdb".to_string(),
        })
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let client =
            build_http_client(config.connection_timeout, config.username.as_deref(), config.password.as_deref())?;
        let base = base_url(config)?;
        let pool_id = format!("influx_{}", uuid::Uuid::new_v4());
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

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        let map = self.clients.read().await;
        let (client, base) = Self::get(&map, handle)?;
        let v = Self::query(client, base, None, "SHOW DATABASES").await?;
        let mut dbs = Vec::new();
        if let Some(series) = v
            .get("results")
            .and_then(|r| r.as_array())
            .and_then(|a| a.first())
            .and_then(|r| r.get("series"))
            .and_then(|s| s.as_array())
            .and_then(|a| a.first())
        {
            if let Some(values) = series.get("values").and_then(|v2| v2.as_array()) {
                for row in values {
                    if let Some(name) = row.as_array().and_then(|a| a.first()).and_then(|n| n.as_str()) {
                        dbs.push(name.to_string());
                    }
                }
            }
        }
        Ok(dbs)
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let map = self.clients.read().await;
        let (client, base) = Self::get(&map, handle)?;
        let v = Self::query(client, base, Some(database), "SHOW MEASUREMENTS").await?;
        let mut tables = Vec::new();
        if let Some(series) = v
            .get("results")
            .and_then(|r| r.as_array())
            .and_then(|a| a.first())
            .and_then(|r| r.get("series"))
            .and_then(|s| s.as_array())
            .and_then(|a| a.first())
        {
            if let Some(values) = series.get("values").and_then(|v2| v2.as_array()) {
                for row in values {
                    if let Some(name) = row.as_array().and_then(|a| a.first()).and_then(|n| n.as_str()) {
                        tables.push(TableInfo {
                            name: name.to_string(),
                            schema: Some(database.to_string()),
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
        let map = self.clients.read().await;
        let (client, base) = Self::get(&map, handle)?;
        let db = String::new();
        let v = Self::query(
            client,
            base,
            Some(&db),
            &format!("SHOW FIELD KEYS FROM \"{}\"", table.replace('"', "\"\"")),
        )
        .await?;
        let mut columns = Vec::new();
        if let Some(series) = v
            .get("results")
            .and_then(|r| r.as_array())
            .and_then(|a| a.first())
            .and_then(|r| r.get("series"))
            .and_then(|s| s.as_array())
            .and_then(|a| a.first())
        {
            if let Some(values) = series.get("values").and_then(|v2| v2.as_array()) {
                for row in values {
                    let Some(arr) = row.as_array() else {
                        continue;
                    };
                    if let Some(name) = arr.first().and_then(|n| n.as_str()) {
                        columns.push(ColumnSchema {
                            name: name.to_string(),
                            data_type: arr
                                .get(1)
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
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

    async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError> {
        let map = self.clients.read().await;
        let (client, base) = Self::get(&map, handle)?;
        let start = Instant::now();
        let v = Self::query(client, base, None, sql).await?;
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

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        let map = self.clients.read().await;
        let (client, base) = Self::get(&map, handle)?;
        let resp = client
            .post(format!("{base}/query"))
            .form(&[("q", sql)])
            .send()
            .await
            .map_err(|e| http_error("InfluxDB execute request failed", e))?;
        let status = resp.status();
        let _ = resp
            .text()
            .await
            .map_err(|e| http_error("InfluxDB read response failed", e))?;
        if !status.is_success() {
            return Err(DriverError::QueryFailed(format!("InfluxDB execute failed: HTTP {}", status.as_u16())));
        }
        Ok(0)
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        statement_command_definitions(
            "Run an InfluxQL query",
            "Run an InfluxQL write/admin statement",
            "InfluxQL",
        )
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

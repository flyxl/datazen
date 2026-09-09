//! `JdbcDriver` — DatabaseDriver facade over the external Java Agent.

use async_trait::async_trait;
use datazen_driver_api::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

use crate::agent_process::AgentProcessManager;
use crate::protocol::methods;

/// Opaque session map: connection handle id → agent sessionId.
struct SessionTable {
    sessions: HashMap<String, String>,
}

pub struct JdbcDriver {
    agent: Arc<AgentProcessManager>,
    sessions: Mutex<SessionTable>,
}

impl JdbcDriver {
    /// Shared agent process for the whole host (Settings-driven launch config).
    pub fn shared() -> Self {
        Self::with_agent(AgentProcessManager::global())
    }

    pub fn new() -> Self {
        Self::shared()
    }

    pub fn with_agent(agent: Arc<AgentProcessManager>) -> Self {
        Self {
            agent,
            sessions: Mutex::new(SessionTable {
                sessions: HashMap::new(),
            }),
        }
    }

    fn map_err(e: String) -> DriverError {
        let lower = e.to_lowercase();
        if (lower.contains("not found") && lower.contains("jar"))
            || lower.contains("agent jar")
            || lower.contains("datazen-jdbc-agent")
        {
            DriverError::InvalidConfig(format!(
                "{e} — set Settings → Extensions → JDBC → Agent JAR, or DATAZEN_JDBC_AGENT_JAR"
            ))
        } else if lower.contains("failed to spawn")
            || (lower.contains("no such file") && lower.contains("java"))
            || lower.contains("cannot run program")
        {
            DriverError::InvalidConfig(format!(
                "{e} — install JRE 17+ and ensure `java` is on PATH, or set javaPath / DATAZEN_JDBC_JAVA"
            ))
        } else if lower.contains("unsupported class version")
            || lower.contains("class file version")
        {
            DriverError::InvalidConfig(format!(
                "{e} — JDBC agent requires Java 17 or newer"
            ))
        } else if lower.contains("agent.hello")
            || lower.contains("agent process exited")
            || lower.contains("agent not running")
        {
            DriverError::ConnectionFailed(format!(
                "{e} — agent failed to start or died; check JRE 17+ and agent jar path"
            ))
        } else if lower.contains("auth") || lower.contains("password") || lower.contains("login") {
            DriverError::AuthenticationFailed(e)
        } else if lower.contains("connect") || lower.contains("connection") {
            DriverError::ConnectionFailed(e)
        } else {
            DriverError::QueryFailed(e)
        }
    }

    fn open_params(config: &ConnectionConfig) -> Result<serde_json::Value, DriverError> {
        let opts = config.options.as_ref();
        let url = opts
            .and_then(|m| m.get("jdbcUrl").or_else(|| m.get("url")))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                DriverError::InvalidConfig(
                    "JDBC connection requires options.jdbcUrl (or options.url)".into(),
                )
            })?;

        let jars: Vec<String> = opts
            .and_then(|m| m.get("jars"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        let driver_class = opts
            .and_then(|m| m.get("driverClass"))
            .and_then(|v| v.as_str())
            .map(str::to_string);

        let props = opts
            .and_then(|m| m.get("props"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        let mut params = serde_json::json!({
            "url": url,
            "user": config.username.clone().unwrap_or_default(),
            "password": config.password.clone().unwrap_or_default(),
            "jars": jars,
            "props": props,
        });
        if let Some(dc) = driver_class {
            params["driverClass"] = serde_json::Value::String(dc);
        }
        Ok(params)
    }

    async fn session_id(&self, handle: &ConnectionHandle) -> Result<String, DriverError> {
        let t = self.sessions.lock().await;
        t.sessions
            .get(&handle.pool_id)
            .cloned()
            .ok_or_else(|| DriverError::ConnectionFailed("JDBC session not found".into()))
    }

    fn json_to_value(v: &serde_json::Value) -> Option<Value> {
        match v {
            serde_json::Value::Null => Some(Value::Null),
            serde_json::Value::Bool(b) => Some(Value::Bool(*b)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Some(Value::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    Some(Value::Float(f))
                } else {
                    Some(Value::String(n.to_string()))
                }
            }
            serde_json::Value::String(s) => Some(Value::String(s.clone())),
            other => Some(Value::Json(other.clone())),
        }
    }

    fn parse_columns(cols: &serde_json::Value) -> Vec<ColumnInfo> {
        cols.as_array()
            .into_iter()
            .flatten()
            .map(|c| ColumnInfo {
                name: c
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                data_type: c
                    .get("type")
                    .and_then(|x| x.as_str())
                    .unwrap_or("UNKNOWN")
                    .to_string(),
                nullable: true,
            })
            .collect()
    }

    fn parse_rows(rows: &serde_json::Value) -> Vec<Vec<Option<Value>>> {
        rows.as_array()
            .into_iter()
            .flatten()
            .map(|row| {
                row.as_array()
                    .into_iter()
                    .flatten()
                    .map(|cell| Self::json_to_value(cell))
                    .collect()
            })
            .collect()
    }

    async fn query_once(
        &self,
        session_id: &str,
        sql: &str,
        max_rows: u32,
    ) -> Result<QueryResult, DriverError> {
        let start = Instant::now();
        let result = self
            .agent
            .rpc(
                methods::QUERY_EXECUTE,
                serde_json::json!({
                    "sessionId": session_id,
                    "sql": sql,
                    "maxRows": max_rows,
                    "fetchSize": 500,
                }),
            )
            .await
            .map_err(Self::map_err)?;

        let mut rows = Self::parse_rows(result.get("rows").unwrap_or(&serde_json::Value::Null));
        let mut has_more = result
            .get("hasMore")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let cursor_id = result
            .get("cursorId")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        if let Some(ref cid) = cursor_id {
            while has_more && (rows.len() as u32) < max_rows {
                let remain = max_rows - rows.len() as u32;
                let batch = self
                    .agent
                    .rpc(
                        methods::QUERY_FETCH,
                        serde_json::json!({
                            "sessionId": session_id,
                            "cursorId": cid,
                            "maxRows": remain,
                        }),
                    )
                    .await
                    .map_err(Self::map_err)?;
                let more_rows =
                    Self::parse_rows(batch.get("rows").unwrap_or(&serde_json::Value::Null));
                has_more = batch
                    .get("hasMore")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                rows.extend(more_rows);
            }
            let _ = self
                .agent
                .rpc(
                    methods::QUERY_CLOSE,
                    serde_json::json!({
                        "sessionId": session_id,
                        "cursorId": cid,
                    }),
                )
                .await;
        }

        Ok(QueryResult {
            columns: Self::parse_columns(result.get("columns").unwrap_or(&serde_json::Value::Null)),
            rows,
            rows_affected: result.get("updateCount").and_then(|v| v.as_u64()),
            execution_time_ms: start.elapsed().as_millis() as u64,
        })
    }
}

impl Default for JdbcDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DatabaseDriver for JdbcDriver {
    fn driver_type(&self) -> DatabaseType {
        "jdbc".to_string()
    }

    fn supports_explain(&self) -> bool {
        false
    }

    fn supports_offset(&self) -> bool {
        true
    }

    fn migration_renderer(&self) -> Option<std::sync::Arc<dyn MigrationRenderer>> {
        None
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        self.agent
            .ensure_running()
            .await
            .map_err(|e| DriverError::ConnectionFailed(Self::map_err(e).to_string()))?;
        let params = Self::open_params(config)?;
        let result = self
            .agent
            .rpc(methods::SESSION_OPEN, params)
            .await
            .map_err(Self::map_err)?;
        let session_id = result
            .get("sessionId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DriverError::ConnectionFailed("session.open missing sessionId".into()))?
            .to_string();
        let pool_id = session_id.clone();
        self.sessions
            .lock()
            .await
            .sessions
            .insert(pool_id.clone(), session_id);
        Ok(ConnectionHandle {
            id: pool_id.clone(),
            pool_id,
        })
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let handle = self.connect(config).await?;
        let info = self.get_server_info(&handle).await;
        let _ = self.disconnect(handle).await;
        info
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        let session_id = {
            let mut t = self.sessions.lock().await;
            t.sessions.remove(&handle.pool_id)
        };
        if let Some(sid) = session_id {
            let _ = self
                .agent
                .rpc(
                    methods::SESSION_CLOSE,
                    serde_json::json!({ "sessionId": sid }),
                )
                .await;
        }
        Ok(())
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        let sid = self.session_id(handle).await?;
        let result = self
            .agent
            .rpc(
                methods::META_DATABASES,
                serde_json::json!({ "sessionId": sid }),
            )
            .await
            .map_err(Self::map_err)?;
        Ok(result
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|o| o.get("name").and_then(|n| n.as_str()).map(str::to_string))
            .collect())
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let sid = self.session_id(handle).await?;
        let result = self
            .agent
            .rpc(
                methods::META_TABLES,
                serde_json::json!({
                    "sessionId": sid,
                    "database": database,
                }),
            )
            .await
            .map_err(Self::map_err)?;
        Ok(result
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|o| {
                let name = o.get("name")?.as_str()?.to_string();
                let ty = o.get("type").and_then(|t| t.as_str()).unwrap_or("TABLE");
                let table_type = match ty.to_uppercase().as_str() {
                    "VIEW" => TableType::View,
                    "SYSTEM TABLE" | "SYSTEM_TABLE" => TableType::SystemTable,
                    _ => TableType::Table,
                };
                Some(TableInfo {
                    name,
                    schema: None,
                    table_type,
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
        let sid = self.session_id(handle).await?;
        let result = self
            .agent
            .rpc(
                methods::META_COLUMNS,
                serde_json::json!({
                    "sessionId": sid,
                    "table": table,
                }),
            )
            .await
            .map_err(Self::map_err)?;
        let columns: Vec<ColumnSchema> = result
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|o| {
                Some(ColumnSchema {
                    name: o.get("name")?.as_str()?.to_string(),
                    data_type: o
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("UNKNOWN")
                        .to_string(),
                    nullable: o.get("nullable").and_then(|n| n.as_bool()).unwrap_or(true),
                    default_value: None,
                    comment: None,
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
        let sid = self.session_id(handle).await?;
        self.query_once(&sid, sql, 10_000).await
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        let sid = self.session_id(handle).await?;
        let max = limit.unwrap_or(10_000);
        let result = self.query_once(&sid, sql, max).await?;
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
        on_event: datazen_driver_api::QueryStreamCallback,
    ) -> Result<(), DriverError> {
        let sid = self.session_id(handle).await?;
        crate::stream_query::stream_query(
            &self.agent,
            &sid,
            sql,
            limit,
            on_event,
            Self::parse_columns,
            Self::parse_rows,
            Self::map_err,
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

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        let sid = self.session_id(handle).await?;
        let result = self
            .agent
            .rpc(
                methods::EXEC_UPDATE,
                serde_json::json!({ "sessionId": sid, "sql": sql }),
            )
            .await
            .map_err(Self::map_err)?;
        Ok(result
            .get("updateCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0))
    }

    async fn begin_transaction(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        let sid = self.session_id(handle).await?;
        self.agent
            .rpc(methods::TX_BEGIN, serde_json::json!({ "sessionId": sid }))
            .await
            .map_err(Self::map_err)?;
        Ok(TransactionHandle {
            id: format!("tx-{}", handle.pool_id),
            connection_id: handle.pool_id.clone(),
        })
    }

    async fn commit(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        self.agent
            .rpc(
                methods::TX_COMMIT,
                serde_json::json!({ "sessionId": tx.connection_id }),
            )
            .await
            .map_err(Self::map_err)?;
        Ok(())
    }

    async fn rollback(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        self.agent
            .rpc(
                methods::TX_ROLLBACK,
                serde_json::json!({ "sessionId": tx.connection_id }),
            )
            .await
            .map_err(Self::map_err)?;
        Ok(())
    }

    async fn cancel_query(&self, handle: &ConnectionHandle) -> Result<(), DriverError> {
        if let Ok(sid) = self.session_id(handle).await {
            crate::stream_query::cancel_session_query(&self.agent, &sid).await?;
        }
        Ok(())
    }

    async fn get_server_info(&self, handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
        let _ = handle;
        Ok(ServerInfo {
            server_version: String::new(),
            server_type: "jdbc".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_type_is_jdbc() {
        let d = JdbcDriver::new();
        assert_eq!(d.driver_type(), "jdbc");
        assert!(!d.supports_explain());
    }

    #[test]
    fn open_params_require_url() {
        let cfg = ConnectionConfig {
            id: "t".into(),
            name: "t".into(),
            database_type: "jdbc".into(),
            host: None,
            port: None,
            database: None,
            schema: None,
            username: Some("sa".into()),
            password: Some("".into()),
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
        };
        assert!(JdbcDriver::open_params(&cfg).is_err());
    }

    #[test]
    fn open_params_reads_options() {
        let mut opts = serde_json::Map::new();
        opts.insert("jdbcUrl".into(), serde_json::json!("jdbc:h2:mem:test"));
        opts.insert("jars".into(), serde_json::json!(["/tmp/h2.jar"]));
        opts.insert("driverClass".into(), serde_json::json!("org.h2.Driver"));
        let cfg = ConnectionConfig {
            id: "t".into(),
            name: "t".into(),
            database_type: "jdbc".into(),
            host: None,
            port: None,
            database: None,
            schema: None,
            username: Some("sa".into()),
            password: Some("".into()),
            ssl_mode: SslMode::Disable,
            connection_timeout: 5,
            max_pool_size: 4,
            ssh_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: Some(opts),
            read_only: false,
            pinned: false,
        };
        let p = JdbcDriver::open_params(&cfg).unwrap();
        assert_eq!(p["url"], "jdbc:h2:mem:test");
        assert_eq!(p["driverClass"], "org.h2.Driver");
    }
}

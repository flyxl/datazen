//! Presto / Trino OLAP SQL drivers (prusto + trino-rust-client).

use super::*;
use async_trait::async_trait;
use prusto::auth::Auth as PrestoAuth;
use prusto::Client as PrestoClient;
use prusto::Presto as _;
use prusto::Row as PrestoRow;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use trino_rust_client::auth::Auth as TrinoAuth;
use trino_rust_client::Client as TrinoClient;
use trino_rust_client::Row as TrinoRow;
use trino_rust_client::Trino as _;

const DEFAULT_PORT: u16 = 8080;
const DEFAULT_SCHEMA: &str = "default";
const DEFAULT_USER: &str = "user";

enum OlapClient {
    Presto(PrestoClient),
    Trino(TrinoClient),
}

struct OlapSession {
    client: OlapClient,
    catalog: String,
    active_schema: Mutex<String>,
}

pub struct OlapDriver {
    db_type: DatabaseType,
    sessions: RwLock<HashMap<String, OlapSession>>,
}

impl OlapDriver {
    pub fn new(db_type: DatabaseType) -> Self {
        assert!(matches!(db_type, DatabaseType::Presto | DatabaseType::Trino));
        Self {
            db_type,
            sessions: RwLock::new(HashMap::new()),
        }
    }

    fn server_label(&self) -> &'static str {
        match self.db_type {
            DatabaseType::Presto => "Presto",
            DatabaseType::Trino => "Trino",
            _ => "OLAP",
        }
    }

    fn get_session<'a>(
        sessions: &'a HashMap<String, OlapSession>,
        handle: &ConnectionHandle,
    ) -> Result<&'a OlapSession, DriverError> {
        sessions
            .get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Session not found".into()))
    }

    fn catalog(config: &ConnectionConfig) -> Result<String, DriverError> {
        config
            .database
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| DriverError::InvalidConfig("Catalog is required".into()))
    }

    fn default_schema(config: &ConnectionConfig) -> String {
        config
            .schema
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_SCHEMA)
            .to_string()
    }

    fn host(config: &ConnectionConfig) -> Result<String, DriverError> {
        config
            .host
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| DriverError::InvalidConfig("Host is required".into()))
    }

    fn port(config: &ConnectionConfig) -> u16 {
        config.port.unwrap_or(DEFAULT_PORT)
    }

    fn username(config: &ConnectionConfig) -> String {
        config
            .username
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_USER)
            .to_string()
    }

    fn ssl_settings(ssl_mode: &SslMode) -> (bool, bool) {
        match ssl_mode {
            SslMode::Disable => (false, false),
            SslMode::Prefer => (false, true),
            SslMode::Require => (true, true),
            SslMode::VerifyCa | SslMode::VerifyFull => (true, false),
        }
    }

    fn build_client(config: &ConnectionConfig, db_type: DatabaseType) -> Result<OlapClient, DriverError> {
        let host = Self::host(config)?;
        let port = Self::port(config);
        let user = Self::username(config);
        let catalog = Self::catalog(config)?;
        let schema = Self::default_schema(config);
        let (secure, no_verify) = Self::ssl_settings(&config.ssl_mode);
        let timeout = Duration::from_secs(config.connection_timeout.max(1) as u64);

        let password = config
            .password
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());

        match db_type {
            DatabaseType::Presto => {
                let mut builder = prusto::ClientBuilder::new(&user, &host)
                    .port(port)
                    .secure(secure)
                    .catalog(&catalog)
                    .schema(&schema)
                    .client_request_timeout(timeout);
                if no_verify {
                    builder = builder.no_verify(true);
                }
                if let Some(pw) = password {
                    builder = builder.auth(PrestoAuth::new_basic(&user, Some(pw)));
                }
                let client = builder
                    .build()
                    .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
                Ok(OlapClient::Presto(client))
            }
            DatabaseType::Trino => {
                let mut builder = trino_rust_client::ClientBuilder::new(&user, &host)
                    .port(port)
                    .secure(secure)
                    .catalog(&catalog)
                    .schema(&schema)
                    .client_request_timeout(timeout);
                if no_verify {
                    builder = builder.no_verify(true);
                }
                if let Some(pw) = password {
                    builder = builder.auth(TrinoAuth::new_basic(&user, Some(pw)));
                    if !secure {
                        builder = builder.auth_http_insecure(true);
                    }
                }
                let client = builder
                    .build()
                    .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
                Ok(OlapClient::Trino(client))
            }
            _ => Err(DriverError::InvalidConfig("Unsupported OLAP type".into())),
        }
    }

    async fn run_query(
        client: &OlapClient,
        sql: String,
    ) -> Result<(Vec<ColumnInfo>, Vec<Vec<Option<Value>>>), DriverError> {
        match client {
            OlapClient::Presto(c) => {
                let ds = c
                    .get_all::<PrestoRow>(sql)
                    .await
                    .map_err(map_presto_err)?;
                let (types, data) = ds.split();
                Ok(dataset_to_result(&types, &data))
            }
            OlapClient::Trino(c) => {
                let ds = c
                    .get_all::<TrinoRow>(sql)
                    .await
                    .map_err(map_trino_err)?;
                let (types, data) = ds.split();
                Ok(dataset_to_result(&types, &data))
            }
        }
    }

    fn quote(name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }

    fn qualified_table(catalog: &str, schema: &str, table: &str) -> String {
        format!(
            "{}.{}.{}",
            Self::quote(catalog),
            Self::quote(schema),
            Self::quote(table)
        )
    }

    fn set_active_schema(session: &OlapSession, schema: &str) {
        *session.active_schema.lock().unwrap() = schema.to_string();
    }
}

fn map_presto_err(e: prusto::error::Error) -> DriverError {
    let msg = e.to_string();
    if msg.contains("Authentication") || msg.contains("401") {
        DriverError::AuthenticationFailed(msg)
    } else if msg.contains("SSL") || msg.contains("certificate") {
        DriverError::SslError(msg)
    } else {
        DriverError::QueryFailed(msg)
    }
}

fn map_trino_err(e: trino_rust_client::error::Error) -> DriverError {
    let msg = e.to_string();
    if msg.contains("Authentication") || msg.contains("401") {
        DriverError::AuthenticationFailed(msg)
    } else if msg.contains("SSL") || msg.contains("certificate") {
        DriverError::SslError(msg)
    } else {
        DriverError::QueryFailed(msg)
    }
}

fn json_to_value(v: &serde_json::Value) -> Option<Value> {
    match v {
        serde_json::Value::Null => None,
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

fn dataset_to_result<T>(types: &[(String, impl std::fmt::Debug)], rows: &[T]) -> (Vec<ColumnInfo>, Vec<Vec<Option<Value>>>)
where
    T: RowValues,
{
    let columns: Vec<ColumnInfo> = types
        .iter()
        .map(|(name, ty)| ColumnInfo {
            name: name.clone(),
            data_type: format!("{ty:?}"),
            nullable: true,
        })
        .collect();

    let result_rows: Vec<Vec<Option<Value>>> = rows
        .iter()
        .map(|row| row.values().iter().map(json_to_value).collect())
        .collect();

    (columns, result_rows)
}

trait RowValues {
    fn values(&self) -> &[serde_json::Value];
}

impl RowValues for PrestoRow {
    fn values(&self) -> &[serde_json::Value] {
        self.value()
    }
}

impl RowValues for TrinoRow {
    fn values(&self) -> &[serde_json::Value] {
        self.value()
    }
}

#[async_trait]
impl DatabaseDriver for OlapDriver {
    fn driver_type(&self) -> DatabaseType {
        self.db_type.clone()
    }

    fn skip_count_query(&self) -> bool {
        true
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let client = Self::build_client(config, self.db_type.clone())?;
        let catalog = Self::catalog(config)?;
        let schema = Self::default_schema(config);
        let pool_id = uuid::Uuid::new_v4().to_string();
        let connection_id = uuid::Uuid::new_v4().to_string();

        self.sessions.write().await.insert(
            pool_id.clone(),
            OlapSession {
                client,
                catalog,
                active_schema: Mutex::new(schema),
            },
        );

        Ok(ConnectionHandle {
            id: connection_id,
            pool_id,
        })
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let client = Self::build_client(config, self.db_type.clone())?;
        let sql = "SELECT version()".to_string();
        let start = Instant::now();
        let (cols, rows) = Self::run_query(&client, sql).await?;
        let _ = start.elapsed();

        let version = rows
            .first()
            .and_then(|r| r.first())
            .and_then(|v| v.as_ref())
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => format!("{other:?}"),
            })
            .unwrap_or_else(|| {
                cols.first()
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| "unknown".into())
            });

        Ok(ServerInfo {
            server_version: version,
            server_type: self.server_label().to_string(),
        })
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        self.sessions.write().await.remove(&handle.pool_id);
        Ok(())
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        let sessions = self.sessions.read().await;
        let s = Self::get_session(&sessions, handle)?;
        let sql = format!("SHOW SCHEMAS FROM {}", Self::quote(&s.catalog));
        let (_, rows) = Self::run_query(&s.client, sql).await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| r.into_iter().next().flatten())
            .map(|v| match v {
                Value::String(s) => s,
                other => format!("{other:?}"),
            })
            .collect())
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let sessions = self.sessions.read().await;
        let s = Self::get_session(&sessions, handle)?;
        Self::set_active_schema(s, database);

        let sql = format!(
            "SELECT table_name, table_type FROM information_schema.tables \
             WHERE table_catalog = '{}' AND table_schema = '{}' \
             ORDER BY table_name",
            s.catalog.replace('\'', "''"),
            database.replace('\'', "''"),
        );

        let (_, rows) = Self::run_query(&s.client, sql).await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let name = r.first().and_then(|v| v.as_ref())?;
                let name = match name {
                    Value::String(s) => s.clone(),
                    other => format!("{other:?}"),
                };
                let table_type = r.get(1).and_then(|v| v.as_ref()).map(|v| match v {
                    Value::String(s) if s.eq_ignore_ascii_case("VIEW") => TableType::View,
                    _ => TableType::Table,
                }).unwrap_or(TableType::Table);
                Some(TableInfo {
                    name,
                    schema: Some(database.to_string()),
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
        let (columns, primary_keys) = self.get_columns(handle, table).await?;
        let sessions = self.sessions.read().await;
        let s = Self::get_session(&sessions, handle)?;
        let schema = s.active_schema.lock().unwrap().clone();
        Ok(TableSchema {
            table_name: Self::qualified_table(&s.catalog, &schema, table),
            columns,
            primary_keys,
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
        })
    }

    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        let sessions = self.sessions.read().await;
        let s = Self::get_session(&sessions, handle)?;
        let schema = s.active_schema.lock().unwrap().clone();
        let qualified = Self::qualified_table(&s.catalog, &schema, table);
        let sql = format!("DESCRIBE {qualified}");

        let (_, rows) = Self::run_query(&s.client, sql).await?;
        let columns: Vec<ColumnSchema> = rows
            .into_iter()
            .filter_map(|r| {
                let name = r.first().and_then(|v| v.as_ref())?;
                let name = match name {
                    Value::String(s) => s.clone(),
                    other => format!("{other:?}"),
                };
                let data_type = r
                    .get(1)
                    .and_then(|v| v.as_ref())
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        other => format!("{other:?}"),
                    })
                    .unwrap_or_else(|| "unknown".into());
                Some(ColumnSchema {
                    name,
                    data_type,
                    nullable: true,
                    default_value: None,
                    comment: r.get(3).and_then(|v| v.as_ref()).and_then(|v| match v {
                        Value::String(s) if !s.is_empty() => Some(s.clone()),
                        _ => None,
                    }),
                    is_primary_key: false,
                    is_auto_increment: false,
                })
            })
            .collect();

        Ok((columns, Vec::new()))
    }

    async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError> {
        let start = Instant::now();
        let sessions = self.sessions.read().await;
        let s = Self::get_session(&sessions, handle)?;
        let (columns, rows) = Self::run_query(&s.client, sql.to_string()).await?;
        Ok(QueryResult {
            columns,
            rows,
            rows_affected: None,
            execution_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        let total_start = Instant::now();
        let statements: Vec<&str> = sql
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        let mut results = Vec::new();
        for stmt in statements {
            let start = Instant::now();
            let limited = if let Some(lim) = limit {
                if stmt.to_uppercase().starts_with("SELECT") && !stmt.to_uppercase().contains("LIMIT") {
                    format!("{stmt} LIMIT {lim}")
                } else {
                    stmt.to_string()
                }
            } else {
                stmt.to_string()
            };

            match self.query(handle, &limited).await {
                Ok(r) => {
                    results.push(StatementResult {
                        sql: limited,
                        columns: r.columns,
                        rows: r.rows,
                        rows_affected: r.rows_affected,
                        execution_time_ms: start.elapsed().as_millis() as u64,
                        truncated: false,
                    });
                }
                Err(e) => {
                    tracing::warn!(stmt, error = %e, "olap query_multi statement failed");
                }
            }
        }

        Ok(MultiQueryResult {
            results,
            total_time_ms: total_start.elapsed().as_millis() as u64,
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
        let sessions = self.sessions.read().await;
        let s = Self::get_session(&sessions, handle)?;
        match &s.client {
            OlapClient::Presto(c) => {
                c.execute(sql.to_string())
                    .await
                    .map_err(map_presto_err)?;
            }
            OlapClient::Trino(c) => {
                let res = c
                    .execute(sql.to_string())
                    .await
                    .map_err(map_trino_err)?;
                return Ok(res.update_count.unwrap_or(0));
            }
        }
        Ok(0)
    }

    async fn explain(&self, handle: &ConnectionHandle, sql: &str) -> Result<ExplainResult, DriverError> {
        let explain_sql = if sql.trim().to_uppercase().starts_with("EXPLAIN") {
            sql.to_string()
        } else {
            format!("EXPLAIN {sql}")
        };
        let result = self.query(handle, &explain_sql).await?;
        let plan_text = result
            .rows
            .iter()
            .filter_map(|row| {
                row.first().and_then(|v| v.as_ref()).map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => format!("{other:?}"),
                })
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ExplainResult {
            plan_text,
            plan_json: None,
            total_cost: None,
            estimated_rows: None,
        })
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_to_value_converts_scalars() {
        assert!(json_to_value(&serde_json::json!(null)).is_none());
        assert!(matches!(
            json_to_value(&serde_json::json!(true)),
            Some(Value::Bool(true))
        ));
        assert!(matches!(
            json_to_value(&serde_json::json!(42)),
            Some(Value::Integer(42))
        ));
        assert!(matches!(
            json_to_value(&serde_json::json!("hello")),
            Some(Value::String(s)) if s == "hello"
        ));
    }

    #[test]
    fn qualified_table_quotes_identifiers() {
        assert_eq!(
            OlapDriver::qualified_table("hive", "default", "users"),
            "\"hive\".\"default\".\"users\""
        );
    }
}

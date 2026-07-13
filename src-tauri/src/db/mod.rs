//! Database driver abstraction and shared types.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Database type enum (aligned with frontend).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    PostgreSQL,
    MySQL,
    MariaDB,
    SQLite,
    Redis,
    Kiwi,
    Presto,
    Trino,
}

/// High-level driver category (SQL vs key-value vs document).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DriverCategory {
    Sql,
    KeyValue,
    Document,
}

/// SSL mode for remote databases.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SslMode {
    #[default]
    Disable,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

/// SSH tunnel configuration (optional).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshTunnelConfig {
    #[serde(default = "default_ssh_enabled")]
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(default = "default_auth_method")]
    pub auth_method: String,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub passphrase: Option<String>,
}

fn default_ssh_enabled() -> bool {
    true
}
fn default_auth_method() -> String {
    "password".to_string()
}

/// Saved / runtime connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionConfig {
    pub id: String,
    pub name: String,
    pub database_type: DatabaseType,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    /// OLAP schema (Presto/Trino); defaults to `default` when unset.
    pub schema: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default)]
    pub ssl_mode: SslMode,
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout: u32,
    pub ssh_tunnel: Option<SshTunnelConfig>,
    pub color_tag: Option<String>,
    pub group: Option<String>,
    pub last_connected_at: Option<String>,
}

fn default_connection_timeout() -> u32 {
    30
}

/// Cell value in result grids / filters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Timestamp(String),
    Json(serde_json::Value),
}

impl Default for Value {
    fn default() -> Self {
        Value::Null
    }
}

/// Column metadata for ad-hoc query results.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

/// Result of executing arbitrary SQL.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<Option<Value>>>,
    pub rows_affected: Option<u64>,
    pub execution_time_ms: u64,
}

/// Result of executing multiple SQL statements.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiQueryResult {
    pub results: Vec<StatementResult>,
    pub total_time_ms: u64,
}

/// Result of a single SQL statement within a multi-statement execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementResult {
    pub sql: String,
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<Option<Value>>>,
    pub rows_affected: Option<u64>,
    pub execution_time_ms: u64,
    #[serde(default)]
    pub truncated: bool,
}

/// Active pooled connection handle.
#[derive(Debug, Clone)]
pub struct ConnectionHandle {
    pub id: String,
    pub pool_id: String,
}

/// Opaque transaction handle (driver-specific).
#[derive(Debug)]
pub struct TransactionHandle {
    pub id: String,
    pub connection_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub server_version: String,
    pub server_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TableType {
    Table,
    View,
    MaterializedView,
    SystemTable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub name: String,
    pub schema: Option<String>,
    pub table_type: TableType,
    pub row_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSchema {
    pub table_name: String,
    pub columns: Vec<ColumnSchema>,
    pub primary_keys: Vec<String>,
    pub indexes: Vec<IndexInfo>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSchema {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub comment: Option<String>,
    pub is_primary_key: bool,
    pub is_auto_increment: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_primary: bool,
    pub index_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
    pub on_update: String,
    pub on_delete: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainResult {
    pub plan_text: String,
    pub plan_json: Option<serde_json::Value>,
    pub total_cost: Option<f64>,
    pub estimated_rows: Option<i64>,
}

/// Paginated table grid payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableDataResult {
    pub columns: Vec<ColumnSchema>,
    pub rows: Vec<Vec<Option<Value>>>,
    pub total_rows: Option<i64>,
    pub page: u32,
    pub page_size: u32,
}

/// One key from a Redis SCAN with metadata (for KV browser).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyEntry {
    pub key: String,
    pub key_type: String,
    pub ttl: i64,
    pub size: u64,
    pub preview: String,
}

/// Full value for a single Redis key (for KV detail view).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyDetail {
    pub key: String,
    pub key_type: String,
    pub ttl: i64,
    pub value: serde_json::Value,
}

#[derive(Debug, Error)]
pub enum DriverError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Query failed: {0}")]
    QueryFailed(String),

    #[error("Connection timeout")]
    ConnectionTimeout,

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("SSL error: {0}")]
    SslError(String),

    #[error("SSH tunnel error: {0}")]
    SshTunnelError(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Driver not found for type: {0:?}")]
    DriverNotFound(DatabaseType),

    #[error("Connection pool exhausted")]
    PoolExhausted,

    #[error("Transaction error: {0}")]
    TransactionError(String),
}

#[async_trait]
pub trait DatabaseDriver: Send + Sync {
    fn driver_type(&self) -> DatabaseType;

    fn driver_category(&self) -> DriverCategory {
        DriverCategory::Sql
    }

    /// Identifier quote character. Override for non-standard quoting (e.g. MySQL uses backtick).
    fn quote_char(&self) -> char {
        '"'
    }

    /// Quote an identifier according to this driver's convention.
    fn quote_ident(&self, name: &str) -> String {
        let q = self.quote_char();
        if q == '`' {
            format!("`{}`", name.replace('`', "``"))
        } else {
            format!("\"{}\"", name.replace('"', "\"\""))
        }
    }

    /// Whether COUNT(*) should be skipped when fetching paginated table data.
    fn skip_count_query(&self) -> bool {
        false
    }

    /// Format a value for SQL literals (UPDATE/INSERT). MySQL uses 1/0 for bool; PG uses TRUE/FALSE.
    fn format_sql_literal(&self, value: &Option<Value>) -> String {
        match value {
            None | Some(Value::Null) => "NULL".to_string(),
            Some(Value::Bool(b)) => {
                if *b {
                    "TRUE".to_string()
                } else {
                    "FALSE".to_string()
                }
            }
            Some(Value::Integer(i)) => i.to_string(),
            Some(Value::Float(f)) => f.to_string(),
            Some(Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(Value::Bytes(b)) => format!("'{}'", String::from_utf8_lossy(b).replace('\'', "''")),
            Some(Value::Timestamp(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(Value::Json(j)) => format!("'{}'", j.to_string().replace('\'', "''")),
        }
    }

    /// Build UPDATE statement for row edit.
    fn build_update_sql(
        &self,
        table: &str,
        set_columns: &[(&str, Option<Value>)],
        pk_columns: &[(&str, Option<Value>)],
    ) -> String {
        let set_clauses: Vec<String> = set_columns
            .iter()
            .map(|(col, val)| {
                format!(
                    "{} = {}",
                    self.quote_ident(col),
                    self.format_sql_literal(val)
                )
            })
            .collect();
        let where_clauses: Vec<String> = pk_columns
            .iter()
            .map(|(col, val)| match val {
                None | Some(Value::Null) => format!("{} IS NULL", self.quote_ident(col)),
                Some(v) => format!(
                    "{} = {}",
                    self.quote_ident(col),
                    self.format_sql_literal(&Some(v.clone()))
                ),
            })
            .collect();
        format!(
            "UPDATE {} SET {} WHERE {}",
            self.quote_ident(table),
            set_clauses.join(", "),
            where_clauses.join(" AND ")
        )
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError>;

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError>;

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError>;

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError>;

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Vec<TableInfo>, DriverError>;

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError>;

    /// Lightweight: only fetch column info (no indexes/foreign keys).
    /// Default delegates to get_table_schema; drivers may override for speed.
    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        let schema = self.get_table_schema(handle, table).await?;
        Ok((schema.columns, schema.primary_keys))
    }

    async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError>;

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError>;

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<QueryResult, DriverError>;

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError>;

    async fn begin_transaction(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        Err(DriverError::TransactionError("Not supported for this driver type".into()))
    }

    async fn commit(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        Err(DriverError::TransactionError("Not supported for this driver type".into()))
    }

    async fn rollback(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        Err(DriverError::TransactionError("Not supported for this driver type".into()))
    }

    async fn explain(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        Err(DriverError::QueryFailed("Not supported for this driver type".into()))
    }

    async fn cancel_query(&self, handle: &ConnectionHandle) -> Result<(), DriverError>;
}

pub mod olap;
pub mod kiwi;
pub mod mysql;
pub mod postgres;
pub mod redis_driver;
pub mod sqlite;
pub mod registry;
pub mod traits;

pub use registry::{init_drivers, DriverRegistry};

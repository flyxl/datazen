//! Shared types for database drivers.

use serde::{Deserialize, Serialize};
use thiserror::Error;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DriverCategory {
    Sql,
    KeyValue,
    Document,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionConfig {
    pub id: String,
    pub name: String,
    pub database_type: DatabaseType,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<Option<Value>>>,
    pub rows_affected: Option<u64>,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiQueryResult {
    pub results: Vec<StatementResult>,
    pub total_time_ms: u64,
}

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

#[derive(Debug, Clone)]
pub struct ConnectionHandle {
    pub id: String,
    pub pool_id: String,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableDataResult {
    pub columns: Vec<ColumnSchema>,
    pub rows: Vec<Vec<Option<Value>>>,
    pub total_rows: Option<i64>,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyEntry {
    pub key: String,
    pub key_type: String,
    pub ttl: i64,
    pub size: u64,
    pub preview: String,
}

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

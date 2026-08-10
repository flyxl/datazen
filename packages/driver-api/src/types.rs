//! Shared types for database drivers.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Database type identifier — a plain string (e.g. "postgresql", "mysql", "superset").
/// Plugins define their own identifiers without modifying this crate.
pub type DatabaseType = String;

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
    pub server_version: Option<String>,
    /// Opaque per-driver connection options (e.g. Redis topology/TLS).
    #[serde(default)]
    pub options: Option<serde_json::Map<String, serde_json::Value>>,
}

fn default_connection_timeout() -> u32 {
    30
}

#[cfg(test)]
mod connection_config_tests {
    use super::*;
    use serde_json::json;

    fn dummy_connection_config() -> ConnectionConfig {
        ConnectionConfig {
            id: "test-id".into(),
            name: "Test".into(),
            database_type: "redis".into(),
            host: Some("127.0.0.1".into()),
            port: Some(6379),
            database: Some("0".into()),
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::Disable,
            connection_timeout: 30,
            ssh_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
        }
    }

    #[test]
    fn connection_options_roundtrip() {
        let mut opts = serde_json::Map::new();
        opts.insert("topology".into(), json!("cluster"));
        let c = ConnectionConfig {
            options: Some(opts),
            ..dummy_connection_config()
        };
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["options"]["topology"], "cluster");

        let restored: ConnectionConfig = serde_json::from_value(v).unwrap();
        assert_eq!(
            restored.options.as_ref().unwrap()["topology"],
            json!("cluster")
        );
    }

    #[test]
    fn connection_options_default_missing() {
        let json = json!({
            "id": "x",
            "name": "n",
            "databaseType": "redis",
        });
        let c: ConnectionConfig = serde_json::from_value(json).unwrap();
        assert!(c.options.is_none());
    }
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

/// Identifies an AI prompt scenario.
///
/// Each scenario has a default system prompt template built into the main app.
/// Drivers can override per-scenario prompts via [`DatabaseDriver::prompt_overrides`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptScenario {
    #[serde(rename = "nl2sql")]
    Nl2Sql,
    Diagnose,
    #[serde(rename = "nl_filter")]
    NlFilter,
    #[serde(rename = "schema_doc_select_tables")]
    SchemaDocSelectTables,
    #[serde(rename = "schema_doc")]
    SchemaDoc,
    #[serde(rename = "connection_diagnose")]
    ConnectionDiagnose,
    #[serde(rename = "query_summary")]
    QuerySummary,
    #[serde(rename = "explain_analysis")]
    ExplainAnalysis,
    Chat,
    #[serde(rename = "workflow_generate")]
    WorkflowGenerate,
}

impl PromptScenario {
    pub fn all() -> &'static [PromptScenario] {
        &[
            Self::Nl2Sql,
            Self::Diagnose,
            Self::NlFilter,
            Self::SchemaDocSelectTables,
            Self::SchemaDoc,
            Self::ConnectionDiagnose,
            Self::QuerySummary,
            Self::ExplainAnalysis,
            Self::Chat,
            Self::WorkflowGenerate,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Nl2Sql => "NL → SQL",
            Self::Diagnose => "SQL Error Diagnosis",
            Self::NlFilter => "NL Filter",
            Self::SchemaDocSelectTables => "Schema Doc (Table Selection)",
            Self::SchemaDoc => "Schema Documentation",
            Self::ConnectionDiagnose => "Connection Diagnosis",
            Self::QuerySummary => "Query Summary",
            Self::ExplainAnalysis => "EXPLAIN Analysis",
            Self::Chat => "AI Chat",
            Self::WorkflowGenerate => "Workflow Generate",
        }
    }
}

impl std::fmt::Display for PromptScenario {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| format!("{:?}", self));
        f.write_str(&s)
    }
}

/// A bilingual prompt template for a specific scenario.
///
/// Templates can contain `{{variable}}` placeholders that get substituted at
/// runtime. Available variables depend on the scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptTemplate {
    pub system_zh: String,
    pub system_en: String,
}

/// Options for driver-native SQL database dumps.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackupDumpOptions {
    pub schema_only: bool,
    pub data_only: bool,
    /// Emit `DROP TABLE IF EXISTS` before each table.
    pub clean: bool,
    /// Emit a driver-specific `CREATE DATABASE` preamble.
    pub create_database: bool,
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

    #[error("Driver not found for type: {0}")]
    DriverNotFound(DatabaseType),

    #[error("Connection pool exhausted")]
    PoolExhausted,

    #[error("Transaction error: {0}")]
    TransactionError(String),

    #[error("Not supported: {0}")]
    NotSupported(String),

    #[error("Unsupported: {0}")]
    Unsupported(String),
}

/// Table structure editor capability flags returned by drivers.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StructureCapabilities {
    pub create_table: bool,
    pub add_column: bool,
    pub drop_column: bool,
    pub rename_column: bool,
    pub alter_type: bool,
    pub alter_nullability: bool,
    pub alter_default: bool,
    pub alter_primary_key: bool,
    pub reorder_column: bool,
    pub comment: bool,
    pub create_index: bool,
    pub drop_index: bool,
    pub rebuild_index: bool,
    pub index_type: bool,
    pub index_include: bool,
    pub index_filter: bool,
    pub index_comment: bool,
    pub alter_strategy: AlterStrategy,
    pub dialect_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub index_methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum AlterStrategy {
    #[default]
    None,
    Direct,
    SqliteRebuild,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StructureChangeMode {
    Create,
    Alter,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StructureColumnDraft {
    pub id: String,
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(default)]
    pub is_primary_key: bool,
    #[serde(default)]
    pub is_auto_increment: bool,
    #[serde(default)]
    pub is_unique: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StructureIndexDraft {
    pub id: String,
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    #[serde(default)]
    pub is_primary: bool,
    #[serde(default)]
    pub index_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include_columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StructureChangeRequest {
    pub mode: StructureChangeMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub table: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub original_columns: Vec<StructureColumnDraft>,
    pub current_columns: Vec<StructureColumnDraft>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub original_indexes: Vec<StructureIndexDraft>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub current_indexes: Vec<StructureIndexDraft>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatementRisk {
    Additive,
    Destructive,
    Rewrite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanStatement {
    pub sql: String,
    pub summary: String,
    pub risk: StatementRisk,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct StructureChangePlan {
    pub statements: Vec<PlanStatement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod backup_dump_options_tests {
    use super::*;

    #[test]
    fn backup_dump_options_default_is_all_false() {
        let opts = BackupDumpOptions::default();
        assert!(!opts.schema_only);
        assert!(!opts.data_only);
        assert!(!opts.clean);
        assert!(!opts.create_database);
    }

    #[test]
    fn not_supported_display() {
        let err = DriverError::NotSupported("create_database".into());
        assert!(err.to_string().contains("Not supported"));
        assert!(err.to_string().contains("create_database"));
    }
}

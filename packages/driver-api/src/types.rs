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

/// DDL atomicity semantics reported by a driver.
///
/// Host code uses this to decide whether schema-diff deploy and similar
/// multi-statement DDL paths should wrap operations in a real transaction.
/// Drivers that do not override [`crate::DatabaseDriver::ddl_atomicity`]
/// return [`Self::Unknown`], which disables transactional wrapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DdlAtomicity {
    /// DDL participates in the surrounding transaction (e.g. PostgreSQL, SQLite).
    Transactional,
    /// Each DDL statement auto-commits (e.g. MySQL / MariaDB).
    AutoCommitPerStatement,
    /// Semantics are unknown; the host executes statements without wrapping.
    Unknown,
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
    /// Optional first hop (ProxyJump). Nested `jump` is allowed.
    #[serde(default)]
    pub jump: Option<Box<SshTunnelConfig>>,
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
    /// Max connections in the driver's sqlx pool (host injects from AppSettings on connect).
    #[serde(default = "default_max_pool_size")]
    pub max_pool_size: u32,
    pub ssh_tunnel: Option<SshTunnelConfig>,
    pub color_tag: Option<String>,
    pub group: Option<String>,
    pub last_connected_at: Option<String>,
    pub server_version: Option<String>,
    /// Opaque per-driver connection options (e.g. Redis topology/TLS).
    #[serde(default)]
    pub options: Option<serde_json::Map<String, serde_json::Value>>,
    /// When true, mutating SQL and row edits are rejected by the host.
    #[serde(default)]
    pub read_only: bool,
    /// When true, the connection is sorted first within its group in the navigator.
    #[serde(default)]
    pub pinned: bool,
}

fn default_connection_timeout() -> u32 {
    30
}

fn default_max_pool_size() -> u32 {
    10
}

impl ConnectionConfig {
    /// Pool size for sqlx drivers: at least 1, capped at 100.
    pub fn effective_max_pool_size(&self) -> u32 {
        self.max_pool_size.clamp(1, 100)
    }
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
            max_pool_size: 10,
            ssh_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
            pinned: false,
        }
    }

    #[test]
    fn max_pool_size_defaults_when_missing() {
        let json = json!({
            "id": "x",
            "name": "n",
            "databaseType": "postgresql",
        });
        let c: ConnectionConfig = serde_json::from_value(json).unwrap();
        assert_eq!(c.max_pool_size, 10);
        assert_eq!(c.effective_max_pool_size(), 10);
        assert!(!c.read_only);
        assert!(!c.pinned);
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

/// Opaque identity for one query execution.
///
/// Drivers may associate this token with private backend state (for example a
/// PostgreSQL backend PID or a MySQL thread id), but that state must never be
/// put in the token or exposed to the host/UI. The host creates one fresh id
/// for every streamed execution.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct QueryExecutionId(String);

impl QueryExecutionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
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

impl TableSchema {
    /// Effective primary key columns. Prefers `primary_keys` field;
    /// falls back to columns marked `is_primary_key`.
    pub fn effective_primary_keys(&self) -> Vec<String> {
        if !self.primary_keys.is_empty() {
            return self.primary_keys.clone();
        }
        self.columns
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.clone())
            .collect()
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExplainPlanDetail {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExplainPlanNode {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<i64>,
    #[serde(default)]
    pub details: Vec<ExplainPlanDetail>,
    #[serde(default)]
    pub children: Vec<ExplainPlanNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainResult {
    pub plan_text: String,
    pub plan_json: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_tree: Option<ExplainPlanNode>,
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
    /// Omit `OWNER` clauses (PostgreSQL); documented no-op when not emitted.
    pub no_owner: bool,
    /// Consistent dump snapshot (mysqldump `--single-transaction`).
    /// This is a **dump-time** isolation hint, not a restore transaction flag.
    pub single_transaction: bool,
    /// Include stored procedures and functions.
    pub routines: bool,
    /// Include triggers.
    pub triggers: bool,
}

/// Progress event while dumping a database (one object at a time).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DumpProgress {
    pub current: u32,
    pub total: u32,
    pub object_name: String,
    pub phase: DumpPhase,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DumpPhase {
    Object,
    Writing,
    Done,
}

/// Options for SQL restore operations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackupRestoreOptions {
    /// Execute the restore inside `BEGIN`/`COMMIT` (rolls back on failure).
    pub single_transaction: bool,
    /// Drop existing tables/views in the target database before applying the dump.
    pub overwrite: bool,
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

    #[error("Query execution not found: {0}")]
    QueryExecutionNotFound(String),

    #[error("Query execution belongs to a different session")]
    QueryExecutionSessionMismatch,

    #[error("Query cancelled")]
    QueryCancelled,
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
mod tests {
    use super::*;

    fn col(name: &str, pk: bool) -> ColumnSchema {
        ColumnSchema {
            name: name.into(),
            data_type: "INT".into(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: pk,
            is_auto_increment: false,
        }
    }

    fn table_schema(columns: Vec<ColumnSchema>, primary_keys: Vec<&str>) -> TableSchema {
        TableSchema {
            table_name: "t".into(),
            columns,
            primary_keys: primary_keys.into_iter().map(str::to_string).collect(),
            indexes: vec![],
            foreign_keys: vec![],
        }
    }

    #[test]
    fn effective_primary_keys_prefers_primary_keys_field() {
        let schema = table_schema(vec![col("id", false), col("other", false)], vec!["id"]);
        assert_eq!(schema.effective_primary_keys(), vec!["id"]);
    }

    #[test]
    fn effective_primary_keys_falls_back_to_column_flags() {
        let schema = table_schema(vec![col("id", true), col("other", false)], vec![]);
        assert_eq!(schema.effective_primary_keys(), vec!["id"]);
    }

    #[test]
    fn effective_primary_keys_composite_from_field() {
        let schema = table_schema(vec![col("a", false), col("b", false)], vec!["a", "b"]);
        assert_eq!(schema.effective_primary_keys(), vec!["a", "b"]);
    }

    #[test]
    fn effective_primary_keys_empty_when_none_marked() {
        let schema = table_schema(vec![col("id", false)], vec![]);
        assert!(schema.effective_primary_keys().is_empty());
    }

    #[test]
    fn backup_dump_options_default_is_all_false() {
        let opts = BackupDumpOptions::default();
        assert!(!opts.schema_only);
        assert!(!opts.data_only);
        assert!(!opts.clean);
        assert!(!opts.create_database);
        assert!(!opts.no_owner);
        assert!(!opts.single_transaction);
        assert!(!opts.routines);
        assert!(!opts.triggers);
    }

    #[test]
    fn backup_restore_options_default_is_all_false() {
        let opts = BackupRestoreOptions::default();
        assert!(!opts.single_transaction);
    }

    #[test]
    fn not_supported_display() {
        let err = DriverError::NotSupported("create_database".into());
        assert!(err.to_string().contains("Not supported"));
        assert!(err.to_string().contains("create_database"));
    }

    #[test]
    fn test_tester_ddl_atomicity_serde_roundtrip() {
        for value in [
            DdlAtomicity::Transactional,
            DdlAtomicity::AutoCommitPerStatement,
            DdlAtomicity::Unknown,
        ] {
            let json = serde_json::to_string(&value).unwrap();
            let decoded: DdlAtomicity = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, value);
        }
        assert_eq!(
            serde_json::to_string(&DdlAtomicity::Transactional).unwrap(),
            "\"transactional\""
        );
        assert_eq!(
            serde_json::to_string(&DdlAtomicity::AutoCommitPerStatement).unwrap(),
            "\"autoCommitPerStatement\""
        );
    }
}

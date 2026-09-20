#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptTemplate {
    #[serde(alias = "systemEn", alias = "system_en")]
    pub system: String,
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

    #[error("HTTP proxy tunnel error: {0}")]
    HttpProxyTunnelError(String),

    #[error("WebSocket tunnel error: {0}")]
    WebSocketTunnelError(String),

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

//! MCP tool and prompt input types.

use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryInput {
    /// Persistent connection id (from list_connections)
    pub connection_id: String,
    /// SQL query to execute
    pub sql: String,
    /// Maximum rows to return (default: 100, max: 50000)
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListTablesInput {
    /// Persistent connection id (from list_connections)
    pub connection_id: String,
    /// Optional database name
    pub database: Option<String>,
    /// Optional schema; omit to list every schema
    pub schema: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchTablesInput {
    /// Persistent connection id (from list_connections)
    pub connection_id: String,
    /// Optional database name
    pub database: Option<String>,
    /// Optional schema; omit to search every schema
    pub schema: Option<String>,
    /// Search keyword to match against table names (case-insensitive)
    pub pattern: String,
    /// Max results to return (default: 20)
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSchemaInput {
    /// Persistent connection id (from list_connections)
    pub connection_id: String,
    /// Table name
    pub table: String,
    /// Optional database name; defaults to the connection's database
    pub database: Option<String>,
    /// Optional schema; required for schema-aware engines (PostgreSQL, SQL Server)
    pub schema: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExplainQueryInput {
    /// Persistent connection id (from list_connections)
    pub connection_id: String,
    /// SQL query to analyze
    pub sql: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DescribeTableInput {
    /// Persistent connection id (from list_connections)
    pub connection_id: String,
    /// Table name
    pub table: String,
    /// Optional database name; defaults to the connection's database
    pub database: Option<String>,
    /// Optional schema; required for schema-aware engines (PostgreSQL, SQL Server)
    pub schema: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListDatabasesInput {
    /// Persistent connection id (from list_connections)
    pub connection_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunWorkflowInput {
    /// Workflow ID to execute
    pub workflow_id: String,
    /// Input variables for the workflow (JSON object)
    #[serde(default)]
    pub variables: serde_json::Value,
    /// Optional persistent connection id (some workflows require a database connection)
    pub connection_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Nl2SqlArgs {
    /// Persistent connection id for schema context
    pub connection_id: String,
    /// Natural language description of the query
    pub question: String,
    /// Optional database name
    pub database: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DiagnoseErrorArgs {
    /// Persistent connection id
    pub connection_id: String,
    /// The SQL that caused the error
    pub sql: String,
    /// The error message
    pub error: String,
}

/// Stateful tool input: open a session on an explicit target (summary §4.2).
#[derive(Debug, Deserialize, JsonSchema)]
pub struct OpenSessionInput {
    /// Persistent connection id (from list_connections).
    pub connection_id: String,
    /// Explicit database to open the session at. Required on purpose: the GUI
    /// active target is never consulted, and a driver default is not an
    /// explicit target.
    pub database: String,
    /// Explicit schema, when the driver has one.
    pub schema: Option<String>,
    /// Requested mapping lifetime in milliseconds (default 300000, capped at 3600000).
    #[serde(default)]
    pub ttl_ms: Option<u64>,
}

/// Stateful tool input: run one command inside a mapping this client opened.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct UseSessionInput {
    /// Opaque reference returned by open_session.
    pub client_session_ref: String,
    /// Driver command name (for example `query`, `execute`, `list_tables`).
    pub command: String,
    /// Command input object.
    #[serde(default)]
    pub input: serde_json::Value,
    /// Milliseconds to wait for a terminal state (default 60000, capped at 300000).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Stateful tool input: close one mapping this client opened.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CloseSessionInput {
    /// Opaque reference returned by open_session.
    pub client_session_ref: String,
    /// `requireNoTransaction` (default) or `rollbackAndClose`. Anything else is rejected:
    /// closing must never discard uncommitted work on a guess.
    #[serde(default)]
    pub mode: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExplainPlanArgs {
    /// Persistent connection id
    pub connection_id: String,
    /// The SQL query to explain
    pub sql: String,
}

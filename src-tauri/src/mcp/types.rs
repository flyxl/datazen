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
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchTablesInput {
    /// Persistent connection id (from list_connections)
    pub connection_id: String,
    /// Optional database name
    pub database: Option<String>,
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExplainPlanArgs {
    /// Persistent connection id
    pub connection_id: String,
    /// The SQL query to explain
    pub sql: String,
}

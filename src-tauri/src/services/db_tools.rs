//! Shared database tool helpers used by both ai_chat tool execution and MCP Server tools.

pub const MCP_QUERY_DEFAULT_LIMIT: u32 = 100;
pub const MCP_QUERY_MAX_LIMIT: u32 = 50_000;

use crate::mcp::permission::{self, McpPermissionMode};
use crate::services::ConnectionManager;
use crate::store::Store;
use datazen_driver_api::{ConnectionHandle, DatabaseDriver};
use std::sync::Arc;

/// Resolve a connection from a **config_id** (persistent UUID from `list_connections`).
/// Tries an existing runtime handle first, then connects via `get_or_connect`.
/// Callers from MCP/AI tools should pass `config_id`; runtime `connection_id` is still accepted internally.
pub async fn resolve_connection(
    connection_manager: &ConnectionManager,
    config_id: &str,
) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), String> {
    if let Ok(conn) = connection_manager.get_connection(config_id).await {
        return Ok(conn);
    }
    let conn_id = connection_manager
        .connect(config_id)
        .await
        .map_err(|e| format!("Cannot connect to '{config_id}': {e}"))?;
    connection_manager
        .get_connection(&conn_id)
        .await
        .map_err(|e| format!("Connection error: {e}"))
}

/// List all configured connections as a JSON string.
pub async fn list_connections(store: &Store) -> Result<String, String> {
    let connections = store.get_connections().await;
    let result: Vec<serde_json::Value> = connections
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "name": c.name,
                "databaseType": format!("{:?}", c.database_type),
                "host": c.host,
                "database": c.database,
            })
        })
        .collect();
    serde_json::to_string_pretty(&result).map_err(|e| format!("Error: {e}"))
}

/// List all databases on a connection identified by config_id.
pub async fn list_databases(
    connection_manager: &ConnectionManager,
    config_id: &str,
) -> Result<String, String> {
    let (driver, handle) = resolve_connection(connection_manager, config_id).await?;
    let dbs = driver
        .get_databases(&handle)
        .await
        .map_err(|e| format!("Error listing databases: {e}"))?;
    serde_json::to_string_pretty(&dbs).map_err(|e| format!("Error: {e}"))
}

/// List all tables in a database on a connection identified by config_id.
pub async fn list_tables(
    connection_manager: &ConnectionManager,
    config_id: &str,
    database: &str,
) -> Result<String, String> {
    let (driver, handle) = resolve_connection(connection_manager, config_id).await?;
    let tables = driver
        .get_tables(&handle, database)
        .await
        .map_err(|e| format!("Error listing tables: {e}"))?;
    serde_json::to_string_pretty(&tables).map_err(|e| format!("Error: {e}"))
}

/// Get detailed schema for one or more tables on a connection identified by config_id.
pub async fn get_table_schema(
    connection_manager: &ConnectionManager,
    config_id: &str,
    tables: &[String],
) -> Result<String, String> {
    let (driver, handle) = resolve_connection(connection_manager, config_id).await?;
    let mut results = Vec::new();
    for table in tables {
        match driver.get_table_schema(&handle, table).await {
            Ok(schema) => results.push(serde_json::to_value(&schema).unwrap_or_default()),
            Err(e) => results.push(serde_json::json!({"table": table, "error": e.to_string()})),
        }
    }
    serde_json::to_string_pretty(&results).map_err(|e| format!("Error: {e}"))
}

/// Get a single table's schema on a connection identified by config_id.
pub async fn get_single_table_schema(
    connection_manager: &ConnectionManager,
    config_id: &str,
    table: &str,
) -> Result<datazen_driver_api::TableSchema, String> {
    let (driver, handle) = resolve_connection(connection_manager, config_id).await?;
    driver
        .get_table_schema(&handle, table)
        .await
        .map_err(|e| format!("Error getting schema: {e}"))
}

/// Resolve MCP query row limit: default 100 when omitted, hard cap 50_000.
pub fn resolve_query_limit(limit: Option<u32>) -> Option<u32> {
    let resolved = limit.unwrap_or(MCP_QUERY_DEFAULT_LIMIT);
    Some(resolved.min(MCP_QUERY_MAX_LIMIT))
}

/// Execute a SQL query on a connection identified by config_id.
/// When `permission_mode` is `Some`, SQL is checked against MCP permission rules.
pub async fn query(
    connection_manager: &ConnectionManager,
    config_id: &str,
    sql: &str,
    limit: Option<u32>,
    permission_mode: Option<McpPermissionMode>,
) -> Result<String, String> {
    if let Some(mode) = permission_mode {
        permission::check_sql_allowed(sql, mode)?;
    }
    let (driver, handle) = resolve_connection(connection_manager, config_id).await?;
    let limit = resolve_query_limit(limit);
    let result = driver
        .query_multi(&handle, sql, limit)
        .await
        .map_err(|e| format!("Error executing query: {e}"))?;
    serde_json::to_string_pretty(&result).map_err(|e| format!("Error: {e}"))
}

/// Run EXPLAIN on a SQL query on a connection identified by config_id.
pub async fn explain_query(
    connection_manager: &ConnectionManager,
    config_id: &str,
    sql: &str,
) -> Result<String, String> {
    let (driver, handle) = resolve_connection(connection_manager, config_id).await?;
    let result = driver
        .explain(&handle, sql)
        .await
        .map_err(|e| format!("Error running EXPLAIN: {e}"))?;
    serde_json::to_string_pretty(&result).map_err(|e| format!("Error: {e}"))
}

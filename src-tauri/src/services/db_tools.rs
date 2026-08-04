//! Shared database tool helpers used by both ai_chat tool execution and MCP Server tools.

use crate::services::ConnectionManager;
use crate::store::Store;
use datazen_driver_api::{ConnectionHandle, DatabaseDriver};
use std::sync::Arc;

/// Resolve a connection ID (config ID or runtime ID) to a driver + handle.
/// Tries to get an existing connection first, falls back to connecting.
pub async fn resolve_connection(
    connection_manager: &ConnectionManager,
    id: &str,
) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), String> {
    if let Ok(conn) = connection_manager.get_connection(id).await {
        return Ok(conn);
    }
    let conn_id = connection_manager
        .connect(id)
        .await
        .map_err(|e| format!("Cannot connect to '{id}': {e}"))?;
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

/// List all databases on a connection.
pub async fn list_databases(
    connection_manager: &ConnectionManager,
    connection_id: &str,
) -> Result<String, String> {
    let (driver, handle) = resolve_connection(connection_manager, connection_id).await?;
    let dbs = driver
        .get_databases(&handle)
        .await
        .map_err(|e| format!("Error listing databases: {e}"))?;
    serde_json::to_string_pretty(&dbs).map_err(|e| format!("Error: {e}"))
}

/// List all tables in a database.
pub async fn list_tables(
    connection_manager: &ConnectionManager,
    connection_id: &str,
    database: &str,
) -> Result<String, String> {
    let (driver, handle) = resolve_connection(connection_manager, connection_id).await?;
    let tables = driver
        .get_tables(&handle, database)
        .await
        .map_err(|e| format!("Error listing tables: {e}"))?;
    serde_json::to_string_pretty(&tables).map_err(|e| format!("Error: {e}"))
}

/// Get detailed schema for one or more tables.
pub async fn get_table_schema(
    connection_manager: &ConnectionManager,
    connection_id: &str,
    tables: &[String],
) -> Result<String, String> {
    let (driver, handle) = resolve_connection(connection_manager, connection_id).await?;
    let mut results = Vec::new();
    for table in tables {
        match driver.get_table_schema(&handle, table).await {
            Ok(schema) => results.push(serde_json::to_value(&schema).unwrap_or_default()),
            Err(e) => results.push(serde_json::json!({"table": table, "error": e.to_string()})),
        }
    }
    serde_json::to_string_pretty(&results).map_err(|e| format!("Error: {e}"))
}

/// Get a single table's schema as a JSON string.
pub async fn get_single_table_schema(
    connection_manager: &ConnectionManager,
    connection_id: &str,
    table: &str,
) -> Result<datazen_driver_api::TableSchema, String> {
    let (driver, handle) = resolve_connection(connection_manager, connection_id).await?;
    driver
        .get_table_schema(&handle, table)
        .await
        .map_err(|e| format!("Error getting schema: {e}"))
}

/// Execute a SQL query and return results as JSON.
pub async fn query(
    connection_manager: &ConnectionManager,
    connection_id: &str,
    sql: &str,
    limit: Option<u32>,
) -> Result<String, String> {
    let (driver, handle) = resolve_connection(connection_manager, connection_id).await?;
    let result = driver
        .query_multi(&handle, sql, limit)
        .await
        .map_err(|e| format!("Error executing query: {e}"))?;
    serde_json::to_string_pretty(&result).map_err(|e| format!("Error: {e}"))
}

/// Run EXPLAIN on a SQL query.
pub async fn explain_query(
    connection_manager: &ConnectionManager,
    connection_id: &str,
    sql: &str,
) -> Result<String, String> {
    let (driver, handle) = resolve_connection(connection_manager, connection_id).await?;
    let result = driver
        .explain(&handle, sql)
        .await
        .map_err(|e| format!("Error running EXPLAIN: {e}"))?;
    serde_json::to_string_pretty(&result).map_err(|e| format!("Error: {e}"))
}

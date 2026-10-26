//! Shared database tool helpers used by both ai_chat tool execution and MCP Server tools.
//!
//! D6 item 4 收尾自 feature-core `services::db_tools` 归位（唯一定义在此，
//! 旧路径改再导出垫片）。SQL 分级沿用 `mcp::domain::permission` 的判据，
//! 只读/Safe Mode 校验沿用 `execution::domain::sql_guard`，不另立第二套。

pub const MCP_QUERY_DEFAULT_LIMIT: u32 = 100;
pub const MCP_QUERY_MAX_LIMIT: u32 = 50_000;

use super::ports::DatabaseToolHost;
use crate::mcp::domain::permission::{self, McpPermissionMode};
use crate::schema::application::scope::metadata_schema;
use datazen_driver_api::DatabaseDriver;
use std::sync::Arc;

/// Explicit metadata target for an MCP call.
///
/// The MCP surface is the one place where a table/database name is typed by a
/// human rather than walked from the object tree, so it has no schema to hand
/// down. It resolves the database from the caller's argument with the
/// connection config as fallback, and the schema from the caller's argument,
/// the config, then the driver's own convention — never a session default.
pub async fn resolve_metadata_target(
    host: &dyn DatabaseToolHost,
    db_session_id: &str,
    driver: &Arc<dyn DatabaseDriver>,
    database: Option<&str>,
    schema: Option<&str>,
) -> Result<(String, Option<String>), String> {
    let config = host
        .get_session_config(db_session_id)
        .await
        .map_err(|e| format!("Cannot read session config: {e}"))?;
    let database = database
        .map(str::trim)
        .filter(|db| !db.is_empty())
        .or(config.database.as_deref())
        .unwrap_or("")
        .to_string();
    let schema = metadata_schema(driver.as_ref(), schema, None, config.schema.as_deref());
    Ok((database, schema))
}

/// List all configured connections as a JSON string.
pub async fn list_connections(
    store: &dyn crate::connection::domain::ConnectionRepository,
    scope: &crate::connection::domain::ConnectionScope,
) -> Result<String, String> {
    let connections = store.list(scope).await.map_err(|e| e.to_string())?;
    let result: Vec<serde_json::Value> = connections
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "name": c.name,
                "databaseType": c.database_type,
                "host": c.host,
                "database": c.database,
            })
        })
        .collect();
    serde_json::to_string_pretty(&result).map_err(|e| format!("Error: {e}"))
}

async fn target_input(
    host: &dyn DatabaseToolHost,
    id: &str,
    database: Option<&str>,
    schema: Option<&str>,
) -> Result<serde_json::Value, String> {
    let config = host.profile_config(id).await?;
    let driver = host.driver_for_connection(id).await?;
    let database = database
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .or(config.database.as_deref())
        .unwrap_or("");
    let schema = metadata_schema(driver.as_ref(), schema, None, config.schema.as_deref());
    Ok(serde_json::json!({ "database": database, "schema": schema }))
}

pub async fn list_databases(host: &dyn DatabaseToolHost, id: &str) -> Result<String, String> {
    let output = host
        .execute_target(id, "list_databases", serde_json::json!({}))
        .await?;
    serde_json::to_string_pretty(&datazen_driver_api::parse_databases_from_command(
        &output.data,
    ))
    .map_err(|error| error.to_string())
}
pub async fn list_tables(
    host: &dyn DatabaseToolHost,
    id: &str,
    database: Option<&str>,
    schema: Option<&str>,
) -> Result<String, String> {
    let output = host
        .execute_target(
            id,
            "list_tables",
            target_input(host, id, database, schema).await?,
        )
        .await?;
    serde_json::to_string_pretty(&datazen_driver_api::parse_tables_from_command(&output.data))
        .map_err(|error| error.to_string())
}
pub async fn search_tables(
    host: &dyn DatabaseToolHost,
    id: &str,
    database: Option<&str>,
    schema: Option<&str>,
    pattern: &str,
    limit: usize,
) -> Result<String, String> {
    let output = host
        .execute_target(
            id,
            "list_tables",
            target_input(host, id, database, schema).await?,
        )
        .await?;
    let tables = datazen_driver_api::parse_tables_from_command(&output.data);
    let pattern_lower = pattern.to_lowercase();
    let matched = tables
        .iter()
        .filter(|table| table.name.to_lowercase().contains(&pattern_lower))
        .take(limit)
        .collect::<Vec<_>>();
    let total_matches = tables
        .iter()
        .filter(|table| table.name.to_lowercase().contains(&pattern_lower))
        .count();
    serde_json::to_string_pretty(&serde_json::json!({ "matched": matched, "totalMatches": total_matches, "totalTables": tables.len(), "pattern": pattern })).map_err(|error| error.to_string())
}
pub async fn get_single_table_schema(
    host: &dyn DatabaseToolHost,
    id: &str,
    table: &str,
    database: Option<&str>,
    schema: Option<&str>,
) -> Result<datazen_driver_api::TableSchema, String> {
    let mut input = target_input(host, id, database, schema).await?;
    input["table"] = serde_json::Value::String(table.into());
    let output = host.execute_target(id, "get_table_schema", input).await?;
    serde_json::from_value(output.data["schema"].clone()).map_err(|error| error.to_string())
}
pub async fn get_table_schema(
    host: &dyn DatabaseToolHost,
    id: &str,
    tables: &[String],
    database: Option<&str>,
    schema: Option<&str>,
) -> Result<String, String> {
    let mut results = vec![];
    for table in tables {
        results.push(
            match get_single_table_schema(host, id, table, database, schema).await {
                Ok(schema) => serde_json::to_value(schema).map_err(|error| error.to_string())?,
                Err(error) => serde_json::json!({ "table": table, "error": error }),
            },
        );
    }
    serde_json::to_string_pretty(&results).map_err(|error| error.to_string())
}
pub fn resolve_query_limit(limit: Option<u32>) -> Option<u32> {
    Some(
        limit
            .unwrap_or(MCP_QUERY_DEFAULT_LIMIT)
            .min(MCP_QUERY_MAX_LIMIT),
    )
}
pub async fn query(
    host: &dyn DatabaseToolHost,
    id: &str,
    sql: &str,
    limit: Option<u32>,
    mode: Option<McpPermissionMode>,
) -> Result<String, String> {
    if let Some(mode) = mode {
        permission::check_sql_allowed(sql, mode)?;
    }
    let policy = host.execution_settings().await;
    let config = host.profile_config(id).await?;
    crate::execution::domain::sql_guard::check_sql(sql, config.read_only, policy.safe_mode)?;
    let mut input = target_input(host, id, None, None).await?;
    input["sql"] = serde_json::Value::String(sql.into());
    input["limit"] = serde_json::json!(resolve_query_limit(limit));
    policy.apply_result_limit(&mut input);
    let result = host.execute_target(id, "query", input).await?;
    serde_json::to_string_pretty(&result.data).map_err(|error| error.to_string())
}
pub async fn explain_query(
    host: &dyn DatabaseToolHost,
    id: &str,
    sql: &str,
) -> Result<String, String> {
    let mut input = target_input(host, id, None, None).await?;
    input["sql"] = serde_json::Value::String(sql.into());
    let output = host.execute_target(id, "explain", input).await?;
    serde_json::to_string_pretty(&output.data).map_err(|error| error.to_string())
}

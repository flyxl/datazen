//! MCP metadata calls through the Runtime connection/resource boundary.

use super::server::DataZenMcpServer;
use crate::ai::budget;
use crate::services::runtime_command_source::RuntimeCommandSource;
use datazen_driver_api::{DatabaseDriver, TableSchema};
use datazen_feature_core::ai::context::format_compact_ddl;
use rmcp::ErrorData as McpError;
use std::sync::Arc;

/// Execute a read-only metadata command against a persisted connection profile.
///
/// This deliberately does not resolve a `ConnectionManager` session. The
/// Runtime command source validates the profile, configuration revision and
/// canonical target, then acquires and releases an independent resource for
/// the command under a backend-generated request identity.
pub(crate) async fn read_target_command(
    server: &DataZenMcpServer,
    connection_id: &str,
    command: &str,
    input: serde_json::Value,
) -> Result<serde_json::Value, McpError> {
    let source = RuntimeCommandSource::new(
        server.app_state.driver_registry.clone(),
        server.app_state.store.clone(),
        server.app_state.platform.clone(),
    );
    source
        .read_target_command(connection_id, command, input)
        .await
        .map_err(DataZenMcpServer::map_err)
}

pub(crate) async fn driver_for_connection(
    server: &DataZenMcpServer,
    connection_id: &str,
) -> Result<Arc<dyn DatabaseDriver>, McpError> {
    let source = RuntimeCommandSource::new(
        server.app_state.driver_registry.clone(),
        server.app_state.store.clone(),
        server.app_state.platform.clone(),
    );
    source
        .driver_for_connection(connection_id)
        .await
        .map_err(DataZenMcpServer::map_err)
}

pub(crate) async fn schema_context(
    server: &DataZenMcpServer,
    connection_id: &str,
    database: &str,
    max_tokens: usize,
) -> Result<String, McpError> {
    let table_output = read_target_command(
        server,
        connection_id,
        "list_tables",
        serde_json::json!({ "database": database, "schema": null }),
    )
    .await?;
    let tables = datazen_driver_api::parse_tables_from_command(&table_output);
    let mut schema_ddl = Vec::new();
    let mut token_estimate = 0;
    for table in tables.iter().take(30) {
        let output = read_target_command(
            server,
            connection_id,
            "get_table_schema",
            serde_json::json!({
                "database": database,
                "schema": null,
                "table": table.name,
            }),
        )
        .await;
        let Ok(output) = output else {
            continue;
        };
        let Ok(schema) = serde_json::from_value::<TableSchema>(output["schema"].clone()) else {
            continue;
        };
        let ddl_line = format_compact_ddl(&table.name, &schema);
        let line_tokens = budget::estimate_tokens(&ddl_line);
        if token_estimate + line_tokens > max_tokens {
            break;
        }
        token_estimate += line_tokens;
        schema_ddl.push(ddl_line);
    }
    Ok(schema_ddl.join("\n"))
}

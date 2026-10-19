//! MCP metadata calls through the Runtime connection/resource boundary.

use super::server::DataZenMcpServer;
use rmcp::ErrorData as McpError;

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
    let source = crate::services::runtime_command_source::RuntimeCommandSource::new(
        server.app_state.driver_registry.clone(),
        server.app_state.store.clone(),
        server.app_state.platform.clone(),
    );
    source
        .read_target_command(connection_id, command, input)
        .await
        .map_err(DataZenMcpServer::map_err)
}

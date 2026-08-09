//! MCP Server module — exposes DataZen's database capabilities via MCP protocol.

pub mod client;
pub mod permission;
mod server;

pub use client::{McpClientManager, McpServerConfig, McpToolInfo};
pub use server::{DataZenMcpServer, MCP_ALL_TOOLS};

use crate::commands::AppState;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub async fn start_mcp_stdio(app_state: Arc<AppState>, cancel: CancellationToken) {
    let settings = app_state.store.get_settings().await;
    let server = DataZenMcpServer::new(app_state)
        .with_disabled_tools(&settings.mcp_disabled_tools)
        .with_permission_mode(settings.mcp_permission_mode);

    use rmcp::ServiceExt;
    use tokio::io::{stdin, stdout};

    tracing::info!("[mcp] starting MCP Server via stdio");
    match server.serve((stdin(), stdout())).await {
        Ok(service) => {
            tracing::info!("[mcp] stdio MCP Server initialized");
            let ct = service.cancellation_token();
            let cancel_task = tokio::spawn(async move {
                cancel.cancelled().await;
                ct.cancel();
            });
            let reason = service.waiting().await;
            cancel_task.abort();
            tracing::info!("[mcp] stdio MCP Server shut down: {reason:?}");
        }
        Err(e) => {
            tracing::error!("[mcp] failed to start stdio MCP Server: {e}");
        }
    }
}

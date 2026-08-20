//! MCP Server module — exposes DataZen's database capabilities via MCP protocol.

pub mod allowlist;
pub mod client;
pub mod permission;
mod server;
pub mod tool_help;

pub use client::{McpClientManager, McpServerConfig, McpToolInfo};
pub use server::{DataZenMcpServer, MCP_ALL_TOOLS};

use crate::commands::AppState;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;

/// Run MCP over real process stdio (`datazen --mcp`).
pub async fn start_mcp_stdio(app_state: Arc<AppState>, cancel: CancellationToken) {
    use tokio::io::{stdin, stdout};
    start_mcp_transport(app_state, cancel, stdin(), stdout()).await;
}

/// Run MCP on an arbitrary async read/write pair (embedded GUI keepalive).
pub async fn start_mcp_transport<R, W>(
    app_state: Arc<AppState>,
    cancel: CancellationToken,
    reader: R,
    writer: W,
) where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let settings = app_state.store.get_settings().await;
    let server = DataZenMcpServer::new(app_state)
        .with_disabled_tools(&settings.mcp_disabled_tools)
        .with_permission_mode(settings.mcp_permission_mode)
        .with_allowed_connections(&settings.mcp_allowed_connection_ids);

    use rmcp::ServiceExt;

    tracing::info!("[mcp] starting MCP Server");
    match server.serve((reader, writer)).await {
        Ok(service) => {
            tracing::info!("[mcp] MCP Server initialized");
            let ct = service.cancellation_token();
            let cancel_task = tokio::spawn(async move {
                cancel.cancelled().await;
                ct.cancel();
            });
            let reason = service.waiting().await;
            cancel_task.abort();
            tracing::info!("[mcp] MCP Server shut down: {reason:?}");
        }
        Err(e) => {
            tracing::error!("[mcp] failed to start MCP Server: {e}");
        }
    }
}

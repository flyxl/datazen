//! MCP Server module — exposes DataZen's database capabilities via MCP protocol.

mod server;
pub mod skills;

pub use server::DataZenMcpServer;
pub use skills::{SkillDefinition, SkillExecutor, SkillListItem, SkillRegistry};

use crate::commands::AppState;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub async fn start_mcp_stdio(app_state: Arc<AppState>, cancel: CancellationToken) {
    let server = DataZenMcpServer::new(app_state);

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

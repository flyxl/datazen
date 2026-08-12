//! MCP Server + Client management IPC commands.

use crate::commands::error::{CmdExt, CommandError};
use crate::commands::AppState;
use crate::mcp;
use serde::Serialize;
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerStatus {
    pub running: bool,
    pub transport: String,
}

struct McpHandle {
    task: tokio::task::JoinHandle<()>,
    cancel: CancellationToken,
}

static MCP_HANDLE: Mutex<Option<McpHandle>> = Mutex::const_new(None);

fn clone_app_state(state: &AppState) -> Arc<AppState> {
    Arc::new(AppState {
        driver_registry: state.driver_registry.clone(),
        connection_manager: state.connection_manager.clone(),
        monitor_connections: state.monitor_connections.clone(),
        monitor_engine: state.monitor_engine.clone(),
        store: state.store.clone(),
        schema_cache: state.schema_cache.clone(),
        sync_adapters: state.sync_adapters.clone(),
        ai_registry: state.ai_registry.clone(),
        schema_context_builder: state.schema_context_builder.clone(),
        prompt_resolver: state.prompt_resolver.clone(),
        workflow_registry: state.workflow_registry.clone(),
        workflow_history: state.workflow_history.clone(),
        mcp_client_manager: state.mcp_client_manager.clone(),
        session_transactions: state.session_transactions.clone(),
        workflow_scheduler: state.workflow_scheduler.clone(),
    })
}

/// Start embedded MCP stdio if not already running. Used by settings toggle and optional GUI auto-start.
pub async fn start_embedded_mcp(state: &AppState) -> Result<(), CommandError> {
    let mut guard = MCP_HANDLE.lock().await;
    if let Some(ref h) = *guard {
        if !h.task.is_finished() {
            return Ok(());
        }
    }

    let app_state = clone_app_state(state);
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    let task = tokio::spawn(async move {
        mcp::start_mcp_stdio(app_state, cancel_clone).await;
    });

    *guard = Some(McpHandle { task, cancel });
    Ok(())
}

#[tauri::command]
pub async fn mcp_get_status() -> Result<McpServerStatus, CommandError> {
    let guard = MCP_HANDLE.lock().await;
    let running = guard
        .as_ref()
        .map(|h| !h.task.is_finished())
        .unwrap_or(false);
    Ok(McpServerStatus {
        running,
        transport: if running {
            "stdio".into()
        } else {
            "none".into()
        },
    })
}

#[tauri::command]
pub async fn mcp_start_stdio(state: State<'_, AppState>) -> Result<(), CommandError> {
    start_embedded_mcp(&state).await
}

#[tauri::command]
pub async fn mcp_stop() -> Result<(), CommandError> {
    let mut guard = MCP_HANDLE.lock().await;
    if let Some(h) = guard.take() {
        h.cancel.cancel();
        let _ = h.task.await;
    }
    Ok(())
}

#[tauri::command]
pub async fn mcp_list_all_tools() -> Result<Vec<String>, CommandError> {
    Ok(crate::mcp::MCP_ALL_TOOLS
        .iter()
        .map(|s| s.to_string())
        .collect())
}

// ─── MCP Client commands ───

pub(crate) async fn mcp_client_connect_impl(
    state: &AppState,
    config: mcp::McpServerConfig,
) -> Result<(), CommandError> {
    state
        .mcp_client_manager
        .connect(&config)
        .await
        .cmd_err("mcp_client_connect")
}

pub(crate) async fn mcp_client_disconnect_impl(
    state: &AppState,
    server_id: String,
) -> Result<(), CommandError> {
    state
        .mcp_client_manager
        .disconnect(&server_id)
        .await
        .map_err(CommandError::Internal)
}

pub(crate) async fn mcp_client_list_impl(
    state: &AppState,
) -> Result<Vec<McpClientStatus>, CommandError> {
    Ok(state
        .mcp_client_manager
        .connected_servers()
        .await
        .into_iter()
        .map(|(id, name, count)| McpClientStatus {
            server_id: id,
            server_name: name,
            tools_count: count,
        })
        .collect())
}

pub(crate) async fn mcp_client_tools_impl(
    state: &AppState,
) -> Result<Vec<mcp::McpToolInfo>, CommandError> {
    Ok(state.mcp_client_manager.all_tools().await)
}

#[tauri::command]
pub async fn mcp_client_connect(
    state: State<'_, AppState>,
    config: mcp::McpServerConfig,
) -> Result<(), CommandError> {
    mcp_client_connect_impl(&state, config).await
}

#[tauri::command]
pub async fn mcp_client_disconnect(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<(), CommandError> {
    mcp_client_disconnect_impl(&state, server_id).await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpClientStatus {
    pub server_id: String,
    pub server_name: String,
    pub tools_count: usize,
}

#[tauri::command]
pub async fn mcp_client_list(
    state: State<'_, AppState>,
) -> Result<Vec<McpClientStatus>, CommandError> {
    mcp_client_list_impl(&state).await
}

#[tauri::command]
pub async fn mcp_client_tools(
    state: State<'_, AppState>,
) -> Result<Vec<mcp::McpToolInfo>, CommandError> {
    mcp_client_tools_impl(&state).await
}

#[tauri::command]
pub async fn mcp_client_call_tool(
    state: State<'_, AppState>,
    server_id: String,
    tool_name: String,
    arguments: serde_json::Value,
) -> Result<String, CommandError> {
    let result = state
        .mcp_client_manager
        .call_tool(&server_id, &tool_name, arguments)
        .await
        .cmd_err("mcp_client_call_tool")?;

    let output = result
        .content
        .iter()
        .filter_map(|c| {
            if let rmcp::model::ContentBlock::Text(t) = c {
                Some(t.text.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if result.is_error == Some(true) {
        return Err(CommandError::Internal(output));
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mcp_get_status_when_stopped() {
        let status = mcp_get_status().await.unwrap();
        assert!(!status.running);
        assert_eq!(status.transport, "none");
    }

    #[tokio::test]
    async fn mcp_list_all_tools_non_empty() {
        let tools = mcp_list_all_tools().await.unwrap();
        assert!(!tools.is_empty());
        assert!(tools
            .iter()
            .any(|t| t.contains("connection") || t.contains("query")));
    }

    #[tokio::test]
    async fn mcp_client_list_empty_and_disconnect_unknown() {
        use crate::testing::app_state::TestAppState;

        let test = TestAppState::new().await;
        assert!(mcp_client_list_impl(&test.state).await.unwrap().is_empty());
        assert!(mcp_client_tools_impl(&test.state).await.unwrap().is_empty());
        assert!(mcp_client_disconnect_impl(&test.state, "missing".into())
            .await
            .is_ok());
    }
}

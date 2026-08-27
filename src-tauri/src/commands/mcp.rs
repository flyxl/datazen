//! MCP Server + Client management IPC commands.

use crate::commands::error::{CmdExt, CommandError};
use crate::commands::AppState;
use crate::mcp;
use serde::Serialize;
use std::sync::Arc;
use tauri::State;
use tokio::io::DuplexStream;
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
    /// Holds the client end of the embedded duplex so the server side stays open.
    keepalive: Option<DuplexStream>,
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
        extensions: state.extensions.clone(),
    })
}

async fn mcp_is_running() -> bool {
    let guard = MCP_HANDLE.lock().await;
    guard
        .as_ref()
        .map(|h| !h.task.is_finished())
        .unwrap_or(false)
}

/// Start embedded MCP if not already running. Uses an in-process duplex so GUI stdin EOF does not stop the server.
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
    let (keepalive, server_end) = tokio::io::duplex(256 * 1024);
    let (reader, writer) = tokio::io::split(server_end);

    let task = tokio::spawn(async move {
        mcp::start_mcp_transport(app_state, cancel_clone, reader, writer).await;
    });

    *guard = Some(McpHandle {
        task,
        cancel,
        keepalive: Some(keepalive),
    });
    Ok(())
}

/// Stop embedded MCP if running.
pub async fn stop_embedded_mcp() -> Result<(), CommandError> {
    let mut guard = MCP_HANDLE.lock().await;
    if let Some(mut h) = guard.take() {
        h.cancel.cancel();
        h.keepalive = None;
        let task = h.task;
        let abort = task.abort_handle();
        if tokio::time::timeout(std::time::Duration::from_secs(3), task)
            .await
            .is_err()
        {
            abort.abort();
        }
    }
    Ok(())
}

/// Stop and restart embedded MCP so settings (permission, tools, allowlist) take effect immediately.
pub async fn reload_embedded_mcp(state: &AppState) -> Result<(), CommandError> {
    let was_running = mcp_is_running().await;
    stop_embedded_mcp().await?;
    if was_running {
        start_embedded_mcp(state).await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn mcp_get_status() -> Result<McpServerStatus, CommandError> {
    let running = mcp_is_running().await;
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
    stop_embedded_mcp().await
}

#[tauri::command]
pub async fn mcp_reload(state: State<'_, AppState>) -> Result<(), CommandError> {
    reload_embedded_mcp(&state).await
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

    let qualified = crate::mcp::client::mcp_qualified_name(&server_id, &tool_name);
    let output = crate::mcp::format_call_tool_result(&result, &qualified);

    if result.is_error == Some(true) {
        return Err(CommandError::Internal(output));
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::app_state::TestAppState;

    /// Serializes every test that touches the process-global MCP_HANDLE:
    /// parallel `reset/start/stop` from sibling tests used to cancel another
    /// test's embedded server mid-flight (flaky only under full-suite load).
    static MCP_LIFECYCLE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    async fn reset_mcp_handle() {
        let _ = stop_embedded_mcp().await;
    }

    /// Poll `mcp_is_running` up to a deadline instead of a fixed sleep — the
    /// embedded server start is asynchronous and a bare 50ms wait races under
    /// parallel test scheduling (observed flaky on loaded machines).
    async fn wait_until_mcp_running(timeout_ms: u64) -> bool {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        loop {
            if mcp_is_running().await {
                return true;
            }
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn mcp_get_status_when_stopped() {
        let _guard = MCP_LIFECYCLE_LOCK.lock().await;
        reset_mcp_handle().await;
        let status = mcp_get_status().await.unwrap();
        assert!(!status.running);
        assert_eq!(status.transport, "none");
    }

    #[tokio::test]
    async fn start_embedded_mcp_reports_running() {
        let _guard = MCP_LIFECYCLE_LOCK.lock().await;
        reset_mcp_handle().await;
        let test = TestAppState::new().await;
        start_embedded_mcp(&test.state).await.unwrap();

        assert!(
            wait_until_mcp_running(2000).await,
            "embedded MCP should stay running with duplex keepalive"
        );
        let status = mcp_get_status().await.unwrap();
        assert_eq!(status.transport, "stdio");

        stop_embedded_mcp().await.unwrap();
        let status = mcp_get_status().await.unwrap();
        assert!(!status.running);
    }

    #[tokio::test]
    async fn reload_embedded_mcp_restarts_when_running() {
        let _guard = MCP_LIFECYCLE_LOCK.lock().await;
        reset_mcp_handle().await;
        let test = TestAppState::new().await;
        start_embedded_mcp(&test.state).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            wait_until_mcp_running(2000).await,
            "embedded MCP should be running after start"
        );

        reload_embedded_mcp(&test.state).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            wait_until_mcp_running(2000).await,
            "reloaded embedded MCP should be running again"
        );

        stop_embedded_mcp().await.unwrap();
    }

    #[tokio::test]
    async fn reload_embedded_mcp_noop_when_stopped() {
        let _guard = MCP_LIFECYCLE_LOCK.lock().await;
        reset_mcp_handle().await;
        let test = TestAppState::new().await;
        reload_embedded_mcp(&test.state).await.unwrap();
        assert!(!mcp_is_running().await);
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
        let test = TestAppState::new().await;
        assert!(mcp_client_list_impl(&test.state).await.unwrap().is_empty());
        assert!(mcp_client_tools_impl(&test.state).await.unwrap().is_empty());
        assert!(mcp_client_disconnect_impl(&test.state, "missing".into())
            .await
            .is_ok());
    }
}

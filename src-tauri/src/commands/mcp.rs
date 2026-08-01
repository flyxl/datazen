//! MCP Server management IPC commands.

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

#[tauri::command]
pub async fn mcp_get_status() -> Result<McpServerStatus, String> {
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
pub async fn mcp_start_stdio(state: State<'_, AppState>) -> Result<(), String> {
    let mut guard = MCP_HANDLE.lock().await;
    if let Some(ref h) = *guard {
        if !h.task.is_finished() {
            return Err("MCP Server is already running".into());
        }
    }

    let app_state = Arc::new(AppState {
        driver_registry: state.driver_registry.clone(),
        connection_manager: state.connection_manager.clone(),
        store: state.store.clone(),
        schema_cache: state.schema_cache.clone(),
        sync_adapters: state.sync_adapters.clone(),
        ai_registry: state.ai_registry.clone(),
        schema_context_builder: state.schema_context_builder.clone(),
    });

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    let task = tokio::spawn(async move {
        mcp::start_mcp_stdio(app_state, cancel_clone).await;
    });

    *guard = Some(McpHandle { task, cancel });
    Ok(())
}

#[tauri::command]
pub async fn mcp_stop() -> Result<(), String> {
    let mut guard = MCP_HANDLE.lock().await;
    if let Some(h) = guard.take() {
        h.cancel.cancel();
        let _ = h.task.await;
    }
    Ok(())
}

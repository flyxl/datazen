//! MCP Server + Client management IPC commands.
//!
//! B2 mcp 刀（commands-layer-review §3.6）：`mcp_client_disconnect/list/tools`
//! 三个 `_impl` 转发体已删，接线直接住在 `#[tauri::command]` 处理函数里
//!（它们只是 `McpClientManager` 端口的纯调用）；`mcp_client_connect_impl`
//! 因**跨域生产调用方**（`bootstrap/run.rs` 启动期自动重连两处）保留为
//! 共享入口（见其文档注释）。嵌入式 server 的启停/重载是宿主进程生命周期
//! 管理（tokio 任务 + 进程内 duplex），不下沉；工具语义（权限档位、白名单、
//! 表描述）早已在 `datazen::mcp`。

use crate::commands::error::{CmdExt, CommandError};
use crate::commands::AppState;
use crate::mcp;
use datazen::mcp::approval::McpApprovalDecision;
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
        query_executions: state.query_executions.clone(),
        workflow_scheduler: state.workflow_scheduler.clone(),
        wapps: state.wapps.clone(),
        language_packs: state.language_packs.clone(),
        cancel_registry: state.cancel_registry.clone(),
        ai_grants: state.ai_grants.clone(),
        schema_diff_jobs: state.schema_diff_jobs.clone(),
        desktop_job_host: state.desktop_job_host.clone(),
        schema_diff_reviewed_plans: state.schema_diff_reviewed_plans.clone(),
        sync_stores: state.sync_stores.clone(),
        transfer_plans: state.transfer_plans.clone(),
        platform: state.platform.clone(),
        organization_scope: state.organization_scope.clone(),
        app_handle: state.app_handle.clone(),
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
    // Stopping the embedded server must also release any MCP call paused for
    // approval. The call then audits `cancel` and never executes its tool body.
    mcp::approval_broker().cancel_all();
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
    let http_was_running = mcp::http::status(state)
        .await
        .map(|status| status.running)
        .unwrap_or(false);
    stop_embedded_mcp().await?;
    if was_running {
        start_embedded_mcp(state).await?;
    }
    if http_was_running {
        let port = state.store.get_settings().await.mcp_http_port;
        mcp::http::stop().await;
        mcp::http::start_existing(clone_app_state(state), port)
            .await
            .map_err(CommandError::Internal)?;
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

/// Start the opt-in Streamable HTTP listener. Returns the one-time plaintext
/// token only when this call created it; later status calls show last4 only.
#[tauri::command]
pub async fn mcp_http_enable(
    state: State<'_, AppState>,
    port: Option<u16>,
) -> Result<mcp::http::McpHttpStatus, CommandError> {
    let mut settings = state.store.get_settings().await;
    let previous = settings.clone();
    let configured_port = port.unwrap_or(settings.mcp_http_port);
    settings.mcp_http_enabled = true;
    settings.mcp_http_port = configured_port;
    state
        .store
        .save_settings(settings)
        .await
        .map_err(|error| CommandError::Internal(error.to_string()))?;
    match mcp::http::start(clone_app_state(&state), configured_port).await {
        Ok(status) => Ok(status),
        Err(error) => {
            // A failed bind must not leave the persisted switch claiming the
            // listener is enabled. If it was running before, try to bring the
            // old port back rather than stranding the user with no listener.
            let was_enabled = previous.mcp_http_enabled;
            let previous_port = previous.mcp_http_port;
            let _ = state.store.save_settings(previous).await;
            if was_enabled {
                let _ = mcp::http::start_existing(clone_app_state(&state), previous_port).await;
            }
            Err(CommandError::Internal(error))
        }
    }
}

#[tauri::command]
pub async fn mcp_http_disable(
    state: State<'_, AppState>,
) -> Result<mcp::http::McpHttpStatus, CommandError> {
    mcp::http::stop().await;
    let mut settings = state.store.get_settings().await;
    settings.mcp_http_enabled = false;
    state
        .store
        .save_settings(settings)
        .await
        .map_err(|error| CommandError::Internal(error.to_string()))?;
    mcp::http::status(&state)
        .await
        .map_err(CommandError::Internal)
}

#[tauri::command]
pub async fn mcp_http_status(
    state: State<'_, AppState>,
) -> Result<mcp::http::McpHttpStatus, CommandError> {
    mcp::http::status(&state)
        .await
        .map_err(CommandError::Internal)
}

/// Rotate the bearer token. A running listener switches its verifier in place,
/// so the old token is rejected on the next request.
#[tauri::command]
pub async fn mcp_http_regenerate_token(
    state: State<'_, AppState>,
) -> Result<mcp::http::McpHttpStatus, CommandError> {
    mcp::http::regenerate_token(&state)
        .await
        .map_err(CommandError::Internal)
}

/// Restart a running HTTP listener so changed scope/permission settings apply
/// to newly created MCP sessions. Leaves a stopped listener stopped.
#[tauri::command]
pub async fn mcp_http_reload(
    state: State<'_, AppState>,
) -> Result<mcp::http::McpHttpStatus, CommandError> {
    let was_running = mcp::http::status(&state)
        .await
        .map(|status| status.running)
        .unwrap_or(false);
    if was_running {
        let settings = state.store.get_settings().await;
        mcp::http::stop().await;
        mcp::http::start_existing(clone_app_state(&state), settings.mcp_http_port)
            .await
            .map_err(CommandError::Internal)?;
    }
    mcp::http::status(&state)
        .await
        .map_err(CommandError::Internal)
}

/// Start HTTP MCP from bootstrap only when it was already enabled and a token
/// exists. It never prints or creates a token behind the user's back.
pub async fn start_http_mcp(state: &AppState) -> Result<(), CommandError> {
    let settings = state.store.get_settings().await;
    mcp::http::start_existing(clone_app_state(state), settings.mcp_http_port)
        .await
        .map(|_| ())
        .map_err(CommandError::Internal)
}

#[tauri::command]
pub async fn mcp_list_all_tools() -> Result<Vec<String>, CommandError> {
    Ok(crate::mcp::MCP_ALL_TOOLS
        .iter()
        .map(|s| s.to_string())
        .collect())
}

/// Answer one pending MCP approval. `allow_once` is the only value that lets
/// the paused call proceed; every other value is terminal for that call.
#[tauri::command]
pub async fn mcp_respond_approval(
    request_id: String,
    decision: String,
) -> Result<bool, CommandError> {
    let decision = match decision.as_str() {
        "allow_once" | "allow" => McpApprovalDecision::AllowOnce,
        "deny" => McpApprovalDecision::Deny,
        "timeout" => McpApprovalDecision::Timeout,
        "cancel" => McpApprovalDecision::Cancel,
        other => {
            return Err(CommandError::Validation(format!(
                "unknown MCP approval decision: {other}"
            )))
        }
    };
    Ok(mcp::approval_broker().respond(&request_id, decision))
}

/// Cancel a pending MCP approval (the dialog's Stop button).
#[tauri::command]
pub async fn mcp_cancel_approval(request_id: String) -> Result<bool, CommandError> {
    Ok(mcp::approval_broker().cancel(&request_id))
}

#[tauri::command]
pub async fn mcp_cancel_all_approvals() -> Result<usize, CommandError> {
    Ok(mcp::approval_broker().cancel_all())
}

/// Newest-first audit rows for the MCP settings panel. No SQL or credentials
/// are stored; a denied Redis call records only its command and keys in the
/// reason field for auditability.
#[tauri::command]
pub async fn mcp_list_audit_log(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<crate::store::McpAuditRecord>, CommandError> {
    let rows = state
        .store
        .mcp_audit_log()
        .list(limit.unwrap_or(100))
        .await
        .map_err(|_| CommandError::Internal("Failed to read MCP audit log".into()))?;
    Ok(rows)
}

// ─── MCP Client commands ───

/// 跨域共享入口（B2 mcp 刀保留）：启动期按设置自动连接外部 MCP server
/// （失败后 5s 重试一次）。
///
/// 调用方（待其域刀把编排收编后可回收）：
/// * 生产：`bootstrap/run.rs`（两处——首连 + 重试，见
///   “Failed to auto-connect external MCP client on startup”）；
/// * 命令：`mcp_client_connect`（与它共享同一实现）。
///
/// 不是命令转发体：`#[tauri::command]` 处理函数与启动路径共用这一份端口
/// 调用与错误映射。
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
    state
        .mcp_client_manager
        .disconnect(&server_id)
        .await
        .map_err(CommandError::Internal)
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

#[tauri::command]
pub async fn mcp_client_tools(
    state: State<'_, AppState>,
) -> Result<Vec<mcp::McpToolInfo>, CommandError> {
    Ok(state.mcp_client_manager.all_tools().await)
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
    use crate::testing::app_state_test_support::TestAppState;
    use tauri::Manager;

    #[tokio::test]
    async fn cloned_app_state_shares_the_active_monitor_engine() {
        let test = TestAppState::with_tables().await;
        let cloned = clone_app_state(&test.state);
        assert!(Arc::ptr_eq(
            &cloned.monitor_engine,
            &test.state.monitor_engine
        ));
        assert!(Arc::ptr_eq(&cloned.store, &test.state.store));
    }

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
        let _http_guard = crate::mcp::http::TEST_LOCK.lock().await;
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
        let _http_guard = crate::mcp::http::TEST_LOCK.lock().await;
        reset_mcp_handle().await;
        let test = TestAppState::new().await;
        reload_embedded_mcp(&test.state).await.unwrap();
        assert!(!mcp_is_running().await);
    }

    #[tokio::test]
    async fn http_commands_enable_report_rotate_and_disable() {
        let _guard = MCP_LIFECYCLE_LOCK.lock().await;
        let _http_guard = crate::mcp::http::TEST_LOCK.lock().await;
        crate::mcp::http::stop().await;
        let test = TestAppState::new().await;
        let (state, _keep) = test.into_state();
        let app = tauri::test::mock_app();
        app.manage(state);

        let initial = mcp_http_status(app.state()).await.expect("initial status");
        assert!(!initial.running);
        assert!(!initial.enabled);
        assert_eq!(initial.configured_port, 0);

        let enabled = mcp_http_enable(app.state(), Some(0))
            .await
            .expect("enable HTTP");
        assert!(enabled.running);
        assert!(enabled.enabled);
        let token = enabled.token.expect("first enable returns token");
        let url = enabled.url.expect("url");
        assert!(url.starts_with("http://127.0.0.1:"));

        let status = mcp_http_status(app.state()).await.expect("status");
        assert!(status.running);
        assert!(status.token.is_none());
        assert_eq!(
            status.token_last4.as_deref(),
            Some(&token[token.len() - 4..])
        );

        let rotated = mcp_http_regenerate_token(app.state())
            .await
            .expect("rotate");
        let new_token = rotated.token.expect("rotation returns new token");
        assert_ne!(token, new_token);
        assert_eq!(
            rotated.token_last4.as_deref(),
            Some(&new_token[new_token.len() - 4..])
        );

        let disabled = mcp_http_disable(app.state()).await.expect("disable");
        assert!(!disabled.running);
        assert!(!disabled.enabled);
        let settings = app.state::<AppState>().store.get_settings().await;
        assert!(!settings.mcp_http_enabled);
    }

    #[tokio::test]
    async fn http_enable_port_conflict_leaves_the_switch_off() {
        let _guard = MCP_LIFECYCLE_LOCK.lock().await;
        let _http_guard = crate::mcp::http::TEST_LOCK.lock().await;
        crate::mcp::http::stop().await;
        let held = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("hold port");
        let port = held.local_addr().expect("port").port();
        let test = TestAppState::new().await;
        let (state, _keep) = test.into_state();
        let app = tauri::test::mock_app();
        app.manage(state);

        let error = mcp_http_enable(app.state(), Some(port))
            .await
            .expect_err("port conflict");
        assert!(error.to_string().contains("bind"), "{error}");
        let settings = app.state::<AppState>().store.get_settings().await;
        assert!(!settings.mcp_http_enabled);
        assert!(!mcp_http_status(app.state()).await.unwrap().running);
        drop(held);
        crate::mcp::http::stop().await;
    }

    #[tokio::test]
    async fn reload_embedded_mcp_restarts_a_running_http_listener() {
        let _guard = MCP_LIFECYCLE_LOCK.lock().await;
        let _http_guard = crate::mcp::http::TEST_LOCK.lock().await;
        crate::mcp::http::stop().await;
        let test = TestAppState::new().await;
        let (state, _keep) = test.into_state();
        let app = tauri::test::mock_app();
        app.manage(state);

        let enabled = mcp_http_enable(app.state(), Some(0))
            .await
            .expect("enable HTTP");
        assert!(enabled.running);

        reload_embedded_mcp(app.state::<AppState>().inner())
            .await
            .expect("reload");
        let status = mcp_http_status(app.state()).await.expect("status");
        assert!(status.running);
        assert!(status.token.is_none());

        mcp_http_disable(app.state()).await.expect("disable");
        crate::mcp::http::stop().await;
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
        // B2 mcp 刀：`mcp_client_disconnect/list/tools` 的 `_impl` 转发体
        // 已删，测试改走真实 `#[tauri::command]` 入口（C4 烟测：命令壳 →
        // McpClientManager 端口 → 空目录/未知 server 的幂等行为）。
        let test = TestAppState::new().await;
        let (state, _keep) = test.into_state();
        let app = tauri::test::mock_app();
        app.manage(state);
        assert!(mcp_client_list(app.state()).await.unwrap().is_empty());
        assert!(mcp_client_tools(app.state()).await.unwrap().is_empty());
        assert!(mcp_client_disconnect(app.state(), "missing".into())
            .await
            .is_ok());
    }
}

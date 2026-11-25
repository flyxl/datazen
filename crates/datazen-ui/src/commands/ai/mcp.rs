//! The `mcp_*` shapers — the MCP host service, the approval faces and the
//! MCP client faces of `commands/ai.ts`.

use serde_json::{Value, json};

use super::wire::AiRequest;

fn native(command: &'static str, value: Value) -> AiRequest {
    AiRequest::native(command, value)
}

// ---- the host service ------------------------------------------------------------

/// `mcpGetStatus` → `mcp_get_status`.
pub fn mcp_get_status_request() -> AiRequest {
    native("mcp_get_status", json!({}))
}

/// `mcpHttpStatus` → `mcp_http_status`.
pub fn mcp_http_status_request() -> AiRequest {
    native("mcp_http_status", json!({}))
}

/// `mcpHttpEnable(port?)` → `mcp_http_enable`; an undefined port sends the
/// empty bag (React's `port === undefined ? {} : { port }`).
pub fn mcp_http_enable_request(port: Option<i64>) -> AiRequest {
    match port {
        Some(port) => native("mcp_http_enable", json!({ "port": port })),
        None => native("mcp_http_enable", json!({})),
    }
}

/// `mcpHttpDisable` → `mcp_http_disable`.
pub fn mcp_http_disable_request() -> AiRequest {
    native("mcp_http_disable", json!({}))
}

/// `mcpHttpRegenerateToken` → `mcp_http_regenerate_token`.
pub fn mcp_http_regenerate_token_request() -> AiRequest {
    native("mcp_http_regenerate_token", json!({}))
}

/// `mcpHttpReload` → `mcp_http_reload`.
pub fn mcp_http_reload_request() -> AiRequest {
    native("mcp_http_reload", json!({}))
}

/// `mcpStartStdio` → `mcp_start_stdio`.
pub fn mcp_start_stdio_request() -> AiRequest {
    native("mcp_start_stdio", json!({}))
}

/// `mcpStop` → `mcp_stop`.
pub fn mcp_stop_request() -> AiRequest {
    native("mcp_stop", json!({}))
}

/// `mcpReload` → `mcp_reload`.
pub fn mcp_reload_request() -> AiRequest {
    native("mcp_reload", json!({}))
}

/// `mcpListAllTools` → `mcp_list_all_tools`.
pub fn mcp_list_all_tools_request() -> AiRequest {
    native("mcp_list_all_tools", json!({}))
}

// ---- approvals ---------------------------------------------------------------------

/// `mcpRespondApproval(requestId, decision)` → `mcp_respond_approval`.
pub fn mcp_respond_approval_request(request_id: &str, decision: &str) -> AiRequest {
    native("mcp_respond_approval", json!({ "requestId": request_id, "decision": decision }))
}

/// `mcpCancelApproval(requestId)` → `mcp_cancel_approval`.
pub fn mcp_cancel_approval_request(request_id: &str) -> AiRequest {
    native("mcp_cancel_approval", json!({ "requestId": request_id }))
}

/// `mcpCancelAllApprovals` → `mcp_cancel_all_approvals`.
pub fn mcp_cancel_all_approvals_request() -> AiRequest {
    native("mcp_cancel_all_approvals", json!({}))
}

/// `mcpListAuditLog(limit?)` → `mcp_list_audit_log` (`limit ?? null`).
pub fn mcp_list_audit_log_request(limit: Option<i64>) -> AiRequest {
    native("mcp_list_audit_log", json!({ "limit": limit }))
}

// ---- the MCP client faces ------------------------------------------------------------

/// `mcpClientConnect(config)` → `mcp_client_connect`.
pub fn mcp_client_connect_request(config: &Value) -> AiRequest {
    native("mcp_client_connect", json!({ "config": config }))
}

/// `mcpClientDisconnect(serverId)` → `mcp_client_disconnect`.
pub fn mcp_client_disconnect_request(server_id: &str) -> AiRequest {
    native("mcp_client_disconnect", json!({ "serverId": server_id }))
}

/// `mcpClientList` → `mcp_client_list`.
pub fn mcp_client_list_request() -> AiRequest {
    native("mcp_client_list", json!({}))
}

/// `mcpClientTools` → `mcp_client_tools`.
pub fn mcp_client_tools_request() -> AiRequest {
    native("mcp_client_tools", json!({}))
}

/// `mcpClientCallTool`'s param bag (`arguments` rides as the tool's own
/// JSON object).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct McpCallToolParams {
    #[serde(rename = "serverId")]
    pub server_id: String,
    #[serde(rename = "toolName")]
    pub tool_name: String,
    pub arguments: Value,
}

/// `mcpClientCallTool(params)` → `mcp_client_call_tool`.
pub fn mcp_client_call_tool_request(params: &McpCallToolParams) -> AiRequest {
    native("mcp_client_call_tool", serde_json::to_value(params).unwrap_or_default())
}

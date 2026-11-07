//! MCP JSON-RPC 线路词汇（server web 形态）。
//!
//! 错误码与桌面形态（rmcp `ErrorData`）逐一对齐：
//! * `invalid_params` = -32602 —— 参数/校验类工具错误（含帮助块）；
//!   SessionLost 家族裸映射（不附帮助块），与
//!   `datazen::mcp::application::session_tools::SessionToolError` 三分一致。
//! * `internal_error` = -32603 —— 运行时失败。
//! * `method_not_found` = -32601 / `parse_error` = -32700 /
//!   `invalid_request` = -32600 —— JSON-RPC 2.0 标准码。
//!
//! 文案逐字保留桌面 `tool_help.rs` 的历史 wire 行为（两形态同错误同文案）。

use serde_json::{json, Value};

use super::contract;

/// streamable HTTP 传输的会话头（MCP 规范 2025-03-26）。
pub const MCP_SESSION_ID_HEADER: &str = "Mcp-Session-Id";
/// 本实现支持的 MCP 协议版本（与桌面 rmcp 对齐的协商值）。
pub const PROTOCOL_VERSION: &str = "2025-03-26";

// JSON-RPC 2.0 标准 + MCP 语义错误码（rmcp ErrorData 同值）。
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;

/// 工具错误的三分通道（宿主原错误映射的显式承载）——与 modules
/// `SessionToolError` 同构，但**不依赖** modules `mcp` feature，让传输层
/// 可以独立编译测试。
pub enum ToolCallError {
    /// 参数/校验类：invalid_params + 工具帮助块。
    WithHelp { tool: String, reason: String },
    /// 参数类但裸 invalid_params，不附帮助块（SessionLost 家族）。
    Plain(String),
    /// 运行时失败：internal_error。
    Internal(String),
}

impl ToolCallError {
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }
}

/// `--help` 风格帮助块（与桌面 `tool_help::format_help_block` 逐字一致）。
pub fn format_help_block(tool_name: &str) -> String {
    let mut out = String::new();
    if let Some(help) = contract::lookup(tool_name) {
        out.push_str(&format!("Tool: {}\n", help.name));
        out.push_str(&format!("Description: {}\n", help.description));
        out.push_str(&format!("Parameters: {}\n", help.parameters));
        // 桌面形态在此附 schemars 生成的 JSON Schema；server 形态的
        // inputSchema 由 contract::tool_definition 承载，帮助块不再重复
        // 内联 schema（golden 契约不含完整 schema，两形态 wire 语义一致）。
        out.push_str("\nExample:\n");
        out.push_str(help.example);
        out.push('\n');
    } else {
        out.push_str(&format!("Tool: {tool_name}\n"));
        out.push_str("(no help available — check list_tools for registered names)\n");
    }
    out
}

/// unknown/disabled 工具的 invalid_params 错误（与桌面
/// `tool_help::unknown_tool_error` 同文案形状）。
pub fn unknown_tool_error(tool_name: &str) -> Value {
    let mut block = format!("Unknown or disabled tool '{tool_name}'.\n\nRegistered tools:\n");
    for help in contract::TOOL_HELPS {
        block.push_str(&format!("  - {} — {}\n", help.name, help.description));
    }
    block.push_str("\nExample (list_connections):\n");
    if let Some(help) = contract::lookup("list_connections") {
        block.push_str(help.example);
        block.push('\n');
    }
    wire_error(
        INVALID_PARAMS,
        &format!("Unknown or disabled tool '{tool_name}'\n\n{block}"),
    )
}

/// JSON-RPC error 对象（MCP 规范：`code`/`message`/可选 `data`）。
pub fn wire_error(code: i64, message: &str) -> Value {
    json!({ "code": code, "message": message })
}

/// 工具错误 → JSON-RPC error（桌面 rmcp 错误映射的镜像）。
pub fn tool_call_error(error: ToolCallError) -> Value {
    match error {
        ToolCallError::WithHelp { tool, reason } => wire_error(
            INVALID_PARAMS,
            &format!("{reason}\n\n{}", format_help_block(&tool)),
        ),
        ToolCallError::Plain(reason) => wire_error(INVALID_PARAMS, &reason),
        ToolCallError::Internal(reason) => wire_error(INTERNAL_ERROR, &reason),
    }
}

/// MCP initialize 结果（serverInfo/capabilities 与桌面 `get_info` 对齐：
/// serverName = datazen，tools 谓词能力开启）。
pub fn initialize_result() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {
            "tools": { "listChanged": false }
        },
        "serverInfo": {
            "name": contract::SERVER_NAME,
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": "DataZen is a database management tool. Use list_connections to discover available databases, then query them with SQL."
    })
}

/// `tools/list` 结果（无游标分页：与桌面 `list_tools` 的 `next_cursor: None`
/// 一致）。
pub fn list_tools_result() -> Value {
    let mut tools: Vec<Value> = contract::MCP_ALL_TOOLS
        .iter()
        .filter_map(|name| contract::tool_definition(name))
        .collect();
    // 两形态一致的呈现顺序：golden 契约（桌面快照）按工具名字典序排列，
    // server wire 输出同序（tools/list 的客户端渲染与 parity 快照同基线）。
    tools.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .cmp(b["name"].as_str().unwrap_or(""))
    });
    json!({ "tools": tools, "nextCursor": null })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_match_rmcp_semantics() {
        assert_eq!(INVALID_PARAMS, -32602);
        assert_eq!(INTERNAL_ERROR, -32603);
        assert_eq!(METHOD_NOT_FOUND, -32601);
    }

    #[test]
    fn with_help_error_carries_reason_and_help_block() {
        let err = tool_call_error(ToolCallError::WithHelp {
            tool: "query".into(),
            reason: "missing connection_id".into(),
        });
        assert_eq!(err["code"], json!(INVALID_PARAMS));
        let message = err["message"].as_str().unwrap();
        assert!(message.contains("missing connection_id"));
        assert!(message.contains("Tool: query"));
        assert!(message.contains("Example:"));
    }

    #[test]
    fn plain_error_is_bare_invalid_params_without_help() {
        // SessionLost 家族：桌面 wire 行为是裸 invalid_params（无帮助块）。
        let err = tool_call_error(ToolCallError::Plain(
            "SessionLost: MCP client disconnected".into(),
        ));
        assert_eq!(err["code"], json!(INVALID_PARAMS));
        assert_eq!(
            err["message"],
            json!("SessionLost: MCP client disconnected")
        );
    }

    #[test]
    fn internal_error_maps_to_minus_32603() {
        let err = tool_call_error(ToolCallError::Internal("runtime gone".into()));
        assert_eq!(err["code"], json!(INTERNAL_ERROR));
    }

    #[test]
    fn unknown_tool_error_lists_registered_tools() {
        let err = unknown_tool_error("not_a_real_tool");
        assert_eq!(err["code"], json!(INVALID_PARAMS));
        let message = err["message"].as_str().unwrap();
        assert!(message.contains("Unknown or disabled tool"));
        assert!(message.contains("list_connections"));
    }

    #[test]
    fn initialize_result_announces_datazen_server() {
        let result = initialize_result();
        assert_eq!(result["serverInfo"]["name"], json!("datazen"));
        assert_eq!(result["protocolVersion"], json!(PROTOCOL_VERSION));
    }

    #[test]
    fn list_tools_result_lists_all_fifteen_tools() {
        let result = list_tools_result();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 15);
        assert!(result["nextCursor"].is_null());
        for tool in tools {
            assert!(tool["name"].is_string());
            assert!(tool["description"].is_string());
            assert_eq!(tool["inputSchema"]["type"], json!("object"));
        }
    }
}

//! MCP 工具面在 AI 工具循环上的装配（M3，跨形态收敛）。
//!
//! 此前 AI 的 MCP 工具定义与执行直接绑在桌面 `McpClientManager`
//! （stdio 子进程管理器）上，web 形态落地时不可实现。现在消费端口是
//! [`McpToolBroker`]（定义见 `ports.rs`），本模块持有端口之上的装配
//! 用例：qualified 命名规则、`ToolDefinition` 映射与 tool loop 的
//! `String` 执行契约。桌面实现见宿主 `services/feature_ports.rs`
//! （`AppState`）；web 形态未来以 HTTP/SSE 客户端实现同一端口。
//!
//! 文案契约：`execute_mcp_tool` 的错误文本逐字保留桌面历史行为
//! （`"MCP tool error ({qualified}): …"`），工具循环测试与桌面
//! MCP client 测试都不感知本迁移。

use super::ports::{McpBrokeredTool, McpToolBroker};
use crate::ai::domain::types::ToolDefinition;

/// Qualified tool name for AI routing: `mcp/{serverId}/{toolName}`.
///
/// The routing rule is part of the shared tool-loop contract (the model sees
/// and echoes this name), so it lives here rather than in any one form's
/// client manager.
pub fn qualified_tool_name(server_id: &str, tool_name: &str) -> String {
    format!("mcp/{server_id}/{tool_name}")
}

/// The brokered tools of every AI-available server, as provider tool
/// definitions.
pub async fn mcp_tool_definitions(broker: &dyn McpToolBroker) -> Vec<ToolDefinition> {
    broker
        .tools_for_ai()
        .await
        .into_iter()
        .map(|tool| ToolDefinition {
            name: tool.qualified_name,
            description: tool.description.unwrap_or_else(|| tool.tool_name.clone()),
            parameters: tool.input_schema,
        })
        .collect()
}

/// Execute one MCP tool for the tool loop.
///
/// The loop's contract is a plain `String` (the DB tools use the same
/// shape): a formatted result on success, a formatted error otherwise.
/// Invalid JSON arguments are an error report, not a panic — the loop keeps
/// running and the model gets to retry with corrected arguments.
pub async fn execute_mcp_tool(
    broker: &dyn McpToolBroker,
    server_id: &str,
    tool_name: &str,
    arguments: &str,
) -> String {
    let qualified = qualified_tool_name(server_id, tool_name);
    let args: serde_json::Value = match serde_json::from_str(arguments) {
        Ok(value) => value,
        Err(error) => {
            return format!("MCP tool error ({qualified}): invalid JSON arguments: {error}");
        }
    };
    match broker.call_tool(server_id, tool_name, args).await {
        Ok(result) => result,
        Err(message) => format!("MCP tool error ({qualified}): {message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeBroker {
        tools: Vec<McpBrokeredTool>,
        result: Result<String, String>,
        seen_arguments: std::sync::Mutex<Vec<serde_json::Value>>,
    }

    #[async_trait::async_trait]
    impl McpToolBroker for FakeBroker {
        async fn tools_for_ai(&self) -> Vec<McpBrokeredTool> {
            self.tools.clone()
        }
        async fn call_tool(
            &self,
            _server_id: &str,
            _tool_name: &str,
            arguments: serde_json::Value,
        ) -> Result<String, String> {
            self.seen_arguments.lock().unwrap().push(arguments);
            self.result.clone()
        }
    }

    fn broker(tools: Vec<McpBrokeredTool>, result: Result<String, String>) -> FakeBroker {
        FakeBroker {
            tools,
            result,
            seen_arguments: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn tool(server_id: &str, tool_name: &str, description: Option<&str>) -> McpBrokeredTool {
        McpBrokeredTool {
            server_id: server_id.to_owned(),
            tool_name: tool_name.to_owned(),
            qualified_name: qualified_tool_name(server_id, tool_name),
            description: description.map(str::to_owned),
            input_schema: serde_json::json!({ "type": "object" }),
        }
    }

    #[test]
    fn qualified_names_carry_the_routing_prefix() {
        assert_eq!(
            qualified_tool_name("linear", "create_issue"),
            "mcp/linear/create_issue"
        );
    }

    #[tokio::test]
    async fn definitions_use_qualified_names_and_fall_back_to_the_tool_name() {
        let fake = broker(
            vec![
                tool("linear", "create_issue", Some("Create an issue")),
                tool("github", "search_prs", None),
            ],
            Ok(String::new()),
        );
        let definitions = mcp_tool_definitions(&fake).await;
        assert_eq!(definitions.len(), 2);
        assert_eq!(definitions[0].name, "mcp/linear/create_issue");
        assert_eq!(definitions[0].description, "Create an issue");
        // A missing description falls back to the bare tool name so the model
        // still sees a usable label.
        assert_eq!(definitions[1].description, "search_prs");
    }

    #[tokio::test]
    async fn execution_reports_the_formatted_result() {
        let fake = broker(
            Vec::new(),
            Ok("issue-42 created".to_owned()),
        );
        let output = execute_mcp_tool(&fake, "linear", "create_issue", r#"{ "title": "x" }"#).await;
        assert_eq!(output, "issue-42 created");
        assert_eq!(
            fake.seen_arguments.lock().unwrap().as_slice(),
            [serde_json::json!({ "title": "x" })]
        );
    }

    #[tokio::test]
    async fn execution_keeps_the_error_wording_verbatim() {
        let fake = broker(Vec::new(), Err("server not connected".to_owned()));
        let output =
            execute_mcp_tool(&fake, "linear", "create_issue", r#"{ "title": "x" }"#).await;
        assert_eq!(
            output,
            "MCP tool error (mcp/linear/create_issue): server not connected"
        );
    }

    #[tokio::test]
    async fn invalid_json_arguments_report_instead_of_panicking() {
        let fake = broker(Vec::new(), Ok(String::new()));
        let output = execute_mcp_tool(&fake, "linear", "create_issue", "not json").await;
        assert!(output.starts_with("MCP tool error (mcp/linear/create_issue): invalid JSON arguments:"));
        // The broker was never called with the broken payload.
        assert!(fake.seen_arguments.lock().unwrap().is_empty());
    }
}

//! MCP Client — connect to external MCP Servers and invoke their tools.

use regex::Regex;
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, Tool};
use rmcp::service::RunningService;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// When false, tools from this server are excluded from AI Chat tool registration.
    #[serde(default = "default_true")]
    pub enabled_for_ai: bool,
}

fn default_true() -> bool {
    true
}

/// Basenames permitted for MCP stdio server spawn (`Command::new`).
const ALLOWED_MCP_COMMANDS: &[&str] = &[
    "node",
    "nodejs",
    "npx",
    "npm",
    "pnpm",
    "yarn",
    "bun",
    "deno",
    "python",
    "python3",
    "uv",
    "uvx",
    "ruby",
    "perl",
    "sh",
    "bash",
    "zsh",
    "datazen",
];

/// Returns the executable basename (strips directory and `.exe` on Windows).
fn command_basename(cmd: &str) -> &str {
    let base = Path::new(cmd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(cmd);
    base.strip_suffix(".exe").unwrap_or(base)
}

/// Validates that `cmd` is on the MCP spawn allowlist (deny-by-default).
pub fn validate_mcp_spawn_command(cmd: &str) -> Result<(), String> {
    let base = command_basename(cmd);
    if ALLOWED_MCP_COMMANDS.contains(&base) {
        Ok(())
    } else {
        Err(format!(
            "MCP server command '{cmd}' is not allowed. Permitted executables: {}",
            ALLOWED_MCP_COMMANDS.join(", ")
        ))
    }
}

fn mcp_server_id_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap())
}

/// Returns true when `id` matches `^[a-zA-Z0-9_-]+$` (non-empty).
pub fn is_valid_mcp_server_id(id: &str) -> bool {
    !id.is_empty() && mcp_server_id_regex().is_match(id)
}

/// Validates MCP server id; exported for IPC/tests and mirrored in frontend validation.
pub fn validate_mcp_server_id(id: &str) -> Result<(), String> {
    if is_valid_mcp_server_id(id) {
        Ok(())
    } else {
        Err(format!(
            "Invalid MCP server id '{id}': must match ^[a-zA-Z0-9_-]+$"
        ))
    }
}

/// Extract text from MCP tool result blocks; warn on non-text; summarize when only non-text.
pub fn format_call_tool_result(result: &CallToolResult, qualified: &str) -> String {
    let mut text_parts = Vec::new();
    let mut non_text_count = 0usize;

    for block in &result.content {
        match block {
            ContentBlock::Text(t) => text_parts.push(t.text.clone()),
            ContentBlock::Image(_) => {
                non_text_count += 1;
                tracing::warn!(
                    qualified = %qualified,
                    block_type = "image",
                    "MCP tool returned non-text content block"
                );
            }
            ContentBlock::Audio(_) => {
                non_text_count += 1;
                tracing::warn!(
                    qualified = %qualified,
                    block_type = "audio",
                    "MCP tool returned non-text content block"
                );
            }
            ContentBlock::Resource(_) => {
                non_text_count += 1;
                tracing::warn!(
                    qualified = %qualified,
                    block_type = "resource",
                    "MCP tool returned non-text content block"
                );
            }
            ContentBlock::ResourceLink(_) => {
                non_text_count += 1;
                tracing::warn!(
                    qualified = %qualified,
                    block_type = "resource_link",
                    "MCP tool returned non-text content block"
                );
            }
            _ => {
                non_text_count += 1;
                tracing::warn!(
                    qualified = %qualified,
                    block_type = "unknown",
                    "MCP tool returned non-text content block"
                );
            }
        }
    }

    let output = text_parts.join("\n");
    if result.is_error == Some(true) {
        return format!("MCP tool error ({qualified}): {output}");
    }
    if output.is_empty() && non_text_count > 0 {
        return format!(
            "MCP tool ({qualified}) returned {non_text_count} non-text content block(s); no text output available."
        );
    }
    output
}

struct McpClientEntry {
    name: String,
    tools: Vec<Tool>,
    service: RunningService<RoleClient, ()>,
}

#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
type TestCallHandler =
    Arc<dyn Fn(&str, serde_json::Value) -> Result<CallToolResult, String> + Send + Sync>;

#[cfg(test)]
struct TestMcpClientEntry {
    name: String,
    tools: Vec<Tool>,
    call_handler: TestCallHandler,
}

pub struct McpClientManager {
    clients: RwLock<HashMap<String, McpClientEntry>>,
    #[cfg(test)]
    test_clients: RwLock<HashMap<String, TestMcpClientEntry>>,
}

/// Qualified tool name for AI routing: `mcp/{serverId}/{toolName}`.
pub fn mcp_qualified_name(server_id: &str, tool_name: &str) -> String {
    format!("mcp/{server_id}/{tool_name}")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolInfo {
    pub server_id: String,
    pub server_name: String,
    pub tool_name: String,
    pub qualified_name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

impl McpClientManager {
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
            #[cfg(test)]
            test_clients: RwLock::new(HashMap::new()),
        }
    }

    /// Register an in-memory MCP server for integration tests (no subprocess).
    #[cfg(test)]
    pub async fn register_test_server(
        &self,
        server_id: impl Into<String>,
        name: impl Into<String>,
        tools: Vec<Tool>,
        call_handler: TestCallHandler,
    ) {
        let server_id = server_id.into();
        let mut test_clients = self.test_clients.write().await;
        test_clients.insert(
            server_id,
            TestMcpClientEntry {
                name: name.into(),
                tools,
                call_handler,
            },
        );
    }

    const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

    pub async fn connect(&self, config: &McpServerConfig) -> Result<(), String> {
        validate_mcp_server_id(&config.id)?;

        if config.transport != "stdio" {
            return Err(format!("Unsupported transport: {}", config.transport));
        }

        let cmd = config
            .command
            .as_ref()
            .ok_or("Missing command for stdio transport")?;

        validate_mcp_spawn_command(cmd)?;

        let mut command = tokio::process::Command::new(cmd);
        command.args(&config.args);
        // Do not inherit the host process environment — only explicit MCP config vars.
        command.env_clear();
        for (k, v) in &config.env {
            command.env(k, v);
        }

        let transport = TokioChildProcess::new(command.configure(|_| {}))
            .map_err(|e| format!("Failed to spawn MCP server process: {e}"))?;

        let service = tokio::time::timeout(Self::CONNECT_TIMEOUT, ().serve(transport))
            .await
            .map_err(|_| "Connection timed out (30s)".to_string())?
            .map_err(|e| format!("Failed to initialize MCP client: {e}"))?;

        let tools = match tokio::time::timeout(
            Self::CONNECT_TIMEOUT,
            service.peer().list_all_tools(),
        )
        .await
        {
            Ok(Ok(tools)) => tools,
            Ok(Err(e)) => {
                let _ = service.cancel().await;
                return Err(format!("Failed to list tools: {e}"));
            }
            Err(_) => {
                let _ = service.cancel().await;
                return Err("Listing tools timed out (30s)".to_string());
            }
        };

        tracing::info!(
            server_id = %config.id,
            tools_count = tools.len(),
            "Connected to external MCP server"
        );

        let mut clients = self.clients.write().await;
        if let Some(old) = clients.remove(&config.id) {
            let _ = old.service.cancel().await;
        }
        clients.insert(
            config.id.clone(),
            McpClientEntry {
                name: config.name.clone(),
                tools,
                service,
            },
        );
        Ok(())
    }

    pub async fn disconnect(&self, server_id: &str) -> Result<(), String> {
        #[cfg(test)]
        {
            self.test_clients.write().await.remove(server_id);
        }
        let mut clients = self.clients.write().await;
        if let Some(entry) = clients.remove(server_id) {
            let _ = entry.service.cancel().await;
            tracing::info!(server_id, "Disconnected from MCP server");
        }
        Ok(())
    }

    pub async fn disconnect_all(&self) {
        let mut clients = self.clients.write().await;
        for (id, entry) in clients.drain() {
            let _ = entry.service.cancel().await;
            tracing::info!(server_id = %id, "Disconnected MCP server");
        }
    }

    pub async fn all_tools(&self) -> Vec<McpToolInfo> {
        let clients = self.clients.read().await;
        #[allow(unused_mut)]
        let mut tools: Vec<McpToolInfo> = clients
            .iter()
            .flat_map(|(server_id, info)| {
                info.tools.iter().map(move |tool| {
                    let tool_name = tool.name.to_string();
                    McpToolInfo {
                        server_id: server_id.clone(),
                        server_name: info.name.clone(),
                        qualified_name: mcp_qualified_name(server_id, &tool_name),
                        tool_name,
                        description: tool.description.as_ref().map(|d| d.to_string()),
                        input_schema: serde_json::Value::Object(tool.input_schema.as_ref().clone()),
                    }
                })
            })
            .collect();

        #[cfg(test)]
        {
            let test_clients = self.test_clients.read().await;
            for (server_id, info) in test_clients.iter() {
                for tool in &info.tools {
                    let tool_name = tool.name.to_string();
                    tools.push(McpToolInfo {
                        server_id: server_id.clone(),
                        server_name: info.name.clone(),
                        qualified_name: mcp_qualified_name(server_id, &tool_name),
                        tool_name,
                        description: tool.description.as_ref().map(|d| d.to_string()),
                        input_schema: serde_json::Value::Object(tool.input_schema.as_ref().clone()),
                    });
                }
            }
        }

        tools
    }

    pub async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<CallToolResult, String> {
        #[cfg(test)]
        {
            let test_clients = self.test_clients.read().await;
            if let Some(entry) = test_clients.get(server_id) {
                return (entry.call_handler)(tool_name, arguments);
            }
        }

        let peer = {
            let clients = self.clients.read().await;
            let entry = clients
                .get(server_id)
                .ok_or_else(|| format!("MCP server '{server_id}' not connected"))?;
            entry.service.peer().clone()
        };

        let args_map = match arguments.as_object() {
            Some(m) => m.clone(),
            None => {
                return Err("Tool arguments must be a JSON object".to_string());
            }
        };

        let mut request = CallToolRequestParams::new(tool_name.to_string());
        request.arguments = Some(args_map);

        peer.call_tool(request)
            .await
            .map_err(|e| format!("Tool call failed: {e}"))
    }

    pub async fn connected_servers(&self) -> Vec<(String, String, usize)> {
        let clients = self.clients.read().await;
        #[allow(unused_mut)]
        let mut servers: Vec<(String, String, usize)> = clients
            .iter()
            .map(|(id, e)| (id.clone(), e.name.clone(), e.tools.len()))
            .collect();

        #[cfg(test)]
        {
            let test_clients = self.test_clients.read().await;
            for (id, e) in test_clients.iter() {
                servers.push((id.clone(), e.name.clone(), e.tools.len()));
            }
        }

        servers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_server_config_deserialization() {
        let json = r#"{
            "id": "test",
            "name": "Test Server",
            "transport": "stdio",
            "command": "/usr/bin/test-mcp",
            "args": ["--flag"],
            "env": {"KEY": "VALUE"},
            "enabled": true
        }"#;
        let config: McpServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.id, "test");
        assert_eq!(config.transport, "stdio");
        assert_eq!(config.command, Some("/usr/bin/test-mcp".to_string()));
        assert_eq!(config.args, vec!["--flag"]);
        assert!(config.enabled);
    }

    #[test]
    fn test_mcp_server_config_defaults() {
        let json = r#"{
            "id": "minimal",
            "name": "Minimal",
            "transport": "stdio"
        }"#;
        let config: McpServerConfig = serde_json::from_str(json).unwrap();
        assert!(config.args.is_empty());
        assert!(config.env.is_empty());
        assert!(config.enabled);
        assert!(config.command.is_none());
    }

    #[test]
    fn mcp_qualified_name_format() {
        assert_eq!(
            mcp_qualified_name("my-server", "search"),
            "mcp/my-server/search"
        );
    }

    #[test]
    fn test_mcp_tool_info_serialization() {
        let info = McpToolInfo {
            server_id: "s1".into(),
            server_name: "Server 1".into(),
            tool_name: "tool1".into(),
            qualified_name: mcp_qualified_name("s1", "tool1"),
            description: Some("A test tool".into()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "query": { "type": "string" } }
            }),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("serverId"));
        assert!(json.contains("serverName"));
        assert!(json.contains("toolName"));
        assert!(json.contains("qualifiedName"));
        assert!(json.contains("inputSchema"));
        assert!(json.contains("mcp/s1/tool1"));
    }

    #[test]
    fn validate_mcp_spawn_command_rejects_unknown_executables() {
        assert!(validate_mcp_spawn_command("node").is_ok());
        assert!(validate_mcp_spawn_command("/usr/bin/python3").is_ok());
        assert!(validate_mcp_spawn_command("/usr/local/bin/npx").is_ok());
        assert!(validate_mcp_spawn_command("/tmp/evil").is_err());
        assert!(validate_mcp_spawn_command("curl").is_err());
    }

    #[test]
    fn default_true_enables_servers_by_default() {
        assert!(default_true());
    }

    #[test]
    fn validate_mcp_server_id_accepts_alphanumeric_dash_underscore() {
        assert!(is_valid_mcp_server_id("my-server_1"));
        assert!(validate_mcp_server_id("test_srv").is_ok());
    }

    #[test]
    fn validate_mcp_server_id_rejects_invalid_chars() {
        assert!(!is_valid_mcp_server_id(""));
        assert!(!is_valid_mcp_server_id("bad id"));
        assert!(!is_valid_mcp_server_id("bad/id"));
        assert!(validate_mcp_server_id("bad id").is_err());
    }

    #[test]
    fn mcp_server_config_enabled_for_ai_defaults_true() {
        let json = r#"{
            "id": "minimal",
            "name": "Minimal",
            "transport": "stdio"
        }"#;
        let config: McpServerConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled_for_ai);
    }

    #[test]
    fn format_call_tool_result_warns_on_non_text_only() {
        let result = CallToolResult::success(vec![ContentBlock::image("abc", "image/png")]);
        let out = format_call_tool_result(&result, "mcp/test/ping");
        assert!(out.contains("non-text content block"));
        assert!(!out.is_empty());
    }

    #[tokio::test]
    async fn new_manager_starts_empty() {
        let mgr = McpClientManager::new();
        assert!(mgr.connected_servers().await.is_empty());
        assert!(mgr.all_tools().await.is_empty());
    }

    #[tokio::test]
    async fn disconnect_unknown_server_is_noop() {
        let mgr = McpClientManager::new();
        mgr.disconnect("missing").await.unwrap();
    }

    #[tokio::test]
    async fn call_tool_errors_when_server_not_connected() {
        let mgr = McpClientManager::new();
        let err = mgr
            .call_tool("nope", "tool", serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(err.contains("not connected"));
    }
}

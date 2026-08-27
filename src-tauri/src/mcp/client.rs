//! MCP Client — connect to external MCP Servers and invoke their tools.

use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use rmcp::service::RunningService;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
}

fn default_true() -> bool {
    true
}

struct McpClientEntry {
    name: String,
    tools: Vec<Tool>,
    service: RunningService<RoleClient, ()>,
}

pub struct McpClientManager {
    clients: RwLock<HashMap<String, McpClientEntry>>,
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
        }
    }

    const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

    pub async fn connect(&self, config: &McpServerConfig) -> Result<(), String> {
        if config.transport != "stdio" {
            return Err(format!("Unsupported transport: {}", config.transport));
        }

        let cmd = config
            .command
            .as_ref()
            .ok_or("Missing command for stdio transport")?;

        let mut command = tokio::process::Command::new(cmd);
        command.args(&config.args);
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
        clients
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
                        input_schema: serde_json::Value::Object(
                            tool.input_schema.as_ref().clone(),
                        ),
                    }
                })
            })
            .collect()
    }

    pub async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<CallToolResult, String> {
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
        clients
            .iter()
            .map(|(id, e)| (id.clone(), e.name.clone(), e.tools.len()))
            .collect()
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
    fn default_true_enables_servers_by_default() {
        assert!(default_true());
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

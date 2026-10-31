use datazen::ai::application::ports::McpToolBroker;
use datazen::services::ports::DatabaseToolHost;
use datazen_driver_api::{ConnectionConfig, DatabaseDriver};
use std::sync::Arc;
#[async_trait::async_trait]
impl DatabaseToolHost for crate::commands::AppState {
    async fn get_session_config(&self, id: &str) -> Result<ConnectionConfig, String> {
        let adapter = self.platform_require().map_err(|error| error.to_string())?;
        let call = adapter.begin_call();
        let view = adapter
            .runtime()
            .map_err(|error| error.to_string())?
            .get_session_by_id(call.context(), id)
            .await
            .map_err(|error| error.to_string())?;
        self.store
            .platform_profile(view.connection_id.as_str())
            .await
            .map(|(config, _)| config)
            .ok_or_else(|| "Profile unavailable".to_string())
    }
    async fn execution_settings(&self) -> datazen::settings::ExecutionSettings {
        self.store.execution_settings().await
    }
    async fn profile_config(&self, id: &str) -> Result<ConnectionConfig, String> {
        self.store
            .platform_profile(id)
            .await
            .filter(|(_, metadata)| metadata.enabled)
            .map(|(config, _)| config)
            .ok_or("Profile unavailable".into())
    }
    async fn driver_for_connection(&self, id: &str) -> Result<Arc<dyn DatabaseDriver>, String> {
        let config = self.profile_config(id).await?;
        self.driver_registry
            .get(&config.database_type)
            .await
            .ok_or("Driver unavailable".into())
    }
    async fn execute_target(
        &self,
        id: &str,
        command: &str,
        input: serde_json::Value,
    ) -> Result<datazen_driver_api::CommandResult, String> {
        let step = datazen::workflow::command::WorkflowCommandStep::new(
            "tool",
            command,
            Some(id.into()),
            input,
        );
        datazen::workflow::resource_execution::execute(self, &step, id, None, None, false)
            .await
            .map_err(|error| error.to_string())
    }
}

/// M3: the desktop `McpToolBroker` — the stdio subprocess manager behind the
/// shared AI tool-loop port. Transport, spawn whitelist and token checks stay
/// here; a web form implements the same port over HTTP/SSE.
#[async_trait::async_trait]
impl McpToolBroker for crate::commands::AppState {
    async fn tools_for_ai(&self) -> Vec<datazen::ai::application::ports::McpBrokeredTool> {
        let settings = self.store.get_settings().await;
        self.mcp_client_manager
            .all_tools()
            .await
            .into_iter()
            .filter(|tool| {
                settings
                    .mcp_client_servers
                    .iter()
                    .find(|c| c.id == tool.server_id)
                    .map(|c| c.enabled_for_ai)
                    .unwrap_or(true)
            })
            .map(|tool| {
                datazen::ai::application::ports::McpBrokeredTool {
                    server_id: tool.server_id,
                    tool_name: tool.tool_name,
                    qualified_name: tool.qualified_name,
                    description: tool.description,
                    input_schema: tool.input_schema,
                }
            })
            .collect()
    }

    async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<String, String> {
        let qualified = datazen::ai::mcp_tools::qualified_tool_name(server_id, tool_name);
        self.mcp_client_manager
            .call_tool(server_id, tool_name, arguments)
            .await
            .map(|result| crate::mcp::format_call_tool_result(&result, &qualified))
    }
}

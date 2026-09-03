//! MCP `ServerHandler` implementation — tools, resources, and capabilities.

use super::server::DataZenMcpServer;
use crate::mcp::permission;
use crate::mcp::tool_help;
use rmcp::model::*;
use rmcp::service::RequestContext;
use rmcp::{prompt_handler, tool_handler, tool_router, ErrorData as McpError, RoleServer, ServerHandler};

#[tool_handler]
#[prompt_handler]
impl ServerHandler for DataZenMcpServer {
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: self.list_tools_inner(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let tool_name = request.name.to_string();

        if !self.tool_is_registered(&tool_name) {
            return Err(tool_help::unknown_tool_error(&tool_name));
        }

        permission::check_tool_call(
            &tool_name,
            self.permission_mode,
            &self.disabled_tools,
            request.arguments.as_ref(),
        )
        .map_err(|e| tool_help::tool_error(&tool_name, &e))?;

        if let Some(args) = request.arguments.as_ref() {
            if let Some(connection_id) = args.get("connection_id").and_then(|v| v.as_str()) {
                self.ensure_allowed(&tool_name, connection_id)?;
            }
        }

        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        Self::tool_router()
            .call(tcc)
            .await
            .map_err(|e| tool_help::enrich_tool_error(&tool_name, e))
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
        .with_server_info(Implementation::new("datazen", env!("CARGO_PKG_VERSION")))
        .with_instructions("DataZen is a database management tool. Use list_connections to discover available databases, then query them with SQL.".to_string())
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: self.list_resources_inner(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![ResourceTemplate::new(
                "datazen://schema/{connectionId}/{database}",
                "Database Schema",
            )],
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        self.read_resource_inner(request.uri.as_str()).await
    }
}

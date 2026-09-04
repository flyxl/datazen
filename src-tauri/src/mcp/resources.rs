//! MCP resource listing/reading and test-oriented tool dispatch helpers.

use super::server::{DataZenMcpServer, MCP_ALL_TOOLS};
#[cfg(test)]
use super::types::*;
use crate::ai::budget;
use crate::mcp::allowlist;
use crate::mcp::permission;
#[cfg(test)]
use crate::mcp::tool_help;
#[cfg(test)]
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::ErrorData as McpError;

impl DataZenMcpServer {
    pub(crate) async fn read_resource_inner(
        &self,
        uri: &str,
    ) -> Result<ReadResourceResult, McpError> {
        if uri == "datazen://connections" {
            let connections = self.app_state.store.get_connections().await;
            let result: Vec<serde_json::Value> = connections
                .iter()
                .filter(|c| allowlist::is_connection_allowed(&c.id, &self.allowed_connection_ids))
                .map(|c| {
                    serde_json::json!({
                        "id": c.id,
                        "name": c.name,
                        "databaseType": format!("{:?}", c.database_type),
                        "host": c.host,
                        "database": c.database,
                    })
                })
                .collect();
            let json = serde_json::to_string_pretty(&result)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(
                json, uri,
            )]));
        }

        if uri == "datazen://workflows" {
            let workflows = self.app_state.workflow_registry.list().await;
            let json = serde_json::to_string_pretty(&workflows)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(
                json, uri,
            )]));
        }

        if uri == "datazen://query-history" {
            if !permission::query_history_content_allowed(self.permission_mode) {
                return Ok(ReadResourceResult::new(vec![ResourceContents::text(
                    "[]", uri,
                )]));
            }
            let history = self
                .app_state
                .store
                .get_query_history(50, None, None, None)
                .await;
            let filtered: Vec<_> = history
                .into_iter()
                .filter(|entry| {
                    allowlist::is_connection_allowed(
                        &entry.connection_id,
                        &self.allowed_connection_ids,
                    )
                })
                .collect();
            let json = serde_json::to_string_pretty(&filtered)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(
                json, uri,
            )]));
        }

        if let Some(rest) = uri.strip_prefix("datazen://schema/") {
            let decoded = urlencoding::decode(rest).map_err(|e| {
                McpError::invalid_params(format!("Invalid URI encoding: {e}"), None)
            })?;
            let parts: Vec<&str> = decoded.splitn(2, '/').collect();
            let connection_id = parts
                .first()
                .ok_or_else(|| McpError::invalid_params("Missing connection ID in URI", None))?;
            let db = parts.get(1).copied().unwrap_or("");

            let (runtime_id, _driver, _handle) = self.resolve_connection(connection_id).await?;

            let context = self
                .app_state
                .schema_context_builder
                .build_sql_context(&runtime_id, db, None, &[], budget::MCP_RESOURCE)
                .await
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;

            return Ok(ReadResourceResult::new(vec![ResourceContents::text(
                context.schema_ddl,
                uri,
            )]));
        }

        Err(McpError::resource_not_found(
            format!("Unknown resource: {uri}"),
            None,
        ))
    }

    #[cfg(test)]
    pub(crate) async fn call_tool_inner(
        &self,
        name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<CallToolResult, McpError> {
        let arg_map = arguments.as_ref().and_then(|v| v.as_object());
        permission::check_tool_call(name, self.permission_mode, &self.disabled_tools, arg_map)
            .map_err(|e| tool_help::tool_error(name, &e))?;

        let text = match name {
            "list_connections" => self.list_connections().await?,
            "list_databases" => {
                let connection_id = arguments
                    .as_ref()
                    .and_then(|a| a.get("connection_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                self.list_databases(Parameters(ListDatabasesInput {
                    connection_id: connection_id.into(),
                }))
                .await?
            }
            "list_tables" => {
                let connection_id = arguments
                    .as_ref()
                    .and_then(|a| a.get("connection_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let database = arguments
                    .as_ref()
                    .and_then(|a| a.get("database"))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                self.list_tables(Parameters(ListTablesInput {
                    connection_id: connection_id.into(),
                    database,
                }))
                .await?
            }
            "search_tables" => {
                let connection_id = arguments
                    .as_ref()
                    .and_then(|a| a.get("connection_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let database = arguments
                    .as_ref()
                    .and_then(|a| a.get("database"))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let pattern = arguments
                    .as_ref()
                    .and_then(|a| a.get("pattern"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let limit = arguments
                    .as_ref()
                    .and_then(|a| a.get("limit"))
                    .and_then(|v| v.as_u64());
                self.search_tables(Parameters(SearchTablesInput {
                    connection_id: connection_id.into(),
                    database,
                    pattern: pattern.into(),
                    limit,
                }))
                .await?
            }
            "query" => {
                let connection_id = arguments
                    .as_ref()
                    .and_then(|a| a.get("connection_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let sql = arguments
                    .as_ref()
                    .and_then(|a| a.get("sql"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let limit = arguments
                    .as_ref()
                    .and_then(|a| a.get("limit"))
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32);
                self.query(Parameters(QueryInput {
                    connection_id: connection_id.into(),
                    sql: sql.into(),
                    limit,
                }))
                .await?
            }
            other => return Err(tool_help::unknown_tool_error(other)),
        };

        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    pub(crate) fn list_tools_inner(&self) -> Vec<Tool> {
        let mut router = Self::tool_router();
        for name in MCP_ALL_TOOLS {
            if !permission::is_tool_listed(name, self.permission_mode, &self.disabled_tools) {
                router.disable_route(name.to_string());
            }
        }
        router.list_all()
    }

    pub(crate) fn list_resources_inner(&self) -> Vec<Resource> {
        [
            ("datazen://connections", "Database Connections"),
            ("datazen://query-history", "Query History"),
            ("datazen://workflows", "Available Workflows"),
        ]
        .into_iter()
        .filter(|(uri, _)| permission::is_resource_listed(uri, self.permission_mode))
        .map(|(uri, name)| Resource::new(uri, name))
        .collect()
    }
}

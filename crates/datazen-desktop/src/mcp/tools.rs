//! MCP tool handlers and helpers.
//!
//! M1 收官后本文件是纯 rmcp 绑定层：`#[tool]` 宏、Input 的 JsonSchema、
//! 连接白名单检查（`ensure_allowed`）与错误映射。工具业务在
//! `datazen::mcp::tools`（无状态十工具）与 `datazen::mcp::session_tools`
//! （有状态 open/use/close_session，M1 收官 + M2 主体归位）；
//! 会话 registry 与清理调度语义在 `datazen::mcp::session`。

use super::server::{DataZenMcpServer, MCP_SESSION_PURPOSE};
use super::types::*;
use crate::mcp::tool_help;
use datazen::mcp::session_tools::{OwnerStamp, SessionToolError, StatefulSessionHost};
use datazen::mcp::tools::ToolError;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router, ErrorData as McpError};
use std::sync::Arc;

pub(crate) use datazen::mcp::describe::format_table_description;

fn tool_shell_error(tool: &str, error: ToolError) -> McpError {
    match error {
        ToolError::InvalidParams(message) => tool_help::tool_error(tool, &message),
        ToolError::Internal(message) => DataZenMcpServer::map_err(message),
    }
}

/// 有状态工具的三分错误映射（宿主历史 wire 行为的显式承载）：
/// WithHelp → invalid_params + 工具帮助块；Plain → 裸 invalid_params
/// （SessionLost 家族不附帮助块）；Internal → internal_error。
fn session_tool_error(tool: &str, error: SessionToolError) -> McpError {
    match error {
        SessionToolError::WithHelp(message) => tool_help::tool_error(tool, &message),
        SessionToolError::Plain(message) => McpError::invalid_params(message, None),
        SessionToolError::Internal(message) => DataZenMcpServer::map_err(message),
    }
}

/// This server instance behind the shared stateful-session port.
fn stateful_host(server: &DataZenMcpServer) -> Arc<dyn StatefulSessionHost> {
    Arc::new(server.clone())
}

#[tool_router(vis = "pub(crate)")]
impl DataZenMcpServer {
    #[tool(
        description = "List all configured database connections. Returns connection IDs, names, database types, and hosts."
    )]
    pub(crate) async fn list_connections(&self) -> Result<String, McpError> {
        datazen::mcp::tools::list_connections(self.app_state.as_ref(), &self.allowed_connection_ids)
            .await
            .map_err(|e| DataZenMcpServer::map_err(e.into_message()))
    }

    #[tool(description = "List all databases on a connected server.")]
    pub(crate) async fn list_databases(
        &self,
        Parameters(input): Parameters<ListDatabasesInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("list_databases", &input.connection_id)?;
        datazen::mcp::tools::list_databases(self.resource_host.as_ref(), &input.connection_id)
            .await
            .map_err(|e| tool_shell_error("list_databases", e))
    }

    #[tool(
        description = "List all tables in a database. Returns table names with types and row counts."
    )]
    pub(crate) async fn list_tables(
        &self,
        Parameters(input): Parameters<ListTablesInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("list_tables", &input.connection_id)?;
        datazen::mcp::tools::list_tables(
            self.resource_host.as_ref(),
            &input.connection_id,
            input.database,
            input.schema,
        )
        .await
        .map_err(|e| tool_shell_error("list_tables", e))
    }

    #[tool(
        description = "Search for tables by name pattern (case-insensitive substring match). Use this instead of list_tables when the database has many tables."
    )]
    pub(crate) async fn search_tables(
        &self,
        Parameters(input): Parameters<SearchTablesInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("search_tables", &input.connection_id)?;
        datazen::mcp::tools::search_tables(
            self.resource_host.as_ref(),
            &input.connection_id,
            input.database,
            input.schema,
            &input.pattern,
            input.limit,
        )
        .await
        .map_err(|e| tool_shell_error("search_tables", e))
    }

    #[tool(
        description = "Execute a SQL query on a connected database. Returns results as JSON. Use list_connections first to get valid connection IDs. Default row limit is 100 (max 50000)."
    )]
    pub(crate) async fn query(
        &self,
        Parameters(input): Parameters<QueryInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("query", &input.connection_id)?;
        let settings = self.app_state.store.execution_settings().await;
        datazen::mcp::tools::query(
            self.resource_host.as_ref(),
            &input.connection_id,
            &input.sql,
            input.limit,
            self.permission_mode,
            &settings,
        )
        .await
        .map_err(|e| tool_shell_error("query", e))
    }

    #[tool(
        description = "Get detailed schema of a table including columns, types, primary keys, foreign keys, and indexes."
    )]
    pub(crate) async fn get_schema(
        &self,
        Parameters(input): Parameters<GetSchemaInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("get_schema", &input.connection_id)?;
        datazen::mcp::tools::get_schema(
            self.resource_host.as_ref(),
            &input.connection_id,
            input.database,
            input.schema,
            &input.table,
        )
        .await
        .map_err(|e| tool_shell_error("get_schema", e))
    }

    #[tool(
        description = "Get the execution plan (EXPLAIN) for a SQL query. Useful for performance analysis."
    )]
    pub(crate) async fn explain_query(
        &self,
        Parameters(input): Parameters<ExplainQueryInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("explain_query", &input.connection_id)?;
        datazen::mcp::tools::explain_query(
            self.resource_host.as_ref(),
            &input.connection_id,
            &input.sql,
        )
        .await
        .map_err(|e| tool_shell_error("explain_query", e))
    }

    #[tool(
        description = "Get a human-readable description of a table including columns, types, constraints, and indexes."
    )]
    pub(crate) async fn describe_table(
        &self,
        Parameters(input): Parameters<DescribeTableInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("describe_table", &input.connection_id)?;
        datazen::mcp::tools::describe_table(
            self.resource_host.as_ref(),
            &input.connection_id,
            input.database,
            input.schema,
            &input.table,
        )
        .await
        .map_err(|e| tool_shell_error("describe_table", e))
    }

    #[tool(
        description = "List all available user-defined workflows. Workflows are reusable AI automations combining prompts and database operations."
    )]
    pub(crate) async fn list_workflows(&self) -> Result<String, McpError> {
        datazen::mcp::tools::list_workflows(self.app_state.as_ref())
            .await
            .map_err(|e| DataZenMcpServer::map_err(e.into_message()))
    }

    #[tool(
        description = "Execute a user-defined workflow by ID. Workflows are reusable automations combining prompts and database operations. Use list_workflows to see available workflows."
    )]
    pub(crate) async fn run_workflow(
        &self,
        Parameters(input): Parameters<RunWorkflowInput>,
    ) -> Result<String, McpError> {
        datazen::mcp::tools::run_workflow(
            self.app_state.as_ref(),
            self.app_state.as_ref(),
            &input.workflow_id,
            input.connection_id.as_deref(),
            &input.variables,
            self.permission_mode,
        )
        .await
        .map_err(|e| tool_shell_error("run_workflow", e))
    }

    /// Stateful protocol step 1 (summary §4.2): open a session on an **explicit**
    /// target and hand back an opaque reference. Nothing about the GUI active
    /// connection, tab or database is consulted.
    #[tool(
        description = "Open a database session on an explicit target and return an opaque clientSessionRef. Use use_session to run commands on it and close_session to release it. Stateful: the same reference always means the same session."
    )]
    pub(crate) async fn open_session(
        &self,
        Parameters(input): Parameters<OpenSessionInput>,
    ) -> Result<String, McpError> {
        const TOOL: &str = "open_session";
        self.ensure_allowed(TOOL, &input.connection_id)?;
        let stamp = OwnerStamp {
            client_instance_id: self.client_instance_id.clone(),
            purpose: MCP_SESSION_PURPOSE.to_owned(),
        };
        datazen::mcp::session_tools::open_session(
            stateful_host(self),
            self.sessions.clone(),
            &stamp,
            &input.connection_id,
            Some(input.database),
            input.schema,
            input.ttl_ms,
        )
        .await
        .map_err(|e| session_tool_error(TOOL, e))
    }

    /// Stateful protocol step 2: run one command inside a mapping this client owns.
    #[tool(
        description = "Run one driver command inside a session opened by open_session. Pass the clientSessionRef returned by open_session. The reference is bound to the MCP client that opened it; a reference held by another client, or one that is closed or expired, returns SessionLost."
    )]
    pub(crate) async fn use_session(
        &self,
        Parameters(input): Parameters<UseSessionInput>,
    ) -> Result<String, McpError> {
        const TOOL: &str = "use_session";
        let stamp = OwnerStamp {
            client_instance_id: self.client_instance_id.clone(),
            purpose: MCP_SESSION_PURPOSE.to_owned(),
        };
        datazen::mcp::session_tools::use_session(
            stateful_host(self).as_ref(),
            &self.sessions,
            self.permission_mode,
            &stamp,
            &input.client_session_ref,
            &input.command,
            input.input,
            input.timeout_ms,
        )
        .await
        .map_err(|e| session_tool_error(TOOL, e))
    }

    /// Stateful protocol step 3: release one mapping this client owns.
    #[tool(
        description = "Close a session opened by open_session and release its database session. Unknown, already closed and expired references all report the same thing, so a stale reference never reveals whether it once existed."
    )]
    pub(crate) async fn close_session(
        &self,
        Parameters(input): Parameters<CloseSessionInput>,
    ) -> Result<String, McpError> {
        const TOOL: &str = "close_session";
        let stamp = OwnerStamp {
            client_instance_id: self.client_instance_id.clone(),
            purpose: MCP_SESSION_PURPOSE.to_owned(),
        };
        datazen::mcp::session_tools::close_session(
            stateful_host(self).as_ref(),
            &self.sessions,
            &stamp,
            &input.client_session_ref,
            input.mode.as_deref(),
        )
        .await
        .map_err(|e| session_tool_error(TOOL, e))
    }
}

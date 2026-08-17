//! DataZen MCP Server handler — tools + resources + prompts.

use crate::ai::budget;
use crate::ai::prompt_resolver;
use crate::commands::AppState;
use crate::mcp::allowlist;
use crate::mcp::permission::{self, McpPermissionMode};
use datazen_driver_api::PromptScenario;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::service::RequestContext;
use rmcp::{
    prompt, prompt_handler, prompt_router, tool, tool_handler, tool_router, ErrorData as McpError,
    RoleServer, ServerHandler,
};
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

// ─── Tool Input Types ───

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryInput {
    /// Persistent connection config id (from list_connections)
    pub config_id: String,
    /// SQL query to execute
    pub sql: String,
    /// Maximum rows to return (default: 100, max: 50000)
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListTablesInput {
    /// Persistent connection config id (from list_connections)
    pub config_id: String,
    /// Optional database name
    pub database: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchTablesInput {
    /// Persistent connection config id (from list_connections)
    pub config_id: String,
    /// Optional database name
    pub database: Option<String>,
    /// Search keyword to match against table names (case-insensitive)
    pub pattern: String,
    /// Max results to return (default: 20)
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSchemaInput {
    /// Persistent connection config id (from list_connections)
    pub config_id: String,
    /// Table name
    pub table: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExplainQueryInput {
    /// Persistent connection config id (from list_connections)
    pub config_id: String,
    /// SQL query to analyze
    pub sql: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DescribeTableInput {
    /// Persistent connection config id (from list_connections)
    pub config_id: String,
    /// Table name
    pub table: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListDatabasesInput {
    /// Persistent connection config id (from list_connections)
    pub config_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunWorkflowInput {
    /// Workflow ID to execute
    pub workflow_id: String,
    /// Input variables for the workflow (JSON object)
    #[serde(default)]
    pub variables: serde_json::Value,
    /// Optional persistent connection config id (some workflows require a database connection)
    pub config_id: Option<String>,
}

// ─── Prompt Argument Types ───

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Nl2SqlArgs {
    /// Persistent connection config id for schema context
    pub config_id: String,
    /// Natural language description of the query
    pub question: String,
    /// Optional database name
    pub database: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DiagnoseErrorArgs {
    /// Persistent connection config id
    pub config_id: String,
    /// The SQL that caused the error
    pub sql: String,
    /// The error message
    pub error: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExplainPlanArgs {
    /// Persistent connection config id
    pub config_id: String,
    /// The SQL query to explain
    pub sql: String,
}

// ─── MCP Server ───

#[derive(Clone)]
pub struct DataZenMcpServer {
    app_state: Arc<AppState>,
    disabled_tools: HashSet<String>,
    permission_mode: McpPermissionMode,
    /// Empty = all saved connections are exposed to MCP.
    allowed_connection_ids: Vec<String>,
}

pub const MCP_ALL_TOOLS: &[&str] = &[
    "list_connections",
    "list_databases",
    "list_tables",
    "search_tables",
    "query",
    "get_schema",
    "explain_query",
    "describe_table",
    "list_workflows",
    "run_workflow",
];

impl DataZenMcpServer {
    pub fn new(app_state: Arc<AppState>) -> Self {
        Self {
            app_state,
            disabled_tools: HashSet::new(),
            permission_mode: McpPermissionMode::default(),
            allowed_connection_ids: Vec::new(),
        }
    }

    pub fn with_disabled_tools(mut self, disabled: &[String]) -> Self {
        self.disabled_tools = disabled.iter().cloned().collect();
        self
    }

    pub fn with_permission_mode(mut self, mode: McpPermissionMode) -> Self {
        self.permission_mode = mode;
        self
    }

    pub fn with_allowed_connections(mut self, allowed: &[String]) -> Self {
        self.allowed_connection_ids = allowed.to_vec();
        self
    }

    fn map_err(e: String) -> McpError {
        McpError::internal_error(e, None)
    }

    fn ensure_allowed(&self, config_id: &str) -> Result<(), McpError> {
        allowlist::ensure_connection_allowed(config_id, &self.allowed_connection_ids)
            .map_err(|e| McpError::invalid_params(e, None))
    }

    async fn resolve_connection(
        &self,
        id: &str,
    ) -> Result<
        (
            String,
            std::sync::Arc<dyn datazen_driver_api::DatabaseDriver>,
            datazen_driver_api::ConnectionHandle,
        ),
        McpError,
    > {
        self.ensure_allowed(id)?;
        crate::services::db_tools::resolve_connection_with_id(
            &self.app_state.connection_manager,
            id,
        )
        .await
        .map_err(Self::map_err)
    }
}

/// Human-readable table description for MCP `describe_table` tool output.
pub(crate) fn format_table_description(
    table: &str,
    schema: &datazen_driver_api::TableSchema,
) -> String {
    let mut desc = format!("Table: {table}\n\nColumns:\n");
    for col in &schema.columns {
        desc.push_str(&format!(
            "  - {} {} {}{}\n",
            col.name,
            col.data_type,
            if col.is_primary_key { "PK " } else { "" },
            if col.nullable { "" } else { "NOT NULL " },
        ));
    }

    if !schema.primary_keys.is_empty() {
        desc.push_str(&format!(
            "\nPrimary Key: ({})\n",
            schema.primary_keys.join(", ")
        ));
    }

    if !schema.indexes.is_empty() {
        desc.push_str("\nIndexes:\n");
        for idx in &schema.indexes {
            desc.push_str(&format!(
                "  - {} ({}) {}\n",
                idx.name,
                idx.columns.join(", "),
                if idx.is_unique { "UNIQUE" } else { "" }
            ));
        }
    }

    if !schema.foreign_keys.is_empty() {
        desc.push_str("\nForeign Keys:\n");
        for fk in &schema.foreign_keys {
            desc.push_str(&format!(
                "  - {} ({}) → {}.{}\n",
                fk.name,
                fk.columns.join(", "),
                fk.referenced_table,
                fk.referenced_columns.join(", ")
            ));
        }
    }

    desc
}

// ─── Tools ───

#[tool_router]
impl DataZenMcpServer {
    #[tool(
        description = "List all configured database connections. Returns config IDs, names, database types, and hosts."
    )]
    async fn list_connections(&self) -> Result<String, McpError> {
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
        serde_json::to_string_pretty(&result).map_err(|e| Self::map_err(format!("Error: {e}")))
    }

    #[tool(description = "List all databases on a connected server.")]
    async fn list_databases(
        &self,
        Parameters(input): Parameters<ListDatabasesInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed(&input.config_id)?;
        crate::services::db_tools::list_databases(
            &self.app_state.connection_manager,
            &input.config_id,
        )
        .await
        .map_err(Self::map_err)
    }

    #[tool(
        description = "List all tables in a database. Returns table names with types and row counts."
    )]
    async fn list_tables(
        &self,
        Parameters(input): Parameters<ListTablesInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed(&input.config_id)?;
        let db = input.database.as_deref().unwrap_or("");
        crate::services::db_tools::list_tables(
            &self.app_state.connection_manager,
            &input.config_id,
            db,
        )
        .await
        .map_err(Self::map_err)
    }

    #[tool(
        description = "Search for tables by name pattern (case-insensitive substring match). Use this instead of list_tables when the database has many tables."
    )]
    async fn search_tables(
        &self,
        Parameters(input): Parameters<SearchTablesInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed(&input.config_id)?;
        let db = input.database.as_deref().unwrap_or("");
        let limit = input.limit.unwrap_or(20) as usize;
        crate::services::db_tools::search_tables(
            &self.app_state.connection_manager,
            &input.config_id,
            db,
            &input.pattern,
            limit,
        )
        .await
        .map_err(Self::map_err)
    }

    #[tool(
        description = "Execute a SQL query on a connected database. Returns results as JSON. Use list_connections first to get valid config IDs. Default row limit is 100 (max 50000)."
    )]
    async fn query(&self, Parameters(input): Parameters<QueryInput>) -> Result<String, McpError> {
        self.ensure_allowed(&input.config_id)?;
        permission::check_sql_allowed(&input.sql, self.permission_mode)
            .map_err(|e| McpError::invalid_params(e, None))?;
        crate::services::db_tools::query(
            &self.app_state.connection_manager,
            &input.config_id,
            &input.sql,
            input.limit,
            Some(self.permission_mode),
        )
        .await
        .map_err(Self::map_err)
    }

    #[tool(
        description = "Get detailed schema of a table including columns, types, primary keys, foreign keys, and indexes."
    )]
    async fn get_schema(
        &self,
        Parameters(input): Parameters<GetSchemaInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed(&input.config_id)?;
        let tables = vec![input.table.clone()];
        crate::services::db_tools::get_table_schema(
            &self.app_state.connection_manager,
            &input.config_id,
            &tables,
        )
        .await
        .map_err(Self::map_err)
    }

    #[tool(
        description = "Get the execution plan (EXPLAIN) for a SQL query. Useful for performance analysis."
    )]
    async fn explain_query(
        &self,
        Parameters(input): Parameters<ExplainQueryInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed(&input.config_id)?;
        crate::services::db_tools::explain_query(
            &self.app_state.connection_manager,
            &input.config_id,
            &input.sql,
        )
        .await
        .map_err(Self::map_err)
    }

    #[tool(
        description = "Get a human-readable description of a table including columns, types, constraints, and indexes."
    )]
    async fn describe_table(
        &self,
        Parameters(input): Parameters<DescribeTableInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed(&input.config_id)?;
        let schema = crate::services::db_tools::get_single_table_schema(
            &self.app_state.connection_manager,
            &input.config_id,
            &input.table,
        )
        .await
        .map_err(Self::map_err)?;

        Ok(format_table_description(&input.table, &schema))
    }

    #[tool(
        description = "List all available user-defined workflows. Workflows are reusable AI automations combining prompts and database operations."
    )]
    async fn list_workflows(&self) -> Result<String, McpError> {
        let workflows = self.app_state.workflow_registry.list().await;
        serde_json::to_string_pretty(&workflows)
            .map_err(|e| McpError::internal_error(e.to_string(), None))
    }

    #[tool(
        description = "Execute a user-defined workflow by ID. Workflows are reusable automations combining prompts and database operations. Use list_workflows to see available workflows."
    )]
    async fn run_workflow(
        &self,
        Parameters(input): Parameters<RunWorkflowInput>,
    ) -> Result<String, McpError> {
        let workflow = self
            .app_state
            .workflow_registry
            .get(&input.workflow_id)
            .await
            .ok_or_else(|| {
                McpError::invalid_params(
                    format!("Workflow '{}' not found", input.workflow_id),
                    None,
                )
            })?;

        let result = crate::workflow::WorkflowExecutor::execute_with_options(
            &workflow,
            &self.app_state,
            input.config_id.as_deref(),
            &input.variables,
            crate::workflow::WorkflowExecuteOptions {
                permission_mode: Some(self.permission_mode),
                query_row_limit: Some(crate::workflow::WORKFLOW_QUERY_ROW_LIMIT),
            },
        )
        .await
        .map_err(|e| McpError::internal_error(e, None))?;

        Ok(result.final_output)
    }
}

// ─── Prompts ───

#[prompt_router]
impl DataZenMcpServer {
    #[prompt(
        name = "nl2sql",
        description = "Convert natural language to SQL based on the database schema"
    )]
    async fn nl2sql_prompt(
        &self,
        Parameters(args): Parameters<Nl2SqlArgs>,
    ) -> Result<GetPromptResult, McpError> {
        let (conn_id, driver, _handle) = self.resolve_connection(&args.config_id).await?;
        let lang = self.app_state.store.get_settings().await.language;
        let db_type = driver.driver_type();
        let db = args.database.as_deref().unwrap_or("");

        let context = self
            .app_state
            .schema_context_builder
            .build_sql_context(&conn_id, db, None, &[], budget::FALLBACK_DDL)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let mut vars = HashMap::new();
        vars.insert("db_type", db_type.as_str());
        vars.insert("version", "");
        vars.insert("schema", context.schema_ddl.as_str());
        vars.insert("recent", "");
        let system = self
            .app_state
            .prompt_resolver
            .resolve(PromptScenario::Nl2Sql, Some(driver.as_ref()), &lang)
            .await;
        let system_content = prompt_resolver::render_template(&system, &vars);

        Ok(GetPromptResult::new(vec![
            PromptMessage::new_text(Role::User, system_content),
            PromptMessage::new_text(Role::User, args.question.clone()),
        ])
        .with_description("Natural language to SQL conversion with schema context"))
    }

    #[prompt(
        name = "diagnose_error",
        description = "Diagnose a SQL error and suggest fixes"
    )]
    async fn diagnose_error_prompt(
        &self,
        Parameters(args): Parameters<DiagnoseErrorArgs>,
    ) -> Result<GetPromptResult, McpError> {
        let (_conn_id, driver, _handle) = self.resolve_connection(&args.config_id).await?;
        let lang = self.app_state.store.get_settings().await.language;
        let db_type = driver.driver_type();

        let mut vars = HashMap::new();
        vars.insert("db_type", db_type.as_str());
        vars.insert("version", "");
        vars.insert("schema", "");
        vars.insert("recent", "");
        let system = self
            .app_state
            .prompt_resolver
            .resolve(PromptScenario::Diagnose, Some(driver.as_ref()), &lang)
            .await;
        let system_content = prompt_resolver::render_template(&system, &vars);

        Ok(GetPromptResult::new(vec![
            PromptMessage::new_text(Role::User, system_content),
            PromptMessage::new_text(
                Role::User,
                format!("SQL:\n```\n{}\n```\n\nError:\n{}", args.sql, args.error),
            ),
        ])
        .with_description("SQL error diagnosis with fix suggestions"))
    }

    #[prompt(
        name = "explain_plan",
        description = "Analyze a query execution plan and suggest optimizations"
    )]
    async fn explain_plan_prompt(
        &self,
        Parameters(args): Parameters<ExplainPlanArgs>,
    ) -> Result<GetPromptResult, McpError> {
        let (_conn_id, driver, handle) = self.resolve_connection(&args.config_id).await?;
        let lang = self.app_state.store.get_settings().await.language;
        let db_type = driver.driver_type();

        let explain_result = driver
            .explain(&handle, &args.sql)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let explain_text = serde_json::to_string_pretty(&explain_result)
            .unwrap_or_else(|_| "Failed to serialize EXPLAIN output".to_string());

        let mut vars = HashMap::new();
        vars.insert("db_type", db_type.as_str());
        vars.insert("version", "");
        vars.insert("schema", "");
        vars.insert("recent", "");
        let system = self
            .app_state
            .prompt_resolver
            .resolve(
                PromptScenario::ExplainAnalysis,
                Some(driver.as_ref()),
                &lang,
            )
            .await;
        let system_content = prompt_resolver::render_template(&system, &vars);

        Ok(GetPromptResult::new(vec![
            PromptMessage::new_text(Role::User, system_content),
            PromptMessage::new_text(
                Role::User,
                format!(
                    "SQL:\n```\n{}\n```\n\nEXPLAIN output:\n```\n{}\n```",
                    args.sql, explain_text
                ),
            ),
        ])
        .with_description("Query execution plan analysis"))
    }
}

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
            let history = self.app_state.store.get_query_history(50).await;
            let json = serde_json::to_string_pretty(&history)
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
            let config_id = parts
                .first()
                .ok_or_else(|| McpError::invalid_params("Missing connection ID in URI", None))?;
            let db = parts.get(1).copied().unwrap_or("");

            let (runtime_id, _driver, _handle) = self.resolve_connection(config_id).await?;

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
            .map_err(|e| McpError::invalid_params(e, None))?;

        let text = match name {
            "list_connections" => self.list_connections().await?,
            "list_databases" => {
                let config_id = arguments
                    .as_ref()
                    .and_then(|a| a.get("config_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                self.list_databases(rmcp::handler::server::wrapper::Parameters(
                    ListDatabasesInput {
                        config_id: config_id.into(),
                    },
                ))
                .await?
            }
            "list_tables" => {
                let config_id = arguments
                    .as_ref()
                    .and_then(|a| a.get("config_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let database = arguments
                    .as_ref()
                    .and_then(|a| a.get("database"))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                self.list_tables(rmcp::handler::server::wrapper::Parameters(
                    ListTablesInput {
                        config_id: config_id.into(),
                        database,
                    },
                ))
                .await?
            }
            "search_tables" => {
                let config_id = arguments
                    .as_ref()
                    .and_then(|a| a.get("config_id"))
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
                self.search_tables(rmcp::handler::server::wrapper::Parameters(
                    SearchTablesInput {
                        config_id: config_id.into(),
                        database,
                        pattern: pattern.into(),
                        limit,
                    },
                ))
                .await?
            }
            "query" => {
                let config_id = arguments
                    .as_ref()
                    .and_then(|a| a.get("config_id"))
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
                self.query(rmcp::handler::server::wrapper::Parameters(QueryInput {
                    config_id: config_id.into(),
                    sql: sql.into(),
                    limit,
                }))
                .await?
            }
            other => {
                return Err(McpError::invalid_params(
                    format!("Unknown tool: {other}"),
                    None,
                ));
            }
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
        vec![
            Resource::new("datazen://connections", "Database Connections"),
            Resource::new("datazen://query-history", "Query History"),
            Resource::new("datazen://workflows", "Available Workflows"),
        ]
    }
}

// ─── ServerHandler (resources + capabilities) ───

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
        permission::check_tool_call(
            request.name.as_ref(),
            self.permission_mode,
            &self.disabled_tools,
            request.arguments.as_ref(),
        )
        .map_err(|e| McpError::invalid_params(e, None))?;

        if let Some(args) = request.arguments.as_ref() {
            if let Some(config_id) = args.get("config_id").and_then(|v| v.as_str()) {
                self.ensure_allowed(config_id)?;
            }
        }

        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        Self::tool_router().call(tcc).await
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uri_encoding_decoding() {
        let conn_id = urlencoding::encode("my connection");
        let db = urlencoding::encode("test db");
        let uri = format!("datazen://schema/{conn_id}/{db}");

        let rest = uri.strip_prefix("datazen://schema/").unwrap();
        let decoded = urlencoding::decode(rest).unwrap();
        let parts: Vec<&str> = decoded.splitn(2, '/').collect();
        assert_eq!(parts[0], "my connection");
        assert_eq!(parts[1], "test db");
    }

    #[test]
    fn test_simple_connection_id_parsing() {
        let uri = "datazen://schema/abc123/testdb";
        let rest = uri.strip_prefix("datazen://schema/").unwrap();
        let decoded = urlencoding::decode(rest).unwrap();
        let parts: Vec<&str> = decoded.splitn(2, '/').collect();
        assert_eq!(parts[0], "abc123");
        assert_eq!(parts[1], "testdb");
    }

    #[test]
    fn test_schema_uri_no_database() {
        let uri = "datazen://schema/abc123";
        let rest = uri.strip_prefix("datazen://schema/").unwrap();
        let decoded = urlencoding::decode(rest).unwrap();
        let parts: Vec<&str> = decoded.splitn(2, '/').collect();
        assert_eq!(parts[0], "abc123");
        assert_eq!(parts.get(1).copied().unwrap_or(""), "");
    }

    #[test]
    fn query_input_rejects_legacy_connection_id_field() {
        let json = r#"{"connection_id":"c1","sql":"SELECT 1"}"#;
        let parsed = serde_json::from_str::<QueryInput>(json);
        assert!(parsed.is_err(), "legacy connection_id must not deserialize");
    }

    #[test]
    fn query_input_accepts_config_id() {
        let json = r#"{"config_id":"c1","sql":"SELECT 1"}"#;
        let parsed: QueryInput = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.config_id, "c1");
    }

    #[test]
    fn test_list_databases_input_deserialization() {
        let json = r#"{"config_id": "test-id"}"#;
        let input: ListDatabasesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.config_id, "test-id");
    }

    #[test]
    fn test_query_input_defaults() {
        let json = r#"{"config_id": "c1", "sql": "SELECT 1"}"#;
        let input: QueryInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.config_id, "c1");
        assert_eq!(input.sql, "SELECT 1");
        assert_eq!(input.limit, None);
    }

    #[test]
    fn test_query_input_with_limit() {
        let json = r#"{"config_id": "c1", "sql": "SELECT 1", "limit": 50}"#;
        let input: QueryInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.limit, Some(50));
    }

    #[test]
    fn test_resolve_query_limit_none_defaults_to_100() {
        assert_eq!(
            crate::services::db_tools::resolve_query_limit(None),
            Some(100)
        );
    }

    #[test]
    fn test_resolve_query_limit_some_passthrough() {
        assert_eq!(
            crate::services::db_tools::resolve_query_limit(Some(50)),
            Some(50)
        );
    }

    #[test]
    fn test_resolve_query_limit_some_capped_at_max() {
        assert_eq!(
            crate::services::db_tools::resolve_query_limit(Some(999_999)),
            Some(50_000)
        );
    }

    #[test]
    fn mcp_all_tools_list_is_stable() {
        assert_eq!(MCP_ALL_TOOLS.len(), 10);
        assert!(MCP_ALL_TOOLS.contains(&"list_connections"));
        assert!(MCP_ALL_TOOLS.contains(&"search_tables"));
        assert!(MCP_ALL_TOOLS.contains(&"run_workflow"));
    }

    #[test]
    fn list_tables_input_deserializes_optional_database() {
        let json = r#"{"config_id":"c1","database":"mydb"}"#;
        let input: ListTablesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.config_id, "c1");
        assert_eq!(input.database.as_deref(), Some("mydb"));
    }

    #[test]
    fn search_tables_input_deserializes() {
        let json = r#"{"config_id":"c1","pattern":"user","database":"app","limit":10}"#;
        let input: SearchTablesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.config_id, "c1");
        assert_eq!(input.pattern, "user");
        assert_eq!(input.database.as_deref(), Some("app"));
        assert_eq!(input.limit, Some(10));
    }

    #[test]
    fn search_tables_input_defaults_optional_fields() {
        let json = r#"{"config_id":"c1","pattern":"order"}"#;
        let input: SearchTablesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.config_id, "c1");
        assert_eq!(input.pattern, "order");
        assert!(input.database.is_none());
        assert!(input.limit.is_none());
    }

    #[test]
    fn run_workflow_input_defaults_variables_to_null() {
        let json = r#"{"workflow_id":"wf-1"}"#;
        let input: RunWorkflowInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.workflow_id, "wf-1");
        assert!(input.variables.is_null());
        assert!(input.config_id.is_none());
    }

    #[test]
    fn nl2sql_args_deserialize() {
        let json = r#"{"config_id":"c1","question":"count users","database":"app"}"#;
        let args: Nl2SqlArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.config_id, "c1");
        assert_eq!(args.question, "count users");
        assert_eq!(args.database.as_deref(), Some("app"));
    }

    #[test]
    fn format_table_description_includes_columns_and_pk() {
        use datazen_driver_api::{ColumnSchema, IndexInfo, TableSchema};

        let schema = TableSchema {
            table_name: "users".into(),
            columns: vec![ColumnSchema {
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: true,
            }],
            primary_keys: vec!["id".into()],
            indexes: vec![IndexInfo {
                name: "users_pkey".into(),
                columns: vec!["id".into()],
                is_unique: true,
                is_primary: true,
                index_type: "btree".into(),
            }],
            foreign_keys: vec![],
        };

        let desc = format_table_description("users", &schema);
        assert!(desc.contains("Table: users"));
        assert!(desc.contains("id integer PK NOT NULL"));
        assert!(desc.contains("Primary Key: (id)"));
        assert!(desc.contains("users_pkey"));
    }

    #[tokio::test]
    async fn mcp_tool_handlers_with_mock_driver() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        test.save_connection("mcp-cfg").await;
        let server = DataZenMcpServer::new(Arc::new(test.state));

        let conns = server.list_connections().await.unwrap();
        assert!(conns.contains("mcp-cfg"));

        let dbs = server
            .list_databases(rmcp::handler::server::wrapper::Parameters(
                ListDatabasesInput {
                    config_id: "mcp-cfg".into(),
                },
            ))
            .await
            .unwrap();
        assert!(dbs.contains("app"));

        let tables = server
            .list_tables(rmcp::handler::server::wrapper::Parameters(
                ListTablesInput {
                    config_id: "mcp-cfg".into(),
                    database: Some("app".into()),
                },
            ))
            .await
            .unwrap();
        assert!(tables.contains("users"));

        let schema = server
            .get_schema(rmcp::handler::server::wrapper::Parameters(GetSchemaInput {
                config_id: "mcp-cfg".into(),
                table: "users".into(),
            }))
            .await
            .unwrap();
        assert!(schema.contains("users"));

        let desc = server
            .describe_table(rmcp::handler::server::wrapper::Parameters(
                DescribeTableInput {
                    config_id: "mcp-cfg".into(),
                    table: "users".into(),
                },
            ))
            .await
            .unwrap();
        assert!(desc.contains("Table: users"));

        let explain = server
            .explain_query(rmcp::handler::server::wrapper::Parameters(
                ExplainQueryInput {
                    config_id: "mcp-cfg".into(),
                    sql: "SELECT 1".into(),
                },
            ))
            .await
            .unwrap();
        assert!(!explain.is_empty());

        let query_out = server
            .query(rmcp::handler::server::wrapper::Parameters(QueryInput {
                config_id: "mcp-cfg".into(),
                sql: "SELECT 1".into(),
                limit: Some(10),
            }))
            .await
            .unwrap();
        assert!(query_out.contains("alice") || query_out.contains("1"));

        let workflows = server.list_workflows().await.unwrap();
        assert!(
            workflows.contains("builtin-hello-query") || workflows.contains("[]"),
            "expected builtin workflows or an empty list, got: {workflows}"
        );

        let search_result = server
            .search_tables(rmcp::handler::server::wrapper::Parameters(
                SearchTablesInput {
                    config_id: "mcp-cfg".into(),
                    database: Some("app".into()),
                    pattern: "user".into(),
                    limit: Some(10),
                },
            ))
            .await
            .unwrap();
        assert!(search_result.contains("users"));
        assert!(search_result.contains("totalMatches"));

        let search_no_match = server
            .search_tables(rmcp::handler::server::wrapper::Parameters(
                SearchTablesInput {
                    config_id: "mcp-cfg".into(),
                    database: Some("app".into()),
                    pattern: "zzz_nonexistent".into(),
                    limit: None,
                },
            ))
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&search_no_match).unwrap();
        assert_eq!(parsed["totalMatches"].as_u64(), Some(0));
    }

    #[tokio::test]
    async fn mcp_search_tables_via_call_tool_inner() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        test.save_connection("cti-search").await;
        let server = DataZenMcpServer::new(Arc::new(test.state));

        let result = server
            .call_tool_inner(
                "search_tables",
                Some(
                    serde_json::json!({"config_id":"cti-search","database":"app","pattern":"user"}),
                ),
            )
            .await
            .unwrap();
        let text = result.content[0].as_text().unwrap().text.as_str();
        assert!(text.contains("users"));
        assert!(text.contains("totalMatches"));
    }

    #[tokio::test]
    async fn mcp_prompt_handlers_build_messages() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("mcp-prompt").await;
        let server = DataZenMcpServer::new(Arc::new(test.state));

        let nl2sql = server
            .nl2sql_prompt(rmcp::handler::server::wrapper::Parameters(Nl2SqlArgs {
                config_id: conn_id.clone(),
                question: "count users".into(),
                database: Some("app".into()),
            }))
            .await
            .unwrap();
        assert!(nl2sql.messages[0]
            .content
            .as_text()
            .unwrap()
            .text
            .contains("Schema:"));
        assert!(nl2sql.messages[1]
            .content
            .as_text()
            .unwrap()
            .text
            .contains("count users"));

        let diag = server
            .diagnose_error_prompt(rmcp::handler::server::wrapper::Parameters(
                DiagnoseErrorArgs {
                    config_id: conn_id.clone(),
                    sql: "SELECT bad".into(),
                    error: "column missing".into(),
                },
            ))
            .await
            .unwrap();
        assert!(diag.messages[1]
            .content
            .as_text()
            .unwrap()
            .text
            .contains("column missing"));

        let plan = server
            .explain_plan_prompt(rmcp::handler::server::wrapper::Parameters(
                ExplainPlanArgs {
                    config_id: conn_id,
                    sql: "SELECT 1".into(),
                },
            ))
            .await
            .unwrap();
        assert!(plan.messages[1]
            .content
            .as_text()
            .unwrap()
            .text
            .contains("EXPLAIN"));
    }

    #[tokio::test]
    async fn mcp_tool_router_lists_registered_tools() {
        let tools = DataZenMcpServer::tool_router().list_all();
        assert_eq!(tools.len(), MCP_ALL_TOOLS.len());
        assert!(tools.iter().any(|t| t.name == "query"));
    }

    #[tokio::test]
    async fn mcp_read_resource_inner_paths() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        test.save_connection("res-inner").await;
        let (_, conn_id) = test.save_and_connect("res-inner").await;
        let server = DataZenMcpServer::new(Arc::new(test.state));

        let conns = server
            .read_resource_inner("datazen://connections")
            .await
            .unwrap();
        match &conns.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => {
                assert!(text.contains("res-inner"));
            }
            _ => panic!("expected text resource"),
        }

        let hist = server
            .read_resource_inner("datazen://query-history")
            .await
            .unwrap();
        assert!(!hist.contents.is_empty());

        let schema_uri = format!("datazen://schema/{conn_id}/app");
        let schema = server.read_resource_inner(&schema_uri).await.unwrap();
        match &schema.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => {
                assert!(!text.is_empty());
            }
            _ => panic!("expected text resource"),
        }

        let err = server
            .read_resource_inner("datazen://missing")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Unknown resource"));
    }

    #[tokio::test]
    async fn mcp_query_rejects_disallowed_sql_in_readonly() {
        use crate::mcp::permission::McpPermissionMode;
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        test.save_connection("ro-cfg").await;
        let server = DataZenMcpServer::new(Arc::new(test.state))
            .with_permission_mode(McpPermissionMode::ReadOnly);

        let err = server
            .query(rmcp::handler::server::wrapper::Parameters(QueryInput {
                config_id: "ro-cfg".into(),
                sql: "DELETE FROM users".into(),
                limit: None,
            }))
            .await
            .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("delete")
                || err.to_string().contains("not allowed")
                || err.to_string().contains("permission")
        );
    }

    #[tokio::test]
    async fn mcp_get_info_exposes_capabilities() {
        use crate::testing::app_state::TestAppState;
        use rmcp::ServerHandler;
        use std::sync::Arc;

        let test = TestAppState::new().await;
        let server = DataZenMcpServer::new(Arc::new(test.state));
        let info = server.get_info();
        assert_eq!(info.server_info.name, "datazen");
        assert!(info.capabilities.tools.is_some());
    }

    #[tokio::test]
    async fn mcp_disabled_tools_and_permission_mode() {
        use crate::mcp::permission::McpPermissionMode;
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::new().await;
        let server = DataZenMcpServer::new(Arc::new(test.state))
            .with_disabled_tools(&["query".into()])
            .with_permission_mode(McpPermissionMode::ReadOnly);

        let mut router = DataZenMcpServer::tool_router();
        for name in MCP_ALL_TOOLS {
            if !crate::mcp::permission::is_tool_listed(
                name,
                McpPermissionMode::ReadOnly,
                &server.disabled_tools,
            ) {
                router.disable_route(name.to_string());
            }
        }
        let listed = router.list_all();
        assert!(!listed.iter().any(|t| t.name == "query"));
        assert!(listed.iter().any(|t| t.name == "list_connections"));
    }

    #[tokio::test]
    async fn mcp_list_tools_and_resources_inner() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::new().await;
        let server = DataZenMcpServer::new(Arc::new(test.state));
        assert_eq!(server.list_tools_inner().len(), MCP_ALL_TOOLS.len());
        assert_eq!(server.list_resources_inner().len(), 3);
    }

    #[tokio::test]
    async fn mcp_call_tool_inner_database_tools() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        test.save_connection("cti-cfg").await;
        let server = DataZenMcpServer::new(Arc::new(test.state));

        let dbs = server
            .call_tool_inner(
                "list_databases",
                Some(serde_json::json!({"config_id":"cti-cfg"})),
            )
            .await
            .unwrap();
        assert!(dbs.content[0].as_text().unwrap().text.contains("app"));

        let tables = server
            .call_tool_inner(
                "list_tables",
                Some(serde_json::json!({"config_id":"cti-cfg","database":"app"})),
            )
            .await
            .unwrap();
        assert!(tables.content[0].as_text().unwrap().text.contains("users"));

        let query = server
            .call_tool_inner(
                "query",
                Some(serde_json::json!({"config_id":"cti-cfg","sql":"SELECT 1","limit":5})),
            )
            .await
            .unwrap();
        assert!(!query.content[0].as_text().unwrap().text.is_empty());

        assert!(server.call_tool_inner("missing_tool", None).await.is_err());
    }

    #[tokio::test]
    async fn mcp_run_workflow_not_found() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::new().await;
        let server = DataZenMcpServer::new(Arc::new(test.state));
        let err = server
            .run_workflow(rmcp::handler::server::wrapper::Parameters(
                RunWorkflowInput {
                    workflow_id: "missing".into(),
                    variables: serde_json::json!({}),
                    config_id: None,
                },
            ))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}

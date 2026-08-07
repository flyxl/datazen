//! DataZen MCP Server handler — tools + resources + prompts.

use crate::commands::AppState;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::service::RequestContext;
use rmcp::{prompt, prompt_handler, prompt_router, tool, tool_handler, tool_router, ErrorData as McpError, RoleServer, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::Arc;

// ─── Tool Input Types ───

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryInput {
    /// The connection ID (from list_connections)
    pub connection_id: String,
    /// SQL query to execute
    pub sql: String,
    /// Maximum rows to return (default: 100, max: 50000)
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListTablesInput {
    /// The connection ID
    pub connection_id: String,
    /// Optional database name
    pub database: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSchemaInput {
    /// The connection ID
    pub connection_id: String,
    /// Table name
    pub table: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExplainQueryInput {
    /// The connection ID
    pub connection_id: String,
    /// SQL query to analyze
    pub sql: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DescribeTableInput {
    /// The connection ID
    pub connection_id: String,
    /// Table name
    pub table: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListDatabasesInput {
    /// The connection ID
    pub connection_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunWorkflowInput {
    /// Workflow ID to execute
    pub workflow_id: String,
    /// Input variables for the workflow (JSON object)
    #[serde(default)]
    pub variables: serde_json::Value,
    /// Optional connection ID (some workflows require a database connection)
    pub connection_id: Option<String>,
}

// ─── Prompt Argument Types ───

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Nl2SqlArgs {
    /// Database connection ID for schema context
    pub connection_id: String,
    /// Natural language description of the query
    pub question: String,
    /// Optional database name
    pub database: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DiagnoseErrorArgs {
    /// Database connection ID
    pub connection_id: String,
    /// The SQL that caused the error
    pub sql: String,
    /// The error message
    pub error: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExplainPlanArgs {
    /// Database connection ID
    pub connection_id: String,
    /// The SQL query to explain
    pub sql: String,
}

// ─── MCP Server ───

#[derive(Clone)]
pub struct DataZenMcpServer {
    app_state: Arc<AppState>,
    disabled_tools: HashSet<String>,
}

pub const MCP_ALL_TOOLS: &[&str] = &[
    "list_connections",
    "list_databases",
    "list_tables",
    "query",
    "get_schema",
    "explain_query",
    "describe_table",
    "list_workflows",
    "run_workflow",
];

impl DataZenMcpServer {
    pub fn new(app_state: Arc<AppState>) -> Self {
        Self { app_state, disabled_tools: HashSet::new() }
    }

    pub fn with_disabled_tools(mut self, disabled: &[String]) -> Self {
        self.disabled_tools = disabled.iter().cloned().collect();
        self
    }

    fn map_err(e: String) -> McpError {
        McpError::internal_error(e, None)
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
        let (driver, handle) = crate::services::db_tools::resolve_connection(
            &self.app_state.connection_manager,
            id,
        )
        .await
        .map_err(Self::map_err)?;
        Ok((id.to_string(), driver, handle))
    }
}

// ─── Tools ───

#[tool_router]
impl DataZenMcpServer {
    #[tool(description = "List all configured database connections. Returns connection IDs, names, database types, and hosts.")]
    async fn list_connections(&self) -> Result<String, McpError> {
        crate::services::db_tools::list_connections(&self.app_state.store)
            .await
            .map_err(Self::map_err)
    }

    #[tool(description = "List all databases on a connected server.")]
    async fn list_databases(
        &self,
        Parameters(input): Parameters<ListDatabasesInput>,
    ) -> Result<String, McpError> {
        crate::services::db_tools::list_databases(&self.app_state.connection_manager, &input.connection_id)
            .await
            .map_err(Self::map_err)
    }

    #[tool(description = "List all tables in a database. Returns table names with types and row counts.")]
    async fn list_tables(
        &self,
        Parameters(input): Parameters<ListTablesInput>,
    ) -> Result<String, McpError> {
        let db = input.database.as_deref().unwrap_or("");
        crate::services::db_tools::list_tables(&self.app_state.connection_manager, &input.connection_id, db)
            .await
            .map_err(Self::map_err)
    }

    #[tool(description = "Execute a SQL query on a connected database. Returns results as JSON. Use list_connections first to get valid connection IDs. Default row limit is 100 (max 50000).")]
    async fn query(
        &self,
        Parameters(input): Parameters<QueryInput>,
    ) -> Result<String, McpError> {
        crate::services::db_tools::query(&self.app_state.connection_manager, &input.connection_id, &input.sql, input.limit)
            .await
            .map_err(Self::map_err)
    }

    #[tool(description = "Get detailed schema of a table including columns, types, primary keys, foreign keys, and indexes.")]
    async fn get_schema(
        &self,
        Parameters(input): Parameters<GetSchemaInput>,
    ) -> Result<String, McpError> {
        let tables = vec![input.table.clone()];
        crate::services::db_tools::get_table_schema(&self.app_state.connection_manager, &input.connection_id, &tables)
            .await
            .map_err(Self::map_err)
    }

    #[tool(description = "Get the execution plan (EXPLAIN) for a SQL query. Useful for performance analysis.")]
    async fn explain_query(
        &self,
        Parameters(input): Parameters<ExplainQueryInput>,
    ) -> Result<String, McpError> {
        crate::services::db_tools::explain_query(&self.app_state.connection_manager, &input.connection_id, &input.sql)
            .await
            .map_err(Self::map_err)
    }

    #[tool(description = "Get a human-readable description of a table including columns, types, constraints, and indexes.")]
    async fn describe_table(
        &self,
        Parameters(input): Parameters<DescribeTableInput>,
    ) -> Result<String, McpError> {
        let schema = crate::services::db_tools::get_single_table_schema(
            &self.app_state.connection_manager,
            &input.connection_id,
            &input.table,
        )
        .await
        .map_err(Self::map_err)?;

        let mut desc = format!("Table: {}\n\nColumns:\n", input.table);
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

        Ok(desc)
    }

    #[tool(description = "List all available user-defined workflows. Workflows are reusable AI automations combining prompts and database operations.")]
    async fn list_workflows(&self) -> Result<String, McpError> {
        let workflows = self.app_state.workflow_registry.list().await;
        serde_json::to_string_pretty(&workflows)
            .map_err(|e| McpError::internal_error(e.to_string(), None))
    }

    #[tool(description = "Execute a user-defined workflow by ID. Workflows are reusable automations combining prompts and database operations. Use list_workflows to see available workflows.")]
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
                McpError::invalid_params(format!("Workflow '{}' not found", input.workflow_id), None)
            })?;

        let result = crate::workflow::WorkflowExecutor::execute(
            &workflow,
            &self.app_state,
            input.connection_id.as_deref(),
            &input.variables,
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
        let (conn_id, driver, _handle) = self.resolve_connection(&args.connection_id).await?;

        let db_type = format!("{:?}", driver.driver_type());
        let db = args.database.as_deref().unwrap_or("");

        let context = self
            .app_state
            .schema_context_builder
            .build_sql_context(&conn_id, db, None, &[], 4000)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(GetPromptResult::new(vec![
            PromptMessage::new_text(
                Role::User,
                format!(
                    "Database: {db_type}\nSchema:\n{}\n\nGenerate SQL for: {}",
                    context.schema_ddl, args.question
                ),
            ),
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
        let (_conn_id, driver, _handle) = self.resolve_connection(&args.connection_id).await?;

        let db_type = format!("{:?}", driver.driver_type());

        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            format!(
                "Database: {db_type}\nSQL:\n```\n{}\n```\n\nError: {}\n\nDiagnose this error and suggest a fix.",
                args.sql, args.error
            ),
        )])
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
        let (_conn_id, driver, handle) = self.resolve_connection(&args.connection_id).await?;

        let db_type = format!("{:?}", driver.driver_type());

        let explain_result = driver
            .explain(&handle, &args.sql)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let explain_text = serde_json::to_string_pretty(&explain_result)
            .unwrap_or_else(|_| "Failed to serialize EXPLAIN output".to_string());

        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            format!(
                "Database: {db_type}\nSQL:\n```\n{}\n```\n\nEXPLAIN output:\n```\n{}\n```\n\nAnalyze this execution plan and suggest optimizations.",
                args.sql, explain_text
            ),
        )])
        .with_description("Query execution plan analysis"))
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
        let mut router = Self::tool_router();
        for name in &self.disabled_tools {
            router.disable_route(name.clone());
        }
        Ok(ListToolsResult {
            tools: router.list_all(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        if self.disabled_tools.contains(request.name.as_ref()) {
            return Err(McpError::invalid_params(
                format!("Tool '{}' is disabled in DataZen settings", request.name),
                None,
            ));
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
            resources: vec![
                Resource::new("datazen://connections", "Database Connections"),
                Resource::new("datazen://query-history", "Query History"),
                Resource::new("datazen://workflows", "Available Workflows"),
            ],
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
        let uri = request.uri.as_str();

        if uri == "datazen://connections" {
            let connections = self.app_state.store.get_connections().await;
            let result: Vec<serde_json::Value> = connections
                .iter()
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
                json,
                &request.uri,
            )]));
        }

        if uri == "datazen://workflows" {
            let workflows = self.app_state.workflow_registry.list().await;
            let json = serde_json::to_string_pretty(&workflows)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(
                json,
                &request.uri,
            )]));
        }

        if uri == "datazen://query-history" {
            let history = self.app_state.store.get_query_history(50).await;
            let json = serde_json::to_string_pretty(&history)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(
                json,
                &request.uri,
            )]));
        }

        if let Some(rest) = uri.strip_prefix("datazen://schema/") {
            let decoded = urlencoding::decode(rest)
                .map_err(|e| McpError::invalid_params(format!("Invalid URI encoding: {e}"), None))?;
            let parts: Vec<&str> = decoded.splitn(2, '/').collect();
            let config_id = parts
                .first()
                .ok_or_else(|| McpError::invalid_params("Missing connection ID in URI", None))?;
            let db = parts.get(1).copied().unwrap_or("");

            let (runtime_id, _driver, _handle) = self.resolve_connection(config_id).await?;

            let context = self
                .app_state
                .schema_context_builder
                .build_sql_context(&runtime_id, db, None, &[], 8000)
                .await
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;

            return Ok(ReadResourceResult::new(vec![ResourceContents::text(
                context.schema_ddl,
                &request.uri,
            )]));
        }

        Err(McpError::resource_not_found(
            format!("Unknown resource: {uri}"),
            None,
        ))
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
    fn test_list_databases_input_deserialization() {
        let json = r#"{"connection_id": "test-id"}"#;
        let input: ListDatabasesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.connection_id, "test-id");
    }

    #[test]
    fn test_query_input_defaults() {
        let json = r#"{"connection_id": "c1", "sql": "SELECT 1"}"#;
        let input: QueryInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.connection_id, "c1");
        assert_eq!(input.sql, "SELECT 1");
        assert_eq!(input.limit, None);
    }

    #[test]
    fn test_query_input_with_limit() {
        let json = r#"{"connection_id": "c1", "sql": "SELECT 1", "limit": 50}"#;
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
}

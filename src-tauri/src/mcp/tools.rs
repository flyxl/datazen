//! MCP tool handlers and helpers.

use super::server::DataZenMcpServer;
use super::types::*;
use crate::mcp::allowlist;
use crate::mcp::permission;
use crate::mcp::tool_help;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router, ErrorData as McpError};

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

#[tool_router(vis = "pub(crate)")]
impl DataZenMcpServer {
    #[tool(
        description = "List all configured database connections. Returns connection IDs, names, database types, and hosts."
    )]
    pub(crate) async fn list_connections(&self) -> Result<String, McpError> {
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
    pub(crate) async fn list_databases(
        &self,
        Parameters(input): Parameters<ListDatabasesInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("list_databases", &input.connection_id)?;
        crate::services::db_tools::list_databases(
            &self.app_state.connection_manager,
            &input.connection_id,
        )
        .await
        .map_err(Self::map_err)
    }

    #[tool(
        description = "List all tables in a database. Returns table names with types and row counts."
    )]
    pub(crate) async fn list_tables(
        &self,
        Parameters(input): Parameters<ListTablesInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("list_tables", &input.connection_id)?;
        let db = input.database.as_deref().unwrap_or("");
        crate::services::db_tools::list_tables(
            &self.app_state.connection_manager,
            &input.connection_id,
            db,
        )
        .await
        .map_err(Self::map_err)
    }

    #[tool(
        description = "Search for tables by name pattern (case-insensitive substring match). Use this instead of list_tables when the database has many tables."
    )]
    pub(crate) async fn search_tables(
        &self,
        Parameters(input): Parameters<SearchTablesInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("search_tables", &input.connection_id)?;
        let db = input.database.as_deref().unwrap_or("");
        let limit = input.limit.unwrap_or(20) as usize;
        crate::services::db_tools::search_tables(
            &self.app_state.connection_manager,
            &input.connection_id,
            db,
            &input.pattern,
            limit,
        )
        .await
        .map_err(Self::map_err)
    }

    #[tool(
        description = "Execute a SQL query on a connected database. Returns results as JSON. Use list_connections first to get valid connection IDs. Default row limit is 100 (max 50000)."
    )]
    pub(crate) async fn query(
        &self,
        Parameters(input): Parameters<QueryInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("query", &input.connection_id)?;
        permission::check_sql_allowed(&input.sql, self.permission_mode)
            .map_err(|e| tool_help::tool_error("query", &e))?;
        crate::services::db_tools::query(
            &self.app_state.connection_manager,
            &input.connection_id,
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
    pub(crate) async fn get_schema(
        &self,
        Parameters(input): Parameters<GetSchemaInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("get_schema", &input.connection_id)?;
        let tables = vec![input.table.clone()];
        crate::services::db_tools::get_table_schema(
            &self.app_state.connection_manager,
            &input.connection_id,
            &tables,
        )
        .await
        .map_err(Self::map_err)
    }

    #[tool(
        description = "Get the execution plan (EXPLAIN) for a SQL query. Useful for performance analysis."
    )]
    pub(crate) async fn explain_query(
        &self,
        Parameters(input): Parameters<ExplainQueryInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("explain_query", &input.connection_id)?;
        crate::services::db_tools::explain_query(
            &self.app_state.connection_manager,
            &input.connection_id,
            &input.sql,
        )
        .await
        .map_err(Self::map_err)
    }

    #[tool(
        description = "Get a human-readable description of a table including columns, types, constraints, and indexes."
    )]
    pub(crate) async fn describe_table(
        &self,
        Parameters(input): Parameters<DescribeTableInput>,
    ) -> Result<String, McpError> {
        self.ensure_allowed("describe_table", &input.connection_id)?;
        let schema = crate::services::db_tools::get_single_table_schema(
            &self.app_state.connection_manager,
            &input.connection_id,
            &input.table,
        )
        .await
        .map_err(Self::map_err)?;

        Ok(format_table_description(&input.table, &schema))
    }

    #[tool(
        description = "List all available user-defined workflows. Workflows are reusable AI automations combining prompts and database operations."
    )]
    pub(crate) async fn list_workflows(&self) -> Result<String, McpError> {
        let workflows = self.app_state.workflow_registry.list().await;
        serde_json::to_string_pretty(&workflows)
            .map_err(|e| McpError::internal_error(e.to_string(), None))
    }

    #[tool(
        description = "Execute a user-defined workflow by ID. Workflows are reusable automations combining prompts and database operations. Use list_workflows to see available workflows."
    )]
    pub(crate) async fn run_workflow(
        &self,
        Parameters(input): Parameters<RunWorkflowInput>,
    ) -> Result<String, McpError> {
        let workflow = self
            .app_state
            .workflow_registry
            .get(&input.workflow_id)
            .await
            .ok_or_else(|| {
                tool_help::tool_error(
                    "run_workflow",
                    &format!("Workflow '{}' not found", input.workflow_id),
                )
            })?;

        let result = crate::workflow::WorkflowExecutor::execute_with_options(
            &workflow,
            &self.app_state,
            input.connection_id.as_deref(),
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

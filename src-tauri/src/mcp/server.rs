//! DataZen MCP Server — orchestration entry (struct, core helpers, tests).

use crate::commands::AppState;
use crate::mcp::allowlist;
use crate::mcp::permission::McpPermissionMode;
use crate::mcp::tool_help;
use rmcp::ErrorData as McpError;
use std::collections::HashSet;
use std::sync::Arc;

pub use super::types::*;

#[derive(Clone)]
pub struct DataZenMcpServer {
    pub(crate) app_state: Arc<AppState>,
    pub(crate) disabled_tools: HashSet<String>,
    pub(crate) permission_mode: McpPermissionMode,
    /// Empty = all saved connections are exposed to MCP.
    pub(crate) allowed_connection_ids: Vec<String>,
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

    pub(crate) fn map_err(e: String) -> McpError {
        McpError::internal_error(e, None)
    }

    pub(crate) fn ensure_allowed(
        &self,
        tool_name: &str,
        connection_id: &str,
    ) -> Result<(), McpError> {
        allowlist::ensure_connection_allowed(connection_id, &self.allowed_connection_ids)
            .map_err(|e| tool_help::tool_error(tool_name, &e))
    }

    pub(crate) fn tool_is_registered(&self, name: &str) -> bool {
        Self::tool_router().get(name).is_some()
    }

    pub(crate) async fn resolve_connection(
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
        self.ensure_allowed("list_connections", id)?;
        crate::services::db_tools::resolve_connection_with_id(
            &self.app_state.connection_manager,
            id,
        )
        .await
        .map_err(Self::map_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::tools::format_table_description;
    use rmcp::model::ResourceContents;

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
    fn query_input_accepts_connection_id() {
        let json = r#"{"connection_id":"c1","sql":"SELECT 1"}"#;
        let parsed: QueryInput = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.connection_id, "c1");
    }

    #[test]
    fn query_input_rejects_removed_config_id_field() {
        let json = r#"{"config_id":"c1","sql":"SELECT 1"}"#;
        let parsed = serde_json::from_str::<QueryInput>(json);
        assert!(
            parsed.is_err(),
            "removed config_id field must not deserialize"
        );
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

    #[test]
    fn mcp_all_tools_list_is_stable() {
        assert_eq!(MCP_ALL_TOOLS.len(), 10);
        assert!(MCP_ALL_TOOLS.contains(&"list_connections"));
        assert!(MCP_ALL_TOOLS.contains(&"search_tables"));
        assert!(MCP_ALL_TOOLS.contains(&"run_workflow"));
    }

    #[test]
    fn list_tables_input_deserializes_optional_database() {
        let json = r#"{"connection_id":"c1","database":"mydb"}"#;
        let input: ListTablesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.connection_id, "c1");
        assert_eq!(input.database.as_deref(), Some("mydb"));
    }

    #[test]
    fn search_tables_input_deserializes() {
        let json = r#"{"connection_id":"c1","pattern":"user","database":"app","limit":10}"#;
        let input: SearchTablesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.connection_id, "c1");
        assert_eq!(input.pattern, "user");
        assert_eq!(input.database.as_deref(), Some("app"));
        assert_eq!(input.limit, Some(10));
    }

    #[test]
    fn search_tables_input_defaults_optional_fields() {
        let json = r#"{"connection_id":"c1","pattern":"order"}"#;
        let input: SearchTablesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.connection_id, "c1");
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
        assert!(input.connection_id.is_none());
    }

    #[test]
    fn nl2sql_args_deserialize() {
        let json = r#"{"connection_id":"c1","question":"count users","database":"app"}"#;
        let args: Nl2SqlArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.connection_id, "c1");
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

    // Remaining integration tests live in server.rs; include from extracted block below.
    include!("server/tests_integration.rs");
}

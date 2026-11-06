//! DataZen MCP Server — orchestration entry (struct, core helpers, tests).

use crate::commands::AppState;
use crate::mcp::session::McpSessionRegistry;
use crate::mcp::tool_help;
use crate::platform::identity::{new_request_id, DesktopIdentity};
use datazen::application::identity_policy::IdentityPolicy;
use datazen::mcp::allowlist;
use datazen::mcp::approval::{approval_preview, McpApprovalDecision, McpApprovalPort};
use datazen::mcp::permission::McpPermissionMode;
use datazen::mcp::policy::{McpCallPlan, McpToolPolicy};
use datazen::mcp::session_tools::{StatefulCall, StatefulSessionHost};
use datazen::shared::context::{OwnerRef, RequestContext};
use datazen::shared::id::ClientInstanceId;
use datazen::workflow::resource_execution::ResourceExecutionHost;
use rmcp::ErrorData as McpError;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

pub use super::types::*;

#[derive(Clone)]
pub struct DataZenMcpServer {
    pub(crate) app_state: Arc<AppState>,
    /// M4: the shared read-only command use cases (`datazen::workflow::runtime_reads`)
    /// run on this port. The desktop implementation is `RuntimeCommandSource`,
    /// assembled once per server instance.
    pub(crate) resource_host: Arc<dyn ResourceExecutionHost>,
    pub(crate) disabled_tools: HashSet<String>,
    pub(crate) permission_mode: McpPermissionMode,
    /// Non-empty allowlist required — empty means deny-all (secure default).
    pub(crate) allowed_connection_ids: Vec<String>,
    /// Additional database scope, intersected with the connection allowlist.
    /// Empty means no additional scope (backward-compatible secure default).
    pub(crate) allowed_databases: Vec<String>,
    /// Additional namespace/schema scope, intersected with the allowlist.
    /// Empty means no additional scope.
    pub(crate) allowed_namespaces: Vec<String>,
    /// Approval presenter; production is the Tauri event broker, tests inject
    /// a deterministic port. Missing presenter fails closed.
    pub(crate) approvals: Arc<dyn McpApprovalPort>,
    /// Stateful tool mappings (memory only). Shared across clones: the tool router
    /// keeps its own clone, and a mapping opened through it must stay visible to
    /// the server that owns the transport.
    pub(crate) sessions: Arc<McpSessionRegistry>,
    /// Backend-generated client instance id (consumer-adapters §4.1). One per MCP
    /// server instance; never taken from the client.
    pub(crate) client_instance_id: String,
}

/// Owner purpose recorded for stateful MCP sessions. Display only — never an
/// authorization input (summary §5.1(3)).
pub const MCP_SESSION_PURPOSE: &str = "mcp-stateful-session";

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
    // §4.2 stateful protocol: explicit open / use / close.
    "open_session",
    "use_session",
    "close_session",
];

impl DataZenMcpServer {
    pub fn new(app_state: Arc<AppState>) -> Self {
        let approvals = default_approval_port(&app_state);
        Self {
            resource_host: crate::services::runtime_command_source::resource_host(&app_state),
            app_state,
            disabled_tools: HashSet::new(),
            permission_mode: McpPermissionMode::default(),
            allowed_connection_ids: Vec::new(),
            allowed_databases: Vec::new(),
            allowed_namespaces: Vec::new(),
            approvals,
            sessions: Arc::new(McpSessionRegistry::new()),
            client_instance_id: crate::platform::identity::new_client_instance_id().to_string(),
        }
    }

    /// Backend-generated client instance id of this MCP server instance.
    pub(crate) fn client_instance_id(&self) -> &str {
        &self.client_instance_id
    }

    /// Call context for this MCP server instance (consumer-adapters §4.1).
    ///
    /// Organization and principal stay the desktop launch subject; only
    /// `clientInstanceId` is this server's own backend-generated id. Another MCP
    /// client is therefore a different owner at the gateway rather than another
    /// call from the same desktop instance, which is what makes a stateful session
    /// unusable by a client that never opened it. `platform::identity` keeps
    /// assembling the launch identity only and must never take an identity as a
    /// parameter, so this assembly lives in the adapter that owns the id.
    pub(crate) fn request_context(&self, identity: &DesktopIdentity) -> RequestContext {
        RequestContext::new(
            identity.organization_id().clone(),
            identity.principal_id().clone(),
            None,
            ClientInstanceId::new(self.client_instance_id.to_owned()),
            new_request_id(),
            None,
        )
    }

    /// Drop every mapping this client opened and close the database sessions
    /// behind them. Called when the transport goes away: a mapping whose client
    /// is gone is not a session to keep alive until its TTL runs out.
    pub(crate) async fn release_sessions(&self) {
        self.sessions
            .disconnected
            .store(true, std::sync::atomic::Ordering::Release);
        self.sessions.close_client(self.client_instance_id());
        self.start_session_cleanup();
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

    pub fn with_allowed_scopes(mut self, databases: &[String], namespaces: &[String]) -> Self {
        self.allowed_databases = databases.to_vec();
        self.allowed_namespaces = namespaces.to_vec();
        self
    }

    pub fn with_approval_port(mut self, approvals: Arc<dyn McpApprovalPort>) -> Self {
        self.approvals = approvals;
        self
    }

    pub(crate) fn tool_policy(&self) -> McpToolPolicy {
        McpToolPolicy {
            permission_mode: self.permission_mode,
            disabled_tools: self.disabled_tools.clone(),
            allowed_connection_ids: self.allowed_connection_ids.clone(),
            allowed_databases: self.allowed_databases.clone(),
            allowed_namespaces: self.allowed_namespaces.clone(),
        }
    }

    /// Pre-dispatch gate. A plan can only come back after permission, scope,
    /// and (for writes) an explicit Allow once. Denied calls are audited here
    /// and never reach a tool body.
    pub(crate) async fn prepare_tool_call(
        &self,
        tool_name: &str,
        arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Result<PreparedToolCall, McpError> {
        let scoped_arguments = self.with_scope_defaults(arguments).await;
        let target_arguments = scoped_arguments.as_ref().or(arguments);
        let plan = match self.tool_policy().evaluate(tool_name, target_arguments) {
            Ok(plan) => plan,
            Err(error) => {
                let (connection_id, database, namespace) = audit_target(target_arguments);
                self.audit_tool_call(
                    tool_name,
                    connection_id,
                    database,
                    namespace,
                    "deny",
                    None,
                    None,
                )
                .await;
                return Err(tool_help::tool_error(tool_name, &error));
            }
        };

        let mut approval_request_id = None;
        if plan.requires_approval {
            let request_id = new_request_id().to_string();
            let timeout = Duration::from_secs(5 * 60);
            let now = chrono::Utc::now();
            let request = approval_preview(
                &request_id,
                tool_name,
                plan.connection_id.as_deref(),
                plan.database.as_deref(),
                plan.namespace.as_deref(),
                plan.sql.as_deref(),
                arguments,
                &now.to_rfc3339(),
                &(now + chrono::Duration::from_std(timeout).unwrap_or_default()).to_rfc3339(),
                timeout.as_millis() as u64,
            );
            let decision = self.approvals.decide(&request).await;
            if !decision.is_allowed() {
                self.audit_tool_call(
                    tool_name,
                    plan.connection_id.clone(),
                    plan.database.clone(),
                    plan.namespace.clone(),
                    decision.audit_decision(),
                    None,
                    Some(request_id),
                )
                .await;
                let reason = match decision {
                    McpApprovalDecision::Deny => "denied by the user",
                    McpApprovalDecision::Timeout => "approval timed out (5 minutes)",
                    McpApprovalDecision::Cancel => "cancelled by the user",
                    McpApprovalDecision::AllowOnce => unreachable!(),
                };
                return Err(tool_help::tool_error(
                    tool_name,
                    &format!("MCP call was not executed: {reason}"),
                ));
            }
            approval_request_id = Some(request_id);
        }

        Ok(PreparedToolCall {
            plan,
            approval_request_id,
        })
    }

    /// Fill the connection's own default database/schema before scope
    /// evaluation. MCP `query` and metadata tools do not take a database
    /// argument, so without this a restricted database scope would reject the
    /// connection's normal default target even when that target is allowed.
    async fn with_scope_defaults(
        &self,
        arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Option<serde_json::Map<String, serde_json::Value>> {
        if self.allowed_databases.is_empty() && self.allowed_namespaces.is_empty() {
            return arguments.cloned();
        }
        let connection_id = arguments
            .and_then(|args| args.get("connection_id"))
            .and_then(|value| value.as_str())?;
        let connection = self.app_state.store.get_connection(connection_id).await?;
        let mut scoped = arguments.cloned().unwrap_or_default();
        if !self.allowed_databases.is_empty() && !scoped.contains_key("database") {
            if let Some(database) = connection.database {
                scoped.insert("database".into(), serde_json::Value::String(database));
            }
        }
        if !self.allowed_namespaces.is_empty() && !scoped.contains_key("schema") {
            if let Some(schema) = connection.schema {
                scoped.insert("schema".into(), serde_json::Value::String(schema));
            }
        }
        Some(scoped)
    }

    /// Persist the final allow/deny outcome. Audit failures are logged and
    /// emitted as a visible event, never propagated into the tool result.
    pub(crate) async fn audit_tool_call(
        &self,
        tool_name: &str,
        connection_id: Option<String>,
        database: Option<String>,
        namespace: Option<String>,
        decision: &str,
        row_count: Option<i64>,
        request_id: Option<String>,
    ) {
        let (organization_id, principal_id) = self.audit_identity();
        let record = crate::store::new_audit_record(
            organization_id,
            principal_id,
            connection_id,
            database,
            namespace,
            tool_name,
            decision,
            row_count,
            request_id,
        );
        if self
            .app_state
            .store
            .mcp_audit_log()
            .append(&record)
            .await
            .is_err()
        {
            // Do not put the storage path or backend error text in a log,
            // error, or event visible to the user.
            tracing::warn!(tool = tool_name, "MCP audit append failed");
            self.emit_audit_error("MCP audit write failed");
        }
    }

    pub(crate) async fn audit_allowed_call(
        &self,
        tool_name: &str,
        prepared: &PreparedToolCall,
        result_text: &str,
    ) {
        self.audit_tool_call(
            tool_name,
            prepared.plan.connection_id.clone(),
            prepared.plan.database.clone(),
            prepared.plan.namespace.clone(),
            "allow",
            matches!(tool_name, "query" | "use_session" | "run_workflow")
                .then(|| row_count_from_output(result_text))
                .flatten(),
            prepared.approval_request_id.clone(),
        )
        .await;
    }

    fn audit_identity(&self) -> (String, String) {
        if let Ok(adapter) = self.app_state.platform_require() {
            return (
                adapter.identity().organization_id().to_string(),
                adapter.identity().principal_id().to_string(),
            );
        }
        (
            self.app_state
                .organization_scope
                .organization_id()
                .to_string(),
            "unknown".to_string(),
        )
    }

    fn emit_audit_error(&self, message: &str) {
        if let Some(handle) = self.app_state.app_handle.get().cloned() {
            use tauri::Emitter;
            let _ = handle.emit(crate::mcp::MCP_AUDIT_ERROR_EVENT, message);
        }
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
}

fn default_approval_port(app_state: &Arc<AppState>) -> Arc<dyn McpApprovalPort> {
    let broker = crate::mcp::approval_broker();
    if let Some(handle) = app_state.app_handle.get().cloned() {
        broker.attach_app_handle(handle);
    }
    broker
}

/// The policy result retained across execution so the allow audit can record
/// the exact target and rowcount.
pub(crate) struct PreparedToolCall {
    pub(crate) plan: McpCallPlan,
    pub(crate) approval_request_id: Option<String>,
}

fn audit_target(
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
) -> (Option<String>, Option<String>, Option<String>) {
    let value = |key: &str| {
        arguments
            .and_then(|args| args.get(key))
            .and_then(|value| value.as_str())
            .map(str::to_owned)
    };
    (
        value("connection_id"),
        value("database"),
        value("schema").or_else(|| value("namespace")),
    )
}

pub(crate) fn row_count_from_output(text: &str) -> Option<i64> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    find_row_count(&value)
}

fn find_row_count(value: &serde_json::Value) -> Option<i64> {
    if let Some(values) = value.as_array() {
        return values.iter().find_map(find_row_count);
    }
    let object = value.as_object()?;
    for key in ["rowCount", "row_count", "rowsAffected", "rows_affected"] {
        if let Some(count) = object.get(key).and_then(|v| v.as_i64()) {
            return Some(count.max(0));
        }
    }
    if let Some(rows) = object.get("rows").and_then(|v| v.as_array()) {
        return Some(rows.len() as i64);
    }
    for nested in object.values() {
        if let Some(count) = find_row_count(nested) {
            return Some(count);
        }
    }
    None
}

/// M2 主体归位后本机是共享有状态会话端口的形态实现：runtime 租约解析、
/// 归属裁决（IdentityPolicy 是 application 词汇，由形态裁决）与 cleanup
/// worker 调度钩子。admission/编排/守卫语义在 `datazen::mcp::session_tools`。
#[async_trait::async_trait]
impl StatefulSessionHost for DataZenMcpServer {
    async fn resolve_call(&self) -> Result<StatefulCall, String> {
        let adapter = self
            .app_state
            .platform_require()
            .map_err(|error| error.to_string())?;
        let runtime = adapter.runtime().map_err(|error| error.to_string())?;
        Ok(StatefulCall {
            runtime,
            context: self.request_context(adapter.identity()),
        })
    }

    fn authorize_owner(&self, context: &RequestContext, owner: &OwnerRef) -> Result<(), String> {
        // Stateful steps share one admission path: build a context whose
        // `clientInstanceId` is the one this server generated, derive the owner
        // from it, and let IdentityPolicy decide. MCP never carries its own allow
        // rule for ownership — if the desktop policy rejects the owner, the tool
        // fails.
        IdentityPolicy::check_owner(context, owner, None).map_err(|error| error.to_string())
    }

    fn ensure_cleanup_worker(&self) {
        self.start_session_cleanup();
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
            datazen::services::db_tools::resolve_query_limit(None),
            Some(100)
        );
    }

    #[test]
    fn test_resolve_query_limit_some_passthrough() {
        assert_eq!(
            datazen::services::db_tools::resolve_query_limit(Some(50)),
            Some(50)
        );
    }

    #[test]
    fn test_resolve_query_limit_some_capped_at_max() {
        assert_eq!(
            datazen::services::db_tools::resolve_query_limit(Some(999_999)),
            Some(50_000)
        );
    }

    #[test]
    fn mcp_all_tools_list_is_stable() {
        assert_eq!(MCP_ALL_TOOLS.len(), 13);
        assert!(MCP_ALL_TOOLS.contains(&"list_connections"));
        assert!(MCP_ALL_TOOLS.contains(&"search_tables"));
        assert!(MCP_ALL_TOOLS.contains(&"run_workflow"));
        assert!(MCP_ALL_TOOLS.contains(&"open_session"));
        assert!(MCP_ALL_TOOLS.contains(&"use_session"));
        assert!(MCP_ALL_TOOLS.contains(&"close_session"));
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
            check_constraints: vec![],
            table_options: Default::default(),
        };

        let desc = format_table_description("users", &schema);
        assert!(desc.contains("Table: users"));
        assert!(desc.contains("id integer PK NOT NULL"));
        assert!(desc.contains("Primary Key: (id)"));
        assert!(desc.contains("users_pkey"));
    }

    fn mcp_server_allowing(
        state: std::sync::Arc<crate::commands::AppState>,
        connection_ids: &[&str],
    ) -> DataZenMcpServer {
        DataZenMcpServer::new(state).with_allowed_connections(
            &connection_ids
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>(),
        )
    }

    // Stateful session tests; include from extracted block below.
    include!("server/tests_stateful.rs");

    // Remaining integration tests live in server.rs; include from extracted block below.
    include!("server/tests_integration.rs");
}

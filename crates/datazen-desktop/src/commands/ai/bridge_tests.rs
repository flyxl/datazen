//! T22 end-to-end bridge tests: real minimal wapp/workflow fixtures, the T19
//! approval port and the shared MCP audit log.

use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use datazen::ai::application::tool_bridge::BridgeToolDecision;
use datazen::ai::tool_loop::AiToolContext;
use datazen::ai::ToolCall;
use datazen::mcp::approval::{McpApprovalDecision, McpApprovalPort, McpApprovalRequest};
use datazen::workflow::model::{
    WorkflowDefinition, WorkflowStep, WorkflowVariable, WorkflowVisibility,
};
use serde_json::json;

use super::bridge::{
    authorize_bridge_tool_with_port, bridge_tool_definitions, execute_plugin_tool,
    execute_workflow_tool,
};
use crate::commands::AppState;
use crate::testing::app_state_test_support::TestAppState;

const PLUGIN_ID: &str = "acme.bridge";
const CONNECTION_ID: &str = "bridge-conn";

struct ScriptedApproval {
    decision: McpApprovalDecision,
    requests: Mutex<Vec<McpApprovalRequest>>,
}

impl ScriptedApproval {
    fn new(decision: McpApprovalDecision) -> Self {
        Self {
            decision,
            requests: Mutex::new(Vec::new()),
        }
    }

    fn request_count(&self) -> usize {
        self.requests.lock().map(|rows| rows.len()).unwrap_or(0)
    }
}

#[async_trait]
impl McpApprovalPort for ScriptedApproval {
    async fn decide(&self, request: &McpApprovalRequest) -> McpApprovalDecision {
        if let Ok(mut requests) = self.requests.lock() {
            requests.push(request.clone());
        }
        self.decision
    }
}

fn write_plugin_fixture(root: &Path) -> String {
    let dir = root.join(PLUGIN_ID);
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = json!({
        "id": PLUGIN_ID,
        "name": "Bridge Fixture",
        "version": "1.0.0",
        "apiVersion": 2,
        "contributes": {},
        "permissions": ["command:invoke"],
        "ai": {
            "tools": [
                {
                    "name": "list_users",
                    "description": "List users",
                    "readOnly": true,
                    "command": "query",
                    "connection": CONNECTION_ID,
                    "inputSchema": {
                        "type": "object",
                        "properties": {},
                        "required": [],
                        "additionalProperties": false
                    },
                    "input": { "sql": "SELECT * FROM users" }
                },
                {
                    "name": "delete_user",
                    "description": "Delete one user",
                    "readOnly": false,
                    "command": "execute",
                    "connection": CONNECTION_ID,
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "integer", "minimum": 1 },
                            "password": { "type": "string" }
                        },
                        "required": ["id"],
                        "additionalProperties": false
                    },
                    "input": { "sql": "DELETE FROM users WHERE id = {{id}}" }
                },
                {
                    "name": "delete_by_password",
                    "description": "Delete by password and name",
                    "readOnly": false,
                    "command": "execute",
                    "connection": CONNECTION_ID,
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "password": { "type": "string" },
                            "name": { "type": "string" },
                            "id": { "type": "integer", "minimum": 1 }
                        },
                        "required": ["password", "name", "id"],
                        "additionalProperties": false
                    },
                    "input": {
                        "sql": "SELECT '{{password}}', '{{name}}' FROM users WHERE id = {{id}}"
                    }
                },
                {
                    "name": "select_password",
                    "description": "Render a password literal",
                    "readOnly": false,
                    "command": "execute",
                    "connection": CONNECTION_ID,
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "password": { "type": "string" }
                        },
                        "required": ["password"],
                        "additionalProperties": false
                    },
                    "input": { "sql": "SELECT '{{password}}'" }
                },
                {
                    "name": "find_user",
                    "description": "Find one user by name",
                    "readOnly": true,
                    "command": "query",
                    "connection": CONNECTION_ID,
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" }
                        },
                        "required": ["name"],
                        "additionalProperties": false
                    },
                    "input": {
                        "sql": "SELECT * FROM users WHERE name = '{{name}}'"
                    }
                },
                {
                    "name": "unsafe_read",
                    "description": "Unsafe read template",
                    "readOnly": true,
                    "command": "query",
                    "connection": CONNECTION_ID,
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" }
                        },
                        "required": ["name"],
                        "additionalProperties": false
                    },
                    "input": {
                        "sql": "SELECT * FROM users WHERE name = {{name}}"
                    }
                },
                {
                    "name": "unsafe_write",
                    "description": "Unsafe write template",
                    "readOnly": false,
                    "command": "execute",
                    "connection": CONNECTION_ID,
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" }
                        },
                        "required": ["name"],
                        "additionalProperties": false
                    },
                    "input": {
                        "sql": "DELETE FROM users WHERE name = {{name}}"
                    }
                }
            ]
        }
    });
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    dir.to_string_lossy().to_string()
}

async fn install_fixture(state: &AppState, root: &Path) {
    let path = write_plugin_fixture(root);
    crate::commands::install_wapp_from_path_impl(state, path)
        .await
        .expect("fixture installs");
    state
        .store
        .save_connection(
            crate::testing::app_state_test_support::sample_postgres_config(CONNECTION_ID),
        )
        .await
        .expect("save connection");
}

async fn opt_in(state: &AppState, plugins: &[&str], workflows: &[&str]) {
    let mut settings = state.store.get_settings().await;
    settings.ai_plugin_tool_ids = plugins.iter().map(|id| (*id).to_string()).collect();
    settings.ai_workflow_tool_ids = workflows.iter().map(|id| (*id).to_string()).collect();
    state.store.save_settings(settings).await.unwrap();
}

fn workflow(id: &str, step: WorkflowStep) -> WorkflowDefinition {
    WorkflowDefinition {
        id: id.into(),
        name: id.into(),
        description: "bridge fixture".into(),
        version: None,
        author: None,
        variables: vec![WorkflowVariable {
            name: "limit".into(),
            var_type: "integer".into(),
            description: "row limit".into(),
            required: Some(false),
            default: Some(json!(10)),
        }],
        connection: Some(CONNECTION_ID.into()),
        database: None,
        steps: vec![step],
        blocks: Vec::new(),
        output: None,
        timeout_secs: None,
        error_handling: None,
        schedule: None,
        visibility: WorkflowVisibility::User,
    }
}

fn query_step() -> WorkflowStep {
    WorkflowStep::Query {
        id: "q1".into(),
        sql: "SELECT * FROM users".into(),
        connection: None,
        database: None,
        timeout_secs: None,
        on_error: None,
    }
}

fn write_step() -> WorkflowStep {
    WorkflowStep::Command {
        id: "c1".into(),
        command: "execute".into(),
        connection: None,
        input: json!({ "sql": "DELETE FROM users WHERE id = 1" }),
        timeout_secs: None,
        on_error: None,
    }
}

fn query_step_sql(sql: &str) -> WorkflowStep {
    WorkflowStep::Query {
        id: "q1".into(),
        sql: sql.into(),
        connection: None,
        database: None,
        timeout_secs: None,
        on_error: None,
    }
}

fn query_step_with_id(id: &str, sql: &str) -> WorkflowStep {
    WorkflowStep::Query {
        id: id.into(),
        sql: sql.into(),
        connection: None,
        database: None,
        timeout_secs: None,
        on_error: None,
    }
}

fn command_step_sql(command: &str, sql: &str) -> WorkflowStep {
    WorkflowStep::Command {
        id: "c1".into(),
        command: command.into(),
        connection: None,
        input: json!({ "sql": sql }),
        timeout_secs: None,
        on_error: None,
    }
}

fn string_variable(name: &str) -> WorkflowVariable {
    WorkflowVariable {
        name: name.into(),
        var_type: "string".into(),
        description: String::new(),
        required: Some(false),
        default: None,
    }
}

fn array_variable(name: &str) -> WorkflowVariable {
    WorkflowVariable {
        name: name.into(),
        var_type: "array".into(),
        description: String::new(),
        required: Some(false),
        default: None,
    }
}

fn integer_variable(name: &str, default: i64) -> WorkflowVariable {
    WorkflowVariable {
        name: name.into(),
        var_type: "integer".into(),
        description: String::new(),
        required: Some(false),
        default: Some(json!(default)),
    }
}

fn call(name: &str, arguments: &str) -> ToolCall {
    ToolCall {
        id: "call-1".into(),
        name: name.into(),
        arguments: arguments.into(),
    }
}

fn ctx() -> AiToolContext {
    AiToolContext {
        allowed_connection_id: Some(CONNECTION_ID.into()),
        max_sample_rows: 100,
        cancel_token: None,
    }
}

async fn audit_for(test: &TestAppState, request_id: &str) -> crate::store::McpAuditRecord {
    test.store
        .mcp_audit_log()
        .list(50)
        .await
        .unwrap()
        .into_iter()
        .find(|row| row.request_id.as_deref() == Some(request_id))
        .unwrap_or_else(|| panic!("missing audit row for {request_id}"))
}

#[tokio::test]
async fn definitions_are_absent_until_opted_in_and_disappear_immediately() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    let fixtures = tempfile::tempdir().unwrap();
    install_fixture(&test.state, fixtures.path()).await;

    assert!(bridge_tool_definitions(&test.state).await.is_empty());

    opt_in(&test.state, &[PLUGIN_ID], &[]).await;
    let definitions = bridge_tool_definitions(&test.state).await;
    assert_eq!(definitions.len(), 7);
    assert!(definitions
        .iter()
        .any(|definition| definition.name == "plugin_acme.bridge__list_users"));

    test.state
        .wapps
        .set_enabled(PLUGIN_ID, false)
        .expect("disable plugin");
    assert!(bridge_tool_definitions(&test.state).await.is_empty());

    test.state
        .wapps
        .set_enabled(PLUGIN_ID, true)
        .expect("re-enable plugin");
    test.state
        .workflow_registry
        .save_workflow(
            &crate::platform::local_organization_scope(),
            &workflow("wf-read", query_step()),
        )
        .await
        .unwrap();
    opt_in(&test.state, &[PLUGIN_ID], &["wf-read"]).await;
    let definitions = bridge_tool_definitions(&test.state).await;
    assert!(definitions
        .iter()
        .any(|definition| definition.name == "workflow_wf-read"));

    opt_in(&test.state, &[], &[]).await;
    assert!(bridge_tool_definitions(&test.state).await.is_empty());
}

#[tokio::test]
async fn read_only_plugin_runs_without_approval_and_audits_allow() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    let fixtures = tempfile::tempdir().unwrap();
    install_fixture(&test.state, fixtures.path()).await;
    opt_in(&test.state, &[PLUGIN_ID], &[]).await;

    let port = ScriptedApproval::new(McpApprovalDecision::Deny);
    let decision = authorize_bridge_tool_with_port(
        &test.state,
        &port,
        "req-read",
        "plugin_acme.bridge__list_users",
        "{}",
        None,
    )
    .await;
    assert_eq!(decision, BridgeToolDecision::Allow);
    assert_eq!(port.request_count(), 0, "read-only must not prompt");

    let output = execute_plugin_tool(
        &test.state,
        "req-read",
        &ctx(),
        &call("plugin_acme.bridge__list_users", "{}"),
    )
    .await;
    assert!(output.contains("alice"), "{output}");
    assert!(test.mock.query_calls() > 0);

    let audit = test.store.mcp_audit_log().list(10).await.unwrap();
    let row = audit
        .iter()
        .find(|row| row.request_id.as_deref() == Some("req-read"))
        .expect("bridge audit row");
    assert_eq!(row.decision, "allow");
    assert_eq!(row.tool, "plugin_acme.bridge__list_users");
    assert_eq!(row.connection_id.as_deref(), Some(CONNECTION_ID));
    assert_eq!(row.row_count, Some(1));
}

#[tokio::test]
async fn non_readonly_plugin_requires_allow_once_and_never_runs_otherwise() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    let fixtures = tempfile::tempdir().unwrap();
    install_fixture(&test.state, fixtures.path()).await;
    opt_in(&test.state, &[PLUGIN_ID], &[]).await;
    let tool = "plugin_acme.bridge__delete_user";

    for decision in [
        McpApprovalDecision::Deny,
        McpApprovalDecision::Timeout,
        McpApprovalDecision::Cancel,
    ] {
        let port = ScriptedApproval::new(decision);
        let before = test.mock.execute_calls();
        let authorized = authorize_bridge_tool_with_port(
            &test.state,
            &port,
            "req-denied",
            tool,
            r#"{"id":7}"#,
            None,
        )
        .await;
        assert!(!authorized.is_allowed(), "{decision:?} must not allow");
        assert_eq!(test.mock.execute_calls(), before, "{decision:?} executed");
        assert_eq!(port.request_count(), 1);
    }

    let port = ScriptedApproval::new(McpApprovalDecision::AllowOnce);
    let authorized = authorize_bridge_tool_with_port(
        &test.state,
        &port,
        "req-allow",
        tool,
        r#"{"id":7,"password":"hunter2"}"#,
        None,
    )
    .await;
    assert_eq!(authorized, BridgeToolDecision::Allow);
    let requests = port.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].arguments["password"], "[REDACTED]");
    assert_eq!(requests[0].timeout_ms, 300_000);
    drop(requests);

    let before = test.mock.execute_calls();
    let output = execute_plugin_tool(
        &test.state,
        "req-allow",
        &ctx(),
        &call(tool, r#"{"id":7,"password":"hunter2"}"#),
    )
    .await;
    assert!(!output.contains("Error"), "{output}");
    assert!(test.mock.execute_calls() > before);
}

#[tokio::test]
async fn approval_sql_is_redacted_and_escaped_for_both_execution_paths() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    let fixtures = tempfile::tempdir().unwrap();
    install_fixture(&test.state, fixtures.path()).await;
    opt_in(&test.state, &[PLUGIN_ID], &[]).await;

    // Read-only: no prompt, but the same SQL-aware template renderer is used
    // on execution; O'Brien must not escape the WHERE literal.
    let read_port = ScriptedApproval::new(McpApprovalDecision::Deny);
    let read = authorize_bridge_tool_with_port(
        &test.state,
        &read_port,
        "req-read-injection",
        "plugin_acme.bridge__find_user",
        r#"{"name":"O'Brien"}"#,
        None,
    )
    .await;
    assert_eq!(read, BridgeToolDecision::Allow);
    assert_eq!(read_port.request_count(), 0);
    let before_queries = test.mock.query_calls();
    let output = execute_plugin_tool(
        &test.state,
        "req-read-injection",
        &ctx(),
        &call("plugin_acme.bridge__find_user", r#"{"name":"O'Brien"}"#),
    )
    .await;
    assert!(!output.contains("Error"), "{output}");
    assert!(test.mock.query_calls() > before_queries);

    // Non-read-only: approval SQL is rendered from redacted arguments with
    // the same literal escaping, so neither the password nor an injected
    // quote reaches the approval payload.
    let write_port = ScriptedApproval::new(McpApprovalDecision::AllowOnce);
    let write = authorize_bridge_tool_with_port(
        &test.state,
        &write_port,
        "req-write-preview",
        "plugin_acme.bridge__delete_by_password",
        r#"{"password":"hunter2","name":"O'Brien","id":7}"#,
        None,
    )
    .await;
    assert_eq!(write, BridgeToolDecision::Allow);
    let requests = write_port.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].arguments["password"], "[REDACTED]");
    assert_eq!(requests[0].arguments["name"], "O'Brien");
    let sql = requests[0].sql.as_deref().expect("SQL preview");
    assert!(!sql.contains("hunter2"), "{sql}");
    assert!(sql.contains("'[REDACTED]'"), "{sql}");
    assert!(sql.contains("'O''Brien'"), "{sql}");
    assert!(!sql.contains("O'Brien"), "{sql}");

    let exact_port = ScriptedApproval::new(McpApprovalDecision::AllowOnce);
    let exact = authorize_bridge_tool_with_port(
        &test.state,
        &exact_port,
        "req-exact-password",
        "plugin_acme.bridge__select_password",
        r#"{"password":"hunter2"}"#,
        None,
    )
    .await;
    assert_eq!(exact, BridgeToolDecision::Allow);
    let exact_requests = exact_port.requests.lock().unwrap();
    assert_eq!(exact_requests.len(), 1);
    assert_eq!(
        exact_requests[0].sql.as_deref(),
        Some("SELECT '[REDACTED]'")
    );
    assert!(!exact_requests[0]
        .sql
        .as_deref()
        .unwrap_or_default()
        .contains("hunter2"));
}

#[tokio::test]
async fn unsafe_sql_templates_are_rejected_before_prompt_or_execution() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    let fixtures = tempfile::tempdir().unwrap();
    install_fixture(&test.state, fixtures.path()).await;
    opt_in(&test.state, &[PLUGIN_ID], &[]).await;

    for tool in [
        "plugin_acme.bridge__unsafe_read",
        "plugin_acme.bridge__unsafe_write",
    ] {
        let port = ScriptedApproval::new(McpApprovalDecision::AllowOnce);
        let before_query = test.mock.query_calls();
        let before_execute = test.mock.execute_calls();
        let decision = authorize_bridge_tool_with_port(
            &test.state,
            &port,
            "req-unsafe",
            tool,
            r#"{"name":"O'Brien"}"#,
            None,
        )
        .await;
        assert!(
            matches!(decision, BridgeToolDecision::Deny { .. }),
            "{tool}: {decision:?}"
        );
        assert_eq!(port.request_count(), 0, "{tool} must not prompt");
        assert_eq!(test.mock.query_calls(), before_query, "{tool} executed");
        assert_eq!(test.mock.execute_calls(), before_execute, "{tool} executed");
    }
}

#[tokio::test]
async fn invalid_schema_is_rejected_before_any_approval_or_execution() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    let fixtures = tempfile::tempdir().unwrap();
    install_fixture(&test.state, fixtures.path()).await;
    opt_in(&test.state, &[PLUGIN_ID], &[]).await;

    let port = ScriptedApproval::new(McpApprovalDecision::AllowOnce);
    let before = test.mock.execute_calls();
    let decision = authorize_bridge_tool_with_port(
        &test.state,
        &port,
        "req-bad",
        "plugin_acme.bridge__delete_user",
        r#"{"id":"not-an-integer"}"#,
        None,
    )
    .await;
    assert!(matches!(decision, BridgeToolDecision::Deny { .. }));
    assert_eq!(port.request_count(), 0, "invalid schema must not prompt");
    assert_eq!(test.mock.execute_calls(), before);

    let audit = test.store.mcp_audit_log().list(10).await.unwrap();
    assert!(audit
        .iter()
        .any(|row| row.request_id.as_deref() == Some("req-bad") && row.decision == "deny"));
}

#[tokio::test]
async fn read_only_workflow_runs_directly_and_write_workflow_needs_approval() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    test.save_connection(CONNECTION_ID).await;
    test.state
        .workflow_registry
        .save_workflow(
            &crate::platform::local_organization_scope(),
            &workflow("wf-read", query_step()),
        )
        .await
        .unwrap();
    test.state
        .workflow_registry
        .save_workflow(
            &crate::platform::local_organization_scope(),
            &workflow("wf-write", write_step()),
        )
        .await
        .unwrap();
    opt_in(&test.state, &[], &["wf-read", "wf-write"]).await;

    let read_port = ScriptedApproval::new(McpApprovalDecision::Deny);
    let read = authorize_bridge_tool_with_port(
        &test.state,
        &read_port,
        "req-wf-read",
        "workflow_wf-read",
        "{}",
        None,
    )
    .await;
    assert_eq!(read, BridgeToolDecision::Allow);
    assert_eq!(read_port.request_count(), 0);
    let output = execute_workflow_tool(
        &test.state,
        "req-wf-read",
        &ctx(),
        &call("workflow_wf-read", "{}"),
    )
    .await;
    assert!(output.contains("\"success\":true"), "{output}");
    assert!(output.contains("workflowId"), "{output}");

    let deny_port = ScriptedApproval::new(McpApprovalDecision::Deny);
    let before = test.mock.execute_calls();
    let denied = authorize_bridge_tool_with_port(
        &test.state,
        &deny_port,
        "req-wf-write",
        "workflow_wf-write",
        "{}",
        None,
    )
    .await;
    assert!(!denied.is_allowed());
    assert_eq!(deny_port.request_count(), 1);
    assert_eq!(test.mock.execute_calls(), before);

    let allow_port = ScriptedApproval::new(McpApprovalDecision::AllowOnce);
    let before_query = test.mock.query_calls();
    let allowed = authorize_bridge_tool_with_port(
        &test.state,
        &allow_port,
        "req-wf-allow",
        "workflow_wf-write",
        "{}",
        None,
    )
    .await;
    assert_eq!(allowed, BridgeToolDecision::Allow);
    let output = execute_workflow_tool(
        &test.state,
        "req-wf-allow",
        &ctx(),
        &call("workflow_wf-write", "{}"),
    )
    .await;
    assert!(!output.contains("Error"), "{output}");
    assert!(
        test.mock.execute_calls() > before || test.mock.query_calls() > before_query,
        "{output}"
    );
}

#[tokio::test]
async fn read_only_workflow_renders_obrien_without_prompting() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    test.save_connection(CONNECTION_ID).await;
    let mut wf = workflow(
        "wf-safe",
        query_step_sql("SELECT '{{name}}' AS name FROM users"),
    );
    wf.variables = vec![string_variable("name")];
    test.state
        .workflow_registry
        .save_workflow(&crate::platform::local_organization_scope(), &wf)
        .await
        .unwrap();
    opt_in(&test.state, &[], &["wf-safe"]).await;

    let port = ScriptedApproval::new(McpApprovalDecision::Deny);
    let decision = authorize_bridge_tool_with_port(
        &test.state,
        &port,
        "req-wf-safe",
        "workflow_wf-safe",
        r#"{"name":"O'Brien"}"#,
        None,
    )
    .await;
    assert_eq!(decision, BridgeToolDecision::Allow);
    assert_eq!(port.request_count(), 0);

    let output = execute_workflow_tool(
        &test.state,
        "req-wf-safe",
        &ctx(),
        &call("workflow_wf-safe", r#"{"name":"O'Brien"}"#),
    )
    .await;
    assert!(output.contains("\"success\":true"), "{output}");
    let log = test.mock.query_log();
    assert!(log.iter().any(|sql| sql.contains("'O''Brien'")), "{log:?}");
}

#[tokio::test]
async fn workflow_approval_preview_redacts_credentials_and_escapes_strings() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    test.save_connection(CONNECTION_ID).await;
    let mut wf = workflow(
        "wf-preview",
        command_step_sql(
            "execute",
            "DELETE FROM users WHERE name = '{{name}}' AND password = '{{password}}' LIMIT {{limit}}",
        ),
    );
    wf.variables = vec![
        string_variable("name"),
        string_variable("password"),
        integer_variable("limit", 10),
    ];
    test.state
        .workflow_registry
        .save_workflow(&crate::platform::local_organization_scope(), &wf)
        .await
        .unwrap();
    opt_in(&test.state, &[], &["wf-preview"]).await;

    let port = ScriptedApproval::new(McpApprovalDecision::AllowOnce);
    let decision = authorize_bridge_tool_with_port(
        &test.state,
        &port,
        "req-wf-preview",
        "workflow_wf-preview",
        r#"{"name":"O'Brien","password":"super-secret"}"#,
        None,
    )
    .await;
    assert_eq!(decision, BridgeToolDecision::Allow);
    let requests = port.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let sql = requests[0].sql.as_deref().unwrap_or_default();
    assert!(sql.contains("'O''Brien'"), "{sql}");
    assert!(sql.contains("[REDACTED]"), "{sql}");
    assert!(sql.contains("LIMIT 10"), "{sql}");
    assert!(!sql.contains("super-secret"), "{sql}");
}

#[tokio::test]
async fn workflow_invalid_sql_template_denies_before_approval_or_execution() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    test.save_connection(CONNECTION_ID).await;
    let mut wf = workflow(
        "wf-invalid",
        query_step_sql("SELECT * FROM users WHERE name = {{name}}"),
    );
    wf.variables = vec![string_variable("name")];
    test.state
        .workflow_registry
        .save_workflow(&crate::platform::local_organization_scope(), &wf)
        .await
        .unwrap();
    opt_in(&test.state, &[], &["wf-invalid"]).await;

    let port = ScriptedApproval::new(McpApprovalDecision::AllowOnce);
    let before_query = test.mock.query_calls();
    let before_execute = test.mock.execute_calls();
    let decision = authorize_bridge_tool_with_port(
        &test.state,
        &port,
        "req-wf-invalid",
        "workflow_wf-invalid",
        r#"{"name":"O'Brien"}"#,
        None,
    )
    .await;
    assert!(!decision.is_allowed());
    assert_eq!(port.request_count(), 0);
    assert_eq!(test.mock.query_calls(), before_query);
    assert_eq!(test.mock.execute_calls(), before_execute);
    let audit = test.store.mcp_audit_log().list(20).await.unwrap();
    assert!(audit.iter().any(|row| {
        row.request_id.as_deref() == Some("req-wf-invalid") && row.decision == "deny"
    }));
}

#[tokio::test]
async fn workflow_placeholder_inside_a_comment_is_rejected() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    test.save_connection(CONNECTION_ID).await;
    let mut wf = workflow(
        "wf-comment",
        command_step_sql("execute", "DELETE FROM users /* {{name}} */ WHERE id = 1"),
    );
    wf.variables = vec![string_variable("name")];
    test.state
        .workflow_registry
        .save_workflow(&crate::platform::local_organization_scope(), &wf)
        .await
        .unwrap();
    opt_in(&test.state, &[], &["wf-comment"]).await;

    let port = ScriptedApproval::new(McpApprovalDecision::AllowOnce);
    let decision = authorize_bridge_tool_with_port(
        &test.state,
        &port,
        "req-wf-comment",
        "workflow_wf-comment",
        r#"{"name":"x"}"#,
        None,
    )
    .await;
    assert!(!decision.is_allowed());
    assert_eq!(port.request_count(), 0);
}

#[tokio::test]
async fn workflow_undefined_template_variable_denies_before_approval() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    test.save_connection(CONNECTION_ID).await;
    let wf = workflow(
        "wf-missing",
        query_step_sql("SELECT '{{missing}}' AS value"),
    );
    test.state
        .workflow_registry
        .save_workflow(&crate::platform::local_organization_scope(), &wf)
        .await
        .unwrap();
    opt_in(&test.state, &[], &["wf-missing"]).await;

    let port = ScriptedApproval::new(McpApprovalDecision::AllowOnce);
    let decision = authorize_bridge_tool_with_port(
        &test.state,
        &port,
        "req-wf-missing",
        "workflow_wf-missing",
        "{}",
        None,
    )
    .await;
    assert!(!decision.is_allowed());
    assert_eq!(port.request_count(), 0);
}

#[tokio::test]
async fn workflow_loop_value_is_known_during_prevalidation_and_escaped_at_execution() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    test.save_connection(CONNECTION_ID).await;
    let mut wf = workflow(
        "wf-loop",
        WorkflowStep::ForEach {
            id: "loop".into(),
            items: "{{items}}".into(),
            as_var: "item".into(),
            steps: vec![query_step_sql("SELECT '{{item}}' AS value")],
            max_iterations: None,
        },
    );
    wf.variables = vec![array_variable("items")];
    test.state
        .workflow_registry
        .save_workflow(&crate::platform::local_organization_scope(), &wf)
        .await
        .unwrap();
    opt_in(&test.state, &[], &["wf-loop"]).await;

    let port = ScriptedApproval::new(McpApprovalDecision::Deny);
    let args = r#"{"items":["O'Brien"]}"#;
    let decision = authorize_bridge_tool_with_port(
        &test.state,
        &port,
        "req-wf-loop",
        "workflow_wf-loop",
        args,
        None,
    )
    .await;
    assert_eq!(decision, BridgeToolDecision::Allow);
    assert_eq!(port.request_count(), 0);

    let output = execute_workflow_tool(
        &test.state,
        "req-wf-loop",
        &ctx(),
        &call("workflow_wf-loop", args),
    )
    .await;
    assert!(output.contains("\"success\":true"), "{output}");
    let log = test.mock.query_log();
    assert!(log.iter().any(|sql| sql.contains("'O''Brien'")), "{log:?}");
}

#[tokio::test]
async fn workflow_step_ids_include_condition_and_foreach_branches() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    test.save_connection(CONNECTION_ID).await;
    let mut wf = workflow(
        "wf-nested-steps",
        query_step_with_id("seed", "SELECT * FROM users"),
    );
    wf.steps.extend([
        WorkflowStep::Condition {
            id: "branch".into(),
            expr: "true".into(),
            then_steps: vec![query_step_with_id(
                "then_q",
                "SELECT '{{steps.seed.rows[0].name}}' AS value",
            )],
            else_steps: Some(vec![query_step_with_id(
                "else_q",
                "SELECT '{{steps.then_q.rows[0].name}}' AS value",
            )]),
        },
        WorkflowStep::ForEach {
            id: "loop".into(),
            items: "[]".into(),
            as_var: "item".into(),
            steps: vec![query_step_with_id(
                "body_q",
                "SELECT '{{steps.else_q.rows[0].name}}' AS value",
            )],
            max_iterations: None,
        },
    ]);
    test.state
        .workflow_registry
        .save_workflow(&crate::platform::local_organization_scope(), &wf)
        .await
        .unwrap();
    opt_in(&test.state, &[], &["wf-nested-steps"]).await;

    let port = ScriptedApproval::new(McpApprovalDecision::Deny);
    let decision = authorize_bridge_tool_with_port(
        &test.state,
        &port,
        "req-wf-nested-steps",
        "workflow_wf-nested-steps",
        "{}",
        None,
    )
    .await;
    assert_eq!(decision, BridgeToolDecision::Allow, "{decision:?}");
    assert_eq!(port.request_count(), 0);
}

#[tokio::test]
async fn execute_revalidates_invalid_template_and_audits_deny_before_dispatch() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    test.save_connection(CONNECTION_ID).await;
    let mut wf = workflow(
        "wf-exec-invalid",
        query_step_sql("SELECT * FROM users WHERE name = {{name}}"),
    );
    wf.variables = vec![string_variable("name")];
    test.state
        .workflow_registry
        .save_workflow(&crate::platform::local_organization_scope(), &wf)
        .await
        .unwrap();
    opt_in(&test.state, &[], &["wf-exec-invalid"]).await;

    let before = test.mock.query_calls();
    let output = execute_workflow_tool(
        &test.state,
        "req-exec-invalid",
        &ctx(),
        &call("workflow_wf-exec-invalid", r#"{"name":"O'Brien"}"#),
    )
    .await;
    assert!(!output.contains("\"success\":true"), "{output}");
    assert_eq!(test.mock.query_calls(), before);
    let audit = audit_for(&test, "req-exec-invalid").await;
    assert_eq!(audit.decision, "deny");
    assert_eq!(
        audit.reason.as_deref(),
        Some("workflowTemplateInvalid"),
        "{audit:?}"
    );
}

#[tokio::test]
async fn execute_revalidates_a_definition_changed_after_authorization() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    test.save_connection(CONNECTION_ID).await;
    let mut wf = workflow(
        "wf-toctou",
        query_step_sql("SELECT '{{name}}' AS name FROM users"),
    );
    wf.variables = vec![string_variable("name")];
    test.state
        .workflow_registry
        .save_workflow(&crate::platform::local_organization_scope(), &wf)
        .await
        .unwrap();
    opt_in(&test.state, &[], &["wf-toctou"]).await;

    let port = ScriptedApproval::new(McpApprovalDecision::Deny);
    let decision = authorize_bridge_tool_with_port(
        &test.state,
        &port,
        "req-wf-toctou",
        "workflow_wf-toctou",
        r#"{"name":"O'Brien"}"#,
        None,
    )
    .await;
    assert_eq!(decision, BridgeToolDecision::Allow);
    assert_eq!(port.request_count(), 0);

    let mut changed = wf.clone();
    changed.steps = vec![query_step_sql("SELECT * FROM users WHERE name = {{name}}")];
    test.state
        .workflow_registry
        .save_workflow(&crate::platform::local_organization_scope(), &changed)
        .await
        .unwrap();

    let before = test.mock.query_calls();
    let output = execute_workflow_tool(
        &test.state,
        "req-wf-toctou",
        &ctx(),
        &call("workflow_wf-toctou", r#"{"name":"O'Brien"}"#),
    )
    .await;
    assert!(!output.contains("\"success\":true"), "{output}");
    assert_eq!(test.mock.query_calls(), before);
    let audit = audit_for(&test, "req-wf-toctou").await;
    assert_eq!(audit.decision, "deny");
    assert_eq!(
        audit.reason.as_deref(),
        Some("workflowTemplateInvalid"),
        "{audit:?}"
    );
}

#[tokio::test]
async fn workflow_undefined_step_and_malformed_path_are_rejected() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    test.save_connection(CONNECTION_ID).await;

    for (workflow_id, sql) in [
        (
            "wf-undefined-step",
            "SELECT '{{steps.nope.rows[0].name}}' AS value",
        ),
        (
            "wf-malformed-path",
            "SELECT '{{steps.q1.rows[abc].name}}' AS value",
        ),
        ("wf-unclosed-path", "SELECT '{{steps.q1.rows[0}}' AS value"),
    ] {
        let wf = workflow(workflow_id, query_step_sql(sql));
        test.state
            .workflow_registry
            .save_workflow(&crate::platform::local_organization_scope(), &wf)
            .await
            .unwrap();
        opt_in(&test.state, &[], &[workflow_id]).await;

        let port = ScriptedApproval::new(McpApprovalDecision::AllowOnce);
        let decision = authorize_bridge_tool_with_port(
            &test.state,
            &port,
            &format!("req-{workflow_id}"),
            &format!("workflow_{workflow_id}"),
            "{}",
            None,
        )
        .await;
        assert!(!decision.is_allowed(), "{workflow_id}: {decision:?}");
        assert_eq!(port.request_count(), 0, "{workflow_id}");
    }
}

#[tokio::test]
async fn workflow_valid_steps_path_survives_prevalidation_and_execution() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    test.save_connection(CONNECTION_ID).await;
    let mut wf = workflow(
        "wf-valid-steps",
        query_step_with_id("q1", "SELECT * FROM users"),
    );
    wf.steps.push(query_step_with_id(
        "q2",
        "SELECT '{{steps.q1.rows[0].name}}' AS value",
    ));
    test.state
        .workflow_registry
        .save_workflow(&crate::platform::local_organization_scope(), &wf)
        .await
        .unwrap();
    opt_in(&test.state, &[], &["wf-valid-steps"]).await;

    let port = ScriptedApproval::new(McpApprovalDecision::Deny);
    let decision = authorize_bridge_tool_with_port(
        &test.state,
        &port,
        "req-wf-valid-steps",
        "workflow_wf-valid-steps",
        "{}",
        None,
    )
    .await;
    assert_eq!(decision, BridgeToolDecision::Allow);
    assert_eq!(port.request_count(), 0);

    let output = execute_workflow_tool(
        &test.state,
        "req-wf-valid-steps",
        &ctx(),
        &call("workflow_wf-valid-steps", "{}"),
    )
    .await;
    assert!(output.contains("\"success\":true"), "{output}");
    assert_eq!(test.mock.query_calls(), 2);
}

#[tokio::test]
async fn stop_cancels_a_pending_approval_without_executing() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    let fixtures = tempfile::tempdir().unwrap();
    install_fixture(&test.state, fixtures.path()).await;
    opt_in(&test.state, &[PLUGIN_ID], &[]).await;

    struct Pending(());
    #[async_trait]
    impl McpApprovalPort for Pending {
        async fn decide(&self, _request: &McpApprovalRequest) -> McpApprovalDecision {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            McpApprovalDecision::AllowOnce
        }
    }

    let token = tokio_util::sync::CancellationToken::new();
    token.cancel();
    let before = test.mock.execute_calls();
    let decision = authorize_bridge_tool_with_port(
        &test.state,
        &Pending(()),
        "req-stop",
        "plugin_acme.bridge__delete_user",
        r#"{"id":7}"#,
        Some(&token),
    )
    .await;
    assert_eq!(decision, BridgeToolDecision::Cancel);
    assert_eq!(test.mock.execute_calls(), before);

    let mut cancelled_ctx = ctx();
    cancelled_ctx.cancel_token = Some(token);
    let output = execute_plugin_tool(
        &test.state,
        "req-stop",
        &cancelled_ctx,
        &call("plugin_acme.bridge__delete_user", r#"{"id":7}"#),
    )
    .await;
    assert!(output.contains("cancelled"), "{output}");
    assert_eq!(test.mock.execute_calls(), before);
}

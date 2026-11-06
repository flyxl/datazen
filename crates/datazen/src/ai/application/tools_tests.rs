//! AI 工具面分发的行为钉子（2026-10-31 自桌面 `commands/ai/tests.rs` 归位：
//! `is_db_tool`/`classify_tool`/`db_tool_definitions` 一直在 modules，
//! 测试却住在宿主命令层——单元测试跟着逻辑走。B3：`ToolLoopGuard`/
//! `is_readonly_db_tool`/`mcp_needs_confirm` 钉子自桌面 `commands/ai/chat.rs`
//! 内联测试归位）。

use super::agent_db_tool_definitions;
use super::classify_tool;
use super::db_tool_definitions;
use super::db_tool_requires_confirmation;
use super::execute_db_tool;
use super::filter_db_tool_definitions;
use super::is_db_tool;
use super::is_readonly_db_tool;
use super::mcp_needs_confirm;
use super::AiToolContext;
use super::ToolKind;
use super::{ToolLoopGuard, ToolLoopLimits};
use crate::ai::application::ports::DatabaseToolHost;
use crate::ai::domain::types::{
    AiSafetyGateConfig, AiToolPermissionPolicy, MessageRole, TokenUsage, ToolDefinition,
};
use crate::connection::domain::{ConnectionRepository, ConnectionRepositoryError, ConnectionScope};
use crate::settings::ExecutionSettings;
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{CommandResult, ConnectionConfig, DatabaseDriver};
use std::sync::{Arc, Mutex};

fn test_config() -> ConnectionConfig {
    ConnectionConfig {
        id: "conn-1".into(),
        name: "fixture".into(),
        database_type: "postgres".into(),
        host: None,
        port: None,
        database: Some("app".into()),
        schema: None,
        username: None,
        password: None,
        ssl_mode: Default::default(),
        connection_timeout: 30,
        max_pool_size: 10,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    }
}

fn execution_settings() -> ExecutionSettings {
    ExecutionSettings {
        limit_select_results: false,
        query_result_limit: 0,
        safe_mode: false,
        connection_pool_size: 10,
        mcp_disabled_tools: Vec::new(),
        mcp_permission_mode: "read_only".into(),
        mcp_allowed_connection_ids: Vec::new(),
        mcp_allowed_databases: Vec::new(),
        mcp_allowed_namespaces: Vec::new(),
        driver_settings: serde_json::Map::new(),
        ai_strict_egress: false,
    }
}

/// Fake host that records the exact `query` target input and returns rows.
struct TestDbHost {
    config: ConnectionConfig,
    driver: Arc<dyn DatabaseDriver>,
    executions: Mutex<Vec<(String, serde_json::Value)>>,
}

impl TestDbHost {
    fn new() -> Self {
        Self {
            config: test_config(),
            driver: MockDriver::new("postgres", MockDriverOptions::default()),
            executions: Mutex::new(Vec::new()),
        }
    }

    fn executions(&self) -> Vec<(String, serde_json::Value)> {
        self.executions.lock().expect("executions").clone()
    }
}

#[async_trait::async_trait]
impl DatabaseToolHost for TestDbHost {
    async fn get_session_config(&self, _id: &str) -> Result<ConnectionConfig, String> {
        Ok(self.config.clone())
    }
    async fn driver_for_connection(&self, _id: &str) -> Result<Arc<dyn DatabaseDriver>, String> {
        Ok(self.driver.clone())
    }
    async fn execute_target(
        &self,
        _id: &str,
        command: &str,
        input: serde_json::Value,
    ) -> Result<CommandResult, String> {
        self.executions
            .lock()
            .expect("executions")
            .push((command.to_string(), input));
        Ok(CommandResult::new(
            serde_json::json!({ "columns": ["n"], "rows": [[1]] }),
        ))
    }
    async fn execution_settings(&self) -> ExecutionSettings {
        execution_settings()
    }
}

/// The read-only tools never touch the catalog, but `execute_db_tool` still
/// takes its port; keep the fake explicit so no test accidentally reaches it.
struct EmptyCatalog;

#[async_trait::async_trait]
impl ConnectionRepository for EmptyCatalog {
    async fn list(
        &self,
        _scope: &ConnectionScope,
    ) -> Result<Vec<ConnectionConfig>, ConnectionRepositoryError> {
        Ok(Vec::new())
    }
    async fn get(
        &self,
        _scope: &ConnectionScope,
        _connection_id: &str,
    ) -> Result<Option<ConnectionConfig>, ConnectionRepositoryError> {
        Ok(None)
    }
    async fn save(
        &self,
        _scope: &ConnectionScope,
        _config: ConnectionConfig,
    ) -> Result<(), ConnectionRepositoryError> {
        Ok(())
    }
    async fn delete(
        &self,
        _scope: &ConnectionScope,
        _connection_id: &str,
    ) -> Result<(), ConnectionRepositoryError> {
        Ok(())
    }
    async fn reorder(
        &self,
        _scope: &ConnectionScope,
        _ordered_ids: Vec<String>,
    ) -> Result<(), ConnectionRepositoryError> {
        Ok(())
    }
    async fn touch_last_connected_at(
        &self,
        _scope: &ConnectionScope,
        _connection_id: &str,
    ) -> Result<(), ConnectionRepositoryError> {
        Ok(())
    }
    async fn groups(
        &self,
        _scope: &ConnectionScope,
    ) -> Result<Vec<String>, ConnectionRepositoryError> {
        Ok(Vec::new())
    }
    async fn save_groups(
        &self,
        _scope: &ConnectionScope,
        _groups: Vec<String>,
    ) -> Result<(), ConnectionRepositoryError> {
        Ok(())
    }
}

fn tool(name: &str, arguments: &str) -> crate::ai::domain::types::ToolCall {
    crate::ai::domain::types::ToolCall {
        id: "call-1".into(),
        name: name.into(),
        arguments: arguments.into(),
    }
}

#[test]
fn test_is_db_tool() {
    assert!(is_db_tool("list_connections"));
    assert!(is_db_tool("list_databases"));
    assert!(is_db_tool("list_tables"));
    assert!(is_db_tool("get_table_schema"));
    assert!(is_db_tool("query_readonly"));
    assert!(is_db_tool("sample_rows"));
    assert!(!is_db_tool("ask_questions"));
    assert!(!is_db_tool("unknown_tool"));
    assert!(!is_db_tool(""));
}

#[test]
fn test_classify_tool() {
    assert_eq!(classify_tool("ask_questions"), ToolKind::AskQuestions);
    assert_eq!(
        classify_tool("list_connections"),
        ToolKind::Db("list_connections".into())
    );
    // 其余 DB 工具名同样归 Db（桌面钉子归位覆盖面）。
    assert!(matches!(classify_tool("list_databases"), ToolKind::Db(_)));
    assert!(matches!(classify_tool("list_tables"), ToolKind::Db(_)));
    assert!(matches!(classify_tool("search_tables"), ToolKind::Db(_)));
    assert!(matches!(classify_tool("get_table_schema"), ToolKind::Db(_)));
    assert!(matches!(classify_tool("query_readonly"), ToolKind::Db(_)));
    assert!(matches!(classify_tool("sample_rows"), ToolKind::Db(_)));
    assert_eq!(
        classify_tool("mcp/files/read_file"),
        ToolKind::Mcp {
            server_id: "files".into(),
            tool_name: "read_file".into(),
        }
    );
    assert_eq!(
        classify_tool("mcp/my-server/my_tool_name"),
        ToolKind::Mcp {
            server_id: "my-server".into(),
            tool_name: "my_tool_name".into(),
        }
    );
    match classify_tool("mcp/server1/get_data") {
        ToolKind::Mcp {
            server_id,
            tool_name,
        } => {
            assert_eq!(server_id, "server1");
            assert_eq!(tool_name, "get_data");
        }
        other => panic!("expected Mcp kind, got: {other:?}"),
    }
    assert_eq!(classify_tool("mcp/bad"), ToolKind::Unknown);
    assert_eq!(classify_tool("mcp//tool"), ToolKind::Unknown);
    assert_eq!(classify_tool("mcp/"), ToolKind::Unknown);
    assert_eq!(classify_tool("mcp/server/"), ToolKind::Unknown);
    assert_eq!(
        classify_tool("list_skills"),
        ToolKind::Skill("list_skills".into())
    );
    assert_eq!(
        classify_tool("use_skill"),
        ToolKind::Skill("use_skill".into())
    );
    assert_eq!(
        classify_tool("read_skill_file"),
        ToolKind::Skill("read_skill_file".into())
    );
    assert!(!is_db_tool("use_skill"));
    assert_eq!(classify_tool("unknown_tool"), ToolKind::Unknown);
    assert_eq!(classify_tool("nonexistent_tool"), ToolKind::Unknown);
}

#[test]
fn test_db_tool_definitions_count() {
    let tools = db_tool_definitions();
    assert_eq!(tools.len(), 5);
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"list_connections"));
    assert!(names.contains(&"list_databases"));
    assert!(names.contains(&"list_tables"));
    assert!(names.contains(&"get_table_schema"));
}

#[test]
fn test_agent_db_tool_definitions_adds_readonly_tools() {
    let names = |tools: Vec<super::ToolDefinition>| {
        tools
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<String>>()
    };
    let metadata = names(db_tool_definitions());
    assert_eq!(metadata.len(), 5);
    assert!(!metadata.contains(&"query_readonly".into()));
    assert!(!metadata.contains(&"sample_rows".into()));

    let unbound = names(agent_db_tool_definitions(false));
    assert_eq!(unbound.len(), 5);
    assert!(!unbound.contains(&"query_readonly".into()));
    assert!(!unbound.contains(&"sample_rows".into()));
    for name in metadata {
        assert!(unbound.contains(&name), "metadata tool {name} must stay");
    }

    let bound = names(agent_db_tool_definitions(true));
    assert_eq!(bound.len(), 7);
    assert!(bound.contains(&"query_readonly".into()));
    assert!(bound.contains(&"sample_rows".into()));
    for name in unbound {
        assert!(bound.contains(&name), "metadata tool {name} must stay");
    }
}

#[tokio::test]
async fn default_context_fails_closed_for_query_and_sampling() {
    // SQL generation also dispatches with `AiToolContext::default()`; a
    // hard-issued query/sample call must be refused before host execution.
    let host = TestDbHost::new();
    let ctx = AiToolContext::default();
    let catalog = EmptyCatalog;
    let scope = ConnectionScope::new("org");

    for (name, arguments) in [
        (
            "query_readonly",
            r#"{"connection_id":"other","sql":"SELECT 1"}"#,
        ),
        (
            "sample_rows",
            r#"{"connection_id":"other","table":"users"}"#,
        ),
    ] {
        let out = execute_db_tool(&host, &ctx, &catalog, &scope, &tool(name, arguments)).await;
        assert!(out.contains("no active connection bound"), "{name}: {out}");
    }
    assert!(
        host.executions().is_empty(),
        "unbound context must not execute"
    );
}

#[tokio::test]
async fn query_readonly_returns_rows_and_clamps_limit() {
    let host = TestDbHost::new();
    let ctx = AiToolContext {
        allowed_connection_id: Some("conn-1".into()),
        max_sample_rows: 3,
        cancel_token: None,
    };
    let catalog = EmptyCatalog;
    let scope = ConnectionScope::new("org");

    let out = execute_db_tool(
        &host,
        &ctx,
        &catalog,
        &scope,
        &tool(
            "query_readonly",
            r#"{"connection_id":"conn-1","sql":"SELECT 1","limit":5000}"#,
        ),
    )
    .await;
    assert!(out.contains("\"rows\""), "{out}");
    let executed = host.executions();
    assert_eq!(executed.len(), 1);
    assert_eq!(executed[0].0, "query");
    assert_eq!(executed[0].1["sql"], "SELECT 1");
    assert_eq!(executed[0].1["limit"], 1000, "query limit caps at 1000");
}

#[tokio::test]
async fn query_readonly_defaults_to_100_and_rejects_zero_to_one() {
    let host = TestDbHost::new();
    let ctx = AiToolContext {
        allowed_connection_id: Some("conn-1".into()),
        max_sample_rows: 3,
        cancel_token: None,
    };
    let catalog = EmptyCatalog;
    let scope = ConnectionScope::new("org");

    for (limit, expected) in [(None, 100), (Some(0), 1)] {
        let arguments = match limit {
            Some(limit) => {
                format!(r#"{{"connection_id":"conn-1","sql":"SELECT 1","limit":{limit}}}"#)
            }
            None => r#"{"connection_id":"conn-1","sql":"SELECT 1"}"#.to_string(),
        };
        execute_db_tool(
            &host,
            &ctx,
            &catalog,
            &scope,
            &tool("query_readonly", &arguments),
        )
        .await;
        let executed = host.executions();
        assert_eq!(executed.last().expect("execution").1["limit"], expected);
    }
}

#[tokio::test]
async fn sample_rows_quotes_relation_and_uses_gate_limit() {
    let host = TestDbHost::new();
    let ctx = AiToolContext {
        allowed_connection_id: Some("conn-1".into()),
        max_sample_rows: 7,
        cancel_token: None,
    };
    let catalog = EmptyCatalog;
    let scope = ConnectionScope::new("org");

    execute_db_tool(
        &host,
        &ctx,
        &catalog,
        &scope,
        &tool(
            "sample_rows",
            r#"{"connection_id":"conn-1","database":"app","schema":"public","table":"users"}"#,
        ),
    )
    .await;

    let executed = host.executions();
    assert_eq!(executed.len(), 1);
    assert_eq!(
        executed[0].1["sql"],
        "SELECT * FROM \"public\".\"users\" LIMIT 7"
    );
    assert_eq!(executed[0].1["limit"], 7);

    execute_db_tool(
        &host,
        &ctx,
        &catalog,
        &scope,
        &tool(
            "sample_rows",
            r#"{"connection_id":"conn-1","table":"orders","limit":500}"#,
        ),
    )
    .await;
    let executed = host.executions();
    assert_eq!(executed.len(), 2);
    assert_eq!(executed[1].1["sql"], "SELECT * FROM \"orders\" LIMIT 100");
    assert_eq!(executed[1].1["limit"], 100);
}

#[tokio::test]
async fn query_readonly_rejects_writes_without_execution() {
    let host = TestDbHost::new();
    let ctx = AiToolContext {
        allowed_connection_id: Some("conn-1".into()),
        max_sample_rows: 3,
        cancel_token: None,
    };
    let catalog = EmptyCatalog;
    let scope = ConnectionScope::new("org");

    let out = execute_db_tool(
        &host,
        &ctx,
        &catalog,
        &scope,
        &tool(
            "query_readonly",
            r#"{"connection_id":"conn-1","sql":"UPDATE users SET name = 'x'"}"#,
        ),
    )
    .await;
    assert!(out.contains("read-only"), "{out}");
    assert!(host.executions().is_empty(), "writes must not execute");
}

#[tokio::test]
async fn sample_rows_zero_gate_limit_still_reads_one_row() {
    let host = TestDbHost::new();
    let ctx = AiToolContext {
        allowed_connection_id: Some("conn-1".into()),
        max_sample_rows: 0,
        cancel_token: None,
    };
    let catalog = EmptyCatalog;
    let scope = ConnectionScope::new("org");

    execute_db_tool(
        &host,
        &ctx,
        &catalog,
        &scope,
        &tool(
            "sample_rows",
            r#"{"connection_id":"conn-1","table":"users"}"#,
        ),
    )
    .await;

    let executed = host.executions();
    assert_eq!(executed.len(), 1);
    assert_eq!(executed[0].1["sql"], "SELECT * FROM \"users\" LIMIT 1");
    assert_eq!(executed[0].1["limit"], 1);
}

#[tokio::test]
async fn read_only_tools_reject_cross_connection_without_execution() {
    let host = TestDbHost::new();
    let ctx = AiToolContext {
        allowed_connection_id: Some("conn-1".into()),
        max_sample_rows: 3,
        cancel_token: None,
    };
    let catalog = EmptyCatalog;
    let scope = ConnectionScope::new("org");

    for (name, arguments) in [
        (
            "query_readonly",
            r#"{"connection_id":"other","sql":"SELECT 1"}"#,
        ),
        (
            "sample_rows",
            r#"{"connection_id":"other","table":"users"}"#,
        ),
    ] {
        let out = execute_db_tool(&host, &ctx, &catalog, &scope, &tool(name, arguments)).await;
        assert!(out.contains("not allowed"), "{name}: {out}");
    }
    assert!(host.executions().is_empty(), "nothing may execute");
}

#[test]
fn test_db_tool_definitions_have_valid_schemas() {
    for tool in db_tool_definitions() {
        assert!(!tool.name.is_empty());
        assert!(!tool.description.is_empty());
        assert!(tool.parameters.is_object());
        let obj = tool.parameters.as_object().unwrap();
        assert_eq!(obj.get("type").and_then(|v| v.as_str()), Some("object"));
    }
}

// ── ToolLoopGuard（B3 自桌面 chat.rs 归位）──

#[test]
fn test_guard_new_defaults() {
    let guard = ToolLoopGuard::new();
    assert_eq!(guard.round, 0);
    assert_eq!(guard.total_tokens, 0);
    assert_eq!(guard.max_rounds, 6);
    assert_eq!(guard.max_total_tokens, 24_000);
    assert_eq!(guard.max_duration.as_secs(), 180);
    assert_eq!(guard.per_tool_cap, 2048);
    assert_eq!(guard.per_round_cap, 12_288);
}

#[test]
fn test_guard_check_round_succeeds_within_limits() {
    let mut guard = ToolLoopGuard::new();
    // First 6 rounds should succeed (max_rounds = 6)
    for i in 1..=6 {
        assert!(guard.check_round().is_ok(), "round {i} should succeed");
    }
    assert_eq!(guard.round, 6);
}

#[test]
fn test_guard_check_round_fails_on_max_rounds() {
    let mut guard = ToolLoopGuard::new();
    for _ in 0..6 {
        guard.check_round().unwrap();
    }
    // 7th round should fail
    let err = guard.check_round().unwrap_err();
    assert!(
        err.contains("max rounds"),
        "error should mention max rounds: {err}"
    );
}

#[test]
fn agent_limits_and_gate_defaults_are_thirty_round_multi_k() {
    let agent = ToolLoopLimits::agent();
    assert_eq!(agent.max_rounds, 30);
    assert_eq!(agent.max_total_tokens, 48_000);
    assert_eq!(agent.max_duration, std::time::Duration::from_secs(300));
    assert_eq!(agent.per_tool_cap, 8_192);

    let gate = crate::ai::domain::types::AiSafetyGateConfig::default();
    let from_gate = ToolLoopLimits::from_gate(&gate);
    assert_eq!(from_gate, agent);
}

#[test]
fn custom_max_rounds_allows_thirty_then_stops_at_thirty_one() {
    let mut guard = ToolLoopGuard::with_limits(ToolLoopLimits {
        max_rounds: 30,
        max_duration: std::time::Duration::from_secs(300),
        ..ToolLoopLimits::default()
    });
    for round in 1..=30 {
        assert!(guard.check_round().is_ok(), "round {round} must be allowed");
    }
    let error = guard.check_round().expect_err("round 31 must stop");
    assert!(error.contains("(30)"), "{error}");
}

#[test]
fn custom_max_rounds_one_stops_on_the_second_round() {
    let mut guard = ToolLoopGuard::with_limits(ToolLoopLimits {
        max_rounds: 1,
        ..ToolLoopLimits::default()
    });
    assert!(guard.check_round().is_ok());
    let error = guard.check_round().expect_err("second round must stop");
    assert!(error.contains("(1)"), "{error}");
}

#[test]
fn custom_token_budget_and_duration_are_enforced() {
    let mut guard = ToolLoopGuard::with_limits(ToolLoopLimits {
        max_total_tokens: 100,
        ..ToolLoopLimits::default()
    });
    guard.accumulate_usage(&TokenUsage {
        prompt_tokens: 60,
        completion_tokens: 40,
        total_tokens: 100,
    });
    let error = guard.check_round().expect_err("token budget must stop");
    assert!(error.contains("token budget"), "{error}");

    let mut slow = ToolLoopGuard::with_limits(ToolLoopLimits {
        max_duration: std::time::Duration::from_millis(1),
        ..ToolLoopLimits::default()
    });
    std::thread::sleep(std::time::Duration::from_millis(5));
    let error = slow.check_round().expect_err("duration must stop");
    assert!(error.contains("time limit"), "{error}");
}

#[test]
fn custom_per_tool_cap_truncates_results() {
    let guard = ToolLoopGuard::with_limits(ToolLoopLimits {
        per_tool_cap: 8,
        ..ToolLoopLimits::default()
    });
    let call = tool("query_readonly", "{}");
    let message = guard.truncate_and_wrap(&call, "0123456789abcdef".into());
    assert!(
        message.content.starts_with("01234567"),
        "{}",
        message.content
    );
    assert!(message.content.contains("truncated"), "{}", message.content);
}

#[test]
fn test_guard_check_round_fails_on_token_budget() {
    let mut guard = ToolLoopGuard::new();
    guard.total_tokens = 24_000; // at budget
    let err = guard.check_round().unwrap_err();
    assert!(
        err.contains("token budget"),
        "error should mention token budget: {err}"
    );
}

#[test]
fn test_guard_check_round_fails_on_time_limit() {
    let mut guard = ToolLoopGuard::new();
    guard.max_duration = std::time::Duration::from_millis(1); // 1ms
                                                              // Wait a bit to exceed the limit
    std::thread::sleep(std::time::Duration::from_millis(5));
    let err = guard.check_round().unwrap_err();
    assert!(
        err.contains("time limit"),
        "error should mention time limit: {err}"
    );
}

#[test]
fn test_guard_accumulate_usage() {
    let mut guard = ToolLoopGuard::new();
    let usage = TokenUsage {
        prompt_tokens: 100,
        completion_tokens: 200,
        total_tokens: 300,
    };
    guard.accumulate_usage(&usage);
    assert_eq!(guard.total_tokens, 300);

    guard.accumulate_usage(&usage);
    assert_eq!(guard.total_tokens, 600);
}

#[test]
fn test_guard_truncate_under_cap() {
    let result = ToolLoopGuard::truncate_tool_result("hello", 100);
    assert_eq!(result, "hello");
}

#[test]
fn test_guard_truncate_over_cap() {
    let long = "a".repeat(3000);
    let result = ToolLoopGuard::truncate_tool_result(&long, 100);
    assert!(result.len() < 3000);
    assert!(result.contains("truncated"));
    assert!(result.contains("bytes omitted"));
}

#[test]
fn test_guard_truncate_exact_cap() {
    let exact = "a".repeat(100);
    let result = ToolLoopGuard::truncate_tool_result(&exact, 100);
    assert_eq!(result, exact);
}

#[test]
fn test_guard_truncate_and_wrap() {
    let guard = ToolLoopGuard::new();
    let tc = crate::ai::domain::types::ToolCall {
        id: "call_1".into(),
        name: "test".into(),
        arguments: "{}".into(),
    };
    let msg = guard.truncate_and_wrap(&tc, "result content".into());
    assert_eq!(msg.role, MessageRole::Tool);
    assert_eq!(msg.tool_call_id.as_deref(), Some("call_1"));
    assert_eq!(msg.content, "result content");
}

#[test]
fn test_guard_truncate_and_wrap_truncates_large() {
    let guard = ToolLoopGuard::new();
    let tc = crate::ai::domain::types::ToolCall {
        id: "call_2".into(),
        name: "test".into(),
        arguments: "{}".into(),
    };
    let big = "x".repeat(4096);
    let msg = guard.truncate_and_wrap(&tc, big);
    assert!(msg.content.len() < 4096);
    assert!(msg.content.contains("truncated"));
}

// ── is_readonly_db_tool（B3 归位）──

#[test]
fn test_is_readonly_db_tool() {
    assert!(is_readonly_db_tool("list_connections"));
    assert!(is_readonly_db_tool("list_databases"));
    assert!(is_readonly_db_tool("list_tables"));
    assert!(is_readonly_db_tool("search_tables"));
    assert!(is_readonly_db_tool("get_table_schema"));
    assert!(is_readonly_db_tool("query_readonly"));
    assert!(is_readonly_db_tool("sample_rows"));
    assert!(!is_readonly_db_tool("ask_questions"));
    assert!(!is_readonly_db_tool("unknown"));
}

// ── mcp_needs_confirm（B3 归位）──

#[test]
fn test_mcp_needs_confirm_schema_based() {
    let schema = serde_json::json!({"x-write": true});
    assert!(mcp_needs_confirm("any_tool", &schema));
}

#[test]
fn test_mcp_needs_confirm_name_based() {
    let schema = serde_json::json!({});
    assert!(mcp_needs_confirm("write_file", &schema));
    assert!(mcp_needs_confirm("delete_record", &schema));
    assert!(mcp_needs_confirm("drop_table", &schema));
    assert!(mcp_needs_confirm("update_user", &schema));
    assert!(mcp_needs_confirm("create_index", &schema));
    assert!(mcp_needs_confirm("insert_data", &schema));
}

#[test]
fn test_mcp_needs_confirm_readonly() {
    let schema = serde_json::json!({});
    assert!(!mcp_needs_confirm("get_data", &schema));
    assert!(!mcp_needs_confirm("list_files", &schema));
}

// ── db_tool_policy tool-face filtering ──

fn fake_db_tool(name: &str) -> ToolDefinition {
    ToolDefinition {
        name: name.into(),
        description: name.into(),
        parameters: serde_json::json!({"type": "object"}),
    }
}

#[test]
fn db_policy_disabled_removes_every_tool() {
    let tools = filter_db_tool_definitions(
        vec![fake_db_tool("list_tables"), fake_db_tool("query_readonly")],
        AiToolPermissionPolicy::Disabled,
    );
    assert!(tools.is_empty());
}

#[test]
fn db_policy_read_only_drops_a_future_write_tool() {
    let tools = filter_db_tool_definitions(
        vec![fake_db_tool("list_tables"), fake_db_tool("drop_table")],
        AiToolPermissionPolicy::ReadOnly,
    );
    let names = tools.into_iter().map(|tool| tool.name).collect::<Vec<_>>();
    assert_eq!(names, vec!["list_tables"]);
}

#[test]
fn db_policy_require_confirm_and_unrestricted_keep_the_full_face() {
    for policy in [
        AiToolPermissionPolicy::RequireConfirm,
        AiToolPermissionPolicy::Unrestricted,
    ] {
        let tools = filter_db_tool_definitions(
            vec![fake_db_tool("list_tables"), fake_db_tool("drop_table")],
            policy,
        );
        assert_eq!(tools.len(), 2, "{policy:?}");
    }
}

#[test]
fn db_confirmation_policy_covers_policy_and_model_sql_switch() {
    let mut gate = AiSafetyGateConfig::default();
    gate.db_tool_policy = AiToolPermissionPolicy::Unrestricted;
    gate.require_sql_confirm = true;
    assert!(db_tool_requires_confirmation(&gate, "query_readonly"));
    assert!(!db_tool_requires_confirmation(&gate, "sample_rows"));
    assert!(!db_tool_requires_confirmation(&gate, "list_tables"));

    gate.require_sql_confirm = false;
    assert!(!db_tool_requires_confirmation(&gate, "query_readonly"));

    gate.db_tool_policy = AiToolPermissionPolicy::RequireConfirm;
    assert!(db_tool_requires_confirmation(&gate, "list_tables"));
    assert!(db_tool_requires_confirmation(&gate, "sample_rows"));
}

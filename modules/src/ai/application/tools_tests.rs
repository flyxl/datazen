//! AI 工具面分发的行为钉子（2026-10-31 自桌面 `commands/ai/tests.rs` 归位：
//! `is_db_tool`/`classify_tool`/`db_tool_definitions` 一直在 modules，
//! 测试却住在宿主命令层——单元测试跟着逻辑走。B3：`ToolLoopGuard`/
//! `is_readonly_db_tool`/`mcp_needs_confirm` 钉子自桌面 `commands/ai/chat.rs`
//! 内联测试归位）。

use super::classify_tool;
use super::db_tool_definitions;
use super::is_db_tool;
use super::is_readonly_db_tool;
use super::mcp_needs_confirm;
use super::ToolKind;
use super::ToolLoopGuard;
use crate::ai::domain::types::{MessageRole, TokenUsage};

#[test]
fn test_is_db_tool() {
    assert!(is_db_tool("list_connections"));
    assert!(is_db_tool("list_databases"));
    assert!(is_db_tool("list_tables"));
    assert!(is_db_tool("get_table_schema"));
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

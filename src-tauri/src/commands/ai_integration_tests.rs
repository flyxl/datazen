//! WireMock-backed integration tests for AI command handlers.

use std::sync::{Arc, Mutex};

use datazen_ai_api::{ChatMessage, MessageRole, ToolCall};
use datazen_driver_api::PromptScenario;

use super::*;
use crate::testing::ai_wiremock::WiremockAi;
use crate::testing::app_state::TestAppState;

fn collecting_stream_callback() -> (StreamCallback, Arc<Mutex<Vec<(String, bool)>>>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let log_c = log.clone();
    let cb: StreamCallback = Arc::new(move |request_id, result| {
        let done = result.as_ref().map(|c| c.done).unwrap_or(false);
        log_c
            .lock()
            .unwrap()
            .push((request_id.to_string(), done));
    });
    (cb, log)
}

#[tokio::test]
async fn resolve_ai_requires_saved_config() {
    let test = TestAppState::new().await;
    test.state.ensure_ai_ready().await;
    assert!(resolve_ai(&test.state).await.is_err());
}

#[tokio::test]
async fn ai_validate_config_hits_wiremock() {
    let (test, mock) = TestAppState::with_wiremock_ai().await;
    let cfg = crate::testing::ai_wiremock::openai_wiremock_config(&mock);
    ai_validate_config_impl(&test.state, cfg).await.unwrap();
}

#[tokio::test]
async fn ai_diagnose_error_returns_parsed_json() {
    let (test, mock) = TestAppState::with_wiremock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("diag-cfg").await;

    mock.mount_chat_completion_text(
        r#"{"explanation":"missing column","suggestedSql":"SELECT id FROM users","changes":[]}"#,
    )
    .await;

    let result = ai_diagnose_error_impl(
        &test.state,
        conn_id,
        "app".into(),
        "SELECT bad FROM users".into(),
        "column bad does not exist".into(),
    )
    .await
    .unwrap();

    assert_eq!(result.explanation, "missing column");
    assert_eq!(
        result.suggested_sql.as_deref(),
        Some("SELECT id FROM users")
    );
}

#[tokio::test]
async fn ai_analyze_explain_parses_response() {
    let (test, mock) = TestAppState::with_wiremock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("explain-cfg").await;

    mock.mount_chat_completion_text(
        r#"{"summary":"Seq scan","bottlenecks":[{"node":"Seq Scan","description":"full scan","severity":"high"}],"suggestions":[{"description":"Add index","sql":"CREATE INDEX ON users(id)","impact":"high"}]}"#,
    )
    .await;

    let result = ai_analyze_explain_impl(
        &test.state,
        conn_id,
        "Seq Scan on users".into(),
        "SELECT * FROM users".into(),
    )
    .await
    .unwrap();

    assert_eq!(result.summary, "Seq scan");
    assert_eq!(result.bottlenecks.len(), 1);
    assert_eq!(result.suggestions.len(), 1);
}

#[tokio::test]
async fn ai_parse_filter_normalizes_null_operators() {
    let (test, mock) = TestAppState::with_wiremock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("filter-cfg").await;

    mock.mount_chat_completion_text(
        r#"[{"column":"name","operator":"eq","value":null},{"column":"id","operator":"gt","value":1}]"#,
    )
    .await;

    let filters = ai_parse_filter_impl(
        &test.state,
        conn_id,
        "app".into(),
        "users".into(),
        "id greater than 1 and name is null".into(),
    )
    .await
    .unwrap();

    use crate::services::query_executor::FilterOperator;
    assert_eq!(filters.len(), 2);
    assert!(matches!(filters[0].operator, FilterOperator::IsNull));
    assert!(matches!(filters[1].operator, FilterOperator::Gt));
}

#[tokio::test]
async fn ai_generate_schema_doc_returns_markdown() {
    let (test, mock) = TestAppState::with_wiremock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("schema-doc-cfg").await;

    mock.mount_chat_completion_text("# Users\n\nTable of users.")
        .await;

    let doc = ai_generate_schema_doc_impl(&test.state, conn_id, "app".into())
        .await
        .unwrap();
    assert!(doc.contains("Users"));
}

#[tokio::test]
async fn ai_diagnose_connection_parses_json() {
    let (test, mock) = TestAppState::with_wiremock_ai().await;
    test.save_connection("conn-diag").await;

    mock.mount_chat_completion_text(
        r#"{"diagnosis":"Auth failed","possibleCauses":["bad password"],"solutions":[{"description":"Reset password","command":null}],"category":"auth"}"#,
    )
    .await;

    let result = ai_diagnose_connection_impl(
        &test.state,
        "conn-diag".into(),
        "password authentication failed".into(),
    )
    .await
    .unwrap();

    assert_eq!(result.diagnosis, "Auth failed");
    assert_eq!(result.category, "auth");
}

#[tokio::test]
async fn ai_analyze_queries_requires_history() {
    let (test, _mock) = TestAppState::with_wiremock_ai().await;
    let err = ai_analyze_queries_impl(&test.state, None).await.unwrap_err();
    assert!(err.to_string().contains("No query history"));
}

#[tokio::test]
async fn ai_analyze_queries_with_history() {
    let (test, mock) = TestAppState::with_wiremock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("hist-cfg").await;

    test.state
        .store
        .add_query_history(crate::store::QueryHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            connection_id: conn_id.clone(),
            database: "app".into(),
            sql: "SELECT 1".into(),
            executed_at: chrono::Utc::now(),
            execution_time_ms: 1,
            rows_affected: Some(1),
            success: true,
            error_message: None,
        })
        .await
        .unwrap();

    mock.mount_chat_completion_text(
        r#"{"summary":"Light reads","categories":[{"name":"SELECT","count":1,"examples":["SELECT 1"]}],"insights":["simple"],"frequentTables":["users"],"recommendations":[]}"#,
    )
    .await;

    let analysis = ai_analyze_queries_impl(&test.state, Some(conn_id))
        .await
        .unwrap();
    assert_eq!(analysis.summary, "Light reads");
}

#[tokio::test]
async fn execute_db_tool_list_connections() {
    let test = TestAppState::with_tables().await;
    test.save_connection("tool-cfg").await;

    let tool = ToolCall {
        id: "t1".into(),
        name: "list_connections".into(),
        arguments: "{}".into(),
    };
    let out = execute_db_tool(&test.state, &tool).await;
    assert!(out.contains("tool-cfg"));
}

#[tokio::test]
async fn execute_db_tool_list_tables() {
    let test = TestAppState::with_tables().await;
    test.save_connection("tables-cfg").await;

    let tool = ToolCall {
        id: "t2".into(),
        name: "list_tables".into(),
        arguments: r#"{"config_id":"tables-cfg","database":"app"}"#.into(),
    };
    let out = execute_db_tool(&test.state, &tool).await;
    assert!(out.contains("users"));
}

#[tokio::test]
async fn build_connections_context_lists_saved_configs() {
    let test = TestAppState::new().await;
    test.save_connection("ctx-cfg").await;
    let ctx = build_connections_context(&test.state, "en").await;
    assert!(ctx.contains("ctx-cfg"));
    assert!(ctx.contains("available"));
}

#[tokio::test]
async fn ai_generate_sql_stream_without_tools() {
    let (test, mock) = TestAppState::with_wiremock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("nl2sql-cfg").await;
    mock.mount_chat_stream_text("SELECT 1").await;

    let (cb, log) = collecting_stream_callback();
    let rid = ai_generate_sql_impl(
        &test.state,
        cb,
        conn_id,
        "app".into(),
        "count users".into(),
        "req-nl2sql".into(),
        Some("users".into()),
        None,
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(rid, "req-nl2sql");
    assert!(!log.lock().unwrap().is_empty());
}

#[tokio::test]
async fn ai_chat_with_schema_and_stream() {
    let (test, mock) = TestAppState::with_wiremock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("chat-cfg").await;
    mock.mount_chat_stream_text("Hello from AI").await;

    let (cb, _log) = collecting_stream_callback();
    let rid = ai_chat_impl(
        &test.state,
        cb,
        Some(conn_id),
        Some("app".into()),
        vec![ChatMessage {
            role: MessageRole::User,
            content: "Hi".into(),
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        "req-chat".into(),
        true,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(rid, "req-chat");
}

#[tokio::test]
async fn ai_chat_workflow_scenario_builds_system_prompt() {
    let (test, mock) = TestAppState::with_wiremock_ai().await;
    test.save_connection("wf-ctx").await;
    mock.mount_chat_stream_text("workflow yaml").await;

    let (cb, _) = collecting_stream_callback();
    ai_chat_impl(
        &test.state,
        cb,
        None,
        None,
        vec![ChatMessage {
            role: MessageRole::User,
            content: "Build a workflow".into(),
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        "req-wf".into(),
        false,
        Some("workflow_generate".into()),
        None,
        None,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn prompt_override_round_trip() {
    use crate::ai::prompt_resolver::PromptSource;

    let test = TestAppState::new().await;
    let entry = PromptOverrideEntry {
        driver_type: "postgres".into(),
        scenario: PromptScenario::Chat,
        system_zh: "自定义".into(),
        system_en: "Custom chat prompt".into(),
    };
    test.state
        .prompt_resolver
        .set_override(entry.clone())
        .await
        .unwrap();

    let prompts = prompt_list_impl(&test.state, Some("postgres".into()))
        .await
        .unwrap();
    assert!(prompts.iter().any(|p| p.source == PromptSource::User));

    test.state
        .prompt_resolver
        .remove_override("postgres", PromptScenario::Chat)
        .await
        .unwrap();
}

#[tokio::test]
async fn workflow_save_list_get_delete() {
    use crate::workflow::{WorkflowDefinition, WorkflowStep};

    let test = TestAppState::new().await;
    test.state.workflow_registry.load_all().await.unwrap();
    let wf = WorkflowDefinition {
        id: "test-wf".into(),
        name: "Test Workflow".into(),
        description: "desc".into(),
        version: Some("1".into()),
        author: None,
        variables: vec![],
        steps: vec![WorkflowStep::Ai {
            id: "s1".into(),
            prompt: "Say hi".into(),
            timeout_secs: None,
            on_error: None,
        }],
        output: None,
        timeout_secs: None,
        error_handling: None,
    };

    test.state.workflow_registry.save_workflow(&wf).await.unwrap();
    let list = workflow_list_impl(&test.state).await.unwrap();
    assert!(list.iter().any(|w| w.id == "test-wf"));

    let loaded = test
        .state
        .workflow_registry
        .get("test-wf")
        .await
        .expect("workflow");
    assert_eq!(loaded.name, "Test Workflow");

    test.state
        .workflow_registry
        .delete_workflow("test-wf")
        .await
        .unwrap();
    let remaining = workflow_list_impl(&test.state).await.unwrap();
    assert!(
        !remaining.iter().any(|w| w.id == "test-wf"),
        "deleted workflow should be gone, got: {remaining:?}"
    );
}

fn many_tables_mock_options() -> crate::testing::mock_driver::MockDriverOptions {
    use crate::db::{TableInfo, TableType};
    let mut opts = crate::testing::app_state::rich_mock_options();
    opts.tables = (0..35)
        .map(|i| TableInfo {
            name: format!("table_{i}"),
            schema: None,
            table_type: TableType::Table,
            row_count: Some(0),
        })
        .collect();
    opts
}

#[tokio::test]
async fn ai_generate_schema_doc_selects_tables_when_many() {
    let (test, mock) =
        TestAppState::with_wiremock_ai_options(many_tables_mock_options()).await;
    let (_, conn_id) = test.save_and_connect("many-tables").await;

    mock.mount_chat_completion_text_once(r#"["table_0","table_1"]"#)
        .await;
    mock.mount_chat_completion_text_once("# Schema doc")
        .await;

    let doc = ai_generate_schema_doc_impl(&test.state, conn_id, "app".into())
        .await
        .unwrap();
    assert!(doc.contains("# Schema doc"));
}

#[tokio::test]
async fn ai_generate_sql_tool_loop_executes_list_connections() {
    let (test, mock) = TestAppState::with_wiremock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("tool-loop").await;

    let tool_sse = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"list_connections","arguments":"{}"}}]}}]}

data: {"choices":[{"finish_reason":"tool_calls"}]}

data: [DONE]

"#;
    let final_sse = r#"data: {"choices":[{"delta":{"content":"SELECT 1"}}]}

data: {"choices":[{"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3}}

data: [DONE]

"#;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, ResponseTemplate};
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", "Bearer test-api-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(tool_sse),
        )
        .up_to_n_times(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", "Bearer test-api-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(final_sse),
        )
        .mount(&mock.server)
        .await;

    let (cb, _) = collecting_stream_callback();
    ai_generate_sql_impl(
        &test.state,
        cb,
        conn_id,
        "app".into(),
        "how many users".into(),
        "req-tools".into(),
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn ai_fetch_remote_models_openai_wiremock() {
    let mock = WiremockAi::start().await;
    mock.mount_models_list().await;
    let models = ai_fetch_remote_models(
        "open_ai_compatible".into(),
        mock.endpoint_v1(),
        "test-api-key".into(),
    )
    .await
    .unwrap();
    assert!(models.iter().any(|m| m.id == "gpt-test"));
}

#[tokio::test]
async fn ai_parse_filter_drops_unknown_columns() {
    let (test, mock) = TestAppState::with_wiremock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("filter-drop").await;

    mock.mount_chat_completion_text(
        r#"[{"column":"ghost","operator":"eq","value":"x"},{"column":"name","operator":"like","value":"a"}]"#,
    )
    .await;

    let filters = ai_parse_filter_impl(
        &test.state,
        conn_id,
        "app".into(),
        "users".into(),
        "name like a".into(),
    )
    .await
    .unwrap();
    assert_eq!(filters.len(), 1);
    assert_eq!(filters[0].column, "name");
}

#[tokio::test]
async fn ai_generate_sql_injects_context_file() {
    use crate::commands::context::resolve_context_dir_from_state;

    let (test, mock) = TestAppState::with_wiremock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("ctx-sql").await;
    let ctx_dir = resolve_context_dir_from_state(&test.state).await.unwrap();
    std::fs::create_dir_all(&ctx_dir).unwrap();
    std::fs::write(ctx_dir.join("notes.md"), "Context hint").unwrap();

    mock.mount_chat_stream_text("SELECT 1").await;

    let (cb, _) = collecting_stream_callback();
    ai_generate_sql_impl(
        &test.state,
        cb,
        conn_id,
        "app".into(),
        "run query".into(),
        "req-ctx".into(),
        None,
        None,
        Some(vec!["notes.md".into()]),
        None,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn resolve_ai_succeeds_with_wiremock_config() {
    let (test, _mock) = TestAppState::with_wiremock_ai().await;
    let (provider, cfg) = resolve_ai(&test.state).await.unwrap();
    assert_eq!(cfg.model, "gpt-test");
    assert!(provider.supports_streaming());
}

#[tokio::test]
async fn execute_db_tool_unknown_returns_error_string() {
    let test = TestAppState::new().await;
    let tool = ToolCall {
        id: "bad".into(),
        name: "nope".into(),
        arguments: "{}".into(),
    };
    let out = execute_db_tool(&test.state, &tool).await;
    assert!(out.contains("Unknown tool"));
}

#[tokio::test]
async fn execute_db_tool_get_table_schema() {
    let test = TestAppState::with_tables().await;
    test.save_connection("schema-tool").await;

    let tool = ToolCall {
        id: "t3".into(),
        name: "get_table_schema".into(),
        arguments: r#"{"config_id":"schema-tool","tables":["users"]}"#.into(),
    };
    let out = execute_db_tool(&test.state, &tool).await;
    assert!(out.contains("users"));
}

#[tokio::test]
async fn ai_delete_config_resets_providers() {
    let (test, _mock) = TestAppState::with_wiremock_ai().await;
    assert!(ai_get_config_impl(&test.state).await.unwrap().is_some());
    ai_delete_config_impl(&test.state).await.unwrap();
    assert!(ai_get_config_impl(&test.state).await.unwrap().is_none());
}

#[tokio::test]
async fn ai_chat_injects_context_files_into_user_message() {
    use crate::commands::context::resolve_context_dir_from_state;

    let (test, mock) = TestAppState::with_wiremock_ai_tables().await;
    let ctx_dir = resolve_context_dir_from_state(&test.state).await.unwrap();
    std::fs::create_dir_all(&ctx_dir).unwrap();
    std::fs::write(ctx_dir.join("ctx.md"), "File context").unwrap();
    mock.mount_chat_stream_text("ok").await;

    let (cb, _) = collecting_stream_callback();
    ai_chat_impl(
        &test.state,
        cb,
        None,
        None,
        vec![ChatMessage {
            role: MessageRole::User,
            content: "Question".into(),
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        "req-ctx-chat".into(),
        false,
        None,
        Some(vec!["ctx.md".into()]),
        None,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn ai_chat_schema_resolve_failure_still_streams() {
    let (test, mock) = TestAppState::with_wiremock_ai().await;
    mock.mount_chat_stream_text("fallback").await;

    let (cb, _) = collecting_stream_callback();
    ai_chat_impl(
        &test.state,
        cb,
        Some("not-connected".into()),
        Some("app".into()),
        vec![ChatMessage {
            role: MessageRole::User,
            content: "Hi".into(),
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        "req-fail-schema".into(),
        true,
        None,
        None,
        None,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn ai_generate_schema_doc_falls_back_when_selection_invalid() {
    let (test, mock) =
        TestAppState::with_wiremock_ai_options(many_tables_mock_options()).await;
    let (_, conn_id) = test.save_and_connect("schema-fallback").await;

    mock.mount_chat_completion_text_once("not-valid-json").await;
    mock.mount_chat_completion_text_once("# Fallback doc").await;

    let doc = ai_generate_schema_doc_impl(&test.state, conn_id, "app".into())
        .await
        .unwrap();
    assert!(doc.contains("Fallback doc"));
}

#[tokio::test]
async fn build_connections_context_chinese_header() {
    let test = TestAppState::new().await;
    test.save_connection("zh-cfg").await;
    let ctx = build_connections_context(&test.state, "zh-CN").await;
    assert!(ctx.contains("可用的数据库连接"));
}

#[tokio::test]
async fn execute_db_tool_list_databases() {
    let test = TestAppState::with_tables().await;
    test.save_connection("db-tool").await;
    let tool = ToolCall {
        id: "ld".into(),
        name: "list_databases".into(),
        arguments: r#"{"config_id":"db-tool"}"#.into(),
    };
    let out = execute_db_tool(&test.state, &tool).await;
    assert!(out.contains("app"));
}

#[tokio::test]
async fn ai_analyze_queries_all_history() {
    let (test, mock) = TestAppState::with_wiremock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("all-hist").await;
    test.state
        .store
        .add_query_history(crate::store::QueryHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            connection_id: conn_id,
            database: "app".into(),
            sql: "SELECT 2".into(),
            executed_at: chrono::Utc::now(),
            execution_time_ms: 2,
            rows_affected: Some(1),
            success: true,
            error_message: None,
        })
        .await
        .unwrap();

    mock.mount_chat_completion_text(
        r#"{"summary":"ok","categories":[],"insights":[],"frequentTables":[],"recommendations":[]}"#,
    )
    .await;

    let analysis = ai_analyze_queries_impl(&test.state, None).await.unwrap();
    assert_eq!(analysis.summary, "ok");
}

#[tokio::test]
async fn ai_generate_sql_pins_current_table() {
    let (test, mock) = TestAppState::with_wiremock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("pin-table").await;
    mock.mount_chat_stream_text("SELECT * FROM users").await;

    let (cb, _) = collecting_stream_callback();
    ai_generate_sql_impl(
        &test.state,
        cb,
        conn_id,
        "app".into(),
        "show users".into(),
        "req-pin".into(),
        Some("users".into()),
        None,
        None,
        Some(vec!["orders".into()]),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn ai_save_config_impl_persists_wiremock_provider() {
    let (test, mock) = TestAppState::with_wiremock_ai().await;
    let cfg = crate::testing::ai_wiremock::openai_wiremock_config(&mock);
    ai_save_config_impl(&test.state, cfg).await.unwrap();
    assert!(ai_get_config_impl(&test.state).await.unwrap().is_some());
}

#[tokio::test]
async fn ai_generate_schema_doc_strips_markdown_fence() {
    let (test, mock) = TestAppState::with_wiremock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("md-doc").await;
    mock.mount_chat_completion_text("```markdown\n# Title\n```").await;

    let doc = ai_generate_schema_doc_impl(&test.state, conn_id, "app".into())
        .await
        .unwrap();
    assert!(doc.contains("# Title"));
}

#[tokio::test]
async fn ai_chat_stream_http_error_propagates() {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    let (test, mock) = TestAppState::with_wiremock_ai().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", "Bearer test-api-key"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock.server)
        .await;

    let (cb, _) = collecting_stream_callback();
    assert!(
        ai_chat_impl(
            &test.state,
            cb,
            None,
            None,
            vec![ChatMessage {
                role: MessageRole::User,
                content: "Hi".into(),
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            }],
            "req-err".into(),
            false,
            None,
            None,
            None,
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn ai_chat_non_db_tool_returns_pending_message() {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    let (test, mock) = TestAppState::with_wiremock_ai().await;
    let ask_sse = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_q","function":{"name":"ask_questions","arguments":"{}"}}]}}]}

data: {"choices":[{"finish_reason":"tool_calls"}]}

data: [DONE]

"#;
    let answer_sse = r#"data: {"choices":[{"delta":{"content":"done"}}]}

data: {"choices":[{"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}

data: [DONE]

"#;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", "Bearer test-api-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(ask_sse),
        )
        .up_to_n_times(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", "Bearer test-api-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(answer_sse),
        )
        .mount(&mock.server)
        .await;

    let (cb, _) = collecting_stream_callback();
    ai_chat_impl(
        &test.state,
        cb,
        None,
        None,
        vec![ChatMessage {
            role: MessageRole::User,
            content: "Pick one".into(),
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        "req-ask".into(),
        false,
        None,
        None,
        None,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn workflow_execute_ai_step_with_wiremock() {
    use crate::workflow::{WorkflowDefinition, WorkflowStep};

    let (test, mock) = TestAppState::with_wiremock_ai().await;
    test.state.workflow_registry.load_all().await.unwrap();
    mock.mount_chat_completion_text("workflow output").await;

    let wf = WorkflowDefinition {
        id: "exec-wf".into(),
        name: "Exec".into(),
        description: String::new(),
        version: None,
        author: None,
        variables: vec![],
        steps: vec![WorkflowStep::Ai {
            id: "ai1".into(),
            prompt: "Say hello".into(),
            timeout_secs: None,
            on_error: None,
        }],
        output: None,
        timeout_secs: None,
        error_handling: None,
    };
    test.state.workflow_registry.save_workflow(&wf).await.unwrap();

    let result = workflow_execute_impl(
        &test.state,
        "exec-wf".into(),
        serde_json::json!({}),
        None,
    )
    .await
    .unwrap();
    assert!(result.final_output.contains("workflow output"));

    let history = test.state.workflow_history.list(Some("exec-wf")).await;
    assert_eq!(history.len(), 1);
}

#[tokio::test]
async fn ai_generate_sql_recent_queries_chinese_label() {
    let (test, mock) = TestAppState::with_wiremock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("zh-sql").await;
    let mut settings = test.state.store.get_settings().await;
    settings.language = "zh-CN".into();
    test.state.store.save_settings(settings).await.unwrap();
    mock.mount_chat_stream_text("SELECT 1").await;

    let (cb, _) = collecting_stream_callback();
    ai_generate_sql_impl(
        &test.state,
        cb,
        conn_id,
        "app".into(),
        "查询".into(),
        "req-zh".into(),
        None,
        Some(vec!["SELECT 1".into()]),
        None,
        None,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn ai_generate_schema_doc_small_schema_single_step() {
    let (test, mock) = TestAppState::with_wiremock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("small-doc").await;
    mock.mount_chat_completion_text("# Small schema doc").await;

    let doc = ai_generate_schema_doc_impl(&test.state, conn_id, "app".into())
        .await
        .unwrap();
    assert!(doc.contains("Small schema"));
}

#[tokio::test]
async fn ai_diagnose_connection_missing_config_errors() {
    let (test, _mock) = TestAppState::with_wiremock_ai().await;
    assert!(
        ai_diagnose_connection_impl(
            &test.state,
            "missing".into(),
            "connection refused".into(),
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn workflow_execute_missing_id_errors() {
    let test = TestAppState::new().await;
    assert!(
        workflow_execute_impl(
            &test.state,
            "nope".into(),
            serde_json::json!({}),
            None,
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn ai_chat_connected_english_schema_message() {
    let (test, mock) = TestAppState::with_wiremock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("en-chat").await;
    mock.mount_chat_stream_text("Hello").await;

    let (cb, _) = collecting_stream_callback();
    ai_chat_impl(
        &test.state,
        cb,
        Some(conn_id),
        Some("app".into()),
        vec![ChatMessage {
            role: MessageRole::User,
            content: "Hi".into(),
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        "req-en".into(),
        true,
        None,
        None,
        None,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn workflow_history_clear_after_execute() {
    use crate::workflow::{WorkflowDefinition, WorkflowStep};

    let (test, mock) = TestAppState::with_wiremock_ai().await;
    test.state.workflow_registry.load_all().await.unwrap();
    mock.mount_chat_completion_text("done").await;

    let wf = WorkflowDefinition {
        id: "hist-wf".into(),
        name: "Hist".into(),
        description: String::new(),
        version: None,
        author: None,
        variables: vec![],
        steps: vec![WorkflowStep::Ai {
            id: "a".into(),
            prompt: "x".into(),
            timeout_secs: None,
            on_error: None,
        }],
        output: None,
        timeout_secs: None,
        error_handling: None,
    };
    test.state.workflow_registry.save_workflow(&wf).await.unwrap();
    workflow_execute_impl(&test.state, "hist-wf".into(), serde_json::json!({}), None)
        .await
        .unwrap();
    let cleared = test
        .state
        .workflow_history
        .clear(Some("hist-wf"))
        .await
        .unwrap();
    assert_eq!(cleared, 1);
}

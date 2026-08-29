//! `commands/ai` tests backed by in-process [`MockAiProvider`] (no HTTP).

use std::sync::{Arc, Mutex};

use datazen_ai_api::{ChatMessage, MessageRole, ToolCall};

use super::*;
use crate::testing::app_state::TestAppState;
use crate::testing::mock_ai_provider::mock_ai_config;

fn collecting_stream_callback() -> (StreamCallback, Arc<Mutex<Vec<(String, bool)>>>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let log_c = log.clone();
    let cb: StreamCallback = Arc::new(move |request_id, result| {
        let done = result.as_ref().map(|c| c.done).unwrap_or(false);
        log_c.lock().unwrap().push((request_id.to_string(), done));
    });
    (cb, log)
}

#[tokio::test]
async fn mock_provider_resolve_ai_and_validate() {
    let (test, mock) = TestAppState::with_mock_ai().await;

    let (provider, cfg) = resolve_ai(&test.state).await.unwrap();
    assert_eq!(provider.display_name(), "Mock LLM");
    assert_eq!(cfg.model, "mock-model");
    assert!(mock.is_initialized());

    ai_validate_config_impl(&test.state, mock_ai_config())
        .await
        .unwrap();

    mock.set_validate_ok(false);
    assert!(ai_validate_config_impl(&test.state, mock_ai_config())
        .await
        .is_err());
}

#[tokio::test]
async fn mock_provider_save_get_delete_config() {
    let (test, mock) = TestAppState::with_mock_ai().await;

    let cfg = mock_ai_config();
    ai_save_config_impl(&test.state, cfg.clone()).await.unwrap();
    assert!(mock.is_initialized());

    let loaded = ai_get_config_impl(&test.state).await.unwrap().unwrap();
    assert_eq!(loaded.model, "mock-model");

    ai_delete_config_impl(&test.state).await.unwrap();
    assert!(!mock.is_initialized());
    assert!(ai_get_config_impl(&test.state).await.unwrap().is_none());
}

#[tokio::test]
async fn mock_provider_diagnose_error_parses_json() {
    let (test, mock) = TestAppState::with_mock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("mock-diag").await;

    mock.push_text(
        r#"{"explanation":"bad column","suggestedSql":"SELECT id FROM users","changes":[]}"#,
    );

    let result = ai_diagnose_error_impl(
        &test.state,
        conn_id,
        "app".into(),
        "SELECT bad FROM users".into(),
        "column bad does not exist".into(),
    )
    .await
    .unwrap();

    assert_eq!(result.explanation, "bad column");
    assert_eq!(
        result.suggested_sql.as_deref(),
        Some("SELECT id FROM users")
    );
    assert_eq!(mock.call_count(), 1);
    let req = mock.last_request().unwrap();
    assert!(req
        .messages
        .iter()
        .any(|m| m.content.contains("bad column") || m.content.contains("column bad")));
}

#[tokio::test]
async fn mock_provider_analyze_explain() {
    let (test, mock) = TestAppState::with_mock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("mock-explain").await;

    mock.push_text(
        r#"{"summary":"Seq scan","bottlenecks":[{"node":"Seq Scan","description":"full scan","severity":"high"}],"suggestions":[{"description":"Add index","sql":"CREATE INDEX ON users(id)","impact":"high"}]}"#,
    );

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
}

#[tokio::test]
async fn mock_provider_parse_filter() {
    let (test, mock) = TestAppState::with_mock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("mock-filter").await;

    mock.push_text(r#"[{"column":"id","operator":"gt","value":1}]"#);

    let filters = ai_parse_filter_impl(
        &test.state,
        conn_id,
        "app".into(),
        "users".into(),
        "id > 1".into(),
    )
    .await
    .unwrap();

    use crate::services::query_executor::FilterOperator;
    assert_eq!(filters.len(), 1);
    assert!(matches!(filters[0].operator, FilterOperator::Gt));
}

#[tokio::test]
async fn mock_provider_schema_doc_and_connection_diag() {
    let (test, mock) = TestAppState::with_mock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("mock-schema").await;

    mock.push_text("# Users\n\nTable docs");
    let doc = ai_generate_schema_doc_impl(&test.state, conn_id, "app".into())
        .await
        .unwrap();
    assert!(doc.contains("Users"));

    mock.push_text(
        r#"{"diagnosis":"Auth failed","possibleCauses":["bad password"],"solutions":[{"description":"Reset","command":null}],"category":"auth"}"#,
    );
    test.save_connection("mock-conn-diag").await;
    let diag = ai_diagnose_connection_impl(
        &test.state,
        "mock-conn-diag".into(),
        "password authentication failed".into(),
    )
    .await
    .unwrap();
    assert_eq!(diag.category, "auth");
}

#[tokio::test]
async fn mock_provider_analyze_queries() {
    let (test, mock) = TestAppState::with_mock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("mock-hist").await;

    test.state
        .store
        .add_query_history(crate::store::QueryHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            connection_id: "mock-hist".into(),
            database: "app".into(),
            schema: None,
            sql: "SELECT 1".into(),
            executed_at: chrono::Utc::now(),
            execution_time_ms: 1,
            rows_affected: Some(1),
            success: true,
            error_message: None,
        })
        .await
        .unwrap();

    mock.push_text(
        r#"{"summary":"Light","categories":[{"name":"SELECT","count":1,"examples":["SELECT 1"]}],"insights":[],"frequentTables":[],"recommendations":[]}"#,
    );

    let analysis = ai_analyze_queries_impl(&test.state, Some(conn_id))
        .await
        .unwrap();
    assert_eq!(analysis.summary, "Light");
}

#[tokio::test]
async fn mock_provider_generate_sql_stream() {
    let (test, mock) = TestAppState::with_mock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("mock-nl2sql").await;
    mock.push_stream_text("SELECT COUNT(*) FROM users");

    let (cb, log) = collecting_stream_callback();
    let rid = ai_generate_sql_impl(
        &test.state,
        cb,
        conn_id,
        "app".into(),
        "count users".into(),
        "req-mock-sql".into(),
        Some("users".into()),
        None,
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(rid, "req-mock-sql");
    assert!(!log.lock().unwrap().is_empty());
    assert!(mock.call_count() >= 1);
}

#[tokio::test]
async fn mock_provider_chat_stream() {
    let (test, mock) = TestAppState::with_mock_ai_tables().await;
    let (_, conn_id) = test.save_and_connect("mock-chat").await;
    mock.push_stream_text("Hello from mock LLM");

    let (cb, _) = collecting_stream_callback();
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
        "req-mock-chat".into(),
        true,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(rid, "req-mock-chat");
    let req = mock.last_request().unwrap();
    assert!(req.messages.iter().any(|m| m.role == MessageRole::User));
}

#[tokio::test]
async fn mock_provider_chat_tool_loop_then_text() {
    let (test, mock) = TestAppState::with_mock_ai_tables().await;
    test.save_connection("mock-tool-cfg").await;

    mock.push_stream_tool_then_text(
        ToolCall {
            id: "call_1".into(),
            name: "list_connections".into(),
            arguments: "{}".into(),
        },
        "Here are your connections.",
    );

    let (cb, log) = collecting_stream_callback();
    ai_chat_impl(
        &test.state,
        cb,
        None,
        None,
        vec![ChatMessage {
            role: MessageRole::User,
            content: "List my DBs".into(),
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        "req-tool-loop".into(),
        true,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    // Two LLM rounds: tool call + final answer.
    assert!(mock.call_count() >= 2);
    assert!(!log.lock().unwrap().is_empty());
}

#[tokio::test]
async fn mock_provider_complete_error_surfaces() {
    let (test, mock) = TestAppState::with_mock_ai().await;
    test.save_connection("err-cfg").await;
    mock.push_error("upstream 503");

    let err =
        ai_diagnose_connection_impl(&test.state, "err-cfg".into(), "connection refused".into())
            .await
            .unwrap_err();
    assert!(
        err.to_string().contains("503") || err.to_string().to_lowercase().contains("fail"),
        "unexpected err: {err}"
    );
}

#[tokio::test]
async fn mock_provider_listed_in_get_providers() {
    let (test, _) = TestAppState::with_mock_ai().await;
    let list = ai_get_providers_impl(&test.state).await.unwrap();
    assert!(
        list.iter().any(|p| p.display_name == "Mock LLM"),
        "expected Mock LLM in provider list: {:?}",
        list.iter().map(|p| &p.display_name).collect::<Vec<_>>()
    );
}

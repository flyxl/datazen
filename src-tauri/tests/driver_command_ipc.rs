//! PoC: pure-IPC spec migration — driver command discovery + execution.
//!
//! Replaces `e2e/specs/driver-commands.ts` (WebDriver → Tauri IPC indirection).
//! Calls the same `*_impl` handlers the IPC layer uses, via `TestAppState` mock
//! driver — no browser, no WebDriver, no beforeSuite overhead.
//!
//! Run:
//! ```text
//! CARGO_TARGET_DIR=target-e2e-ipc cargo test -p datazen \
//!   --features test-harness --test driver_command_ipc
//! ```

use datazen::test_harness::{
    connect, execute_driver_command, get_connection_commands, get_connections,
    ExecuteDriverCommandRequest, TestAppState,
};

/// Mirrors E2E: get_connections → connect → get_connection_commands → query.
#[tokio::test]
async fn discovers_commands_from_connection_and_executes_query() {
    let test = TestAppState::new().await;
    test.save_connection("ipc-poc-discover").await;

    let conns = get_connections(&test.state).await.unwrap();
    assert!(!conns.is_empty(), "expected at least one saved connection");

    let connection_id = conns[0].id.clone();
    let db_session_id = connect(&test.state, connection_id)
        .await
        .expect("connect should return a runtime dbSessionId");

    let definitions = get_connection_commands(&test.state, db_session_id.clone())
        .await
        .unwrap();
    assert!(
        definitions.iter().any(|d| d.id == "query"),
        "command_definitions should include 'query', got: {:?}",
        definitions.iter().map(|d| &d.id).collect::<Vec<_>>()
    );

    let result = execute_driver_command(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(db_session_id),
            driver_type: None,
            command: "query".into(),
            input: serde_json::json!({ "sql": "SELECT 1 AS n" }),
            database: None,
            schema: None,
        },
    )
    .await
    .unwrap();
    assert!(
        result.data.is_object(),
        "query command should return structured data"
    );
}

/// Mirrors E2E: execute_driver_command with unknown command → validation error.
#[tokio::test]
async fn rejects_unsupported_driver_command() {
    let test = TestAppState::new().await;
    let (_, db_session_id) = test.save_and_connect("ipc-poc-unknown-cmd").await;

    let err = execute_driver_command(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(db_session_id),
            driver_type: None,
            command: "not-a-real-command".into(),
            input: serde_json::json!({}),
            database: None,
            schema: None,
        },
    )
    .await
    .unwrap_err();

    assert!(
        err.contains("Unsupported driver command"),
        "unexpected error: {err}"
    );
}

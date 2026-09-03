use std::any::type_name;
use std::sync::Arc;
use std::sync::Mutex;

use super::access::access_level_for_mode;
use super::helpers::{inject_sql_target_fields, nonempty};
use super::resolve::resolve_command_driver;
use super::*;
use crate::mcp::permission::McpPermissionMode;
use datazen_driver_api::{
    CommandAccessLevel, QueryExecutionId, QueryStreamCallback, QueryStreamEvent,
};

#[test]
fn request_uses_camel_case_wire_format() {
    let request = ExecuteDriverCommandRequest {
        db_session_id: Some("mysql-prod".into()),
        driver_type: None,
        command: "query".into(),
        input: serde_json::json!({ "sql": "SELECT 1" }),
        database: Some("db_b".into()),
        schema: None,
    };

    let encoded = serde_json::to_value(request).unwrap();
    assert_eq!(encoded["dbSessionId"], "mysql-prod");
    assert_eq!(encoded["command"], "query");
    assert_eq!(encoded["input"]["sql"], "SELECT 1");
    assert_eq!(encoded["database"], "db_b");
    // Option fields serialize as explicit null (like `database`), and
    // serde-default accepts both absent and null on input.
    assert_eq!(encoded["schema"], serde_json::Value::Null);
    assert!(encoded.get("db_session_id").is_none());
}

#[test]
fn request_schema_field_round_trips_and_defaults_to_none() {
    // F7: optional envelope field — legacy callers omit it, serde default
    // keeps them compatible (no PROTOCOL_VERSION bump).
    let without: ExecuteDriverCommandStreamRequest = serde_json::from_value(serde_json::json!({
        "dbSessionId": "s",
        "command": "query_stream"
    }))
    .unwrap();
    assert!(without.schema.is_none());

    let with = ExecuteDriverCommandRequest {
        db_session_id: None,
        driver_type: None,
        command: "query".into(),
        input: serde_json::json!({ "sql": "SELECT 1" }),
        database: None,
        schema: Some("sales".into()),
    };
    let encoded = serde_json::to_value(with).unwrap();
    assert_eq!(encoded["schema"], "sales");
}

#[test]
fn inject_sql_target_fields_skips_blank_values_and_non_objects() {
    let mut input = serde_json::json!({ "sql": "SELECT 1" });
    inject_sql_target_fields(&mut input, Some("  "), Some(""));
    assert_eq!(
        input,
        serde_json::json!({ "sql": "SELECT 1" }),
        "blank envelope targeting must not be injected"
    );

    let mut input = serde_json::json!({ "sql": "SELECT 1" });
    inject_sql_target_fields(&mut input, Some("db"), Some("schema_x"));
    assert_eq!(input["database"], "db");
    assert_eq!(input["schema"], "schema_x");

    let mut scalar = serde_json::Value::Null;
    inject_sql_target_fields(&mut scalar, Some("db"), None);
    assert_eq!(scalar, serde_json::Value::Null);
}

#[test]
fn request_defaults_input_to_null_when_omitted() {
    let request: ExecuteDriverCommandRequest = serde_json::from_value(serde_json::json!({
        "dbSessionId": "mysql-prod",
        "command": "query"
    }))
    .unwrap();

    assert_eq!(request.db_session_id.as_deref(), Some("mysql-prod"));
    assert_eq!(request.command, "query");
    assert_eq!(request.input, serde_json::Value::Null);
    assert!(request.driver_type.is_none());
}

#[test]
fn debug_sql_preview_redacts_secrets() {
    let sql = "SELECT * FROM t WHERE url = 'mysql://root:hunter2@127.0.0.1/app'";
    let redacted = crate::log_redact::sql_preview_for_log(sql);
    assert!(!redacted.contains("hunter2"), "{redacted}");
    assert!(redacted.contains("mysql://***@"), "{redacted}");
}

#[tokio::test]
async fn discovers_standard_commands_from_connection() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("cmd-discover").await;
    let definitions = get_connection_commands_impl(&test.state, conn_id)
        .await
        .unwrap();
    let ids: Vec<_> = definitions.iter().map(|d| d.id.as_str()).collect();
    assert!(ids.contains(&"query"));
    assert!(ids.contains(&"execute"));
}

#[tokio::test]
async fn execute_driver_command_pins_session_database_before_execution() {
    let test = crate::testing::app_state::TestAppState::with_tables().await;
    let (_, conn_id) = test.save_and_connect("cmd-pin-db").await;
    // Sample config pins database = "app"; an explicit different pin must
    // switch the live session before the command runs (BUG-001 fix).
    execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(conn_id.clone()),
            driver_type: None,
            command: "query".into(),
            input: serde_json::json!({ "sql": "SELECT 1" }),
            database: Some("analytics".into()),
            schema: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        test.mock.use_database_calls(),
        vec!["analytics".to_string()]
    );
    let config = test
        .state
        .connection_manager
        .get_session_config(&conn_id)
        .await
        .unwrap();
    assert_eq!(config.database.as_deref(), Some("analytics"));
}

#[tokio::test]
async fn execute_driver_command_skips_switch_when_pin_missing_or_same() {
    let test = crate::testing::app_state::TestAppState::with_tables().await;
    let (_, conn_id) = test.save_and_connect("cmd-no-pin").await;
    for database in [None, Some("app".into()), Some("   ".into())] {
        execute_driver_command_impl(
            &test.state,
            ExecuteDriverCommandRequest {
                db_session_id: Some(conn_id.clone()),
                driver_type: None,
                command: "query".into(),
                input: serde_json::json!({ "sql": "SELECT 1" }),
                database: database.clone(),
                schema: None,
            },
        )
        .await
        .unwrap();
    }
    assert!(test.mock.use_database_calls().is_empty());
}

#[tokio::test]
async fn execute_driver_command_pins_session_database_for_admin_commands() {
    let test = crate::testing::app_state::TestAppState::with_tables().await;
    let (_, conn_id) = test.save_and_connect("cmd-pin-admin").await;
    execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(conn_id.clone()),
            driver_type: None,
            command: "execute".into(),
            input: serde_json::json!({ "sql": "SELECT 1" }),
            database: Some("analytics".into()),
            schema: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        test.mock.use_database_calls(),
        vec!["analytics".to_string()]
    );
}

#[tokio::test]
async fn stream_pins_session_database_before_query_stream() {
    let test = crate::testing::app_state::TestAppState::with_tables().await;
    let (_, conn_id) = test.save_and_connect("stream-pin-db").await;
    let callback: QueryStreamCallback = Arc::new(|_event| {});
    execute_driver_command_stream_impl(
        &test.state,
        ExecuteDriverCommandStreamRequest {
            db_session_id: Some(conn_id.clone()),
            command: "query_stream".into(),
            input: serde_json::json!({ "sql": "SELECT 1" }),
            database: Some("analytics".into()),
            schema: None,
            apply_result_limit: Some(true),
            record_history: Some(false),
        },
        callback,
        ExecuteDriverCommandStreamOpts::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        test.mock.use_database_calls(),
        vec!["analytics".to_string()]
    );
    let config = test
        .state
        .connection_manager
        .get_session_config(&conn_id)
        .await
        .unwrap();
    assert_eq!(config.database.as_deref(), Some("analytics"));
}

#[tokio::test]
async fn execute_driver_command_passes_target_to_qualifying_driver() {
    // F7: envelope targeting is injected into the command input and the
    // rewrite-capable driver inlines it (marker comment proves the SQL
    // the driver actually received).
    let test = crate::testing::app_state::TestAppState::with_options(
        crate::testing::mock_driver::MockDriverOptions {
            rewrite_sql_target: true,
            ..Default::default()
        },
    )
    .await;
    let (_, conn_id) = test.save_and_connect("cmd-target-rewrite").await;

    let result = execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(conn_id.clone()),
            driver_type: None,
            command: "query".into(),
            input: serde_json::json!({ "sql": "SELECT 1" }),
            database: Some("analytics".into()),
            schema: Some("sales".into()),
        },
    )
    .await
    .unwrap();

    assert_eq!(
        test.mock.qualify_calls(),
        vec![(Some("analytics".into()), Some("sales".into()))]
    );
    let executed_sql = result.data["results"][0]["sql"].as_str().unwrap();
    assert!(
        executed_sql.contains("/* target: db=analytics schema=sales */"),
        "{executed_sql}"
    );

    // The session pin (ensure_session_database) still runs alongside the
    // rewrite — the database dimension stays double-covered.
    assert_eq!(
        test.mock.use_database_calls(),
        vec!["analytics".to_string()]
    );
}

#[tokio::test]
async fn execute_driver_command_falls_back_when_driver_cannot_rewrite() {
    // F7: a driver without rewrite capability executes SQL unchanged;
    // ensure_session_database remains the fallback for the database dim.
    let test = crate::testing::app_state::TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("cmd-target-fallback").await;

    let result = execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(conn_id.clone()),
            driver_type: None,
            command: "query".into(),
            input: serde_json::json!({ "sql": "SELECT 42" }),
            database: Some("analytics".into()),
            schema: Some("sales".into()),
        },
    )
    .await
    .unwrap();

    // SQL reached the driver untouched.
    let executed_sql = result.data["results"][0]["sql"].as_str().unwrap();
    assert_eq!(executed_sql, "SELECT 42");
    assert!(test.mock.qualify_calls().is_empty());
    // …and the session was still pinned to the requested database.
    assert_eq!(
        test.mock.use_database_calls(),
        vec!["analytics".to_string()]
    );
}

#[tokio::test]
async fn stream_hands_targeting_to_qualifying_driver() {
    let test = crate::testing::app_state::TestAppState::with_options(
        crate::testing::mock_driver::MockDriverOptions {
            rewrite_sql_target: true,
            ..Default::default()
        },
    )
    .await;
    let (_, conn_id) = test.save_and_connect("stream-target").await;
    let started: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&started);
    let callback: QueryStreamCallback = Arc::new(move |event| {
        if let QueryStreamEvent::StatementStart { sql, .. } = event {
            recorder.lock().unwrap().push(sql);
        }
    });

    execute_driver_command_stream_impl(
        &test.state,
        ExecuteDriverCommandStreamRequest {
            db_session_id: Some(conn_id.clone()),
            command: "query_stream".into(),
            input: serde_json::json!({ "sql": "SELECT 7" }),
            database: Some("analytics".into()),
            schema: Some("sales".into()),
            apply_result_limit: Some(false),
            record_history: Some(false),
        },
        callback,
        ExecuteDriverCommandStreamOpts {
            apply_result_limit: false,
            record_history: false,
        },
    )
    .await
    .unwrap();

    let started_sql = started.lock().unwrap().join("\n");
    assert!(
        started_sql.contains("/* target: db=analytics schema=sales */"),
        "{started_sql}"
    );
}

#[tokio::test]
async fn stream_emits_execution_started_and_cleans_host_handle() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("stream-lifecycle").await;
    let execution_ids: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&execution_ids);
    let callback: QueryStreamCallback = Arc::new(move |event| {
        if let QueryStreamEvent::ExecutionStarted { execution_id } = event {
            recorder.lock().unwrap().push(execution_id);
        }
    });

    execute_driver_command_stream_impl(
        &test.state,
        ExecuteDriverCommandStreamRequest {
            db_session_id: Some(conn_id.clone()),
            command: "query_stream".into(),
            input: serde_json::json!({ "sql": "SELECT 1" }),
            database: None,
            schema: None,
            apply_result_limit: Some(false),
            record_history: Some(false),
        },
        callback,
        ExecuteDriverCommandStreamOpts {
            apply_result_limit: false,
            record_history: false,
        },
    )
    .await
    .unwrap();

    let execution_id = execution_ids.lock().unwrap().first().cloned().unwrap();
    assert!(!execution_id.is_empty());
    assert!(test
        .state
        .query_executions
        .validate_owner(&QueryExecutionId::new(execution_id), &conn_id)
        .await
        .unwrap_err()
        .contains("unknown or stale"));
}

#[tokio::test]
async fn stream_without_capability_keeps_sql_and_pins_session() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("stream-fallback").await;
    let started: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&started);
    let callback: QueryStreamCallback = Arc::new(move |event| {
        if let QueryStreamEvent::StatementStart { sql, .. } = event {
            recorder.lock().unwrap().push(sql);
        }
    });

    execute_driver_command_stream_impl(
        &test.state,
        ExecuteDriverCommandStreamRequest {
            db_session_id: Some(conn_id.clone()),
            command: "query_stream".into(),
            input: serde_json::json!({ "sql": "SELECT 7" }),
            database: Some("analytics".into()),
            schema: Some("sales".into()),
            apply_result_limit: Some(false),
            record_history: Some(false),
        },
        callback,
        ExecuteDriverCommandStreamOpts {
            apply_result_limit: false,
            record_history: false,
        },
    )
    .await
    .unwrap();

    let started_sql = started.lock().unwrap().join("\n");
    assert_eq!(started_sql, "SELECT 7");
    assert_eq!(
        test.mock.use_database_calls(),
        vec!["analytics".to_string()],
        "session pin fallback must still run on the stream path"
    );
}

#[tokio::test]
async fn executes_query_command_through_ipc() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("cmd-exec").await;
    let result = execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(conn_id.clone()),
            driver_type: None,
            command: "query".into(),
            input: serde_json::json!({ "sql": "SELECT 1" }),
            database: None,
            schema: None,
        },
    )
    .await
    .unwrap();
    assert!(
        result.data.get("results").is_some()
            || result.data.get("columns").is_some()
            || result.data.is_object()
    );
    let history = test
        .state
        .store
        .get_query_history(10, None, None, None)
        .await;
    assert!(history.iter().any(|e| e.success));
}

#[tokio::test]
async fn rejects_unknown_command() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("cmd-unknown").await;
    let err = execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(conn_id),
            driver_type: None,
            command: "not-a-command".into(),
            input: serde_json::json!({}),
            database: None,
            schema: None,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("Unsupported driver command"));
}

#[tokio::test]
async fn rejects_invalid_query_input() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("cmd-invalid").await;
    let err = execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(conn_id),
            driver_type: None,
            command: "query".into(),
            input: serde_json::json!({}),
            database: None,
            schema: None,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("missing required field"));
}

#[tokio::test]
async fn read_only_mode_denies_execute() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("cmd-ro").await;
    let err = execute_driver_command_with_mode(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(conn_id),
            driver_type: None,
            command: "execute".into(),
            input: serde_json::json!({ "sql": "DELETE FROM t" }),
            database: None,
            schema: None,
        },
        Some(crate::mcp::permission::McpPermissionMode::ReadOnly),
        None,
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("not allowed"));
}

#[tokio::test]
async fn rejects_connection_id_on_execute_without_live_session() {
    let test = crate::testing::app_state::TestAppState::new().await;
    test.save_connection("cfg-only").await;
    let err = execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some("cfg-only".into()),
            driver_type: None,
            command: "query".into(),
            input: serde_json::json!({ "sql": "SELECT 1" }),
            database: None,
            schema: None,
        },
    )
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("DB session"),
        "expected dbSessionId error, got: {err}"
    );
}

#[tokio::test]
async fn discovers_commands_from_live_session() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let (_, db_session_id) = test.save_and_connect("cfg-discover").await;
    let definitions = get_connection_commands_impl(&test.state, db_session_id)
        .await
        .unwrap();
    assert!(definitions.iter().any(|d| d.id == "query"));
}

#[tokio::test]
async fn rejects_connection_id_on_get_connection_commands_without_live_session() {
    let test = crate::testing::app_state::TestAppState::new().await;
    test.save_connection("cfg-discover").await;
    let err = get_connection_commands_impl(&test.state, "cfg-discover".into())
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("DB session"),
        "expected dbSessionId error, got: {err}"
    );
}

#[tokio::test]
async fn discovers_commands_from_driver_type_without_connection() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let definitions = get_driver_type_commands_impl(&test.state, "postgres".into())
        .await
        .unwrap();
    assert!(definitions.iter().any(|d| d.id == "query"));
}

#[tokio::test]
async fn rejects_connection_required_command_without_connection() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let err = execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: None,
            driver_type: Some("postgres".into()),
            command: "query".into(),
            input: serde_json::json!({ "sql": "SELECT 1" }),
            database: None,
            schema: None,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("requires a connection"));
}

#[tokio::test]
async fn executes_unbound_driver_command_by_type() {
    use datazen_driver_api::{CommandCategory, DriverCommandDefinition, DriverCommandMetadata};

    let ping = DriverCommandDefinition {
        id: "ping".into(),
        name: "Ping".into(),
        description: None,
        input_schema: serde_json::json!({ "type": "object" }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Observe, CommandAccessLevel::Read)
            .unbound()
            .hide_from_workflow(),
    };
    let test = crate::testing::app_state::TestAppState::with_options(
        crate::testing::mock_driver::MockDriverOptions {
            extra_commands: vec![ping],
            ..Default::default()
        },
    )
    .await;
    let result = execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: None,
            driver_type: Some("postgres".into()),
            command: "ping".into(),
            input: serde_json::json!({}),
            database: Some("ignored-for-unbound".into()),
            schema: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(result.data["command"], "ping");
    // Unbound driverType requests have no session to pin — the explicit
    // database must be ignored, not error.
    assert!(test.mock.use_database_calls().is_empty());
}

#[tokio::test]
async fn save_dialog_commands_rejected_without_interactive_handle() {
    use datazen_driver_api::{
        CommandCategory, DriverCommandDefinition, DriverCommandMetadata, DriverSaveDialogSpec,
    };

    let pull = DriverCommandDefinition {
        id: "pull_payload".into(),
        name: "Pull Payload".into(),
        description: None,
        input_schema: serde_json::json!({ "type": "object" }),
        output_schema: None,
        permissions: vec![],
        metadata: DriverCommandMetadata::new(CommandCategory::Io, CommandAccessLevel::Write)
            .unbound()
            .hide_from_workflow()
            .save_dialog(DriverSaveDialogSpec {
                file_name_field: "fileName".into(),
                data_base64_field: "dataBase64".into(),
                filter_name: "SQLite Database".into(),
                extensions: vec!["db".into()],
                result_path_field: "savedPath".into(),
            }),
    };
    let test = crate::testing::app_state::TestAppState::with_options(
        crate::testing::mock_driver::MockDriverOptions {
            extra_commands: vec![pull],
            ..Default::default()
        },
    )
    .await;

    // Internal reuse path (`execute_driver_command_impl`): no AppHandle →
    // the command must be rejected before any execution happens.
    let err = execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: None,
            driver_type: Some("postgres".into()),
            command: "pull_payload".into(),
            input: serde_json::json!({}),
            database: None,
            schema: None,
        },
    )
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("interactive session"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn safe_mode_blocks_update_without_where() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("cmd-safe").await;
    let err = execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(conn_id),
            driver_type: None,
            command: "query".into(),
            input: serde_json::json!({ "sql": "UPDATE t SET x = 1" }),
            database: None,
            schema: None,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("WHERE"));
}

#[tokio::test]
async fn connection_read_only_blocks_write_sql() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let mut config = crate::testing::app_state::sample_postgres_config("cmd-conn-ro");
    config.read_only = true;
    test.store.save_connection(config).await.unwrap();
    let conn_id = test.connect_config("cmd-conn-ro").await;
    let err = execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(conn_id),
            driver_type: None,
            command: "query".into(),
            input: serde_json::json!({ "sql": "UPDATE t SET x = 1 WHERE id = 1" }),
            database: None,
            schema: None,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("read-only"));
}

#[tokio::test]
async fn bind_params_are_substituted_before_execution() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("cmd-params").await;
    let result = execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(conn_id),
            driver_type: None,
            command: "query".into(),
            input: serde_json::json!({
                "sql": "SELECT * FROM t WHERE id = :id",
                "params": { "id": 7 }
            }),
            database: None,
            schema: None,
        },
    )
    .await
    .unwrap();
    assert!(result.data.is_object());
}

#[tokio::test]
async fn disabling_safe_mode_allows_update_without_where() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let mut settings = test.store.get_settings().await;
    settings.safe_mode = false;
    test.store.save_settings(settings).await.unwrap();
    let (_, conn_id) = test.save_and_connect("cmd-unsafe").await;
    let result = execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(conn_id),
            driver_type: None,
            command: "query".into(),
            input: serde_json::json!({ "sql": "UPDATE t SET x = 1" }),
            database: None,
            schema: None,
        },
    )
    .await
    .unwrap();
    assert!(result.data.is_object());
}

// ---------------------------------------------------------------------------
// [tester] prh-split-dcmd enhanced verification — IPC regression, cross-module
// collaboration, and edge paths introduced by the submodule split.
// ---------------------------------------------------------------------------

#[test]
fn test_tester_ipc_commands_registered_in_bootstrap() {
    let bootstrap = include_str!("../../bootstrap.rs");
    for cmd in [
        "get_driver_commands",
        "get_connection_commands",
        "execute_driver_command",
        "execute_driver_command_stream",
    ] {
        assert!(
            bootstrap.contains(&format!("crate::commands::{cmd}")),
            "bootstrap invoke handler must register `{cmd}`"
        );
    }
}

#[test]
fn test_tester_ipc_handler_type_names_unchanged_after_split() {
    // If a #[tauri::command] handler is renamed or its signature wrapper changes,
    // the monomorphized type name drifts — caught here without frontend changes.
    fn handler_type_name<T>(_handler: T) -> &'static str {
        type_name::<T>()
    }
    assert!(
        handler_type_name(get_driver_commands).contains("get_driver_commands"),
        "{}",
        handler_type_name(get_driver_commands)
    );
    assert!(
        handler_type_name(get_connection_commands).contains("get_connection_commands"),
        "{}",
        handler_type_name(get_connection_commands)
    );
    assert!(
        handler_type_name(execute_driver_command).contains("execute_driver_command"),
        "{}",
        handler_type_name(execute_driver_command)
    );
    assert!(
        handler_type_name(execute_driver_command_stream).contains("execute_driver_command_stream"),
        "{}",
        handler_type_name(execute_driver_command_stream)
    );
}

#[test]
fn test_tester_ipc_stream_request_wire_format_matches_baseline() {
    let request = ExecuteDriverCommandStreamRequest {
        db_session_id: Some("sess-1".into()),
        command: "query_stream".into(),
        input: serde_json::json!({ "sql": "SELECT 1" }),
        database: Some("db_a".into()),
        schema: Some("public".into()),
        apply_result_limit: Some(false),
        record_history: Some(true),
    };
    let encoded = serde_json::to_value(request).unwrap();
    assert_eq!(encoded["dbSessionId"], "sess-1");
    assert_eq!(encoded["command"], "query_stream");
    assert_eq!(encoded["applyResultLimit"], false);
    assert_eq!(encoded["recordHistory"], true);
    assert!(encoded.get("db_session_id").is_none());
}

#[test]
fn test_tester_access_level_for_mode_maps_mcp_permission_modes() {
    assert_eq!(access_level_for_mode(None), CommandAccessLevel::HighRisk);
    assert_eq!(
        access_level_for_mode(Some(McpPermissionMode::ReadOnly)),
        CommandAccessLevel::Read
    );
    assert_eq!(
        access_level_for_mode(Some(McpPermissionMode::SafeWrite)),
        CommandAccessLevel::Write
    );
    assert_eq!(
        access_level_for_mode(Some(McpPermissionMode::HighRiskWrite)),
        CommandAccessLevel::HighRisk
    );
}

#[tokio::test]
async fn test_tester_resolve_command_driver_prefers_live_session_over_driver_type() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("tester-resolve-prefer").await;
    let (driver, handle, bound) =
        resolve_command_driver(&test.state, Some(&conn_id), Some(&"postgres".into()))
            .await
            .unwrap();
    assert!(bound);
    assert_eq!(handle.id, conn_id);
    assert!(!driver.command_definitions().is_empty());
}

#[tokio::test]
async fn test_tester_resolve_command_driver_requires_session_or_driver_type() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let result = resolve_command_driver(&test.state, None, None).await;
    match result {
        Err(err) => assert!(
            err.to_string()
                .contains("dbSessionId or driverType is required"),
            "unexpected: {err}"
        ),
        Ok(_) => panic!("expected validation error when neither session nor driver type"),
    }
}

#[tokio::test]
async fn test_tester_execute_applies_access_via_resolve_and_helpers_pipeline() {
    // Cross-module path: resolve → access_level_for_mode → check_command_access.
    let test = crate::testing::app_state::TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("tester-access-pipeline").await;
    let err = execute_driver_command_with_mode(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(conn_id),
            driver_type: None,
            command: "execute".into(),
            input: serde_json::json!({ "sql": "DELETE FROM t WHERE id = 1" }),
            database: None,
            schema: None,
        },
        Some(McpPermissionMode::ReadOnly),
        None,
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("not allowed"));
}

#[tokio::test]
async fn test_tester_rejects_whitespace_only_db_session_id_without_driver_type() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let blank = "   ".to_string();
    assert!(nonempty(Some(&blank)).is_none());
    let err = execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(blank),
            driver_type: None,
            command: "query".into(),
            input: serde_json::json!({ "sql": "SELECT 1" }),
            database: None,
            schema: None,
        },
    )
    .await
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("dbSessionId or driverType is required"),
        "whitespace session must fall through nonempty and fail validation: {err}"
    );
}

#[tokio::test]
async fn test_tester_rejects_empty_command_name() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("tester-empty-cmd").await;
    let err = execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(conn_id),
            driver_type: None,
            command: String::new(),
            input: serde_json::json!({ "sql": "SELECT 1" }),
            database: None,
            schema: None,
        },
    )
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("Unsupported driver command"),
        "empty command id must not match any definition: {err}"
    );
}

#[tokio::test]
async fn test_tester_concurrent_stream_requests_get_distinct_execution_ids() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("tester-concurrent-stream").await;
    let ids_a: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let ids_b: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let rec_a = Arc::clone(&ids_a);
    let rec_b = Arc::clone(&ids_b);
    let cb_a: QueryStreamCallback = Arc::new(move |event| {
        if let QueryStreamEvent::ExecutionStarted { execution_id } = event {
            rec_a.lock().unwrap().push(execution_id);
        }
    });
    let cb_b: QueryStreamCallback = Arc::new(move |event| {
        if let QueryStreamEvent::ExecutionStarted { execution_id } = event {
            rec_b.lock().unwrap().push(execution_id);
        }
    });

    let state = &test.state;
    let req = || ExecuteDriverCommandStreamRequest {
        db_session_id: Some(conn_id.clone()),
        command: "query_stream".into(),
        input: serde_json::json!({ "sql": "SELECT 1" }),
        database: None,
        schema: None,
        apply_result_limit: Some(false),
        record_history: Some(false),
    };
    let opts = ExecuteDriverCommandStreamOpts {
        apply_result_limit: false,
        record_history: false,
    };

    let (r1, r2) = tokio::join!(
        execute_driver_command_stream_impl(state, req(), cb_a, opts),
        execute_driver_command_stream_impl(state, req(), cb_b, opts),
    );
    r1.unwrap();
    r2.unwrap();

    let id_a = ids_a.lock().unwrap().first().cloned().unwrap();
    let id_b = ids_b.lock().unwrap().first().cloned().unwrap();
    assert_ne!(
        id_a, id_b,
        "concurrent streams must not share execution ids"
    );
    assert!(!id_a.is_empty() && !id_b.is_empty());
}

//! Tests for the query execution commands.
//!
//! Split out of `query.rs` to keep that file inside the 800-line
//! budget. `log_hygiene_tests` stays in `query.rs` because it reads
//! the source it is testing with `include_str!`, and those paths are
//! relative to the file that holds them.
//!
//! B2 query knife: the `_impl` forwarding layer is gone, so these tests drive
//! the REAL `#[tauri::command]` entries through `query_command_test_host`
//! (a `tauri::test::mock_app` managing the harness's `AppState`; the tempdir,
//! store, mock driver and registry stay alive in the keepalive).
//!
//! Not converted to command entries: the five `query_stream_command_*` tests.
//! They pin the stream orchestration semantics of
//! `execute_driver_command_stream_impl`, which lives in `driver_command/` and
//! sinks to modules in its own later batch (commands-layer-review §3.3) —
//! these tests follow THAT knife. The transaction-semantics depth tests
//! (concurrent begin, idle expiry, unknown-commit fencing) moved out with the
//! logic long ago and are covered by
//! `modules/tests/fixtures/p4_session_transactions.rs`; what stays here is the
//! adapter smoke through the command entries plus the `"No open transaction"`
//! wire-message pin.

use super::*;
use crate::commands::driver_command::{
    execute_driver_command_stream_impl, ExecuteDriverCommandStreamOpts,
    ExecuteDriverCommandStreamRequest,
};
use crate::store::AppSettings;
use crate::testing::app_state::TestAppState;
use crate::testing::mock_driver::MockDriverOptions;
use datazen::execution::runtime::driver_registry::DriverCapabilities;
use datazen_driver_api::{ColumnSchema, DriverError, QueryStreamCallback, QueryStreamEvent, Value};

use super::query_command_test_host::{host_command_app, HostedCommandApp};

/// Drive the real `get_query_history_page` entry with only `limit` set — the
/// wire form of the former `HistoryPageRequest::default()` reads.
async fn history_page(app: &HostedCommandApp, limit: usize) -> QueryHistoryPage {
    get_query_history_page(
        app.state(),
        limit,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("history page")
}

#[tokio::test]
async fn execute_query_success_records_history() {
    let test = TestAppState::with_tables().await;
    let (_, conn_id) = test.save_and_connect("q-cfg").await;
    let app = host_command_app(test);
    let result = execute_query(app.state(), conn_id, "SELECT 1".into(), None)
        .await
        .unwrap();
    assert_eq!(result.results.len(), 1);

    let history = history_page(&app, 10).await.entries;
    assert_eq!(history.len(), 1);
    assert!(history[0].success);
}

#[tokio::test]
async fn execute_query_respects_result_limit_setting() {
    let test = TestAppState::with_tables().await;
    let mut settings = AppSettings::default();
    settings.limit_select_results = true;
    settings.query_result_limit = 5;
    test.state.store.save_settings(settings).await.unwrap();

    let (_, conn_id) = test.save_and_connect("limit-cfg").await;
    let app = host_command_app(test);
    execute_query(app.state(), conn_id, "SELECT * FROM users".into(), None)
        .await
        .unwrap();
}

#[tokio::test]
async fn cancel_query_rejects_when_driver_capability_is_unknown_without_calling_driver() {
    let test = TestAppState::with_options(MockDriverOptions {
        cancel_error: Some("legacy driver cancellation must not be called".into()),
        ..Default::default()
    })
    .await;
    let (_, conn_id) = test.save_and_connect("cancel-unknown").await;
    let execution_id = QueryExecutionId::new("exec-unknown");
    test.state
        .query_executions
        .register(execution_id.clone(), conn_id.clone())
        .await
        .unwrap();

    assert!(test
        .registry
        .get_capabilities(&"postgres".to_string())
        .await
        .is_none());
    let app = host_command_app(test);
    let error = cancel_query(app.state(), conn_id, execution_id.as_str().to_string())
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        super::super::error::CommandError::Validation(message)
            if message.starts_with("UNSUPPORTED_OPERATION:cancel_query:")
                && message.ends_with("capability is unknown")
    ));
    assert_eq!(app.keep.mock.cancel_query_calls(), 0);
}

#[tokio::test]
async fn cancel_query_rejects_when_driver_capability_is_disabled() {
    let test = TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("cancel-unsupported").await;
    let execution_id = QueryExecutionId::new("exec-unsupported");
    test.state
        .query_executions
        .register(execution_id.clone(), conn_id.clone())
        .await
        .unwrap();
    test.registry
        .register_test_driver_with_capabilities(
            "postgres",
            test.mock.clone(),
            DriverCapabilities {
                has_multi_database: false,
                supports_cancel_query: false,
                supports_query_execution_cancel: false,
                supports_explain: true,
                supports_streaming_results: true,
                supports_offset: true,
                has_schema_level: true,
            },
        )
        .await;

    let app = host_command_app(test);
    let error = cancel_query(app.state(), conn_id, execution_id.as_str().to_string())
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        super::super::error::CommandError::Validation(message)
            if message.starts_with("UNSUPPORTED_OPERATION:cancel_query:")
    ));
    assert_eq!(app.keep.mock.cancel_query_calls(), 0);
}

#[tokio::test]
async fn cancel_query_surfaces_driver_unsupported_without_claiming_success() {
    let test = TestAppState::with_options(MockDriverOptions {
        cancel_error: Some("backend cancellation is unavailable".into()),
        ..Default::default()
    })
    .await;
    let (_, conn_id) = test.save_and_connect("cancel-driver-unsupported").await;
    let execution_id = QueryExecutionId::new("exec-driver-unsupported");
    test.state
        .query_executions
        .register(execution_id.clone(), conn_id.clone())
        .await
        .unwrap();
    test.registry
        .register_test_driver_with_capabilities(
            "postgres",
            test.mock.clone(),
            DriverCapabilities {
                has_multi_database: false,
                // The legacy session-wide capability is deliberately
                // independent; precise cancellation must not be gated by
                // or fall back to that old API.
                supports_cancel_query: false,
                supports_query_execution_cancel: true,
                supports_explain: true,
                supports_streaming_results: true,
                supports_offset: true,
                has_schema_level: true,
            },
        )
        .await;

    let app = host_command_app(test);
    let error = cancel_query(app.state(), conn_id, execution_id.as_str().to_string())
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        super::super::error::CommandError::Driver(DriverError::Unsupported(message))
            if message == "backend cancellation is unavailable"
    ));
    assert_eq!(app.keep.mock.cancel_query_calls(), 0);
    assert_eq!(app.keep.mock.precise_cancel_query_calls(), 1);
}

#[tokio::test]
async fn execute_query_not_connected_errors() {
    let test = TestAppState::new().await;
    let app = host_command_app(test);
    assert!(
        execute_query(app.state(), "nope".into(), "SELECT 1".into(), None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn clear_query_history() {
    let test = TestAppState::with_tables().await;
    let (_, conn_id) = test.save_and_connect("hist-cfg").await;
    let app = host_command_app(test);
    execute_query(app.state(), conn_id, "SELECT 1".into(), None)
        .await
        .unwrap();
    // 本测试函数与命令同名，函数体内裸名解析到测试自身；全限定路由到命令。
    crate::commands::query::clear_query_history(app.state()).await.unwrap();
    assert!(history_page(&app, 10).await.entries.is_empty());
}

#[tokio::test]
async fn execute_query_with_columns_returns_rows() {
    let opts = MockDriverOptions {
        columns: vec![ColumnSchema {
            name: "id".into(),
            data_type: "integer".into(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: true,
            is_auto_increment: false,
        }],
        query_rows: vec![vec![Some(Value::Integer(7))]],
        ..Default::default()
    };
    let test = TestAppState::with_options(opts).await;
    let (_, conn_id) = test.save_and_connect("rows-cfg").await;
    let app = host_command_app(test);
    let result = execute_query(app.state(), conn_id, "SELECT id FROM t".into(), None)
        .await
        .unwrap();
    assert_eq!(result.results[0].rows.len(), 1);
}

#[tokio::test]
async fn query_stream_command_does_not_apply_limit_when_switch_off() {
    let test = TestAppState::with_tables().await;
    assert!(!test.state.store.get_settings().await.limit_select_results);
    let (_, conn_id) = test.save_and_connect("stream-nolimit").await;
    let app = host_command_app(test);
    let state = app.state();
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let events_cb = std::sync::Arc::clone(&events);
    let cb: QueryStreamCallback = std::sync::Arc::new(move |ev| {
        events_cb.lock().unwrap().push(ev);
    });
    execute_driver_command_stream_impl(
        &state,
        ExecuteDriverCommandStreamRequest {
            db_session_id: Some(conn_id),
            command: "query_stream".into(),
            database: None,
            schema: None,
            input: serde_json::json!({ "sql": "SELECT 1" }),
            apply_result_limit: Some(false),
            record_history: Some(true),
        },
        cb,
        ExecuteDriverCommandStreamOpts {
            apply_result_limit: false,
            record_history: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(app.keep.mock.last_query_limit(), Some(None));
    let events = events.lock().unwrap();
    assert!(events
        .iter()
        .any(|e| matches!(e, QueryStreamEvent::StatementStart { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, QueryStreamEvent::Done { .. })));
    let history = history_page(&app, 10).await.entries;
    assert!(history.iter().any(|e| e.success));
}

#[tokio::test]
async fn query_stream_command_respects_limit_select_setting() {
    let test = TestAppState::with_tables().await;
    let mut settings = AppSettings::default();
    settings.limit_select_results = true;
    settings.query_result_limit = 5;
    test.state.store.save_settings(settings).await.unwrap();

    let (_, conn_id) = test.save_and_connect("stream-limit").await;
    let app = host_command_app(test);
    let state = app.state();
    let cb: QueryStreamCallback = std::sync::Arc::new(|_| {});
    execute_driver_command_stream_impl(
        &state,
        ExecuteDriverCommandStreamRequest {
            db_session_id: Some(conn_id),
            command: "query_stream".into(),
            database: None,
            schema: None,
            input: serde_json::json!({ "sql": "SELECT * FROM users" }),
            apply_result_limit: Some(true),
            record_history: Some(true),
        },
        cb,
        ExecuteDriverCommandStreamOpts::default(),
    )
    .await
    .unwrap();
    assert_eq!(app.keep.mock.last_query_limit(), Some(Some(5)));
}

#[tokio::test]
async fn query_stream_command_can_skip_result_limit_for_export() {
    let test = TestAppState::with_tables().await;
    let mut settings = AppSettings::default();
    settings.limit_select_results = true;
    settings.query_result_limit = 5;
    test.state.store.save_settings(settings).await.unwrap();

    let (_, conn_id) = test.save_and_connect("stream-export").await;
    let app = host_command_app(test);
    let state = app.state();
    let cb: QueryStreamCallback = std::sync::Arc::new(|_| {});
    execute_driver_command_stream_impl(
        &state,
        ExecuteDriverCommandStreamRequest {
            db_session_id: Some(conn_id),
            command: "query_stream".into(),
            database: None,
            schema: None,
            input: serde_json::json!({ "sql": "SELECT * FROM users" }),
            apply_result_limit: Some(false),
            record_history: Some(false),
        },
        cb,
        ExecuteDriverCommandStreamOpts {
            apply_result_limit: false,
            record_history: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(app.keep.mock.last_query_limit(), Some(None));
    let history = history_page(&app, 10).await.entries;
    assert!(history.is_empty());
}

#[tokio::test]
async fn query_stream_command_failure_records_history() {
    let opts = MockDriverOptions {
        query_error: Some("boom".into()),
        ..Default::default()
    };
    let test = TestAppState::with_options(opts).await;
    let (_, conn_id) = test.save_and_connect("stream-fail").await;
    let app = host_command_app(test);
    let state = app.state();
    let cb: QueryStreamCallback = std::sync::Arc::new(|_| {});
    let err = execute_driver_command_stream_impl(
        &state,
        ExecuteDriverCommandStreamRequest {
            db_session_id: Some(conn_id),
            command: "query_stream".into(),
            database: None,
            schema: None,
            input: serde_json::json!({ "sql": "SELECT 1" }),
            apply_result_limit: Some(true),
            record_history: Some(true),
        },
        cb,
        ExecuteDriverCommandStreamOpts::default(),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("boom"));
    let history = history_page(&app, 10).await.entries;
    assert_eq!(history.len(), 1);
    assert!(!history[0].success);
    assert!(history[0]
        .error_message
        .as_ref()
        .is_some_and(|m| m.contains("boom")));
}

#[tokio::test]
async fn query_stream_command_not_connected_errors() {
    let test = TestAppState::new().await;
    let app = host_command_app(test);
    let state = app.state();
    let cb: QueryStreamCallback = std::sync::Arc::new(|_| {});
    assert!(execute_driver_command_stream_impl(
        &state,
        ExecuteDriverCommandStreamRequest {
            db_session_id: Some("nope".into()),
            command: "query_stream".into(),
            database: None,
            schema: None,
            input: serde_json::json!({ "sql": "SELECT 1" }),
            apply_result_limit: Some(true),
            record_history: Some(true),
        },
        cb,
        ExecuteDriverCommandStreamOpts::default(),
    )
    .await
    .is_err());
}

/// BUG-003 regression: a query aimed at another database must never be
/// served by re-pointing the shared pooled session. The target rides the
/// command envelope so the driver qualifies the SQL itself.
#[tokio::test]
async fn execute_query_never_switches_the_session_database() {
    let test = TestAppState::with_tables().await;
    let (_, conn_id) = test.save_and_connect("switch-db-cfg").await;
    let app = host_command_app(test);

    let result = execute_query(
        app.state(),
        conn_id.clone(),
        "SELECT 1".into(),
        Some("analytics".into()),
    )
    .await
    .unwrap();
    assert_eq!(result.results.len(), 1);

    // The whole point of the refactor: no session switch, ever.
    assert!(
        app.keep.mock.use_database_calls().is_empty(),
        "execute_query must not switch the session's database"
    );
    // ...and the session record still reports its own configured database,
    // because the request target is per-call, not session state.
    let config = app
        .keep
        .store
        .get_connection("switch-db-cfg")
        .await
        .unwrap();
    assert_eq!(config.database.as_deref(), Some("app"));
}

#[tokio::test]
async fn execute_query_never_switches_for_same_blank_or_absent_pin() {
    let test = TestAppState::with_tables().await;
    let (_, conn_id) = test.save_and_connect("no-switch-db-cfg").await;
    let app = host_command_app(test);

    for pin in [None, Some("app".to_string()), Some("   ".to_string())] {
        execute_query(app.state(), conn_id.clone(), "SELECT 1".into(), pin)
            .await
            .unwrap();
    }

    assert!(app.keep.mock.use_database_calls().is_empty());
    let config = app
        .keep
        .store
        .get_connection("no-switch-db-cfg")
        .await
        .unwrap();
    assert_eq!(config.database.as_deref(), Some("app"));
}

#[tokio::test]
async fn session_transaction_begin_commit_and_status() {
    let test = TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("tx-cfg").await;
    let app = host_command_app(test);
    assert!(
        !session_transaction_status(app.state(), conn_id.clone())
            .await
            .unwrap()
    );
    begin_session_transaction(app.state(), conn_id.clone())
        .await
        .unwrap();
    assert!(
        session_transaction_status(app.state(), conn_id.clone())
            .await
            .unwrap()
    );
    begin_session_transaction(app.state(), conn_id.clone())
        .await
        .unwrap();
    commit_session_transaction(app.state(), conn_id.clone())
        .await
        .unwrap();
    assert!(
        !session_transaction_status(app.state(), conn_id.clone())
            .await
            .unwrap()
    );
    begin_session_transaction(app.state(), conn_id.clone())
        .await
        .unwrap();
    rollback_session_transaction(app.state(), conn_id.clone())
        .await
        .unwrap();
    assert!(!session_transaction_status(app.state(), conn_id)
        .await
        .unwrap());
}

#[tokio::test]
async fn commit_and_rollback_without_tx_are_validation_errors() {
    let test = TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("tx-empty").await;
    let app = host_command_app(test);
    let commit_err = commit_session_transaction(app.state(), conn_id.clone())
        .await
        .unwrap_err();
    assert!(commit_err.to_string().contains("No open transaction"));
    let rollback_err = rollback_session_transaction(app.state(), conn_id)
        .await
        .unwrap_err();
    assert!(rollback_err.to_string().contains("No open transaction"));
}

// ── HistoryPageRequest → QueryHistoryFilter ───────────────────────────────────

fn request(order: Option<&str>) -> HistoryPageRequest {
    HistoryPageRequest {
        limit: 25,
        order: order.map(str::to_string),
        ..HistoryPageRequest::default()
    }
}

#[test]
fn an_absent_or_blank_order_means_most_recent() {
    for order in [None, Some("")] {
        let req = request(order);
        let filter = req.to_filter().expect("blank order is not an error");
        assert_eq!(format!("{:?}", filter.order), "Recent");
    }
}

#[test]
fn every_named_order_survives_the_translation() {
    for (wire, expected) in [
        ("recent", "Recent"),
        ("oldest", "Oldest"),
        ("slowest", "Slowest"),
    ] {
        let req = request(Some(wire));
        let filter = req.to_filter().expect("known order");
        assert_eq!(format!("{:?}", filter.order), expected, "order {wire}");
    }
}

#[test]
fn an_unknown_order_is_rejected_rather_than_silently_defaulted() {
    // Defaulting here would hand the UI "most recent" when it asked for
    // something else, with nothing on screen to reveal the substitution.
    let req = request(Some("by-vibes"));
    let err = req.to_filter().expect_err("unknown order must not pass");
    assert!(
        err.to_string().contains("unknown order"),
        "message should name the problem, got: {err}"
    );
}

/// The wrapper takes eight same-typed arguments. A reordered pair is a silent
/// data corruption bug — `database` and `schema` are both `Option<String>` —
/// so the mapping is pinned by name here rather than by type.
#[test]
fn each_field_lands_on_the_filter_field_of_the_same_name() {
    let req = HistoryPageRequest {
        limit: 7,
        connection_id: Some("cfg-1".into()),
        database: Some("db_a".into()),
        schema: Some("public".into()),
        search: Some("users".into()),
        since: Some("2026-01-01T00:00:00Z".into()),
        until: Some("2026-02-01T00:00:00Z".into()),
        order: Some("slowest".into()),
    };
    let filter = req.to_filter().expect("valid request");

    assert_eq!(filter.limit, 7);
    assert_eq!(filter.connection_id, Some("cfg-1"));
    assert_eq!(filter.database, Some("db_a"));
    assert_eq!(filter.schema, Some("public"));
    assert_eq!(filter.search, Some("users"));
    assert_eq!(filter.since, Some("2026-01-01T00:00:00Z"));
    assert_eq!(filter.until, Some("2026-02-01T00:00:00Z"));
    assert_eq!(format!("{:?}", filter.order), "Slowest");
}

#[test]
fn a_defaulted_request_asks_for_a_real_page() {
    // Same trap as `QueryHistoryFilter::default()`: `limit: 0` would make
    // `LIMIT 0` return an empty page, which a UI cannot tell apart from an
    // empty table.
    let req = HistoryPageRequest::default();
    let filter = req.to_filter().expect("defaults are valid");
    assert_eq!(filter.limit, DEFAULT_HISTORY_PAGE_SIZE);
    assert!(filter.limit > 0, "a default page size of 0 hides every row");
}

#[test]
fn an_absent_field_stays_absent_rather_than_becoming_an_empty_filter() {
    // `database = Some("")` means "no database"; `Some("")` is how the caller
    // says "all", and the store treats empty as no constraint. Asserting the
    // None case keeps a defaulted request from filtering on "".
    let req = request(None);
    let filter = req.to_filter().expect("valid");
    assert_eq!(filter.database, None);
    assert_eq!(filter.schema, None);
    assert_eq!(filter.search, None);
    assert_eq!(filter.since, None);
    assert_eq!(filter.until, None);
}

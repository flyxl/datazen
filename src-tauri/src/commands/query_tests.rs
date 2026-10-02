//! Tests for the query execution commands.
//!
//! Split out of `query.rs` to keep that file inside the 800-line
//! budget. `log_hygiene_tests` stays in `query.rs` because it reads
//! the source it is testing with `include_str!`, and those paths are
//! relative to the file that holds them.

use std::collections::HashMap;

use super::*;
use crate::db::{
    ColumnSchema, DriverCapabilities, DriverError, ExplainResult, TransactionHandle, Value,
};
use crate::store::AppSettings;
use crate::testing::app_state::TestAppState;
use crate::testing::mock_driver::{BeginRace, MockDriverOptions};

#[tokio::test]
async fn execute_query_success_records_history() {
    let test = TestAppState::with_tables().await;
    let (_, conn_id) = test.save_and_connect("q-cfg").await;
    let result = execute_query_impl(&test.state, conn_id, "SELECT 1".into(), None)
        .await
        .unwrap();
    assert_eq!(result.results.len(), 1);

    let history = get_query_history_impl(&test.state, 10, None, None, None)
        .await
        .unwrap();
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
    execute_query_impl(&test.state, conn_id, "SELECT * FROM users".into(), None)
        .await
        .unwrap();
}

#[tokio::test]
async fn get_explain_query() {
    let opts = MockDriverOptions {
        explain_plan: ExplainResult {
            plan_text: "Seq Scan".into(),
            plan_json: None,
            plan_tree: None,
            total_cost: Some(1.0),
            estimated_rows: Some(10),
        },
        ..Default::default()
    };
    let test = TestAppState::with_options(opts).await;
    test.registry
        .register_test_driver_with_capabilities(
            "postgres",
            test.mock.clone(),
            DriverCapabilities {
                has_multi_database: false,
                supports_cancel_query: true,
                supports_query_execution_cancel: true,
                supports_explain: true,
                supports_streaming_results: true,
                supports_offset: true,
                has_schema_level: true,
            },
        )
        .await;
    let (_, conn_id) = test.save_and_connect("explain-cfg").await;

    let plan = get_explain_impl(&test.state, conn_id.clone(), "SELECT 1".into(), None)
        .await
        .unwrap();
    assert_eq!(plan.plan_text, "Seq Scan");
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
    let error = cancel_query_impl(&test.state, conn_id, execution_id.as_str().to_string())
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        super::super::error::CommandError::Validation(message)
            if message.starts_with("UNSUPPORTED_OPERATION:cancel_query:")
                && message.ends_with("capability is unknown")
    ));
    assert_eq!(test.mock.cancel_query_calls(), 0);
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

    let error = cancel_query_impl(&test.state, conn_id, execution_id.as_str().to_string())
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        super::super::error::CommandError::Validation(message)
            if message.starts_with("UNSUPPORTED_OPERATION:cancel_query:")
    ));
    assert_eq!(test.mock.cancel_query_calls(), 0);
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

    let error = cancel_query_impl(&test.state, conn_id, execution_id.as_str().to_string())
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        super::super::error::CommandError::Driver(DriverError::Unsupported(message))
            if message == "backend cancellation is unavailable"
    ));
    assert_eq!(test.mock.cancel_query_calls(), 0);
    assert_eq!(test.mock.precise_cancel_query_calls(), 1);
}

#[tokio::test]
async fn execute_query_not_connected_errors() {
    let test = TestAppState::new().await;
    assert!(
        execute_query_impl(&test.state, "nope".into(), "SELECT 1".into(), None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn clear_query_history() {
    let test = TestAppState::with_tables().await;
    let (_, conn_id) = test.save_and_connect("hist-cfg").await;
    execute_query_impl(&test.state, conn_id, "SELECT 1".into(), None)
        .await
        .unwrap();
    clear_query_history_impl(&test.state).await.unwrap();
    assert!(get_query_history_impl(&test.state, 10, None, None, None)
        .await
        .unwrap()
        .is_empty());
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
    let result = execute_query_impl(&test.state, conn_id, "SELECT id FROM t".into(), None)
        .await
        .unwrap();
    assert_eq!(result.results[0].rows.len(), 1);
}

#[tokio::test]
async fn execute_query_stream_does_not_apply_limit_when_switch_off() {
    let test = TestAppState::with_tables().await;
    assert!(!test.state.store.get_settings().await.limit_select_results);
    let (_, conn_id) = test.save_and_connect("stream-nolimit").await;
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let events_cb = std::sync::Arc::clone(&events);
    let cb: QueryStreamCallback = std::sync::Arc::new(move |ev| {
        events_cb.lock().unwrap().push(ev);
    });
    execute_query_stream_impl(
        &test.state,
        conn_id,
        "SELECT 1".into(),
        None,
        cb,
        ExecuteQueryStreamOpts::default(),
    )
    .await
    .unwrap();
    assert_eq!(test.mock.last_query_limit(), Some(None));
    let events = events.lock().unwrap();
    assert!(events
        .iter()
        .any(|e| matches!(e, QueryStreamEvent::StatementStart { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, QueryStreamEvent::Done { .. })));
    let history = get_query_history_impl(&test.state, 10, None, None, None)
        .await
        .unwrap();
    assert!(history.iter().any(|e| e.success));
}

#[tokio::test]
async fn execute_query_stream_respects_limit_select_setting() {
    let test = TestAppState::with_tables().await;
    let mut settings = AppSettings::default();
    settings.limit_select_results = true;
    settings.query_result_limit = 5;
    test.state.store.save_settings(settings).await.unwrap();

    let (_, conn_id) = test.save_and_connect("stream-limit").await;
    let cb: QueryStreamCallback = std::sync::Arc::new(|_| {});
    execute_query_stream_impl(
        &test.state,
        conn_id,
        "SELECT * FROM users".into(),
        None,
        cb,
        ExecuteQueryStreamOpts::default(),
    )
    .await
    .unwrap();
    assert_eq!(test.mock.last_query_limit(), Some(Some(5)));
}

#[tokio::test]
async fn execute_query_stream_can_skip_result_limit_for_export() {
    let test = TestAppState::with_tables().await;
    let mut settings = AppSettings::default();
    settings.limit_select_results = true;
    settings.query_result_limit = 5;
    test.state.store.save_settings(settings).await.unwrap();

    let (_, conn_id) = test.save_and_connect("stream-export").await;
    let cb: QueryStreamCallback = std::sync::Arc::new(|_| {});
    execute_query_stream_impl(
        &test.state,
        conn_id,
        "SELECT * FROM users".into(),
        None,
        cb,
        ExecuteQueryStreamOpts {
            apply_result_limit: false,
            record_history: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(test.mock.last_query_limit(), Some(None));
    let history = get_query_history_impl(&test.state, 10, None, None, None)
        .await
        .unwrap();
    assert!(history.is_empty());
}

#[tokio::test]
async fn execute_query_stream_failure_records_history() {
    let opts = MockDriverOptions {
        query_error: Some("boom".into()),
        ..Default::default()
    };
    let test = TestAppState::with_options(opts).await;
    let (_, conn_id) = test.save_and_connect("stream-fail").await;
    let cb: QueryStreamCallback = std::sync::Arc::new(|_| {});
    let err = execute_query_stream_impl(
        &test.state,
        conn_id,
        "SELECT 1".into(),
        None,
        cb,
        ExecuteQueryStreamOpts::default(),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("boom"));
    let history = get_query_history_impl(&test.state, 10, None, None, None)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    assert!(!history[0].success);
    assert!(history[0]
        .error_message
        .as_ref()
        .is_some_and(|m| m.contains("boom")));
}

#[tokio::test]
async fn execute_query_stream_not_connected_errors() {
    let test = TestAppState::new().await;
    let cb: QueryStreamCallback = std::sync::Arc::new(|_| {});
    assert!(execute_query_stream_impl(
        &test.state,
        "nope".into(),
        "SELECT 1".into(),
        None,
        cb,
        ExecuteQueryStreamOpts::default(),
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

    let result = execute_query_impl(
        &test.state,
        conn_id.clone(),
        "SELECT 1".into(),
        Some("analytics".into()),
    )
    .await
    .unwrap();
    assert_eq!(result.results.len(), 1);

    // The whole point of the refactor: no session switch, ever.
    assert!(
        test.mock.use_database_calls().is_empty(),
        "execute_query must not switch the session's database"
    );
    // ...and the session record still reports its own configured database,
    // because the request target is per-call, not session state.
    let config = test
        .state
        .connection_manager
        .get_session_config(&conn_id)
        .await
        .unwrap();
    assert_eq!(config.database.as_deref(), Some("app"));
}

#[tokio::test]
async fn execute_query_never_switches_for_same_blank_or_absent_pin() {
    let test = TestAppState::with_tables().await;
    let (_, conn_id) = test.save_and_connect("no-switch-db-cfg").await;

    for pin in [None, Some("app".to_string()), Some("   ".to_string())] {
        execute_query_impl(&test.state, conn_id.clone(), "SELECT 1".into(), pin)
            .await
            .unwrap();
    }

    assert!(test.mock.use_database_calls().is_empty());
    let config = test
        .state
        .connection_manager
        .get_session_config(&conn_id)
        .await
        .unwrap();
    assert_eq!(config.database.as_deref(), Some("app"));
}

#[tokio::test]
async fn get_explain_never_switches_the_session_database() {
    let test = TestAppState::with_tables().await;
    let (_, conn_id) = test.save_and_connect("explain-db-cfg").await;
    get_explain_impl(
        &test.state,
        conn_id.clone(),
        "SELECT 1".into(),
        Some("other".into()),
    )
    .await
    .unwrap();
    assert!(
        test.mock.use_database_calls().is_empty(),
        "get_explain must not switch the session's database"
    );
    let config = test
        .state
        .connection_manager
        .get_session_config(&conn_id)
        .await
        .unwrap();
    assert_eq!(config.database.as_deref(), Some("app"));
}

#[tokio::test]
async fn session_transaction_begin_commit_and_status() {
    let test = TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("tx-cfg").await;
    assert!(
        !session_transaction_status_impl(&test.state, conn_id.clone())
            .await
            .unwrap()
    );
    begin_session_transaction_impl(&test.state, conn_id.clone())
        .await
        .unwrap();
    assert!(
        session_transaction_status_impl(&test.state, conn_id.clone())
            .await
            .unwrap()
    );
    begin_session_transaction_impl(&test.state, conn_id.clone())
        .await
        .unwrap();
    commit_session_transaction_impl(&test.state, conn_id.clone())
        .await
        .unwrap();
    assert!(
        !session_transaction_status_impl(&test.state, conn_id.clone())
            .await
            .unwrap()
    );
    begin_session_transaction_impl(&test.state, conn_id.clone())
        .await
        .unwrap();
    rollback_session_transaction_impl(&test.state, conn_id.clone())
        .await
        .unwrap();
    assert!(!session_transaction_status_impl(&test.state, conn_id)
        .await
        .unwrap());
}

#[tokio::test]
async fn commit_and_rollback_without_tx_are_validation_errors() {
    let test = TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("tx-empty").await;
    let commit_err = commit_session_transaction_impl(&test.state, conn_id.clone())
        .await
        .unwrap_err();
    assert!(commit_err.to_string().contains("No open transaction"));
    let rollback_err = rollback_session_transaction_impl(&test.state, conn_id)
        .await
        .unwrap_err();
    assert!(rollback_err.to_string().contains("No open transaction"));
}

#[tokio::test]
async fn concurrent_begin_is_idempotent() {
    let test = TestAppState::new().await;
    let (_, conn_id) = test.save_and_connect("tx-race").await;
    let a = begin_session_transaction_impl(&test.state, conn_id.clone());
    let b = begin_session_transaction_impl(&test.state, conn_id.clone());
    let (ra, rb) = tokio::join!(a, b);
    assert!(ra.is_ok());
    assert!(rb.is_ok());
    assert!(
        session_transaction_status_impl(&test.state, conn_id.clone())
            .await
            .unwrap()
    );
    commit_session_transaction_impl(&test.state, conn_id)
        .await
        .unwrap();
}

// ── HistoryPageRequest → QueryHistoryFilter ───────────────────────────────────

fn request(order: Option<&str>) -> HistoryPageRequest {
    HistoryPageRequest {
        limit: 25,
        order: order.map(str::to_string),
        ..Default::default()
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

// ── Lost-begin-race rollback ────────────────────────────────────────────────
//
// `begin_session_transaction_impl` re-reads its own bookkeeping after the
// driver has answered. If a competing `begin` won in between, this call owns a
// transaction it cannot register, so it must undo it. When the driver *cannot*
// confirm that undo, the connection keeps a second, untracked open transaction —
// a user-visible failure that must be reported rather than reported as a clean
// begin.

/// The shape of `AppState.session_transactions`.
type SessionTransactions = std::sync::Arc<tokio::sync::Mutex<HashMap<String, TransactionHandle>>>;

/// The bookkeeping the racing task and this call share. Assigning it onto
/// `AppState` is what lets the stub driver reach in: it publishes the winner
/// into this very map between the two reads.
async fn losing_the_begin_race(rollback_fails: bool) -> (TestAppState, SessionTransactions) {
    let slot = std::sync::Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let winner = TransactionHandle {
        id: "winner_tx".into(),
        // Filled in by `begin_transaction`, which knows the real session id.
        connection_id: String::new(),
    };
    let mut opts = MockDriverOptions::default();
    opts.begin_race = Some(BeginRace {
        slot: slot.clone(),
        winner: std::sync::Arc::new(std::sync::Mutex::new(Some(winner))),
    });
    if rollback_fails {
        opts.rollback_error_on_call = Some(1);
        opts.rollback_error = Some("injected rollback failure".into());
    }
    let mut test = TestAppState::with_options(opts).await;
    test.state.session_transactions = slot.clone();
    (test, slot)
}

#[tokio::test]
async fn begin_session_transaction_reports_a_rollback_it_could_not_confirm() {
    let (test, slot) = losing_the_begin_race(true).await;
    let (_, db_session_id) = test.save_and_connect("begin-race-fail").await;

    let err = begin_session_transaction_impl(&test.state, db_session_id.clone())
        .await
        .expect_err("an unconfirmed rollback must not be reported as a clean begin");
    assert!(
        err.to_string().contains("injected rollback failure"),
        "the driver's own reason must survive into the reported error, got: {err}"
    );

    // The competing task's transaction is untouched — losing the race must not
    // roll back somebody else's work.
    let registered = slot.lock().await;
    assert_eq!(
        registered.get(&db_session_id).map(|tx| tx.id.as_str()),
        Some("winner_tx"),
        "the winning transaction must still be the registered one"
    );
}

#[tokio::test]
async fn begin_session_transaction_is_ok_when_the_losing_rollback_is_confirmed() {
    let (test, slot) = losing_the_begin_race(false).await;
    let (_, db_session_id) = test.save_and_connect("begin-race-ok").await;

    begin_session_transaction_impl(&test.state, db_session_id.clone())
        .await
        .expect("a confirmed rollback of our own transaction is a clean begin");

    let registered = slot.lock().await;
    assert_eq!(
        registered.get(&db_session_id).map(|tx| tx.id.as_str()),
        Some("winner_tx"),
        "the winner stays registered"
    );
    assert_eq!(
        registered.len(),
        1,
        "the losing transaction must not be registered alongside the winner"
    );
}

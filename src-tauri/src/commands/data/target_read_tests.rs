//! The target-binding table read.
//!
//! The browser does not call `get_table_data`: it binds a target and reads
//! through it. That path puts the row count and the rows into **one**
//! statement batch, and it is the only place where a failed estimate can
//! take the rows down with it — so the round trips and the degradation are
//! pinned here.

use super::table_target::{self, TableReadRequest};
use super::*;
use crate::db::{CountStrategy, Value};
use crate::testing::app_state::TestAppState;
use crate::testing::mock_driver::MockDriverOptions;

fn request(connection_id: &str, conn_id: &str) -> TableReadRequest {
    TableReadRequest {
        manual_handle: None,
        context: RowChangeTableContext {
            connection_id: connection_id.into(),
            db_session_id: conn_id.into(),
            driver_type: "postgres".into(),
            database: Some("app".into()),
            schema: None,
            table: "users".into(),
        },
        page: 0,
        page_size: 10,
        filters: None,
        sorts: None,
        skip_count: None,
        filter_logic: None,
        count_strategy: None,
        default_order_columns: None,
    }
}

fn rows() -> Vec<Vec<Option<Value>>> {
    vec![vec![
        Some(Value::Integer(1)),
        Some(Value::String("alice".into())),
    ]]
}

#[tokio::test]
async fn the_count_and_the_rows_travel_in_one_batch() {
    let test = TestAppState::with_options(MockDriverOptions {
        count_total: 2,
        query_rows: rows(),
        ..Default::default()
    })
    .await;
    let (_, conn_id) = test.save_and_connect("target-read-exact").await;
    let before = test.mock.query_calls();

    let data = table_target::read(&test.state, request("target-read-exact", &conn_id))
        .await
        .expect("the read succeeds");

    assert_eq!(
        test.mock.query_calls() - before,
        1,
        "the count must not cost a second round trip"
    );
    assert_eq!(data.total_rows, Some(2));
    assert_eq!(data.total_rows_kind, Some(CountStrategy::Exact));
}

#[tokio::test]
async fn the_estimate_and_the_rows_travel_in_one_batch() {
    let test = TestAppState::with_options(MockDriverOptions {
        count_total: 2,
        count_strategy: Some(CountStrategy::Estimated),
        count_estimate_rows: Some(4242),
        query_rows: rows(),
        ..Default::default()
    })
    .await;
    let (_, conn_id) = test.save_and_connect("target-read-estimate").await;
    let before = test.mock.query_calls();

    let data = table_target::read(&test.state, request("target-read-estimate", &conn_id))
        .await
        .expect("the read succeeds");

    assert_eq!(
        test.mock.query_calls() - before,
        1,
        "the estimate must not cost a second round trip"
    );
    assert_eq!(data.total_rows, Some(4242));
    assert_eq!(data.total_rows_kind, Some(CountStrategy::Estimated));
}

#[tokio::test]
async fn a_failed_estimate_still_returns_the_rows() {
    let test = TestAppState::with_options(MockDriverOptions {
        count_total: 2,
        count_strategy: Some(CountStrategy::Estimated),
        count_estimate_rows: Some(4242),
        count_estimate_error: Some("the estimate is not available".into()),
        query_rows: rows(),
        ..Default::default()
    })
    .await;
    let (_, conn_id) = test.save_and_connect("target-read-estimate-fail").await;
    let before = test.mock.query_calls();

    let data = table_target::read(&test.state, request("target-read-estimate-fail", &conn_id))
        .await
        .expect("a failed estimate must not fail the read");

    assert_eq!(
        test.mock.query_calls() - before,
        2,
        "the failure path costs one more round trip — and only that path"
    );
    assert_eq!(data.rows.len(), 1, "the rows survive the failed estimate");
    assert_eq!(data.total_rows, None);
    assert_eq!(data.total_rows_kind, Some(CountStrategy::Unsupported));
}

#[tokio::test]
async fn a_count_that_is_not_a_number_leaves_the_kind_at_exact() {
    // The count was taken; it just did not come back as a number. The
    // answer is "the rows are there, the number is not".
    let test = TestAppState::with_options(MockDriverOptions {
        count_total_value: Some(Value::String("not a number".into())),
        query_rows: rows(),
        ..Default::default()
    })
    .await;
    let (_, conn_id) = test
        .save_and_connect("target-read-count-not-a-number")
        .await;

    let data = table_target::read(
        &test.state,
        request("target-read-count-not-a-number", &conn_id),
    )
    .await
    .expect("the read succeeds");

    assert_eq!(data.total_rows, None);
    assert_eq!(data.total_rows_kind, Some(CountStrategy::Exact));
}

#[tokio::test]
async fn a_driver_that_cannot_count_says_unsupported() {
    let test = TestAppState::with_options(MockDriverOptions {
        skip_count_query: true,
        query_rows: rows(),
        ..Default::default()
    })
    .await;
    let (_, conn_id) = test.save_and_connect("target-read-unsupported").await;
    let before = test.mock.query_calls();

    let data = table_target::read(&test.state, request("target-read-unsupported", &conn_id))
        .await
        .expect("the read succeeds");

    assert_eq!(test.mock.query_calls() - before, 1, "no count statement");
    assert_eq!(data.total_rows, None);
    assert_eq!(data.total_rows_kind, Some(CountStrategy::Unsupported));
}

/// PostgreSQL reports `-1` for a relation the planner never looked at.
/// That is "no estimate", not "minus one row".
///
/// This case was red once: the target-binding read carried
/// `total_rows: Some(-1)` next to `total_rows_kind: Some(Unsupported)`,
/// while the shared executor (`QueryExecutor::get_table_data`) carried
/// `total_rows: None` — the same DTO in two shapes, depending on which
/// host entry point served it, and a negative row count on the wire.
///
/// Both entry points now read the number through the shared layer's gate
/// (`table_browsing::estimated_count_cell`), so an unusable number cannot
/// travel as a row count whichever door the read came through.
#[tokio::test]
async fn a_negative_estimate_is_not_a_row_count() {
    let test = TestAppState::with_options(MockDriverOptions {
        count_total: 2,
        count_strategy: Some(CountStrategy::Estimated),
        count_estimate_rows: Some(-1),
        query_rows: rows(),
        ..Default::default()
    })
    .await;
    let (_, conn_id) = test.save_and_connect("target-read-estimate-negative").await;

    let data = table_target::read(
        &test.state,
        request("target-read-estimate-negative", &conn_id),
    )
    .await
    .expect("the read succeeds");

    assert_eq!(data.total_rows_kind, Some(CountStrategy::Unsupported));
    assert_eq!(
        data.total_rows, None,
        "a negative estimate must not travel as a row count"
    );
}

#[tokio::test]
async fn a_caller_that_reuses_the_total_gets_no_kind_at_all() {
    let test = TestAppState::with_options(MockDriverOptions {
        count_total: 2,
        query_rows: rows(),
        ..Default::default()
    })
    .await;
    let (_, conn_id) = test.save_and_connect("target-read-reuse").await;
    let before = test.mock.query_calls();

    let mut request = request("target-read-reuse", &conn_id);
    request.skip_count = Some(true);
    let data = table_target::read(&test.state, request)
        .await
        .expect("the read succeeds");

    assert_eq!(test.mock.query_calls() - before, 1);
    assert_eq!(data.total_rows, None);
    // Not "unsupported": the caller keeps the total it already had.
    assert_eq!(data.total_rows_kind, None);
}

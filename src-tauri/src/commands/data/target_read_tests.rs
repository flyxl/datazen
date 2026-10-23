//! The target-binding table read.
//!
//! The browser does not call `get_table_data`: it binds a target and reads
//! through it. That path puts the row count and the rows into **one**
//! statement batch, and it is the only place where a failed estimate can
//! take the rows down with it — so the round trips and the degradation are
//! pinned here.

use super::table_target::{self, TableReadRequest};
use super::*;
use crate::testing::app_state::TestAppState;
use crate::testing::mock_driver::MockDriverOptions;
use datazen_driver_api::{CountStrategy, Value};

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
        after_key: None,
        require_keyset: None,
        cursor_depth: None,
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

/// The paging half of the target read: what a page is read by, and what the
/// caller is told about it.
mod keyset {
    use super::*;
    use crate::services::SortCondition;
    use datazen_driver_api::keyset::{CursorValue, KeysetUnavailableReason, PagingMode};

    fn keyset_driver() -> MockDriverOptions {
        MockDriverOptions {
            columns: vec![
                datazen_driver_api::ColumnSchema {
                    name: "id".into(),
                    data_type: "integer".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: true,
                    is_auto_increment: false,
                },
                datazen_driver_api::ColumnSchema {
                    name: "name".into(),
                    data_type: "text".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
            ],
            primary_keys: vec!["id".into()],
            supports_keyset_pagination: true,
            count_total: 2,
            query_rows: rows(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn a_driver_without_the_capability_reads_by_offset_and_says_so() {
        let test = TestAppState::with_options(MockDriverOptions {
            columns: keyset_driver().columns,
            primary_keys: vec!["id".into()],
            count_total: 2,
            query_rows: rows(),
            ..Default::default()
        })
        .await;
        let (_, conn_id) = test.save_and_connect("target-read-no-keyset").await;

        let data = table_target::read(&test.state, request("target-read-no-keyset", &conn_id))
            .await
            .expect("the read succeeds");

        let paging = data.paging.expect("the read reports its paging");
        assert_eq!(paging.mode, PagingMode::Offset);
        assert_eq!(
            paging.unavailable_reason,
            Some(KeysetUnavailableReason::DriverUnsupported)
        );
    }

    #[tokio::test]
    async fn a_driver_with_the_capability_reports_the_boundary_of_the_next_page() {
        let test = TestAppState::with_options(keyset_driver()).await;
        let (_, conn_id) = test.save_and_connect("target-read-keyset").await;

        let mut request = request("target-read-keyset", &conn_id);
        // The mock answers one statement at a time, so the read asks for the
        // rows only — the count is not what this case is about.
        request.skip_count = Some(true);
        let data = table_target::read(&test.state, request)
            .await
            .expect("the read succeeds");

        let paging = data.paging.expect("the read reports its paging");
        assert_eq!(paging.mode, PagingMode::Keyset);
        assert_eq!(paging.unavailable_reason, None);
        assert_eq!(
            paging.next_after_key,
            Some(vec![CursorValue::Integer(1)]),
            "the boundary of the next page is the last row that was read"
        );
    }

    #[tokio::test]
    async fn a_caller_that_insists_on_keyset_paging_is_refused_when_it_cannot_have_it() {
        let test = TestAppState::with_options(MockDriverOptions {
            columns: keyset_driver().columns,
            primary_keys: vec!["id".into()],
            count_total: 2,
            query_rows: rows(),
            ..Default::default()
        })
        .await;
        let (_, conn_id) = test.save_and_connect("target-read-keyset-refused").await;

        let mut request = request("target-read-keyset-refused", &conn_id);
        request.require_keyset = Some(true);
        let error = table_target::read(&test.state, request)
            .await
            .expect_err("the read is refused");

        let message = error.to_string();
        assert!(
            message.starts_with("grid.paging.keysetUnavailable: "),
            "the prefix is part of the contract, got: {message}"
        );
    }

    /// A schema whose key column and whose text column are both **not
    /// nullable**, so that a sort on the text column is a total order once
    /// the key is appended to it.
    fn non_nullable_schema() -> datazen_driver_api::TableSchema {
        datazen_driver_api::TableSchema {
            table_name: "users".into(),
            columns: vec![
                datazen_driver_api::ColumnSchema {
                    name: "id".into(),
                    data_type: "integer".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: true,
                    is_auto_increment: false,
                },
                datazen_driver_api::ColumnSchema {
                    name: "name".into(),
                    data_type: "text".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
            ],
            primary_keys: vec!["id".into()],
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            check_constraints: Vec::new(),
            table_options: Default::default(),
        }
    }

    #[tokio::test]
    async fn a_boundary_that_does_not_fit_the_order_is_refused_as_an_invalidated_cursor() {
        // The boundary has one value; the order in force has two columns
        // (the user's sort and the appended key). The boundary belongs to
        // another sort, and using it would locate the wrong rows.
        let test = TestAppState::with_options(MockDriverOptions {
            columns: keyset_driver().columns,
            primary_keys: vec!["id".into()],
            table_schema: Some(non_nullable_schema()),
            supports_keyset_pagination: true,
            count_total: 2,
            query_rows: rows(),
            ..Default::default()
        })
        .await;
        let (_, conn_id) = test.save_and_connect("target-read-keyset-stale").await;

        let mut request = request("target-read-keyset-stale", &conn_id);
        request.sorts = Some(vec![SortCondition {
            column: "name".into(),
            descending: false,
        }]);
        request.after_key = Some(vec![CursorValue::Integer(1)]);
        request.require_keyset = Some(true);
        let error = table_target::read(&test.state, request)
            .await
            .expect_err("the read is refused");

        let message = error.to_string();
        assert!(
            message.starts_with("grid.paging.cursorInvalidated: "),
            "a stale cursor is the client's mistake and has its own code, got: {message}"
        );
    }

    #[tokio::test]
    async fn a_deep_boundary_chain_is_refused() {
        let test = TestAppState::with_options(keyset_driver()).await;
        let (_, conn_id) = test.save_and_connect("target-read-keyset-deep").await;

        let mut request = request("target-read-keyset-deep", &conn_id);
        request.cursor_depth = Some(100_000);
        let error = table_target::read(&test.state, request)
            .await
            .expect_err("the read is refused");

        let message = error.to_string();
        assert!(
            message.starts_with("grid.paging.cursorTooDeep: "),
            "the prefix is part of the contract, got: {message}"
        );
    }

    #[tokio::test]
    async fn an_old_caller_without_a_boundary_still_reads_the_page() {
        // The request shape did not change: a caller that knows nothing about
        // boundaries sends none and gets the page it always got.
        let test = TestAppState::with_options(keyset_driver()).await;
        let (_, conn_id) = test.save_and_connect("target-read-keyset-old").await;

        let mut request = request("target-read-keyset-old", &conn_id);
        request.after_key = None;
        request.skip_count = Some(true);
        let data = table_target::read(&test.state, request)
            .await
            .expect("the read succeeds");

        assert_eq!(data.rows.len(), 1);
        let paging = data.paging.expect("the read reports its paging");
        assert_eq!(paging.mode, PagingMode::Keyset);
    }

    #[tokio::test]
    async fn a_jumped_to_page_holds_that_page_and_hands_back_its_own_last_row() {
        // A caller that jumps (no boundary) lands on page 2. Reading that page
        // from the top of the table would return the first row under the label
        // of the third — silently wrong data, not a slow page. So the rows are
        // asserted, not just the report: `rows_start_at_offset` makes the mock
        // behave like a table whose pages differ from each other.
        let four_rows = || {
            vec![
                vec![Some(Value::Integer(1)), Some(Value::String("alice".into()))],
                vec![Some(Value::Integer(2)), Some(Value::String("bob".into()))],
                vec![Some(Value::Integer(3)), Some(Value::String("carol".into()))],
                vec![Some(Value::Integer(4)), Some(Value::String("dave".into()))],
            ]
        };
        let test = TestAppState::with_options(MockDriverOptions {
            rows_start_at_offset: true,
            count_total: 4,
            query_rows: four_rows(),
            ..keyset_driver()
        })
        .await;
        let (_, conn_id) = test.save_and_connect("target-read-keyset-jump").await;

        let mut request = request("target-read-keyset-jump", &conn_id);
        request.page = 2;
        request.page_size = 1;
        request.skip_count = Some(true);
        request.after_key = None;
        let data = table_target::read(&test.state, request)
            .await
            .expect("the read succeeds");

        assert_eq!(data.rows.len(), 1, "one row per page");
        assert!(
            matches!(data.rows[0][0], Some(Value::Integer(3))),
            "the jumped-to page holds its own rows, not the table head: {:?}",
            data.rows[0][0]
        );

        let paging = data.paging.expect("the read reports its paging");
        assert_eq!(paging.mode, PagingMode::Offset);
        assert_eq!(
            paging.unavailable_reason,
            Some(KeysetUnavailableReason::NotAdjacent),
            "a jump is offset-read and says why"
        );
        // The boundary is taken from the rows this page holds, so the next seek
        // continues after `carol` instead of repeating or skipping a row.
        assert_eq!(
            paging.next_after_key,
            Some(vec![CursorValue::Integer(3)]),
            "the boundary is this page's last row"
        );
    }
}

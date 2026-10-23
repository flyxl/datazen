//! The paging half of the query executor's tests: what a page is read by,
//! and what the statement looks like when it is read by seek.
//!
//! Two things are pinned here, and they are the two that can silently break
//! a product:
//!
//! * **the offset statement does not move.** Every table read that has no
//!   boundary sends the statement it has always sent, byte for byte. A
//!   driver that never opted into keyset paging must not see a different
//!   query.
//! * **the seek statement is the driver's.** The host places the condition
//!   the driver gave; it never writes a comparison of its own.

use super::keyset_refusal;
use super::*;
use datazen_driver_api::keyset::{CursorValue, KeysetUnavailableReason, OrderColumn, PagingMode};
use datazen_kernel::driver_registry::DriverRegistry;
use datazen_kernel::schema_cache::SchemaCache;

/// A column that is not nullable — the shape a key column has.
fn key_column(name: &str) -> ColumnSchema {
    ColumnSchema {
        nullable: false,
        ..make_column(name, true)
    }
}

fn plain_column(name: &str, nullable: bool) -> ColumnSchema {
    ColumnSchema {
        nullable,
        ..make_column(name, false)
    }
}

/// The order a read uses, as the paging decision sees it.
fn order_spec(columns: Vec<(String, bool)>, unique_key: Vec<String>) -> OrderSpec {
    OrderSpec {
        columns: columns
            .into_iter()
            .map(|(column, descending)| OrderColumn { column, descending })
            .collect(),
        source: crate::services::table_paging::OrderSource::ExplicitSort,
        unique_key,
    }
}

/// The golden statements of the offset path.
///
/// These are the statements the product sent before keyset paging existed.
/// They are compared **as whole strings**: a change in one of them is a
/// change in every query the product sends.
mod offset_statement_is_unchanged {
    use super::*;

    #[test]
    fn offset_path_sql_is_byte_identical_to_pre_keyset_golden() {
        let columns = vec![key_column("id"), plain_column("name", true)];

        // 1. The plain PostgreSQL shape.
        assert_eq!(
            QueryExecutor::build_select_sql(
                "users",
                &columns,
                None,
                None,
                &[],
                &DEFAULT,
                50,
                0,
                None
            )
            .unwrap(),
            "SELECT \"id\", \"name\" FROM \"users\" LIMIT 50 OFFSET 0"
        );

        // 2. A page that is not the first one.
        assert_eq!(
            QueryExecutor::build_select_sql(
                "users",
                &columns,
                None,
                None,
                &[],
                &DEFAULT,
                50,
                150,
                None
            )
            .unwrap(),
            "SELECT \"id\", \"name\" FROM \"users\" LIMIT 50 OFFSET 150"
        );

        // 3. The T-SQL shape.
        assert_eq!(
            QueryExecutor::build_select_sql("users", &columns, None, None, &[], &TSQL, 25, 50, None)
                .unwrap(),
            "SELECT \"id\", \"name\" FROM \"users\" ORDER BY (SELECT NULL) OFFSET 50 ROWS FETCH NEXT 25 ROWS ONLY"
        );

        // 4. A dialect without OFFSET.
        assert_eq!(
            QueryExecutor::build_select_sql(
                "users",
                &columns,
                None,
                None,
                &[],
                &LIMIT_ONLY,
                50,
                0,
                None
            )
            .unwrap(),
            "SELECT \"id\", \"name\" FROM \"users\" LIMIT 50"
        );

        // 5. An explicit sort, and the default order beside it.
        let explicit = QueryExecutor::build_select_sql(
            "users",
            &columns,
            None,
            Some(OrderBy {
                column: "name".to_string(),
                descending: true,
            }),
            &[],
            &DEFAULT,
            50,
            0,
            None,
        )
        .unwrap();
        assert_eq!(
            explicit,
            "SELECT \"id\", \"name\" FROM \"users\" ORDER BY \"name\" DESC LIMIT 50 OFFSET 0"
        );
        let defaulted = QueryExecutor::build_select_sql(
            "users",
            &columns,
            None,
            None,
            &["id".to_string()],
            &DEFAULT,
            50,
            0,
            None,
        )
        .unwrap();
        assert_eq!(
            defaulted,
            "SELECT \"id\", \"name\" FROM \"users\" ORDER BY \"id\" ASC LIMIT 50 OFFSET 0"
        );
    }

    #[test]
    fn offset_path_keeps_the_filters_ungrouped() {
        // The filters are grouped only when a seek condition is placed next
        // to them. Without one there is nothing to bind.
        let columns = vec![key_column("id")];
        let filters = vec![FilterCondition {
            column: "id".into(),
            operator: FilterOperator::Eq,
            value: Value::Integer(1),
            connector: None,
        }];
        let sql = QueryExecutor::build_select_sql(
            "users",
            &columns,
            Some(filters),
            None,
            &[],
            &DEFAULT,
            50,
            0,
            Some("or"),
        )
        .unwrap();
        assert_eq!(
            sql,
            "SELECT \"id\" FROM \"users\" WHERE \"id\" = 1 LIMIT 50 OFFSET 0"
        );
    }
}

/// The statement of a page that is read by seek.
mod seek_statement {
    use super::*;

    #[test]
    fn keyset_predicate_is_joined_with_and_after_user_filters() {
        let columns = vec![key_column("id")];
        let filters = vec![
            FilterCondition {
                column: "id".into(),
                operator: FilterOperator::Eq,
                value: Value::Integer(1),
                connector: None,
            },
            FilterCondition {
                column: "name".into(),
                operator: FilterOperator::Eq,
                value: Value::String("alice".into()),
                connector: None,
            },
        ];
        let order = order_spec(vec![("id".to_string(), false)], vec!["id".to_string()]);
        let paged = table_read_sql::build_paged_select_sql(
            "users",
            &columns,
            Some(filters),
            &order,
            PagingMode::Keyset,
            Some("(\"id\" > 1)"),
            &DEFAULT,
            50,
            0,
            Some("or"),
        )
        .unwrap();
        assert_eq!(
            paged.sql,
            "SELECT \"id\" FROM \"users\" WHERE (\"id\" = 1 OR \"name\" = 'alice') AND ((\"id\" > 1)) ORDER BY \"id\" ASC LIMIT 50 OFFSET 0",
            "the seek condition must be joined with AND, never with OR"
        );
    }

    #[test]
    fn keyset_always_orders_by_appended_primary_key() {
        let columns = vec![key_column("id"), plain_column("name", false)];
        let order = order_spec(vec![("name".to_string(), false)], vec!["id".to_string()]);
        let decision = crate::services::table_paging::keyset_decision(order, &columns);
        let crate::services::table_paging::KeysetSeek::Eligible { order } = decision.seek else {
            panic!("the key makes the order total");
        };
        assert_eq!(
            order.columns.len(),
            2,
            "the boundary has one value per column"
        );

        let paged = table_read_sql::build_paged_select_sql(
            "users",
            &columns,
            None,
            &order,
            PagingMode::Keyset,
            Some("(\"name\" > 'a')"),
            &DEFAULT,
            50,
            0,
            None,
        )
        .unwrap();
        assert!(
            paged.sql.contains("ORDER BY \"name\" ASC, \"id\" ASC"),
            "the key column must be part of the order, got: {}",
            paged.sql
        );
    }

    #[test]
    fn keyset_uses_pagination_clause_with_zero_offset() {
        let columns = vec![key_column("id")];
        let order = order_spec(vec![("id".to_string(), false)], vec!["id".to_string()]);
        let paged = table_read_sql::build_paged_select_sql(
            "users",
            &columns,
            None,
            &order,
            PagingMode::Keyset,
            Some("(\"id\" > 1)"),
            &DEFAULT,
            50,
            1500,
            None,
        )
        .unwrap();
        assert!(
            paged.sql.ends_with("LIMIT 50 OFFSET 0"),
            "a seek skips nothing, got: {}",
            paged.sql
        );
    }

    #[test]
    fn tsql_keyset_never_emits_select_null_fallback() {
        let columns = vec![key_column("id")];
        let order = order_spec(vec![("id".to_string(), false)], vec!["id".to_string()]);
        let paged = table_read_sql::build_paged_select_sql(
            "users",
            &columns,
            None,
            &order,
            PagingMode::Keyset,
            Some("(\"id\" > 1)"),
            &TSQL,
            50,
            0,
            None,
        )
        .unwrap();
        assert!(
            !paged.sql.contains("(SELECT NULL)"),
            "a seek always has an order, got: {}",
            paged.sql
        );
        assert!(
            paged.sql.ends_with("OFFSET 0 ROWS FETCH NEXT 50 ROWS ONLY"),
            "the dialect's own clause is used, got: {}",
            paged.sql
        );
        assert!(!paged.sql.contains("LIMIT"), "got: {}", paged.sql);
    }
}

/// The paging decision of a real read: the mock driver is the driver, and
/// the statement it was asked to run is the assertion.
mod executor_paging {
    use super::*;

    fn handle() -> ConnectionHandle {
        ConnectionHandle {
            id: "h1".into(),
            pool_id: "p1".into(),
        }
    }

    fn executor() -> QueryExecutor {
        let cache = Arc::new(SchemaCache::new(Arc::new(DriverRegistry::new())));
        QueryExecutor::new(cache)
    }

    fn columns() -> Vec<ColumnSchema> {
        vec![key_column("id"), plain_column("name", false)]
    }

    fn rows() -> Vec<Vec<Option<Value>>> {
        vec![
            vec![Some(Value::Integer(1)), Some(Value::String("alice".into()))],
            vec![Some(Value::Integer(2)), Some(Value::String("bob".into()))],
        ]
    }

    async fn read_page(
        executor: &QueryExecutor,
        mock: &Arc<MockDriver>,
        after_key: Option<&[CursorValue]>,
        sort: Option<OrderBy>,
    ) -> TableDataResult {
        read_page_at(executor, mock, 0, after_key, sort).await
    }

    /// A read of a **specific** page.
    ///
    /// The page number matters: a page beyond the first, with no boundary,
    /// is a page the caller could not reach by seek, and it is read by
    /// offset.
    async fn read_page_at(
        executor: &QueryExecutor,
        mock: &Arc<MockDriver>,
        page: u32,
        after_key: Option<&[CursorValue]>,
        sort: Option<OrderBy>,
    ) -> TableDataResult {
        read_page_sized(executor, mock, page, 50, after_key, sort).await
    }

    /// A read of a specific page, with a specific page size.
    ///
    /// The offset of a page is `page × pageSize`, so the page size decides
    /// which rows a page holds.
    async fn read_page_sized(
        executor: &QueryExecutor,
        mock: &Arc<MockDriver>,
        page: u32,
        page_size: u32,
        after_key: Option<&[CursorValue]>,
        sort: Option<OrderBy>,
    ) -> TableDataResult {
        let driver = mock.clone() as Arc<dyn DatabaseDriver>;
        executor
            .get_table_data_page(
                &driver,
                &handle(),
                "conn1",
                "db1",
                None,
                "users",
                page,
                page_size,
                None,
                sort,
                true,
                None,
                None,
                None,
                after_key,
            )
            .await
            .expect("the read succeeds")
    }

    #[tokio::test]
    async fn get_table_data_page_returns_keyset_metadata_when_mock_supports_it() {
        let mock = MockDriver::new(
            "postgres",
            MockDriverOptions {
                columns: columns(),
                primary_keys: vec!["id".into()],
                supports_keyset_pagination: true,
                query_rows: rows(),
                ..Default::default()
            },
        );
        let result = read_page(&executor(), &mock, None, None).await;

        let paging = result.paging.expect("the read reports its paging");
        assert_eq!(paging.mode, PagingMode::Keyset);
        assert_eq!(paging.unavailable_reason, None);
        assert_eq!(paging.order_columns.len(), 1);
        assert_eq!(paging.order_columns[0].column, "id");
        let next = paging
            .next_after_key
            .expect("the last row is the boundary of the next page");
        assert_eq!(next, vec![CursorValue::Integer(2)]);
    }

    /// The promise the whole contract rests on: a driver that did not opt
    /// in reads **exactly** as it did before the contract existed.
    ///
    /// The sort is an **explicit** one, because that is where the promise
    /// used to break: the seek's tie-breaker columns (the appended primary
    /// key) were written into the `ORDER BY` of a driver that cannot seek
    /// at all, so every read of every table with a key changed its query.
    /// The statement below is the statement the product sent before this
    /// contract existed, compared as a whole string.
    #[tokio::test]
    async fn a_driver_without_the_capability_reads_the_baseline_statement_verbatim() {
        let mock = MockDriver::new(
            "postgres",
            MockDriverOptions {
                columns: columns(),
                primary_keys: vec!["id".into()],
                query_rows: rows(),
                ..Default::default()
            },
        );
        let result = read_page_at(
            &executor(),
            &mock,
            1,
            None,
            Some(OrderBy {
                column: "name".to_string(),
                descending: false,
            }),
        )
        .await;

        let paging = result.paging.expect("the read reports its paging");
        assert_eq!(paging.mode, PagingMode::Offset);
        assert_eq!(paging.next_after_key, None);
        let log = mock.query_log();
        assert_eq!(
            log.last().map(String::as_str),
            Some("SELECT \"id\", \"name\" FROM \"users\" ORDER BY \"name\" ASC LIMIT 50 OFFSET 50"),
            "a driver that did not opt in must send the statement it always sent"
        );
    }

    #[tokio::test]
    async fn a_driver_without_the_capability_says_so_when_a_page_is_asked_for() {
        let mock = MockDriver::new(
            "postgres",
            MockDriverOptions {
                columns: columns(),
                primary_keys: vec!["id".into()],
                query_rows: rows(),
                ..Default::default()
            },
        );
        // Page 1 is a page the caller expects to have been read by seek.
        let result = read_page_at(&executor(), &mock, 1, None, None).await;

        let paging = result.paging.expect("the read reports its paging");
        assert_eq!(paging.mode, PagingMode::Offset);
        assert_eq!(
            paging.unavailable_reason,
            Some(KeysetUnavailableReason::DriverUnsupported)
        );
        assert_eq!(paging.next_after_key, None);
    }

    /// A page the caller could not reach by seek — it sent no boundary and
    /// asked for a page beyond the first.
    ///
    /// It is read by **offset**: the caller's page number is the truth, and
    /// the rows of that page are the rows of that page. Reading it from the
    /// top of the table and labelling it "page 7" would be a page of
    /// **wrong data**, silently.
    #[tokio::test]
    async fn a_page_the_caller_cannot_reach_is_read_by_offset() {
        let mut paged_rows = rows();
        paged_rows.push(vec![
            Some(Value::Integer(3)),
            Some(Value::String("cara".into())),
        ]);
        paged_rows.push(vec![
            Some(Value::Integer(4)),
            Some(Value::String("dan".into())),
        ]);
        let mock = MockDriver::new(
            "postgres",
            MockDriverOptions {
                columns: columns(),
                primary_keys: vec!["id".into()],
                supports_keyset_pagination: true,
                rows_start_at_offset: true,
                query_rows: paged_rows,
                ..Default::default()
            },
        );
        let result = read_page_at(&executor(), &mock, 7, None, None).await;

        let paging = result.paging.expect("the read reports its paging");
        assert_eq!(paging.mode, PagingMode::Offset);
        assert_eq!(
            paging.unavailable_reason,
            Some(KeysetUnavailableReason::NotAdjacent)
        );
        let log = mock.query_log();
        assert_eq!(
            log.last().map(String::as_str),
            Some("SELECT \"id\", \"name\" FROM \"users\" ORDER BY \"id\" ASC LIMIT 50 OFFSET 350"),
            "the page number is the truth: the offset is the caller's page"
        );
        assert!(
            result.rows.is_empty(),
            "the mock has four rows and page 7 is at offset 350: the page is empty **because of the offset**. A page read from the top of the table would have held all four rows — that is the bug this case pins."
        );
    }

    /// The same page, with rows behind it: the page holds the rows **that
    /// page** holds.
    #[tokio::test]
    async fn a_non_adjacent_page_holds_the_rows_of_that_page() {
        let mut paged_rows = rows();
        paged_rows.push(vec![
            Some(Value::Integer(3)),
            Some(Value::String("cara".into())),
        ]);
        paged_rows.push(vec![
            Some(Value::Integer(4)),
            Some(Value::String("dan".into())),
        ]);
        let mock = MockDriver::new(
            "postgres",
            MockDriverOptions {
                columns: columns(),
                primary_keys: vec!["id".into()],
                supports_keyset_pagination: true,
                rows_start_at_offset: true,
                query_rows: paged_rows,
                ..Default::default()
            },
        );
        // One row per page, so page 1 is the second row.
        let result = read_page_sized(&executor(), &mock, 1, 1, None, None).await;

        assert_eq!(result.rows.len(), 1, "page 1 holds one row");
        // `Value` has no `PartialEq`, so the cell is read by pattern.
        match result.rows[0][0].as_ref() {
            Some(Value::Integer(2)) => {}
            other => panic!("page 1 holds the second row, not the first: {other:?}"),
        }
        match result.rows[0][1].as_ref() {
            Some(Value::String(name)) if name == "bob" => {}
            other => panic!("the row is the second one, whole: {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_table_data_page_sends_the_seek_condition_when_a_boundary_is_given() {
        let mock = MockDriver::new(
            "postgres",
            MockDriverOptions {
                columns: columns(),
                primary_keys: vec!["id".into()],
                supports_keyset_pagination: true,
                query_rows: rows(),
                ..Default::default()
            },
        );
        let boundary = vec![CursorValue::Integer(2)];
        let result = read_page(&executor(), &mock, Some(&boundary), None).await;

        let paging = result.paging.expect("the read reports its paging");
        assert_eq!(paging.mode, PagingMode::Keyset);
        let log = mock.query_log();
        let statement = log.last().expect("a statement was sent");
        assert!(
            statement.contains("WHERE ((\"id\" > 2))"),
            "the driver's condition must be in the statement, got: {statement}"
        );
        assert!(
            statement.ends_with("LIMIT 50 OFFSET 0"),
            "a seek skips nothing, got: {statement}"
        );
    }

    #[tokio::test]
    async fn get_table_data_page_ignores_cursor_when_decision_is_unavailable() {
        // A table without a key has no total order, so the boundary cannot
        // locate anything: the cursor is dropped, not used.
        let mock = MockDriver::new(
            "postgres",
            MockDriverOptions {
                columns: vec![plain_column("name", false)],
                primary_keys: Vec::new(),
                supports_keyset_pagination: true,
                query_rows: rows(),
                ..Default::default()
            },
        );
        let boundary = vec![CursorValue::Integer(2)];
        let result = read_page(&executor(), &mock, Some(&boundary), None).await;

        let paging = result.paging.expect("the read reports its paging");
        assert_eq!(paging.mode, PagingMode::Offset);
        assert_eq!(
            paging.unavailable_reason,
            Some(KeysetUnavailableReason::NoStableOrder)
        );
        let log = mock.query_log();
        let statement = log.last().expect("a statement was sent");
        assert!(
            !statement.contains("> 2"),
            "a dropped cursor must not reach the statement, got: {statement}"
        );
    }

    #[tokio::test]
    async fn stale_cursor_signature_mismatch_returns_page_zero() {
        // The boundary has one value, the order has two columns: the cursor
        // belongs to another sort and cannot be used.
        let mock = MockDriver::new(
            "postgres",
            MockDriverOptions {
                columns: vec![key_column("id"), plain_column("name", false)],
                primary_keys: vec!["id".into()],
                supports_keyset_pagination: true,
                query_rows: rows(),
                ..Default::default()
            },
        );
        let boundary = vec![CursorValue::Integer(2)];
        let result = read_page(
            &executor(),
            &mock,
            Some(&boundary),
            Some(OrderBy {
                column: "name".to_string(),
                descending: false,
            }),
        )
        .await;

        let paging = result.paging.expect("the read reports its paging");
        assert_eq!(paging.mode, PagingMode::Offset);
        assert_eq!(
            paging.unavailable_reason,
            Some(KeysetUnavailableReason::StaleCursor)
        );
    }

    #[tokio::test]
    async fn get_table_data_without_after_key_still_compiles_and_behaves() {
        // The old entry point, unchanged: no boundary, and the statement it
        // sends is the one the table read always sent.
        let mock = MockDriver::new(
            "postgres",
            MockDriverOptions {
                columns: columns(),
                primary_keys: vec!["id".into()],
                query_rows: rows(),
                ..Default::default()
            },
        );
        let driver = mock.clone() as Arc<dyn DatabaseDriver>;
        let result = executor()
            .get_table_data(
                &driver,
                &handle(),
                "conn1",
                "db1",
                None,
                "users",
                0,
                50,
                None,
                None,
                true,
                None,
                None,
                None,
            )
            .await
            .expect("the read succeeds");
        assert_eq!(result.rows.len(), 2);
        let log = mock.query_log();
        assert_eq!(
            log.last().map(String::as_str),
            Some("SELECT \"id\", \"name\" FROM \"users\" ORDER BY \"id\" ASC LIMIT 50 OFFSET 0")
        );
    }

    #[tokio::test]
    async fn a_null_boundary_degrades_to_offset() {
        let mock = MockDriver::new(
            "postgres",
            MockDriverOptions {
                columns: columns(),
                primary_keys: vec!["id".into()],
                supports_keyset_pagination: true,
                query_rows: rows(),
                ..Default::default()
            },
        );
        let boundary = vec![CursorValue::Null];
        let result = read_page(&executor(), &mock, Some(&boundary), None).await;

        let paging = result.paging.expect("the read reports its paging");
        assert_eq!(paging.mode, PagingMode::Offset);
        assert_eq!(
            paging.unavailable_reason,
            Some(KeysetUnavailableReason::NullBoundary)
        );
        let log = mock.query_log();
        let statement = log.last().expect("a statement was sent");
        assert!(
            !statement.contains("> NULL"),
            "a NULL must never become a comparison, got: {statement}"
        );
    }
}

/// The three refusals the paging contract has.
///
/// They exist for a caller that **insists** on keyset paging: a caller that
/// does not insist gets a page read by offset and a reason in the answer.
/// A degradation is not an error — an error would be a page the user cannot
/// read at all.
mod refusals {
    use super::*;

    fn plan(reason: Option<KeysetUnavailableReason>, predicate: Option<&str>) -> PagingPlan {
        PagingPlan {
            mode: PagingMode::Offset,
            order: order_spec(vec![("id".to_string(), false)], vec!["id".to_string()]),
            seek_order: None,
            predicate: predicate.map(str::to_string),
            unavailable_reason: reason,
        }
    }

    #[test]
    fn a_caller_that_does_not_insist_is_never_refused() {
        let plan = plan(Some(KeysetUnavailableReason::DriverUnsupported), None);
        assert_eq!(
            keyset_refusal(&plan, false, 0),
            None,
            "degrading is the normal path; an error would be a page the user cannot read"
        );
    }

    #[test]
    fn keyset_unavailable_is_refused_when_the_caller_insists() {
        let plan = plan(Some(KeysetUnavailableReason::NoStableOrder), None);
        let message = keyset_refusal(&plan, true, 0).expect("the refusal");
        assert!(
            message.starts_with("grid.paging.keysetUnavailable: "),
            "the prefix is part of the contract, got: {message}"
        );
        assert!(
            message.contains("no key"),
            "the reason has to say why, got: {message}"
        );
    }

    /// The cursor is the **client's** mistake, and it is a different
    /// failure from "this page cannot be read by seek".
    ///
    /// The plan here is a **real** one: a boundary of one value sent for an
    /// order of two columns. A hand-made plan would prove nothing — it
    /// would only prove that the refusal function reads the field it is
    /// given.
    #[test]
    fn a_stale_cursor_is_refused_as_an_invalidated_cursor() {
        let columns = vec![key_column("id"), plain_column("name", false)];
        let order = order_spec(vec![("name".to_string(), false)], vec!["id".to_string()]);
        let decision = crate::services::table_paging::keyset_decision(order, &columns);
        let stale = vec![CursorValue::Integer(2)];
        let driver = MockDriver::new(
            "postgres",
            MockDriverOptions {
                supports_keyset_pagination: true,
                ..Default::default()
            },
        );
        let plan = resolve_paging(
            driver.as_ref(),
            &decision,
            Some(&stale),
            1,
            &crate::services::query_executor::tests::DEFAULT,
            50,
        );
        assert_eq!(
            plan.unavailable_reason,
            Some(KeysetUnavailableReason::StaleCursor),
            "the boundary does not fit the order: that is a stale cursor"
        );
        let message = keyset_refusal(&plan, true, 0).expect("the refusal");
        assert!(
            message.starts_with("grid.paging.cursorInvalidated: "),
            "the prefix is part of the contract, got: {message}"
        );
    }

    #[test]
    fn cursor_too_deep_is_refused_past_the_chain_limit() {
        let plan = plan(None, Some("(\"id\" > 1)"));
        let message = keyset_refusal(&plan, true, 100_000).expect("the refusal");
        assert!(
            message.starts_with("grid.paging.cursorTooDeep: "),
            "the prefix is part of the contract, got: {message}"
        );
    }

    #[test]
    fn a_shallow_chain_is_not_refused() {
        let plan = plan(None, Some("(\"id\" > 1)"));
        assert_eq!(keyset_refusal(&plan, true, 3), None);
    }
}

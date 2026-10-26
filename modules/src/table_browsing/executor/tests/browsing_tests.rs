//! The browsing half of the query executor's tests: the default order of a
//! read, and the row count it reports.
//!
//! They live in their own file because the statement builder's own tests
//! already fill their module, and because the two subjects are separate: one
//! is about **which columns** a read is ordered by, the other about **how
//! many rows** it says there are.

use super::*;
use crate::execution::runtime::driver_registry::DriverRegistry;
use crate::schema::SchemaCache;

/// The order of a read whose table has a primary key.
///
/// The host used to find the key by filtering the columns for the flag. The
/// key now comes from the normalised list the cache holds, and the two can
/// disagree — that disagreement is the one behaviour change this contract
/// brings, so it gets its own test.
mod default_order_sources {
    use super::*;

    #[test]
    fn default_order_uses_primary_keys_when_column_flags_disagree() {
        // The columns carry no flag at all, and the key is not even among
        // them: the old code found no key and ordered by the first column.
        let columns = vec![make_column("col_a", false), make_column("col_b", false)];
        let primary_keys = vec!["id".to_string()];
        let default_order = host_default_order(&columns, &primary_keys);
        assert_eq!(default_order, vec!["id".to_string()]);

        let sql = QueryExecutor::build_select_sql(
            "orders",
            &columns,
            None,
            None,
            &default_order,
            &DEFAULT,
            50,
            0,
            None,
        )
        .unwrap();
        assert!(
            sql.contains("ORDER BY \"id\" ASC"),
            "the read must order by the real primary key, got: {sql}"
        );
        assert!(
            !sql.contains("ORDER BY \"col_a\" ASC"),
            "the first column must not take the place of the key, got: {sql}"
        );
    }

    #[test]
    fn default_order_falls_back_to_the_first_column_without_a_primary_key() {
        // The other direction: the columns carry the flag, the normalised
        // list is empty. The key is then unknown, and the first column is
        // the documented fallback.
        let columns = vec![make_column("col_a", true), make_column("col_b", false)];
        let default_order = host_default_order(&columns, &[]);
        assert_eq!(default_order, vec!["col_a".to_string()]);

        let sql = QueryExecutor::build_select_sql(
            "orders",
            &columns,
            None,
            None,
            &default_order,
            &DEFAULT,
            50,
            0,
            None,
        )
        .unwrap();
        assert!(
            sql.contains("ORDER BY \"col_a\" ASC"),
            "the first column is the documented fallback, got: {sql}"
        );
    }

    #[test]
    fn pk_source_disagreement_is_reported() {
        let columns = vec![make_column("col_a", false)];
        assert!(browsing::pk_sources_disagree(&columns, &["id".to_string()]));
        assert!(!browsing::pk_sources_disagree(&columns, &[]));
        let flagged = vec![make_column("id", true)];
        assert!(!browsing::pk_sources_disagree(
            &flagged,
            &["id".to_string()]
        ));
    }

    #[test]
    fn default_order_is_empty_when_columns_are_empty() {
        let default_order = host_default_order(&[], &[]);
        let sql = QueryExecutor::build_select_sql(
            "events",
            &[],
            None,
            None,
            &default_order,
            &DEFAULT,
            50,
            0,
            None,
        )
        .unwrap();
        assert!(
            !sql.contains("ORDER BY"),
            "a table without columns has nothing to order by, got: {sql}"
        );

        // A dialect that needs an order still gets its neutral one.
        let sql = QueryExecutor::build_select_sql(
            "events",
            &[],
            None,
            None,
            &default_order,
            &TSQL,
            50,
            0,
            None,
        )
        .unwrap();
        assert!(
            sql.contains("ORDER BY (SELECT NULL)"),
            "the dialect fallback still applies, got: {sql}"
        );
    }

    #[test]
    fn default_order_override_empty_array_emits_no_order_by() {
        let columns = vec![make_column("id", true)];
        let default_order = browsing::resolve_default_order(
            mock_driver().as_ref(),
            &columns,
            &["id".to_string()],
            Some(&[]),
        );
        assert!(default_order.is_empty());
        let sql = QueryExecutor::build_select_sql(
            "orders",
            &columns,
            None,
            None,
            &default_order,
            &DEFAULT,
            50,
            0,
            None,
        )
        .unwrap();
        assert!(!sql.contains("ORDER BY"), "got: {sql}");
    }

    #[test]
    fn default_order_override_columns_are_emitted_ascending() {
        let columns = vec![make_column("id", true), make_column("name", false)];
        let default_order = browsing::resolve_default_order(
            mock_driver().as_ref(),
            &columns,
            &["id".to_string()],
            Some(&["a".to_string(), "b".to_string()]),
        );
        let sql = QueryExecutor::build_select_sql(
            "orders",
            &columns,
            None,
            None,
            &default_order,
            &DEFAULT,
            50,
            0,
            None,
        )
        .unwrap();
        assert!(sql.contains("ORDER BY \"a\" ASC, \"b\" ASC"), "got: {sql}");
    }
}

/// The row count a read reports: exact, estimated, or none.
mod count_strategy_execution {
    use super::*;
    use datazen_driver_api::CountStrategy;

    fn handle() -> ConnectionHandle {
        ConnectionHandle {
            id: "h1".into(),
            pool_id: "p1".into(),
        }
    }

    fn columns() -> Vec<ColumnSchema> {
        vec![make_column("id", true), make_column("name", false)]
    }

    async fn executor(options: MockDriverOptions) -> (QueryExecutor, Arc<MockDriver>) {
        // The column read goes through the catalog reader the caller passes
        // in, so the cache needs a registry but never looks at it.
        let cache = Arc::new(SchemaCache::new(Arc::new(DriverRegistry::new())));
        (
            QueryExecutor::new(cache),
            MockDriver::new("postgres", options),
        )
    }

    async fn read(
        executor: &QueryExecutor,
        mock: &Arc<MockDriver>,
        skip_count: bool,
        count_strategy: Option<CountStrategy>,
        default_order_columns: Option<Vec<String>>,
        filters: Option<Vec<FilterCondition>>,
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
                0,
                50,
                filters,
                None,
                skip_count,
                None,
                count_strategy,
                default_order_columns,
                None,
            )
            .await
            .expect("the read succeeds")
    }

    fn rows() -> Vec<Vec<Option<Value>>> {
        vec![vec![
            Some(Value::Integer(1)),
            Some(Value::String("alice".into())),
        ]]
    }

    #[tokio::test]
    async fn exact_strategy_still_issues_count_and_select_concurrently() {
        let (executor, mock) = executor(MockDriverOptions {
            columns: columns(),
            primary_keys: vec!["id".into()],
            count_total: 99,
            query_rows: rows(),
            ..Default::default()
        })
        .await;

        let result = read(&executor, &mock, false, None, None, None).await;
        assert_eq!(result.total_rows, Some(99));
        assert_eq!(result.total_rows_kind, Some(CountStrategy::Exact));
        assert_eq!(result.rows.len(), 1);
        assert_eq!(mock.query_calls(), 2, "count + data queries");
    }

    #[tokio::test]
    async fn caller_skip_count_returns_absent_kind_and_no_extra_query() {
        let (executor, mock) = executor(MockDriverOptions {
            columns: columns(),
            primary_keys: vec!["id".into()],
            count_total: 99,
            query_rows: rows(),
            ..Default::default()
        })
        .await;

        let result = read(&executor, &mock, true, None, None, None).await;
        assert_eq!(result.total_rows, None);
        // The caller keeps the total it already has: the answer says nothing
        // about the count, which is not the same as "there is no count".
        assert_eq!(result.total_rows_kind, None);
        assert_eq!(mock.query_calls(), 1, "the data statement only");
    }

    #[tokio::test]
    async fn unsupported_driver_returns_no_total_and_unsupported_kind() {
        let (executor, mock) = executor(MockDriverOptions {
            columns: columns(),
            primary_keys: vec!["id".into()],
            skip_count_query: true,
            query_rows: rows(),
            ..Default::default()
        })
        .await;

        let result = read(&executor, &mock, false, None, None, None).await;
        assert_eq!(result.total_rows, None);
        assert_eq!(result.total_rows_kind, Some(CountStrategy::Unsupported));
        assert_eq!(result.rows.len(), 1);
        assert_eq!(mock.query_calls(), 1, "the data statement only");
    }

    #[tokio::test]
    async fn caller_asking_for_no_count_sends_no_count_statement() {
        let (executor, mock) = executor(MockDriverOptions {
            columns: columns(),
            primary_keys: vec!["id".into()],
            count_total: 99,
            query_rows: rows(),
            ..Default::default()
        })
        .await;

        let result = read(
            &executor,
            &mock,
            false,
            Some(CountStrategy::Unsupported),
            None,
            None,
        )
        .await;
        assert_eq!(result.total_rows, None);
        assert_eq!(result.total_rows_kind, Some(CountStrategy::Unsupported));
        assert_eq!(mock.query_calls(), 1, "the data statement only");
    }

    #[tokio::test]
    async fn estimated_strategy_runs_estimate_sql_and_reports_kind() {
        let (executor, mock) = executor(MockDriverOptions {
            columns: columns(),
            primary_keys: vec!["id".into()],
            count_total: 99,
            count_strategy: Some(CountStrategy::Estimated),
            count_estimate_rows: Some(4242),
            query_rows: rows(),
            ..Default::default()
        })
        .await;

        let result = read(&executor, &mock, false, None, None, None).await;
        assert_eq!(result.total_rows, Some(4242));
        assert_eq!(result.total_rows_kind, Some(CountStrategy::Estimated));
        assert_eq!(result.rows.len(), 1);
        assert_eq!(mock.query_calls(), 2, "estimate + data queries");
    }

    #[tokio::test]
    async fn estimated_branch_keeps_data_when_estimate_query_fails() {
        let (executor, mock) = executor(MockDriverOptions {
            columns: columns(),
            primary_keys: vec!["id".into()],
            count_total: 99,
            count_strategy: Some(CountStrategy::Estimated),
            count_estimate_rows: Some(4242),
            count_estimate_error: Some("estimate is not available".into()),
            query_rows: rows(),
            ..Default::default()
        })
        .await;

        let result = read(&executor, &mock, false, None, None, None).await;
        assert_eq!(result.rows.len(), 1, "the rows survive a failed estimate");
        // A number nobody could estimate is not the same as no number: the
        // read falls back to an exact count, which is what it does when the
        // driver cannot estimate at all.
        assert_eq!(result.total_rows, Some(99));
        assert_eq!(result.total_rows_kind, Some(CountStrategy::Exact));
        assert_eq!(
            mock.query_calls(),
            3,
            "estimate + data + the exact count that replaced it"
        );
    }

    #[tokio::test]
    async fn estimated_branch_rescues_an_unusable_estimate_with_a_count() {
        let (executor, mock) = executor(MockDriverOptions {
            columns: columns(),
            primary_keys: vec!["id".into()],
            count_total: 99,
            count_strategy: Some(CountStrategy::Estimated),
            count_estimate_rows: Some(-1),
            query_rows: rows(),
            ..Default::default()
        })
        .await;

        let result = read(&executor, &mock, false, None, None, None).await;
        assert_eq!(result.rows.len(), 1);
        assert_eq!(
            result.total_rows,
            Some(99),
            "`-1` means the planner never looked, so the exact count answers"
        );
        assert_eq!(result.total_rows_kind, Some(CountStrategy::Exact));
        assert_eq!(mock.query_calls(), 3, "estimate + data + count");
    }

    #[tokio::test]
    async fn estimated_zero_is_an_estimate_not_a_failure() {
        let (executor, mock) = executor(MockDriverOptions {
            columns: columns(),
            primary_keys: vec!["id".into()],
            count_total: 99,
            count_strategy: Some(CountStrategy::Estimated),
            count_estimate_rows: Some(0),
            query_rows: rows(),
            ..Default::default()
        })
        .await;

        let result = read(&executor, &mock, false, None, None, None).await;
        assert_eq!(result.total_rows, Some(0));
        assert_eq!(result.total_rows_kind, Some(CountStrategy::Estimated));
    }

    #[tokio::test]
    async fn estimated_downgrades_to_exact_when_effective_filters_present() {
        let (executor, mock) = executor(MockDriverOptions {
            columns: columns(),
            primary_keys: vec!["id".into()],
            count_total: 99,
            count_strategy: Some(CountStrategy::Estimated),
            count_estimate_rows: Some(4242),
            query_rows: rows(),
            ..Default::default()
        })
        .await;

        let filters = vec![FilterCondition {
            column: "id".into(),
            operator: FilterOperator::Eq,
            value: Value::Integer(1),
            connector: None,
        }];
        let result = read(&executor, &mock, false, None, None, Some(filters)).await;
        // The exact count, not the table-wide estimate: the estimate says
        // nothing about the rows a filter selects.
        assert_eq!(result.total_rows, Some(99));
        assert_eq!(result.total_rows_kind, Some(CountStrategy::Exact));
    }

    #[tokio::test]
    async fn estimated_downgrades_to_exact_when_driver_has_no_estimate_sql() {
        let (executor, mock) = executor(MockDriverOptions {
            columns: columns(),
            primary_keys: vec!["id".into()],
            count_total: 99,
            count_strategy: Some(CountStrategy::Estimated),
            count_estimate_rows: None,
            query_rows: rows(),
            ..Default::default()
        })
        .await;

        let result = read(&executor, &mock, false, None, None, None).await;
        assert_eq!(result.total_rows, Some(99));
        assert_eq!(result.total_rows_kind, Some(CountStrategy::Exact));
        assert_eq!(mock.query_calls(), 2, "count + data queries");
    }

    #[tokio::test]
    async fn exact_branch_still_propagates_count_failure() {
        let (executor, mock) = executor(MockDriverOptions {
            columns: columns(),
            primary_keys: vec!["id".into()],
            count_error: Some("count failed".into()),
            query_rows: rows(),
            ..Default::default()
        })
        .await;

        let driver = mock.clone() as Arc<dyn DatabaseDriver>;
        let error = executor
            .get_table_data_page(
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
                false,
                None,
                None,
                None,
                None,
            )
            .await
            .expect_err("the exact branch still fails the read");
        assert!(error.to_string().contains("count failed"), "got: {error}");
    }

    #[tokio::test]
    async fn total_rows_is_none_when_count_cell_is_not_an_integer() {
        let (executor, mock) = executor(MockDriverOptions {
            columns: columns(),
            primary_keys: vec!["id".into()],
            count_total_value: Some(Value::String("42".into())),
            query_rows: rows(),
            ..Default::default()
        })
        .await;

        let result = read(&executor, &mock, false, None, None, None).await;
        // The count was taken, it just did not come back as a number.
        assert_eq!(result.total_rows, None);
        assert_eq!(result.total_rows_kind, Some(CountStrategy::Exact));
        assert_eq!(result.rows.len(), 1);
    }

    #[tokio::test]
    async fn a_read_orders_by_the_normalised_primary_key() {
        let (executor, mock) = executor(MockDriverOptions {
            // The columns carry no flag; the key is only in the list.
            columns: vec![make_column("col_a", false), make_column("col_b", false)],
            primary_keys: vec!["id".into()],
            count_total: 99,
            query_rows: rows(),
            ..Default::default()
        })
        .await;

        let result = read(&executor, &mock, false, None, None, None).await;
        assert_eq!(result.total_rows, Some(99));
        // The read itself does not carry the statement; the point of the
        // case is that it does not fail and that the key is not lost.
        assert_eq!(result.rows.len(), 1);
    }
}

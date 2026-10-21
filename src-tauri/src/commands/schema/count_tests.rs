//! The row count a read reports, and the estimate command beside it.
//!
//! They live in their own file because the schema commands' own tests already
//! fill their module, and because the subject is separate: the other tests
//! are about *what* a read returns, these are about *how much* it says there
//! are.

use super::*;
use crate::db::Value;
use crate::testing::app_state::TestAppState;

/// The row count a read reports, and the estimate command beside it.
mod count_strategy {
    use super::*;
    use crate::db::CountStrategy;
    use crate::testing::mock_driver::MockDriverOptions;

    async fn read_with(
        opts: MockDriverOptions,
        count_strategy: Option<CountStrategy>,
        default_order_columns: Option<Vec<String>>,
    ) -> (crate::db::TableDataResult, TestAppState) {
        let test = TestAppState::with_options(opts).await;
        let (_, conn_id) = test.save_and_connect_runtime("count-strategy").await;
        let data = get_table_data_impl(
            &test.state,
            conn_id,
            "users".into(),
            0,
            10,
            None,
            None,
            None,
            None,
            None,
            None,
            count_strategy,
            default_order_columns,
        )
        .await
        .expect("the read succeeds");
        (data, test)
    }

    #[tokio::test]
    async fn get_table_data_forwards_count_strategy_and_default_order_columns() {
        let (data, _test) = read_with(
            MockDriverOptions {
                columns: vec![
                    crate::db::ColumnSchema {
                        name: "id".into(),
                        data_type: "integer".into(),
                        nullable: false,
                        default_value: None,
                        comment: None,
                        is_primary_key: true,
                        is_auto_increment: false,
                    },
                    crate::db::ColumnSchema {
                        name: "name".into(),
                        data_type: "text".into(),
                        nullable: true,
                        default_value: None,
                        comment: None,
                        is_primary_key: false,
                        is_auto_increment: false,
                    },
                ],
                primary_keys: vec!["id".into()],
                count_total: 7,
                query_rows: vec![vec![Some(Value::Integer(1)), Some(Value::Null)]],
                ..Default::default()
            },
            Some(CountStrategy::Unsupported),
            Some(vec![]),
        )
        .await;
        assert_eq!(data.total_rows_kind, Some(CountStrategy::Unsupported));
        assert_eq!(data.total_rows, None);
    }

    #[tokio::test]
    async fn get_table_data_defaults_keep_legacy_shape() {
        let (data, _test) = read_with(
            MockDriverOptions {
                columns: vec![crate::db::ColumnSchema {
                    name: "id".into(),
                    data_type: "integer".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: true,
                    is_auto_increment: false,
                }],
                primary_keys: vec!["id".into()],
                count_total: 7,
                query_rows: vec![vec![Some(Value::Integer(1))]],
                ..Default::default()
            },
            None,
            None,
        )
        .await;
        // The legacy shape: the count is there and it is exact.
        assert_eq!(data.total_rows, Some(7));
        assert_eq!(data.total_rows_kind, Some(CountStrategy::Exact));
    }

    #[tokio::test]
    async fn estimate_table_rows_returns_none_when_driver_cannot_estimate() {
        let test = TestAppState::with_options(MockDriverOptions {
            columns: vec![crate::db::ColumnSchema {
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: false,
            }],
            primary_keys: vec!["id".into()],
            ..Default::default()
        })
        .await;
        let (_, conn_id) = test.save_and_connect_runtime("estimate-none").await;
        let calls = test.mock.query_calls();
        let estimate = estimate_table_rows_impl(
            &test.state,
            conn_id,
            "users".into(),
            Some("app".into()),
            None,
        )
        .await
        .expect("a driver without an estimate is not a failure");
        assert_eq!(estimate, None);
        assert_eq!(
            test.mock.query_calls(),
            calls,
            "the command must not run a statement of its own"
        );
    }

    #[tokio::test]
    async fn estimate_table_rows_returns_value_when_driver_estimates() {
        let test = TestAppState::with_options(MockDriverOptions {
            columns: vec![crate::db::ColumnSchema {
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: false,
            }],
            primary_keys: vec!["id".into()],
            count_strategy: Some(CountStrategy::Estimated),
            count_estimate_rows: Some(4242),
            ..Default::default()
        })
        .await;
        let (_, conn_id) = test.save_and_connect_runtime("estimate-value").await;
        let estimate = estimate_table_rows_impl(
            &test.state,
            conn_id,
            "users".into(),
            Some("app".into()),
            None,
        )
        .await
        .expect("the estimate succeeds");
        assert_eq!(estimate, Some(4242));
    }

    #[tokio::test]
    async fn estimate_table_rows_returns_none_for_unusable_estimate_value() {
        let test = TestAppState::with_options(MockDriverOptions {
            columns: vec![crate::db::ColumnSchema {
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: false,
            }],
            primary_keys: vec!["id".into()],
            count_strategy: Some(CountStrategy::Estimated),
            count_estimate_rows: Some(-1),
            ..Default::default()
        })
        .await;
        let (_, conn_id) = test.save_and_connect_runtime("estimate-bad").await;
        let estimate = estimate_table_rows_impl(
            &test.state,
            conn_id,
            "users".into(),
            Some("app".into()),
            None,
        )
        .await
        .expect("an unusable estimate is an answer, not a failure");
        assert_eq!(estimate, None);
    }

    #[tokio::test]
    async fn estimate_table_rows_requires_live_session() {
        let test = TestAppState::new().await;
        let error = estimate_table_rows_impl(
            &test.state,
            "missing".into(),
            "users".into(),
            Some("app".into()),
            None,
        )
        .await
        .expect_err("there is no session to estimate against");
        assert!(
            error.to_string().contains("DB session"),
            "expected the missing-session error, got: {error}"
        );
    }
}

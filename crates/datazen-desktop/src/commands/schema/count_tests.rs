//! The row count the estimate command reports.
//!
//! They live in their own file because the schema commands' own tests already
//! fill their module, and because the subject is separate. The read-side
//! count behavior is pinned by the target-bound read (`data/target_read_tests.rs`);
//! the session-bound `get_table_data` command was retired, and its count locks
//! moved to that path.
//!
//! B2 schema knife: the `estimate_table_rows_impl` forwarding body is gone, so
//! these tests drive the REAL `#[tauri::command]` entry through
//! `schema/command_test_host` (a `tauri::test::mock_app` managing the
//! harness's `AppState`; the mock driver stays programmable through the
//! keepalive).

use super::command_test_host::host_command_app;
use super::*;
use crate::testing::app_state_test_support::TestAppState;

/// The estimate command, and the strategies its drivers declare.
mod count_strategy {
    use super::*;
    use crate::testing::mock_driver::MockDriverOptions;
    use datazen_driver_api::CountStrategy;

    #[tokio::test]
    async fn estimate_table_rows_returns_none_when_driver_cannot_estimate() {
        let test = TestAppState::with_options(MockDriverOptions {
            columns: vec![datazen_driver_api::ColumnSchema {
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
        let (_, conn_id) = test.save_and_connect("estimate-none").await;
        let app = host_command_app(test);
        let calls = app.keep.mock.query_calls();
        let estimate = estimate_table_rows(
            app.state(),
            conn_id,
            "users".into(),
            Some("app".into()),
            None,
        )
        .await
        .expect("a driver without an estimate is not a failure");
        assert_eq!(estimate, None);
        assert_eq!(
            app.keep.mock.query_calls(),
            calls,
            "the command must not run a statement of its own"
        );
    }

    #[tokio::test]
    async fn estimate_table_rows_returns_value_when_driver_estimates() {
        let test = TestAppState::with_options(MockDriverOptions {
            columns: vec![datazen_driver_api::ColumnSchema {
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
        let (_, conn_id) = test.save_and_connect("estimate-value").await;
        let app = host_command_app(test);
        let estimate = estimate_table_rows(
            app.state(),
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
            columns: vec![datazen_driver_api::ColumnSchema {
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
        let (_, conn_id) = test.save_and_connect("estimate-bad").await;
        let app = host_command_app(test);
        let estimate = estimate_table_rows(
            app.state(),
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
        let app = host_command_app(test);
        let error = estimate_table_rows(
            app.state(),
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

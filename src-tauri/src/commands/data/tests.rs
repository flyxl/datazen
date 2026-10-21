use super::test_fixtures::*;
use super::*;
use crate::db::Value;
use crate::testing::app_state::TestAppState;
use std::collections::BTreeMap;

#[tokio::test]
async fn preview_builds_driver_sql_without_opening_a_transaction() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("preview-only").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        table_context("preview-only", &conn_id),
        vec![update_change(1, "Alice", "Updated")],
    )
    .await
    .unwrap();

    assert_eq!(plan.table.db_session_id, conn_id);
    assert_eq!(plan.updates.len(), 1);
    assert!(plan.updates[0].sql_template.contains("UPDATE \"users\""));
    assert!(plan.updates[0].sql_template.contains("\"id\" = ?2"));
    assert!(!plan.updates[0].sql_template.contains("Updated"));
    assert!(!crate::commands::session_transaction_status_impl(
        &test.state,
        plan.table.db_session_id.clone()
    )
    .await
    .unwrap());
}

#[tokio::test]
async fn commit_reuses_plan_fingerprint_and_returns_plan_id() {
    let mut options = crate::testing::app_state::rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    let test = TestAppState::with_resource_provider(options).await;
    let (_, conn_id) = test.save_and_connect("commit-plan").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        table_context("commit-plan", &conn_id),
        vec![update_change(1, "Alice", "Updated"), delete_change(2)],
    )
    .await
    .unwrap();
    let response = commit_pending_changes_impl(
        &test.state,
        CommitPendingChangesRequest {
            db_session_id: conn_id,
            fingerprint: plan.fingerprint.clone(),
            plan: plan.clone(),
        },
    )
    .await
    .unwrap();

    assert_eq!(response.plan_id, plan.plan_id);
    assert_eq!(response.fingerprint, plan.fingerprint);
    assert_eq!(response.statements.len(), 2);
    assert_eq!(response.affected_rows, 2);
}

#[tokio::test]
async fn commit_rejects_stale_fingerprint_before_execution() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("stale-plan").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        table_context("stale-plan", &conn_id),
        vec![update_change(1, "Alice", "Updated")],
    )
    .await
    .unwrap();
    let error = commit_pending_changes_impl(
        &test.state,
        CommitPendingChangesRequest {
            db_session_id: conn_id.clone(),
            fingerprint: "stale".into(),
            plan,
        },
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("fingerprint"));
    assert!(
        !crate::commands::session_transaction_status_impl(&test.state, conn_id)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn preview_rejects_changes_without_primary_key_identity() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("preview-no-pk").await;
    let error = preview_pending_changes_impl(
        &test.state,
        table_context("preview-no-pk", &conn_id),
        vec![PendingRowChange {
            row_identity: BTreeMap::new(),
            original_values: BTreeMap::new(),
            current_values: BTreeMap::from([(
                "name".into(),
                Some(Value::String("Updated".into())),
            )]),
            changed_columns: vec!["name".into()],
            delete_marked: false,
            insert_marked: false,
        }],
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("primary-key"));
}

#[tokio::test]
async fn commit_rejects_database_context_change_without_switching_session() {
    let mut options = crate::testing::app_state::rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    let test = TestAppState::with_options(options).await;
    let (_, conn_id) = test.save_and_connect("context-database").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        RowChangeTableContext {
            database: Some("other_db".into()),
            ..table_context("context-database", &conn_id)
        },
        vec![update_change(1, "Alice", "Updated")],
    )
    .await
    .unwrap();

    let error = commit_pending_changes_impl(
        &test.state,
        CommitPendingChangesRequest {
            db_session_id: conn_id,
            fingerprint: plan.fingerprint.clone(),
            plan,
        },
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("Database context changed"));
    assert!(test.mock.use_database_calls().is_empty());
}

#[tokio::test]
async fn commit_allows_concrete_table_schema_when_connection_schema_is_unspecified() {
    let mut options = crate::testing::app_state::rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    let test = TestAppState::with_resource_provider(options).await;
    let (_, conn_id) = test.save_and_connect("context-schema-default").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        RowChangeTableContext {
            schema: Some("public".into()),
            ..table_context("context-schema-default", &conn_id)
        },
        vec![update_change(1, "Alice", "Updated")],
    )
    .await
    .unwrap();

    commit_pending_changes_impl(
        &test.state,
        CommitPendingChangesRequest {
            db_session_id: conn_id,
            fingerprint: plan.fingerprint.clone(),
            plan,
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn commit_rejects_explicit_schema_context_change() {
    let mut options = crate::testing::app_state::rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    let test = TestAppState::with_options(options).await;
    let mut config = crate::testing::app_state::sample_postgres_config("context-schema-explicit");
    config.schema = Some("private".into());
    test.store.save_connection(config).await.unwrap();
    let conn_id = test.connect_config("context-schema-explicit").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        RowChangeTableContext {
            schema: Some("public".into()),
            ..table_context("context-schema-explicit", &conn_id)
        },
        vec![update_change(1, "Alice", "Updated")],
    )
    .await
    .unwrap();

    let error = commit_pending_changes_impl(
        &test.state,
        CommitPendingChangesRequest {
            db_session_id: conn_id,
            fingerprint: plan.fingerprint.clone(),
            plan,
        },
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("Schema context changed"));
}

#[tokio::test]
async fn commit_row_updates_rejects_legacy_write() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("data-cfg").await;
    let error = commit_row_updates_impl(
        &test.state,
        conn_id,
        "users".into(),
        vec![RowUpdateBatch {
            set_columns: vec![CellUpdate {
                column: "name".into(),
                value: Some(Value::String("Alice".into())),
            }],
            pk_columns: vec![CellUpdate {
                column: "id".into(),
                value: Some(Value::Integer(1)),
            }],
        }],
    )
    .await
    .unwrap_err();
    assert_legacy_write_disabled(error);
}

#[tokio::test]
async fn commit_row_updates_rejects_legacy_write_without_session() {
    let test = TestAppState::new().await;
    let error = commit_row_updates_impl(&test.state, "missing".into(), "users".into(), vec![])
        .await
        .unwrap_err();
    assert_legacy_write_disabled(error);
}

#[tokio::test]
async fn commit_row_updates_rejects_legacy_write_for_read_only_connection() {
    let test = with_parameterized_driver().await;
    let mut config = crate::testing::app_state::sample_postgres_config("ro-edit");
    config.read_only = true;
    test.store.save_connection(config).await.unwrap();
    let conn_id = test.connect_config("ro-edit").await;
    let err = commit_row_updates_impl(
        &test.state,
        conn_id,
        "users".into(),
        vec![RowUpdateBatch {
            set_columns: vec![CellUpdate {
                column: "name".into(),
                value: Some(Value::String("x".into())),
            }],
            pk_columns: vec![CellUpdate {
                column: "id".into(),
                value: Some(Value::Integer(1)),
            }],
        }],
    )
    .await
    .unwrap_err();
    assert_legacy_write_disabled(err);
}

#[tokio::test]
async fn commit_row_updates_does_not_join_open_session_transaction() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("tx-edit").await;
    crate::commands::begin_session_transaction_impl(&test.state, conn_id.clone())
        .await
        .unwrap();
    let error = commit_row_updates_impl(
        &test.state,
        conn_id.clone(),
        "users".into(),
        vec![RowUpdateBatch {
            set_columns: vec![CellUpdate {
                column: "name".into(),
                value: Some(Value::String("Bob".into())),
            }],
            pk_columns: vec![CellUpdate {
                column: "id".into(),
                value: Some(Value::Integer(1)),
            }],
        }],
    )
    .await
    .unwrap_err();
    assert_legacy_write_disabled(error);
    assert!(
        crate::commands::session_transaction_status_impl(&test.state, conn_id.clone())
            .await
            .unwrap()
    );
    crate::commands::commit_session_transaction_impl(&test.state, conn_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn commit_row_deletes_rejects_legacy_write() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("data-del").await;
    let error = commit_row_deletes_impl(
        &test.state,
        conn_id,
        "users".into(),
        vec![RowDeleteBatch {
            pk_columns: vec![CellUpdate {
                column: "id".into(),
                value: Some(Value::Integer(1)),
            }],
        }],
    )
    .await
    .unwrap_err();
    assert_legacy_write_disabled(error);
}

#[tokio::test]
async fn commit_row_deletes_rejects_legacy_write_for_read_only_connection() {
    let test = with_parameterized_driver().await;
    let mut config = crate::testing::app_state::sample_postgres_config("ro-del");
    config.read_only = true;
    test.store.save_connection(config).await.unwrap();
    let conn_id = test.connect_config("ro-del").await;
    let err = commit_row_deletes_impl(
        &test.state,
        conn_id,
        "users".into(),
        vec![RowDeleteBatch {
            pk_columns: vec![CellUpdate {
                column: "id".into(),
                value: Some(Value::Integer(1)),
            }],
        }],
    )
    .await
    .unwrap_err();
    assert_legacy_write_disabled(err);
}

#[tokio::test]
async fn commit_row_deletes_rejects_legacy_write_before_pk_validation() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("data-del-empty-pk").await;
    let err = commit_row_deletes_impl(
        &test.state,
        conn_id,
        "users".into(),
        vec![RowDeleteBatch { pk_columns: vec![] }],
    )
    .await
    .unwrap_err();
    assert_legacy_write_disabled(err);
}

#[tokio::test]
async fn empty_identity_is_rejected_before_any_sql_is_generated() {
    // `parameterized_writes` stays false: every builder call is unsupported.
    let test = TestAppState::with_options(crate::testing::app_state::rich_mock_options()).await;
    let (_, conn_id) = test.save_and_connect("guard-before-sql").await;

    let mut delete = delete_change(1);
    delete.row_identity = BTreeMap::new();
    let error = preview_pending_changes_impl(
        &test.state,
        table_context("guard-before-sql", &conn_id),
        vec![delete],
    )
    .await
    .unwrap_err();

    let message = error.to_string();
    assert!(message.contains("DELETE requires"), "got: {message}");
    assert!(
        !message.contains("parameterized row writes are not supported"),
        "SQL was generated before the identity guard ran: {message}"
    );
    assert_eq!(test.mock.execute_calls(), 0);
    assert!(
        !crate::commands::session_transaction_status_impl(&test.state, conn_id)
            .await
            .unwrap(),
        "a rejected plan must not open a transaction"
    );
}

// ---------------------------------------------------------------------------
// [tester round-4] The four `commit_changes` refusals that now carry a code.
// Each asserts the code is at position zero while the descriptive tail the
// pre-round-4 message had is still present, so substring callers keep working.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_stale_config_revision_is_refused_with_the_stale_plan_code() {
    let mut options = crate::testing::app_state::rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    let test = TestAppState::with_options(options).await;
    let (_, conn_id) = test.save_and_connect("r4-config-revision").await;
    let mut plan = preview_pending_changes_impl(
        &test.state,
        table_context("r4-config-revision", &conn_id),
        vec![update_change(1, "Alice", "Updated")],
    )
    .await
    .unwrap();
    plan.config_revision = Some(plan.config_revision.unwrap().saturating_increment());

    let error = commit_pending_changes_impl(
        &test.state,
        CommitPendingChangesRequest {
            db_session_id: conn_id,
            fingerprint: plan.fingerprint.clone(),
            plan,
        },
    )
    .await
    .unwrap_err()
    .to_string();

    assert!(error.starts_with("grid.commit.stalePlan: "), "got: {error}");
    assert!(
        error.contains("Connection configuration changed; preview the changes again"),
        "got: {error}"
    );
    assert_eq!(test.mock.execute_with_params_calls(), 0);
}

#[tokio::test]
async fn a_database_context_drift_is_refused_with_the_stale_plan_code() {
    let mut options = crate::testing::app_state::rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    let test = TestAppState::with_options(options).await;
    let (_, conn_id) = test.save_and_connect("r4-db-drift").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        RowChangeTableContext {
            database: Some("other_db".into()),
            ..table_context("r4-db-drift", &conn_id)
        },
        vec![update_change(1, "Alice", "Updated")],
    )
    .await
    .unwrap();

    let error = commit_pending_changes_impl(
        &test.state,
        CommitPendingChangesRequest {
            db_session_id: conn_id,
            fingerprint: plan.fingerprint.clone(),
            plan,
        },
    )
    .await
    .unwrap_err()
    .to_string();

    assert!(error.starts_with("grid.commit.stalePlan: "), "got: {error}");
    assert!(
        error.contains("Database context changed; preview the changes again"),
        "got: {error}"
    );
    assert!(test.mock.use_database_calls().is_empty());
}

#[tokio::test]
async fn a_schema_context_drift_is_refused_with_the_stale_plan_code() {
    let mut options = crate::testing::app_state::rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    let test = TestAppState::with_options(options).await;
    let mut config = crate::testing::app_state::sample_postgres_config("r4-schema-drift");
    config.schema = Some("private".into());
    test.store.save_connection(config).await.unwrap();
    let conn_id = test.connect_config("r4-schema-drift").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        RowChangeTableContext {
            schema: Some("public".into()),
            ..table_context("r4-schema-drift", &conn_id)
        },
        vec![update_change(1, "Alice", "Updated")],
    )
    .await
    .unwrap();

    let error = commit_pending_changes_impl(
        &test.state,
        CommitPendingChangesRequest {
            db_session_id: conn_id,
            fingerprint: plan.fingerprint.clone(),
            plan,
        },
    )
    .await
    .unwrap_err()
    .to_string();

    assert!(error.starts_with("grid.commit.stalePlan: "), "got: {error}");
    assert!(
        error.contains("Schema context changed; preview the changes again"),
        "got: {error}"
    );
}

#[tokio::test]
async fn a_read_only_connection_is_refused_with_the_read_only_code() {
    let mut options = crate::testing::app_state::rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    let test = TestAppState::with_options(options).await;
    let mut config = crate::testing::app_state::sample_postgres_config("r4-read-only");
    config.read_only = true;
    test.store.save_connection(config).await.unwrap();
    let conn_id = test.connect_config("r4-read-only").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        table_context("r4-read-only", &conn_id),
        vec![update_change(1, "Alice", "Updated")],
    )
    .await
    .unwrap();

    let error = commit_pending_changes_impl(
        &test.state,
        CommitPendingChangesRequest {
            db_session_id: conn_id,
            fingerprint: plan.fingerprint.clone(),
            plan,
        },
    )
    .await
    .unwrap_err()
    .to_string();

    assert!(error.starts_with("grid.cell.readOnly: "), "got: {error}");
    assert!(
        error.contains("Connection is read-only; row changes are not allowed"),
        "got: {error}"
    );
    assert_eq!(test.mock.execute_with_params_calls(), 0);
}

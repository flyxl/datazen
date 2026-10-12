use super::*;
use crate::testing::app_state::TestAppState;

async fn with_parameterized_driver() -> TestAppState {
    let mut options = crate::testing::app_state::rich_mock_options();
    options.parameterized_writes = true;
    TestAppState::with_options(options).await
}

fn table_context(connection_id: &str, db_session_id: &str) -> RowChangeTableContext {
    RowChangeTableContext {
        connection_id: connection_id.into(),
        db_session_id: db_session_id.into(),
        driver_type: "postgres".into(),
        database: Some("app".into()),
        schema: None,
        table: "users".into(),
    }
}

fn update_change(id: i64, original: &str, current: &str) -> PendingRowChange {
    PendingRowChange {
        row_identity: BTreeMap::from([("id".into(), Some(Value::Integer(id)))]),
        original_values: BTreeMap::from([("name".into(), Some(Value::String(original.into())))]),
        current_values: BTreeMap::from([("name".into(), Some(Value::String(current.into())))]),
        changed_columns: vec!["name".into()],
        delete_marked: false,
    }
}

fn delete_change(id: i64) -> PendingRowChange {
    PendingRowChange {
        row_identity: BTreeMap::from([("id".into(), Some(Value::Integer(id)))]),
        original_values: BTreeMap::new(),
        current_values: BTreeMap::new(),
        changed_columns: Vec::new(),
        delete_marked: true,
    }
}

fn composite_update_change(tenant_id: i64, id: i64, next_id: Option<i64>) -> PendingRowChange {
    let mut current_values =
        BTreeMap::from([("name".into(), Some(Value::String("Updated".into())))]);
    let mut changed_columns = vec!["name".into()];
    if let Some(next_id) = next_id {
        current_values.insert("id".into(), Some(Value::Integer(next_id)));
        changed_columns.push("id".into());
    }
    PendingRowChange {
        row_identity: BTreeMap::from([
            ("tenant_id".into(), Some(Value::Integer(tenant_id))),
            ("id".into(), Some(Value::Integer(id))),
        ]),
        original_values: BTreeMap::new(),
        current_values,
        changed_columns,
        delete_marked: false,
    }
}

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
    let test = TestAppState::with_options(options).await;
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
        }],
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("primary-key"));
}

#[test]
fn canonicalize_rejects_null_and_duplicate_composite_identities() {
    let null_identity = PendingRowChange {
        row_identity: BTreeMap::from([
            ("tenant_id".into(), Some(Value::Integer(1))),
            ("id".into(), None),
        ]),
        original_values: BTreeMap::new(),
        current_values: BTreeMap::from([("name".into(), Some(Value::String("Updated".into())))]),
        changed_columns: vec!["name".into()],
        delete_marked: false,
    };
    let error = canonicalize_changes(&[null_identity]).unwrap_err();
    assert!(error.to_string().contains("non-NULL"));

    let duplicate = composite_update_change(1, 9, None);
    let error = canonicalize_changes(&[duplicate.clone(), duplicate]).unwrap_err();
    assert!(error.to_string().contains("duplicate"));
}

#[test]
fn canonicalize_rejects_composite_current_identity_collision() {
    let first = composite_update_change(1, 9, Some(11));
    let second = composite_update_change(1, 10, Some(11));
    let error = canonicalize_changes(&[first, second]).unwrap_err();
    assert!(error.to_string().contains("collision"));
}

/// [tester] Regression: sort-by-identity uses validated keys without panicking.
#[test]
fn test_tester_canonicalize_sorts_by_identity_key() {
    let changes = vec![
        update_change(3, "c", "c"),
        update_change(1, "a", "a"),
        update_change(2, "b", "b"),
    ];
    let canonical = canonicalize_changes(&changes).unwrap();
    let ids: Vec<i64> = canonical
        .iter()
        .map(|change| match change.row_identity.get("id") {
            Some(Some(Value::Integer(id))) => *id,
            other => panic!("expected integer id, got {other:?}"),
        })
        .collect();
    assert_eq!(ids, vec![1, 2, 3]);
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
    let test = TestAppState::with_options(options).await;
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
async fn commit_row_updates_success() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("data-cfg").await;
    commit_row_updates_impl(
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
    .unwrap();
}

#[tokio::test]
async fn commit_row_updates_not_connected_errors() {
    let test = TestAppState::new().await;
    assert!(
        commit_row_updates_impl(&test.state, "missing".into(), "users".into(), vec![],)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn commit_row_updates_rejects_read_only_connection() {
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
    assert!(err.to_string().contains("read-only"));
}

#[tokio::test]
async fn commit_row_updates_joins_open_session_transaction() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("tx-edit").await;
    crate::commands::begin_session_transaction_impl(&test.state, conn_id.clone())
        .await
        .unwrap();
    commit_row_updates_impl(
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
    .unwrap();
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
async fn commit_row_deletes_success() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("data-del").await;
    commit_row_deletes_impl(
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
    .unwrap();
}

#[tokio::test]
async fn commit_row_deletes_rejects_read_only_connection() {
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
    assert!(err.to_string().contains("read-only"));
}

#[tokio::test]
async fn commit_row_deletes_rejects_empty_pk() {
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
    assert!(err.to_string().contains("primary-key"));
}

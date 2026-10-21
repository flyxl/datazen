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
        insert_marked: false,
    }
}

fn delete_change(id: i64) -> PendingRowChange {
    PendingRowChange {
        row_identity: BTreeMap::from([("id".into(), Some(Value::Integer(id)))]),
        original_values: BTreeMap::new(),
        current_values: BTreeMap::new(),
        changed_columns: Vec::new(),
        delete_marked: true,
        insert_marked: false,
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
        insert_marked: false,
    }
}

fn assert_legacy_write_disabled(error: crate::commands::error::CommandError) {
    assert!(matches!(
        error,
        crate::commands::error::CommandError::Validation(message)
            if message.contains("Legacy row writes are disabled")
    ));
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
        insert_marked: false,
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

fn insert_change(values: &[(&str, Option<Value>)]) -> PendingRowChange {
    PendingRowChange {
        row_identity: BTreeMap::new(),
        original_values: BTreeMap::new(),
        current_values: values
            .iter()
            .map(|(column, value)| ((*column).to_string(), value.clone()))
            .collect(),
        changed_columns: values
            .iter()
            .map(|(column, _)| (*column).to_string())
            .collect(),
        delete_marked: false,
        insert_marked: true,
    }
}

fn insert_capable_options() -> crate::testing::mock_driver::MockDriverOptions {
    let mut options = crate::testing::app_state::rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    options
}

#[tokio::test]
async fn insert_preview_builds_statement_with_only_written_columns() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("insert-preview").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        table_context("insert-preview", &conn_id),
        vec![insert_change(&[
            ("name", Some(Value::String("Ada".into()))),
            ("note", None),
        ])],
    )
    .await
    .unwrap();

    assert_eq!(plan.inserts.len(), 1);
    assert_eq!(plan.updates.len(), 0);
    assert_eq!(plan.deletes.len(), 0);
    let statement = &plan.inserts[0];
    assert!(statement.row_identity.is_empty());
    assert!(statement.original_values.is_empty());
    assert_eq!(statement.changed_columns, vec!["name", "note"]);
    assert_eq!(
        statement.sql_template,
        "INSERT INTO \"users\" (\"name\", \"note\") VALUES (?1, ?2)"
    );
    assert!(statement
        .parameter_summary
        .iter()
        .any(|line| line == "INSERT name=\"Ada\""));
    assert!(statement
        .parameter_summary
        .iter()
        .any(|line| line == "INSERT note=NULL"));
    assert!(!crate::commands::session_transaction_status_impl(
        &test.state,
        plan.table.db_session_id.clone()
    )
    .await
    .unwrap());
}

#[tokio::test]
async fn insert_all_unset_row_is_a_default_values_statement() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("insert-defaults").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        table_context("insert-defaults", &conn_id),
        vec![insert_change(&[])],
    )
    .await
    .unwrap();

    // An all-Unset new row is legal (DEFAULT VALUES), not "no change" to drop.
    assert_eq!(plan.inserts.len(), 1);
    assert_eq!(
        plan.inserts[0].sql_template,
        "INSERT INTO \"users\" DEFAULT VALUES"
    );
    assert!(plan.inserts[0].changed_columns.is_empty());
}

#[tokio::test]
async fn insert_plan_keeps_request_order_between_rows() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("insert-order").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        table_context("insert-order", &conn_id),
        vec![
            insert_change(&[("name", Some(Value::String("first".into())))]),
            insert_change(&[("name", Some(Value::String("second".into())))]),
        ],
    )
    .await
    .unwrap();

    let names: Vec<String> = plan
        .inserts
        .iter()
        .map(|statement| match statement.current_values.get("name") {
            Some(Some(Value::String(name))) => name.clone(),
            other => panic!("expected string name, got {other:?}"),
        })
        .collect();
    assert_eq!(names, vec!["first", "second"]);
}

#[tokio::test]
async fn insert_fingerprint_is_reconstructible_from_the_plan_alone() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("insert-fingerprint").await;
    let context = table_context("insert-fingerprint", &conn_id);
    let plan = preview_pending_changes_impl(
        &test.state,
        context.clone(),
        vec![
            insert_change(&[("name", Some(Value::String("Ada".into()))), ("note", None)]),
            update_change(1, "Alice", "Updated"),
        ],
    )
    .await
    .unwrap();

    let rebuilt = canonicalize_changes(&plan_changes(&plan)).unwrap();
    assert_eq!(
        changes_fingerprint(&plan.table, &rebuilt).unwrap(),
        plan.fingerprint,
        "the fingerprint must be reproducible from RowChangePlan alone"
    );

    let other = preview_pending_changes_impl(
        &test.state,
        context,
        vec![insert_change(&[(
            "name",
            Some(Value::String("Grace".into())),
        )])],
    )
    .await
    .unwrap();
    assert_ne!(other.fingerprint, plan.fingerprint);
}

#[tokio::test]
async fn reject_empty_identity_for_update_and_delete() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("reject-empty-identity").await;

    let mut update = update_change(1, "Alice", "Updated");
    update.row_identity = BTreeMap::new();
    let error = preview_pending_changes_impl(
        &test.state,
        table_context("reject-empty-identity", &conn_id),
        vec![update],
    )
    .await
    .unwrap_err();
    let message = error.to_string();
    assert!(message.contains("UPDATE requires"));
    assert!(message.contains("primary-key"));

    let mut delete = delete_change(1);
    delete.row_identity = BTreeMap::new();
    let error = preview_pending_changes_impl(
        &test.state,
        table_context("reject-empty-identity", &conn_id),
        vec![delete],
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("DELETE requires"));
}

#[tokio::test]
async fn reject_null_identity_column_names_the_column() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("reject-null-identity").await;
    let mut update = update_change(1, "Alice", "Updated");
    update.row_identity.insert("tenant".into(), None);

    let error = preview_pending_changes_impl(
        &test.state,
        table_context("reject-null-identity", &conn_id),
        vec![update],
    )
    .await
    .unwrap_err();
    let message = error.to_string();
    assert!(message.contains("NULL primary-key values: tenant"));
}

#[tokio::test]
async fn insert_rejects_a_delete_marker_on_a_new_row() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("insert-conflict").await;
    let mut change = insert_change(&[("name", Some(Value::String("Ada".into())))]);
    change.delete_marked = true;

    let error = preview_pending_changes_impl(
        &test.state,
        table_context("insert-conflict", &conn_id),
        vec![change],
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("conflictingIntents"));
}

#[tokio::test]
async fn commit_executes_insert_only_plan_instead_of_returning_empty_success() {
    let test = TestAppState::with_resource_provider(insert_capable_options()).await;
    let (_, conn_id) = test.save_and_connect("insert-only").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        table_context("insert-only", &conn_id),
        vec![insert_change(&[(
            "name",
            Some(Value::String("Ada".into())),
        )])],
    )
    .await
    .unwrap();

    let response = commit_pending_changes_impl(
        &test.state,
        CommitPendingChangesRequest {
            db_session_id: conn_id,
            fingerprint: plan.fingerprint.clone(),
            plan,
        },
    )
    .await
    .unwrap();

    assert_eq!(response.affected_rows, 1);
    assert_eq!(response.statements.len(), 1);
    assert_eq!(response.statements[0].operation, "insert");
    assert!(response.statements[0].row_identity.is_empty());
    // One bound execution, one commit: the plan was not "successfully" skipped.
    assert_eq!(test.mock.execute_with_params_calls(), 1);
    assert_eq!(test.mock.commit_calls(), 1);
    assert_eq!(test.mock.open_transaction_count(), 0);
}

#[tokio::test]
async fn insert_and_update_share_one_transaction_and_one_commit() {
    let test = TestAppState::with_resource_provider(insert_capable_options()).await;
    let (_, conn_id) = test.save_and_connect("insert-and-update").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        table_context("insert-and-update", &conn_id),
        vec![
            insert_change(&[("name", Some(Value::String("Ada".into())))]),
            update_change(1, "Alice", "Updated"),
        ],
    )
    .await
    .unwrap();

    let response = commit_pending_changes_impl(
        &test.state,
        CommitPendingChangesRequest {
            db_session_id: conn_id,
            fingerprint: plan.fingerprint.clone(),
            plan,
        },
    )
    .await
    .unwrap();

    assert_eq!(response.affected_rows, 2);
    assert_eq!(response.statements.len(), 2);
    // inserts → updates: the new row is written before the existing one.
    assert_eq!(response.statements[0].operation, "insert");
    assert_eq!(response.statements[1].operation, "update");
    assert_eq!(test.mock.execute_with_params_calls(), 2);
    assert_eq!(test.mock.commit_calls(), 1);
    assert_eq!(test.mock.open_transaction_count(), 0);
}

#[tokio::test]
async fn insert_rolls_back_the_whole_batch_on_failure() {
    let mut options = insert_capable_options();
    options.execute_rows_affected = 2;
    let test = TestAppState::with_resource_provider(options).await;
    let (_, conn_id) = test.save_and_connect("insert-rollback").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        table_context("insert-rollback", &conn_id),
        vec![
            insert_change(&[("name", Some(Value::String("Ada".into())))]),
            update_change(1, "Alice", "Updated"),
        ],
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

    assert!(error.to_string().contains("affected 2 rows"));
    assert_eq!(test.mock.commit_calls(), 0);
    assert_eq!(test.mock.open_transaction_count(), 0);
}

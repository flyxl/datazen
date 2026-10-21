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

// ---------------------------------------------------------------------------
// [tester] Coverage-driven additions for DB-04 (new rows).
//
// Everything below exists because the shipped tests did not reach the branch,
// or because the assertion could pass without the behaviour being true.
// ---------------------------------------------------------------------------

/// The pre-INSERT struct shape with the exact serde names/order of
/// `062a0d001:src-tauri/src/commands/data.rs`. Recomputing the old payload here
/// is what makes "insert-free fingerprints are byte-identical" checkable
/// without checking out the old commit.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BaselinePendingRowChange {
    row_identity: BTreeMap<String, Option<Value>>,
    original_values: BTreeMap<String, Option<Value>>,
    current_values: BTreeMap<String, Option<Value>>,
    changed_columns: Vec<String>,
    delete_marked: bool,
}

#[derive(Serialize)]
struct BaselineFingerprintPayload<'a> {
    table: &'a RowChangeTableContext,
    changes: &'a [BaselinePendingRowChange],
}

fn baseline_fingerprint(table: &RowChangeTableContext, changes: &[PendingRowChange]) -> String {
    let changes: Vec<BaselinePendingRowChange> = changes
        .iter()
        .map(|change| BaselinePendingRowChange {
            row_identity: change.row_identity.clone(),
            original_values: change.original_values.clone(),
            current_values: change.current_values.clone(),
            changed_columns: change.changed_columns.clone(),
            delete_marked: change.delete_marked,
        })
        .collect();
    let bytes = serde_json::to_vec(&BaselineFingerprintPayload {
        table,
        changes: &changes,
    })
    .unwrap();
    format!("{:x}", Sha256::digest(bytes))
}

fn fingerprint_of(table: &RowChangeTableContext, changes: &[PendingRowChange]) -> String {
    changes_fingerprint(table, &canonicalize_changes(changes).unwrap()).unwrap()
}

/// Backward compatibility, checked against the old algorithm rather than
/// against the new one's own output.
#[test]
fn insert_free_fingerprint_is_byte_identical_to_the_pre_insert_payload() {
    let table = table_context("baseline-conn", "baseline-session");
    let changes = vec![update_change(7, "Alice", "Updated"), delete_change(3)];

    let canonical = canonicalize_changes(&changes).unwrap();
    let actual = changes_fingerprint(&table, &canonical).unwrap();
    let expected = baseline_fingerprint(&table, &canonical);

    assert_eq!(
        actual, expected,
        "an insert-free plan must hash exactly the pre-INSERT payload"
    );

    // The new marker must be absent from the wire form, not merely `false`:
    // serializing it as `insertMarked: false` would change every fingerprint.
    let json = serde_json::to_string(&canonical[0]).unwrap();
    assert!(
        !json.contains("insertMarked"),
        "the marker leaked into the fingerprint payload: {json}"
    );
    assert!(json.contains("deleteMarked"));
}

#[test]
fn insert_values_columns_and_row_order_all_reach_the_fingerprint() {
    let table = table_context("fp-conn", "fp-session");
    let first = insert_change(&[("name", Some(Value::String("first".into())))]);
    let second = insert_change(&[("name", Some(Value::String("second".into())))]);

    let base = fingerprint_of(&table, &[first.clone(), second.clone()]);
    assert_eq!(
        base,
        fingerprint_of(&table, &[first.clone(), second.clone()]),
        "the same plan must hash the same"
    );

    // Swapping two drafts changes execution order (auto-increment allocation),
    // so it must change the fingerprint. `draft_id` does not exist here, so this
    // is the strongest available statement of "order is covered".
    assert_ne!(
        base,
        fingerprint_of(&table, &[second.clone(), first.clone()]),
        "insert row order must be covered by the fingerprint"
    );

    let mut edited = first.clone();
    edited
        .current_values
        .insert("name".into(), Some(Value::String("edited".into())));
    assert_ne!(
        base,
        fingerprint_of(&table, &[edited, second.clone()]),
        "editing a written value must invalidate the confirmed fingerprint"
    );

    // NULL-vs-unwritten is a real intent difference and must be visible too.
    let with_null = insert_change(&[("note", None)]);
    assert_ne!(
        fingerprint_of(&table, &[with_null]),
        fingerprint_of(&table, &[insert_change(&[])]),
        "writing NULL must differ from leaving the column to its default"
    );
}

/// W-2: the identity guard must fire *before* SQL generation. Rendering SQL on
/// this driver fails, so a driver error would prove the guard ran too late.
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

/// The guard must not misfire on new rows, which have no identity by design.
#[tokio::test]
async fn the_identity_guard_name_only_the_offending_operation() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("guard-mixed").await;

    let mut delete = delete_change(1);
    delete.row_identity = BTreeMap::new();
    let error = preview_pending_changes_impl(
        &test.state,
        table_context("guard-mixed", &conn_id),
        vec![
            insert_change(&[("name", Some(Value::String("Ada".into())))]),
            delete,
        ],
    )
    .await
    .unwrap_err();

    let message = error.to_string();
    assert!(message.contains("DELETE requires"), "got: {message}");
    assert!(
        !message.contains("INSERT requires"),
        "the new row was wrongly subject to the identity guard: {message}"
    );
}

#[tokio::test]
async fn mixed_insert_update_delete_plan_runs_in_insert_update_delete_order() {
    let test = TestAppState::with_resource_provider(insert_capable_options()).await;
    let (_, conn_id) = test.save_and_connect("mixed-plan").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        table_context("mixed-plan", &conn_id),
        vec![
            delete_change(9),
            update_change(1, "Alice", "Updated"),
            insert_change(&[("name", Some(Value::String("Ada".into())))]),
        ],
    )
    .await
    .unwrap();

    assert_eq!(plan.inserts.len(), 1);
    assert_eq!(plan.updates.len(), 1);
    assert_eq!(plan.deletes.len(), 1);

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

    let operations: Vec<&str> = response
        .statements
        .iter()
        .map(|statement| statement.operation.as_str())
        .collect();
    assert_eq!(
        operations,
        vec!["insert", "update", "delete"],
        "execution order is fixed: parent rows first, deletes last"
    );
    assert_eq!(response.affected_rows, 3);
    assert!(response
        .statements
        .iter()
        .all(|statement| statement.affected_rows == 1));
    assert_eq!(test.mock.execute_with_params_calls(), 3);
    assert_eq!(test.mock.commit_calls(), 1);
    assert_eq!(test.mock.open_transaction_count(), 0);
}

#[tokio::test]
async fn insert_with_no_written_column_executes_default_values() {
    let test = TestAppState::with_resource_provider(insert_capable_options()).await;
    let (_, conn_id) = test.save_and_connect("insert-empty-columns").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        table_context("insert-empty-columns", &conn_id),
        vec![insert_change(&[])],
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
    assert_eq!(response.statements[0].operation, "insert");
    assert!(response.statements[0].row_identity.is_empty());
    assert_eq!(response.statements[0].affected_rows, 1);
    assert_eq!(test.mock.execute_with_params_calls(), 1);
    assert_eq!(test.mock.commit_calls(), 1);
}

#[tokio::test]
async fn commit_rejects_an_insert_plan_edited_after_preview() {
    let test = TestAppState::with_resource_provider(insert_capable_options()).await;
    let (_, conn_id) = test.save_and_connect("insert-tampered").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        table_context("insert-tampered", &conn_id),
        vec![insert_change(&[(
            "name",
            Some(Value::String("Ada".into())),
        )])],
    )
    .await
    .unwrap();

    let mut tampered = plan.clone();
    tampered.inserts[0]
        .current_values
        .insert("name".into(), Some(Value::String("Mallory".into())));

    let error = commit_pending_changes_impl(
        &test.state,
        CommitPendingChangesRequest {
            db_session_id: conn_id,
            fingerprint: plan.fingerprint.clone(),
            plan: tampered,
        },
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("fingerprint"), "got: {error}");
    assert_eq!(
        test.mock.execute_with_params_calls(),
        0,
        "a stale plan must be rejected before anything is written"
    );
}

/// Rebuilding the plan from its own metadata must be a fixed point.
#[tokio::test]
async fn canonicalizing_the_rebuilt_plan_is_idempotent_for_new_rows() {
    let test = with_parameterized_driver().await;
    let (_, conn_id) = test.save_and_connect("insert-rebuild").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        table_context("insert-rebuild", &conn_id),
        vec![
            insert_change(&[("name", Some(Value::String("Ada".into()))), ("note", None)]),
            delete_change(4),
        ],
    )
    .await
    .unwrap();

    let once = canonicalize_changes(&plan_changes(&plan)).unwrap();
    let twice = canonicalize_changes(&once).unwrap();
    assert_eq!(
        changes_fingerprint(&plan.table, &once).unwrap(),
        plan.fingerprint
    );
    assert_eq!(
        changes_fingerprint(&plan.table, &twice).unwrap(),
        plan.fingerprint
    );
    assert!(
        once.iter().any(|change| change.insert_marked),
        "the rebuild dropped the new row"
    );
    // A new row's identity only exists after execution; the rebuilt plan must
    // not invent one out of `current_values`.
    for change in once.iter().filter(|change| change.insert_marked) {
        assert!(change.row_identity.is_empty());
        assert!(change.original_values.is_empty());
    }
}

/// The caller's explicit transaction must swallow the inserts, not a new one.
#[tokio::test]
async fn insert_joins_the_callers_manual_transaction_instead_of_opening_one() {
    let test = TestAppState::with_resource_provider(insert_capable_options()).await;
    let (_, conn_id) = test.save_and_connect("insert-manual-tx").await;
    let plan = preview_pending_changes_impl(
        &test.state,
        table_context("insert-manual-tx", &conn_id),
        vec![
            insert_change(&[("name", Some(Value::String("Ada".into())))]),
            update_change(1, "Alice", "Updated"),
        ],
    )
    .await
    .unwrap();

    crate::commands::begin_session_transaction_impl(&test.state, conn_id.clone())
        .await
        .unwrap();
    assert_eq!(test.mock.open_transaction_count(), 1);
    let commits_before = test.mock.commit_calls();

    let platform = test.state.platform_require().unwrap();
    let scope = platform.begin_call();
    let runtime = platform.runtime().unwrap();
    let handle = runtime
        .get_session_by_id(scope.context(), &conn_id)
        .await
        .unwrap()
        .handle;

    let response = commit_pending_changes_with_handle(
        &test.state,
        CommitPendingChangesRequest {
            db_session_id: conn_id.clone(),
            fingerprint: plan.fingerprint.clone(),
            plan,
        },
        Some(handle),
    )
    .await
    .unwrap();

    assert_eq!(response.affected_rows, 2);
    assert_eq!(
        test.mock.execute_with_params_calls(),
        2,
        "insert + update must really run inside the caller's transaction"
    );
    assert_eq!(
        test.mock.commit_calls(),
        commits_before,
        "the ChangeSet must not commit the caller's transaction"
    );
    assert_eq!(
        test.mock.open_transaction_count(),
        1,
        "no second transaction may be opened"
    );
    assert!(
        crate::commands::session_transaction_status_impl(&test.state, conn_id.clone())
            .await
            .unwrap(),
        "the caller keeps control of its transaction"
    );
    crate::commands::rollback_session_transaction_impl(&test.state, conn_id)
        .await
        .unwrap();
}

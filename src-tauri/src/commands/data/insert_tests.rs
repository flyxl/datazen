//! New-row (INSERT) plan, fingerprint and execution tests.
//!
//! Split out of `tests.rs` when that file crossed the repository's 800-line
//! ceiling. The cases, their names and their assertions are moved verbatim.

use super::test_fixtures::*;
use super::*;
use crate::db::Value;
use crate::testing::app_state::TestAppState;
use datazen_feature_core::data_browsing::{
    canonicalize_changes, changes_fingerprint, plan_changes,
};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// [tester] Coverage-driven additions for DB-04 (new rows).
//
// Everything below exists because the shipped tests did not reach the branch,
// or because the assertion could pass without the behaviour being true.
// ---------------------------------------------------------------------------

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
    assert!(
        error.to_string().contains("grid.commit.affectedMismatch: "),
        "got: {error}"
    );
    assert_eq!(test.mock.commit_calls(), 0);
    assert_eq!(test.mock.open_transaction_count(), 0);
}

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

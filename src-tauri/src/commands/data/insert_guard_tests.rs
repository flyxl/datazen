//! Guard tests for the new-row/identity contract.
//!
//! These live beside `data/tests.rs` rather than inside it: the column-metadata
//! guard (W-3), unknown-column refusal (V-2), the insert/identity conflict
//! (IB-D2) and the returning-degradation warning (IB-D4) are the review round's
//! additions, and keeping them in their own file also keeps both suites under
//! the repository's 800-line ceiling.

use super::*;
use crate::db::Value;
use crate::testing::app_state::{rich_mock_options, TestAppState};
use crate::testing::mock_driver::MockDriverOptions;
use datazen_driver_api::ColumnSchema;
use std::collections::BTreeMap;

fn column(name: &str, primary_key: bool) -> ColumnSchema {
    ColumnSchema {
        name: name.into(),
        data_type: "text".into(),
        nullable: true,
        default_value: None,
        comment: None,
        is_primary_key: primary_key,
        is_auto_increment: false,
    }
}

/// Fixture whose flat `columns` answer stands in for every table, so a test can
/// choose the primary key it wants to be verified against.
fn table_options(columns: Vec<ColumnSchema>) -> MockDriverOptions {
    let mut options = rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    // The flat `columns` fixture carries its own `primary_keys` list; it is not
    // derived from the flags, so mirror them explicitly.
    options.primary_keys = columns
        .iter()
        .filter(|column| column.is_primary_key)
        .map(|column| column.name.clone())
        .collect();
    options.columns = columns;
    options
}

fn context(connection_id: &str, db_session_id: &str, table: &str) -> RowChangeTableContext {
    RowChangeTableContext {
        connection_id: connection_id.into(),
        db_session_id: db_session_id.into(),
        driver_type: "postgres".into(),
        database: Some("app".into()),
        schema: None,
        table: table.into(),
    }
}

fn update(identity: &[(&str, i64)], column: &str, value: &str) -> PendingRowChange {
    PendingRowChange {
        row_identity: identity
            .iter()
            .map(|(name, key)| ((*name).into(), Some(Value::Integer(*key))))
            .collect(),
        original_values: BTreeMap::new(),
        current_values: BTreeMap::from([((*column).into(), Some(Value::String(value.into())))]),
        changed_columns: vec![column.into()],
        delete_marked: false,
        insert_marked: false,
    }
}

fn new_row(values: &[(&str, Option<Value>)]) -> PendingRowChange {
    PendingRowChange {
        row_identity: BTreeMap::new(),
        original_values: BTreeMap::new(),
        current_values: values
            .iter()
            .map(|(name, value)| ((*name).into(), value.clone()))
            .collect(),
        changed_columns: values.iter().map(|(name, _)| (*name).into()).collect(),
        delete_marked: false,
        insert_marked: true,
    }
}

fn warning_codes(plan: &RowChangePlan) -> Vec<&str> {
    plan.warnings
        .iter()
        .map(|warning| warning.code.as_str())
        .collect()
}

// ---------------------------------------------------------------------------
// W-3: the identity must name exactly the table's primary key
// ---------------------------------------------------------------------------

#[tokio::test]
async fn composite_identity_missing_a_key_column_is_rejected() {
    let test = TestAppState::with_resource_provider(table_options(vec![
        column("tenant_id", true),
        column("order_no", true),
        column("amount", false),
    ]))
    .await;
    let (_, conn_id) = test.save_and_connect("w3-partial").await;

    let error = preview_pending_changes_impl(
        &test.state,
        context("w3-partial", &conn_id, "orders"),
        vec![update(&[("tenant_id", 7)], "amount", "9.99")],
    )
    .await
    .unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("missing primary-key column"),
        "got: {message}"
    );
    assert!(message.contains("order_no"), "got: {message}");
    assert!(
        message.contains("tenant_id, order_no"),
        "the refusal must print the real primary key: {message}"
    );
    assert_eq!(test.mock.get_columns_calls(), 1);
}

#[tokio::test]
async fn composite_identity_with_an_extra_column_is_rejected() {
    let test = TestAppState::with_resource_provider(table_options(vec![
        column("tenant_id", true),
        column("order_no", true),
        column("id", false),
        column("amount", false),
    ]))
    .await;
    let (_, conn_id) = test.save_and_connect("w3-extra").await;

    let error = preview_pending_changes_impl(
        &test.state,
        context("w3-extra", &conn_id, "orders"),
        vec![update(
            &[("tenant_id", 7), ("order_no", 3), ("id", 11)],
            "amount",
            "9.99",
        )],
    )
    .await
    .unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("not part of 'orders' primary key"),
        "got: {message}"
    );
    assert!(message.contains("id"), "got: {message}");
}

#[tokio::test]
async fn composite_identity_with_the_full_key_is_accepted() {
    let test = TestAppState::with_resource_provider(table_options(vec![
        column("tenant_id", true),
        column("order_no", true),
        column("amount", false),
    ]))
    .await;
    let (_, conn_id) = test.save_and_connect("w3-full").await;

    let plan = preview_pending_changes_impl(
        &test.state,
        context("w3-full", &conn_id, "orders"),
        vec![update(
            &[("tenant_id", 7), ("order_no", 3)],
            "amount",
            "9.99",
        )],
    )
    .await
    .unwrap();

    assert_eq!(plan.updates.len(), 1);
    assert_eq!(
        plan.updates[0].row_identity.keys().collect::<Vec<_>>(),
        vec!["order_no", "tenant_id"],
        "the plan keeps the submitted identity"
    );
}

#[tokio::test]
async fn single_primary_key_table_still_accepts_its_key() {
    let mut options = rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    let test = TestAppState::with_resource_provider(options).await;
    let (_, conn_id) = test.save_and_connect("w3-single").await;

    let plan = preview_pending_changes_impl(
        &test.state,
        context("w3-single", &conn_id, "users"),
        vec![update(&[("id", 1)], "name", "Ada")],
    )
    .await
    .unwrap();

    assert_eq!(plan.updates.len(), 1);
}

#[tokio::test]
async fn unavailable_schema_rejects_instead_of_skipping_the_guard() {
    let mut options = rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    options
        .column_errors_by_table
        .insert("orders".into(), "permission denied for table orders".into());
    let test = TestAppState::with_resource_provider(options).await;
    let (_, conn_id) = test.save_and_connect("w3-unavailable").await;

    let error = preview_pending_changes_impl(
        &test.state,
        context("w3-unavailable", &conn_id, "orders"),
        vec![update(&[("id", 1)], "name", "Ada")],
    )
    .await
    .unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("Table schema unavailable for 'orders'"),
        "got: {message}"
    );
    assert!(message.contains("permission denied"), "got: {message}");
    assert!(
        message.contains("primary key cannot be verified"),
        "got: {message}"
    );
}

// ---------------------------------------------------------------------------
// V-2: writes must name columns the table really has
// ---------------------------------------------------------------------------

#[tokio::test]
async fn insert_writing_an_unknown_column_is_rejected_by_name() {
    let mut options = rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    let test = TestAppState::with_resource_provider(options).await;
    let (_, conn_id) = test.save_and_connect("v2-unknown").await;

    let error = preview_pending_changes_impl(
        &test.state,
        context("v2-unknown", &conn_id, "users"),
        vec![new_row(&[("nmae", Some(Value::String("Ada".into())))])],
    )
    .await
    .unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("grid.insert.unknownColumn"),
        "got: {message}"
    );
    assert!(message.contains("nmae"), "got: {message}");
}

#[tokio::test]
async fn insert_writing_a_known_nullable_column_is_accepted() {
    let mut options = rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    let test = TestAppState::with_resource_provider(options).await;
    let (_, conn_id) = test.save_and_connect("v2-known").await;

    let plan = preview_pending_changes_impl(
        &test.state,
        context("v2-known", &conn_id, "users"),
        vec![new_row(&[
            ("name", Some(Value::String("Ada".into()))),
            ("note", None),
        ])],
    )
    .await
    .unwrap();

    assert_eq!(plan.inserts.len(), 1);
    assert_eq!(plan.inserts[0].changed_columns, vec!["name", "note"]);
}

// ---------------------------------------------------------------------------
// IB-D2: the insert mark and existing-row state are mutually exclusive
// ---------------------------------------------------------------------------

#[tokio::test]
async fn insert_row_carrying_an_identity_is_rejected() {
    let mut options = rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    let test = TestAppState::with_resource_provider(options).await;
    let (_, conn_id) = test.save_and_connect("d2-identity").await;

    let mut change = new_row(&[("name", Some(Value::String("Ada".into())))]);
    change
        .row_identity
        .insert("id".into(), Some(Value::Integer(1)));

    let error = preview_pending_changes_impl(
        &test.state,
        context("d2-identity", &conn_id, "users"),
        vec![change],
    )
    .await
    .unwrap_err();

    let message = error.to_string();
    assert!(message.contains("conflictingIntents"), "got: {message}");
    assert!(
        message.contains("existing row") && message.contains("new row"),
        "got: {message}"
    );
}

#[tokio::test]
async fn insert_row_carrying_original_values_is_rejected() {
    let mut options = rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    let test = TestAppState::with_resource_provider(options).await;
    let (_, conn_id) = test.save_and_connect("d2-original").await;

    let mut change = new_row(&[("name", Some(Value::String("Ada".into())))]);
    change
        .original_values
        .insert("name".into(), Some(Value::String("Grace".into())));

    let error = preview_pending_changes_impl(
        &test.state,
        context("d2-original", &conn_id, "users"),
        vec![change],
    )
    .await
    .unwrap_err();

    assert!(
        error.to_string().contains("conflictingIntents"),
        "got: {error}"
    );
}

#[tokio::test]
async fn insert_row_with_empty_existing_row_state_is_accepted() {
    let mut options = rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    let test = TestAppState::with_resource_provider(options).await;
    let (_, conn_id) = test.save_and_connect("d2-empty").await;

    let plan = preview_pending_changes_impl(
        &test.state,
        context("d2-empty", &conn_id, "users"),
        vec![new_row(&[("name", Some(Value::String("Ada".into())))])],
    )
    .await
    .unwrap();

    assert_eq!(plan.inserts.len(), 1);
    assert!(plan.inserts[0].row_identity.is_empty());
    assert!(plan.inserts[0].original_values.is_empty());
}

// ---------------------------------------------------------------------------
// IB-D4: say that a new row cannot be located after commit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn insert_without_returning_seam_warns_that_the_page_will_refresh() {
    let mut options = rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    let test = TestAppState::with_resource_provider(options).await;
    let (_, conn_id) = test.save_and_connect("d4-none").await;

    let plan = preview_pending_changes_impl(
        &test.state,
        context("d4-none", &conn_id, "users"),
        vec![new_row(&[("name", Some(Value::String("Ada".into())))])],
    )
    .await
    .unwrap();

    let warning = plan
        .warnings
        .iter()
        .find(|warning| warning.code == "grid.insert.returningUnavailable")
        .expect("the degradation must be announced");
    assert_eq!(warning.severity, "warning");
    assert!(
        warning.message.contains("refreshed") && warning.message.contains("primary key"),
        "got: {}",
        warning.message
    );
}

#[tokio::test]
async fn insert_with_returning_seam_has_no_degradation_warning() {
    let mut options = rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    options.insert_returning_clause = Some("RETURNING id".into());
    let test = TestAppState::with_resource_provider(options).await;
    let (_, conn_id) = test.save_and_connect("d4-returning").await;

    let plan = preview_pending_changes_impl(
        &test.state,
        context("d4-returning", &conn_id, "users"),
        vec![new_row(&[("name", Some(Value::String("Ada".into())))])],
    )
    .await
    .unwrap();

    assert!(
        !warning_codes(&plan).contains(&"grid.insert.returningUnavailable"),
        "got warnings: {:?}",
        warning_codes(&plan)
    );
}

#[tokio::test]
async fn plan_without_inserts_has_no_returning_warning() {
    let mut options = rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    let test = TestAppState::with_resource_provider(options).await;
    let (_, conn_id) = test.save_and_connect("d4-update-only").await;

    let plan = preview_pending_changes_impl(
        &test.state,
        context("d4-update-only", &conn_id, "users"),
        vec![update(&[("id", 1)], "name", "Ada")],
    )
    .await
    .unwrap();

    assert!(
        !warning_codes(&plan).contains(&"grid.insert.returningUnavailable"),
        "got warnings: {:?}",
        warning_codes(&plan)
    );
}

// ---------------------------------------------------------------------------
// [tester round-2] W-3 failure modes the shipped suite leaves open.
//
// The guard must refuse when it cannot learn the primary key, not silently
// skip. The shipped test covers the read-error path; the "context carries no
// database" path is a different branch of the host catalog adapter and was
// uncovered. (The "driver reported no columns" branch is unreachable with the
// current mock fixture: `MockDriver::get_table_columns` appends a `note`
// column, so the adapter never sees an empty list — reported, not worked
// around here.)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_context_without_a_database_is_refused_before_any_catalog_read() {
    let mut options = rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    let test = TestAppState::with_resource_provider(options).await;
    let (_, conn_id) = test.save_and_connect("w3-no-database").await;

    let mut target = context("w3-no-database", &conn_id, "users");
    target.database = None;

    let error = preview_pending_changes_impl(
        &test.state,
        target,
        vec![update(&[("id", 1)], "name", "Ada")],
    )
    .await
    .unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("Table schema unavailable for 'users'"),
        "got: {message}"
    );
    assert!(message.contains("carries no database"), "got: {message}");
    assert!(
        message.contains("primary key cannot be verified"),
        "got: {message}"
    );
    assert_eq!(
        test.mock.get_columns_calls(),
        0,
        "no catalog read may be attempted without a database"
    );
}

#[tokio::test]
async fn an_empty_catalog_answer_is_refused_instead_of_skipping_the_guard() {
    let mut options = rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    // A per-database column tier that answers "no columns" for this table: the
    // mock returns an empty list instead of falling back to a default fixture.
    options
        .columns_by_database
        .insert("app".into(), std::collections::HashMap::new());
    let test = TestAppState::with_resource_provider(options).await;
    let (_, conn_id) = test.save_and_connect("w3-empty-columns").await;

    let error = preview_pending_changes_impl(
        &test.state,
        context("w3-empty-columns", &conn_id, "users"),
        vec![update(&[("id", 1)], "name", "Ada")],
    )
    .await
    .unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("Table schema unavailable for 'users'"),
        "got: {message}"
    );
    assert!(message.contains("reported no columns"), "got: {message}");
    assert!(
        message.contains("primary key cannot be verified"),
        "got: {message}"
    );
}

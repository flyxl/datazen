//! Round-3 contract tests: error codes, the delete/write conflict, and the
//! connection-facts gate.
//!
//! These are independent re-derivations of the round-3 claims, written against
//! the shared layer only (no host imports). They answer three questions the
//! shipped tests leave open:
//!
//! - is `grid.<domain>.<reason>: ` really at **position zero** on every path
//!   the contract names, or only on the two the fix happened to touch?
//! - does the delete/write refusal happen **before** SQL is rendered, and does
//!   it refuse only the contradictory payloads (no over-rejection)?
//! - is the driver-type gate exact, and does it run where a second transport
//!   cannot forget it?

use async_trait::async_trait;
use datazen::data_browsing::change_set::validate_immutable_plan;
use datazen::data_browsing::{
    build_row_change_plan, plan_changes, preview_changes, validate_connection_facts,
    ConnectionFacts, PendingRowChange, RowChangeTableContext, TableColumnSource,
};
use datazen::schema::CachedColumns;
use datazen::table_editing::TableEditingError as FeatureError;
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{ColumnSchema, Value};
use std::collections::BTreeMap;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn column(name: &str, data_type: &str, is_pk: bool) -> ColumnSchema {
    ColumnSchema {
        name: name.into(),
        data_type: data_type.into(),
        nullable: !is_pk,
        default_value: None,
        comment: None,
        is_primary_key: is_pk,
        is_auto_increment: false,
    }
}

fn catalog() -> CachedColumns {
    CachedColumns {
        columns: vec![
            column("id", "integer", true),
            column("name", "text", false),
            column("note", "text", false),
        ],
        primary_keys: vec!["id".into()],
        table_name: "users".into(),
        cached_at: Instant::now(),
    }
}

fn table(tag: &str) -> RowChangeTableContext {
    RowChangeTableContext {
        connection_id: format!("conn-{tag}"),
        db_session_id: format!("sess-{tag}"),
        driver_type: "postgres".into(),
        database: Some("app".into()),
        schema: None,
        table: "users".into(),
    }
}

fn driver_with(parameterized_writes: bool) -> std::sync::Arc<MockDriver> {
    MockDriver::new(
        "postgres",
        MockDriverOptions {
            parameterized_writes,
            ..MockDriverOptions::default()
        },
    )
}

fn update(name: &str) -> PendingRowChange {
    let mut identity = BTreeMap::new();
    identity.insert("id".to_string(), Some(Value::Integer(1)));
    let mut original = BTreeMap::new();
    original.insert("id".to_string(), Some(Value::Integer(1)));
    original.insert("name".to_string(), Some(Value::String("Ada".into())));
    let mut current = BTreeMap::new();
    current.insert("id".to_string(), Some(Value::Integer(1)));
    current.insert("name".to_string(), Some(Value::String(name.into())));
    PendingRowChange {
        row_identity: identity,
        original_values: original,
        current_values: current,
        changed_columns: vec!["name".into()],
        delete_marked: false,
        insert_marked: false,
    }
}

fn delete() -> PendingRowChange {
    let mut identity = BTreeMap::new();
    identity.insert("id".to_string(), Some(Value::Integer(4)));
    let mut original = BTreeMap::new();
    original.insert("id".to_string(), Some(Value::Integer(4)));
    original.insert("name".to_string(), Some(Value::String("Ada".into())));
    PendingRowChange {
        row_identity: identity,
        original_values: original,
        current_values: BTreeMap::new(),
        changed_columns: Vec::new(),
        delete_marked: true,
        insert_marked: false,
    }
}

fn delete_with_write(column_name: &str, value: &str) -> PendingRowChange {
    let mut change = delete();
    change.changed_columns = vec![column_name.into()];
    change
        .current_values
        .insert(column_name.into(), Some(Value::String(value.into())));
    change
}

fn new_row() -> PendingRowChange {
    let mut current = BTreeMap::new();
    current.insert("name".to_string(), Some(Value::String("Ada".into())));
    PendingRowChange {
        row_identity: BTreeMap::new(),
        original_values: BTreeMap::new(),
        current_values: current,
        changed_columns: vec!["name".into()],
        delete_marked: false,
        insert_marked: true,
    }
}

fn plan_error(changes: &[PendingRowChange], columns: &CachedColumns) -> String {
    build_row_change_plan(driver_with(true).as_ref(), table("plan"), changes, columns)
        .unwrap_err()
        .to_string()
}

// ---------------------------------------------------------------------------
// IB2-D4: `grid.commit.stalePlan` on BOTH fingerprint paths, at position zero
// ---------------------------------------------------------------------------

#[test]
fn stale_plan_from_a_mismatched_fingerprint_is_coded_at_the_start() {
    let columns = catalog();
    let table = table("stale-a");
    let plan = build_row_change_plan(
        driver_with(true).as_ref(),
        table.clone(),
        &[update("Updated")],
        &columns,
    )
    .unwrap();

    // Path (a): the caller hands over a fingerprint that is not the plan's own.
    let message = validate_immutable_plan(
        driver_with(true).as_ref(),
        &table.db_session_id,
        &plan,
        "0000000000000000",
        &columns,
    )
    .unwrap_err()
    .to_string();

    assert!(
        message.starts_with("grid.commit.stalePlan: "),
        "code must be at position zero, got: {message}"
    );
    assert!(
        message.contains("does not match the confirmed plan"),
        "descriptive tail must survive, got: {message}"
    );
    assert!(
        message.contains("fingerprint"),
        "the refusal must still say what mismatched, got: {message}"
    );
}

#[test]
fn stale_plan_from_a_rebuilt_plan_is_coded_and_keeps_its_wording() {
    let columns = catalog();
    let table = table("stale-b");
    let plan = build_row_change_plan(
        driver_with(true).as_ref(),
        table.clone(),
        &[new_row()],
        &columns,
    )
    .unwrap();

    let mut tampered = plan.clone();
    tampered.inserts[0]
        .current_values
        .insert("name".into(), Some(Value::String("Mallory".into())));

    let message = validate_immutable_plan(
        driver_with(true).as_ref(),
        &table.db_session_id,
        &tampered,
        &plan.fingerprint,
        &columns,
    )
    .unwrap_err()
    .to_string();

    assert!(
        message.starts_with("grid.commit.stalePlan: "),
        "code must be at position zero, got: {message}"
    );
    assert!(
        message.contains("stale or was modified"),
        "descriptive tail must survive, got: {message}"
    );
}

// ---------------------------------------------------------------------------
// IB2-D4: `unknownColumn` is an insert-domain code, and the DELETE branch is
// unreachable because the delete/write conflict refuses first
// ---------------------------------------------------------------------------

#[test]
fn insert_unknown_column_gets_the_insert_code_at_the_start() {
    let columns = catalog();
    let mut change = new_row();
    change.changed_columns = vec!["nmae".into()];
    change
        .current_values
        .insert("nmae".into(), Some(Value::String("Ada".into())));

    let message = plan_error(&[change], &columns);

    assert!(
        message.starts_with("grid.insert.unknownColumn: "),
        "got: {message}"
    );
    assert!(message.contains("INSERT"), "got: {message}");
    assert!(
        message.contains("nmae"),
        "the column must be named: {message}"
    );
}

#[test]
fn update_unknown_column_gets_plain_validation_without_an_insert_code() {
    let columns = catalog();
    let mut change = update("Updated");
    change.changed_columns = vec!["nmae".into()];
    change
        .current_values
        .insert("nmae".into(), Some(Value::String("Ada".into())));

    let message = plan_error(&[change], &columns);

    assert!(
        !message.contains("grid."),
        "an UPDATE has no draft, so it must not borrow a grid code: {message}"
    );
    assert!(
        message.contains("UPDATE writes column(s) that do not exist on 'users'"),
        "got: {message}"
    );
    assert!(message.contains("nmae"), "got: {message}");
}

#[test]
fn a_delete_carrying_writes_never_reaches_the_unknown_column_branch() {
    let columns = catalog();
    // Even a column that does not exist is reported as the intent conflict:
    // the delete/write guard runs first, so DELETE never renders the plain
    // "writes column(s) that do not exist" message.
    let message = plan_error(&[delete_with_write("nmae", "x")], &columns);

    assert!(
        message.starts_with("grid.commit.conflictingIntents: "),
        "got: {message}"
    );
    assert!(
        !message.contains("do not exist on"),
        "the unknown-column branch must stay unreachable for DELETE: {message}"
    );
    assert!(
        !message.contains("grid.insert.unknownColumn"),
        "got: {message}"
    );
}

// ---------------------------------------------------------------------------
// IB2-D7: the refusal happens before SQL is rendered
// ---------------------------------------------------------------------------

#[test]
fn delete_with_writes_is_refused_before_any_statement_is_built() {
    // With bound writes disabled, reaching the statement builder would fail
    // with the driver's "parameterized row writes are not supported". Getting
    // the contract refusal instead proves the guard runs first.
    let columns = catalog();
    let message = build_row_change_plan(
        driver_with(false).as_ref(),
        table("d7-order"),
        &[delete_with_write("name", "Ada")],
        &columns,
    )
    .unwrap_err()
    .to_string();

    assert!(
        message.starts_with("grid.commit.conflictingIntents: "),
        "got: {message}"
    );
    assert!(
        !message.contains("parameterized row writes"),
        "no statement may be built before the guard fires: {message}"
    );
    assert!(
        message.contains("cannot also carry column writes"),
        "the refusal must name the conflict: {message}"
    );
    assert!(
        message.contains("clear the staged updates"),
        "the refusal must say how to fix it: {message}"
    );
}

// ---------------------------------------------------------------------------
// IB2-D7: the three delete boundaries (no over-rejection)
// ---------------------------------------------------------------------------

#[test]
fn a_plain_delete_still_plans_and_carries_no_suppression_warning() {
    let columns = catalog();
    let plan = build_row_change_plan(
        driver_with(true).as_ref(),
        table("d7-plain"),
        &[delete()],
        &columns,
    )
    .unwrap();

    assert_eq!(plan.deletes.len(), 1);
    assert!(plan.updates.is_empty());
    let codes: Vec<&str> = plan.warnings.iter().map(|w| w.code.as_str()).collect();
    assert_eq!(codes, vec!["delete-rows"], "the row-count notice stays");
    assert!(
        !codes.contains(&"delete-suppresses-update"),
        "the suppression warning is gone: {codes:?}"
    );
    // The plan must survive its own immutable rebuild (fingerprint fixed point).
    let rebuilt = plan_changes(&plan);
    let again = build_row_change_plan(
        driver_with(true).as_ref(),
        plan.table.clone(),
        &rebuilt,
        &columns,
    )
    .unwrap();
    assert_eq!(again.fingerprint, plan.fingerprint);
}

#[test]
fn a_delete_with_empty_changed_columns_but_staged_values_still_plans() {
    let columns = catalog();
    let mut change = delete();
    // A UI that staged an edit before the row was marked for deletion can leave
    // values behind; without a changed_columns entry it is not a write.
    change
        .current_values
        .insert("name".into(), Some(Value::String("Edited".into())));

    let plan = build_row_change_plan(
        driver_with(true).as_ref(),
        table("d7-empty-columns"),
        &[change],
        &columns,
    )
    .unwrap();

    assert_eq!(plan.deletes.len(), 1);
    assert!(plan.updates.is_empty());
    let codes: Vec<&str> = plan.warnings.iter().map(|w| w.code.as_str()).collect();
    assert!(
        !codes.contains(&"delete-suppresses-update"),
        "a staged value with no changed_columns is not a write: {codes:?}"
    );
}

#[test]
fn a_no_op_write_on_a_deleted_row_is_refused_rather_than_silently_dropped() {
    let columns = catalog();
    // `changed_columns` says "this column was written" even though the value
    // equals the original. The payload still carries both intents, so the
    // refusal is correct; the message says which one to clear.
    let mut change = delete_with_write("name", "Ada");
    change
        .current_values
        .insert("name".into(), Some(Value::String("Ada".into())));

    let message = plan_error(&[change], &columns);

    assert!(
        message.starts_with("grid.commit.conflictingIntents: "),
        "got: {message}"
    );
    assert!(
        message.contains("clear the staged updates"),
        "the user must be told how to proceed: {message}"
    );
}

// ---------------------------------------------------------------------------
// IB2-D2: the shared driver-type gate
// ---------------------------------------------------------------------------

fn facts(database_type: &str) -> ConnectionFacts {
    ConnectionFacts {
        database_type: database_type.into(),
        read_only: false,
        database: Some("app".into()),
        schema: None,
        config_revision: 1,
    }
}

#[test]
fn connection_facts_reject_a_different_driver_type() {
    let table = table("facts");
    let message = validate_connection_facts(&facts("mysql"), &table)
        .unwrap_err()
        .to_string();

    assert!(
        message.contains("Table connection context changed"),
        "got: {message}"
    );
    assert!(
        validate_connection_facts(&facts("postgres"), &table).is_ok(),
        "the same driver type must pass"
    );
}

#[test]
fn connection_facts_compare_the_driver_type_exactly() {
    // Both sides are produced by the same two inputs on the desktop path
    // (`config.database_type` and the request's `driver_type`), so an exact
    // comparison cannot introduce a new refusal there. This pins the fact that
    // the shared gate is byte-exact rather than case-folding: a transport that
    // transmits a display name would be refused, which is the safe direction.
    let table = table("facts-case");

    assert!(validate_connection_facts(&facts("postgres"), &table).is_ok());
    for variant in ["PostgreSQL", "postgresql", "postgres ", "Postgres"] {
        assert!(
            validate_connection_facts(&facts(variant), &table).is_err(),
            "{variant} must not pass as postgres"
        );
    }
}

// ---------------------------------------------------------------------------
// IB2-D3: the comment now matches the order of operations
// ---------------------------------------------------------------------------

struct FailingCatalog;

#[async_trait]
impl TableColumnSource for FailingCatalog {
    async fn table_columns(
        &self,
        _table: &RowChangeTableContext,
    ) -> Result<CachedColumns, FeatureError> {
        Err(FeatureError::Validation(
            "Table schema unavailable for 'users': the cache read failed, so the primary key cannot be verified"
                .into(),
        ))
    }
}

struct WorkingCatalog;

#[async_trait]
impl TableColumnSource for WorkingCatalog {
    async fn table_columns(
        &self,
        _table: &RowChangeTableContext,
    ) -> Result<CachedColumns, FeatureError> {
        Ok(catalog())
    }
}

fn empty_identity_delete() -> PendingRowChange {
    let mut change = delete();
    change.row_identity = BTreeMap::new();
    change
}

#[tokio::test]
async fn a_catalog_failure_is_reported_before_the_identity_shape_check() {
    // The catalog is read first (that is what the corrected doc comment says),
    // so an unreachable catalog reports the schema problem even when the row
    // identity is also empty.
    let message = preview_changes(
        driver_with(true).as_ref(),
        table("d3"),
        &[empty_identity_delete()],
        &FailingCatalog,
        1,
    )
    .await
    .unwrap_err()
    .to_string();

    assert!(
        message.contains("Table schema unavailable"),
        "the catalog failure must win: {message}"
    );
    assert!(
        !message.contains("row identity is empty"),
        "the identity check must not run before the read: {message}"
    );
}

#[tokio::test]
async fn the_same_payload_with_a_working_catalog_reports_the_identity_error() {
    let message = preview_changes(
        driver_with(true).as_ref(),
        table("d3-control"),
        &[empty_identity_delete()],
        &WorkingCatalog,
        1,
    )
    .await
    .unwrap_err()
    .to_string();

    assert!(
        message.contains("row identity is empty"),
        "the shape check still runs once the catalog answers: {message}"
    );
}

// ---------------------------------------------------------------------------
// IB2-D7 on the commit path: the guard is re-applied to the rebuilt plan, so a
// client cannot smuggle a write onto a delete after preview.
// ---------------------------------------------------------------------------

#[test]
fn a_tampered_delete_that_gains_a_write_is_refused_on_the_commit_path() {
    let columns = catalog();
    let table = table("d7-tamper");
    let plan = build_row_change_plan(
        driver_with(true).as_ref(),
        table.clone(),
        &[delete()],
        &columns,
    )
    .unwrap();

    let mut tampered = plan.clone();
    tampered.deletes[0].changed_columns = vec!["name".into()];
    tampered.deletes[0]
        .current_values
        .insert("name".into(), Some(Value::String("injected".into())));

    let message = validate_immutable_plan(
        driver_with(true).as_ref(),
        &table.db_session_id,
        &tampered,
        &plan.fingerprint,
        &columns,
    )
    .unwrap_err()
    .to_string();

    // The rebuild validates payload semantics before it compares fingerprints,
    // so the injected intent — not "stale plan" — is what the user sees.
    assert!(
        message.starts_with("grid.commit.conflictingIntents: "),
        "got: {message}"
    );
    assert!(
        message.contains("cannot also carry column writes"),
        "got: {message}"
    );
}

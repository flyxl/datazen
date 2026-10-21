//! Plan, guard and fingerprint tests for the shared table ChangeSet layer.
//!
//! These used to live in the Desktop host; they moved with the logic so a
//! second transport gets the same protection. Setup is a plain `MockDriver`
//! plus a column fixture — no Tauri state, no runtime session — because these
//! cases assert what the plan/guards decide, not how a host wires them.

use super::change_set::{
    build_row_change_plan, canonicalize_changes, changes_fingerprint, plan_changes,
    validate_immutable_plan, PendingRowChange, RowChangePlan, RowChangeTableContext,
};
use super::{validate_connection_facts, ConnectionFacts};
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{ColumnSchema, Value};
use datazen_runtime::schema_cache::CachedColumns;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;

fn column(name: &str, primary_key: bool, nullable: bool) -> ColumnSchema {
    ColumnSchema {
        name: name.into(),
        data_type: "text".into(),
        nullable,
        default_value: None,
        comment: None,
        is_primary_key: primary_key,
        is_auto_increment: false,
    }
}

fn users_columns() -> CachedColumns {
    CachedColumns {
        columns: vec![
            column("id", true, false),
            column("name", false, true),
            column("note", false, true),
        ],
        primary_keys: vec!["id".into()],
        table_name: "users".into(),
        cached_at: std::time::Instant::now(),
    }
}

fn driver() -> Arc<MockDriver> {
    MockDriver::new(
        "postgres",
        MockDriverOptions {
            execute_rows_affected: 1,
            parameterized_writes: true,
            ..MockDriverOptions::default()
        },
    )
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

fn plan_for(
    table: RowChangeTableContext,
    changes: &[PendingRowChange],
    columns: &CachedColumns,
) -> Result<RowChangePlan, crate::error::FeatureError> {
    build_row_change_plan(driver().as_ref(), table, changes, columns)
}

// ---------------------------------------------------------------------------
// Canonicalization and fingerprints
// ---------------------------------------------------------------------------

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
    // so it must change the fingerprint.
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

#[test]
fn insert_fingerprint_is_reconstructible_from_the_plan_alone() {
    let context = table_context("insert-fingerprint", "insert-fingerprint-session");
    let columns = users_columns();
    let plan = plan_for(
        context.clone(),
        &[
            insert_change(&[("name", Some(Value::String("Ada".into()))), ("note", None)]),
            update_change(1, "Alice", "Updated"),
        ],
        &columns,
    )
    .unwrap();

    let rebuilt = canonicalize_changes(&plan_changes(&plan)).unwrap();
    assert_eq!(
        changes_fingerprint(&plan.table, &rebuilt).unwrap(),
        plan.fingerprint,
        "the fingerprint must be reproducible from RowChangePlan alone"
    );

    let other = plan_for(
        context,
        &[insert_change(&[(
            "name",
            Some(Value::String("Grace".into())),
        )])],
        &columns,
    )
    .unwrap();
    assert_ne!(other.fingerprint, plan.fingerprint);
}

// ---------------------------------------------------------------------------
// Identity guards
// ---------------------------------------------------------------------------

#[test]
fn reject_empty_identity_for_update_and_delete() {
    let columns = users_columns();
    let table = table_context("reject-empty-identity", "reject-empty-identity-session");

    let mut update = update_change(1, "Alice", "Updated");
    update.row_identity = BTreeMap::new();
    let error = plan_for(table.clone(), &[update], &columns).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("UPDATE requires"));
    assert!(message.contains("primary-key"));

    let mut delete = delete_change(1);
    delete.row_identity = BTreeMap::new();
    let error = plan_for(table, &[delete], &columns).unwrap_err();
    assert!(error.to_string().contains("DELETE requires"));
}

#[test]
fn reject_null_identity_column_names_the_column() {
    let columns = users_columns();
    let table = table_context("reject-null-identity", "reject-null-identity-session");
    let mut update = update_change(1, "Alice", "Updated");
    update.row_identity.insert("tenant".into(), None);

    let error = plan_for(table, &[update], &columns).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("NULL primary-key values: tenant"));
}

#[test]
fn composite_identity_missing_a_key_column_is_rejected() {
    let columns = CachedColumns {
        columns: vec![
            column("tenant_id", true, false),
            column("order_no", true, false),
            column("amount", false, true),
        ],
        primary_keys: vec!["tenant_id".into(), "order_no".into()],
        table_name: "orders".into(),
        cached_at: std::time::Instant::now(),
    };
    let table = RowChangeTableContext {
        table: "orders".into(),
        ..table_context("w3-partial", "w3-partial-session")
    };
    let change = PendingRowChange {
        row_identity: BTreeMap::from([("tenant_id".into(), Some(Value::Integer(7)))]),
        original_values: BTreeMap::new(),
        current_values: BTreeMap::from([("amount".into(), Some(Value::String("9.99".into())))]),
        changed_columns: vec!["amount".into()],
        delete_marked: false,
        insert_marked: false,
    };

    let message = plan_for(table, &[change], &columns)
        .unwrap_err()
        .to_string();
    assert!(
        message.contains("missing primary-key column"),
        "got: {message}"
    );
    assert!(message.contains("order_no"), "got: {message}");
    assert!(
        message.contains("tenant_id, order_no"),
        "the refusal must print the real primary key: {message}"
    );
}

#[test]
fn composite_identity_with_an_extra_column_is_rejected() {
    let columns = CachedColumns {
        columns: vec![
            column("tenant_id", true, false),
            column("order_no", true, false),
            column("id", false, false),
            column("amount", false, true),
        ],
        primary_keys: vec!["tenant_id".into(), "order_no".into()],
        table_name: "orders".into(),
        cached_at: std::time::Instant::now(),
    };
    let table = RowChangeTableContext {
        table: "orders".into(),
        ..table_context("w3-extra", "w3-extra-session")
    };
    let change = PendingRowChange {
        row_identity: BTreeMap::from([
            ("tenant_id".into(), Some(Value::Integer(7))),
            ("order_no".into(), Some(Value::Integer(3))),
            ("id".into(), Some(Value::Integer(11))),
        ]),
        original_values: BTreeMap::new(),
        current_values: BTreeMap::from([("amount".into(), Some(Value::String("9.99".into())))]),
        changed_columns: vec!["amount".into()],
        delete_marked: false,
        insert_marked: false,
    };

    let message = plan_for(table, &[change], &columns)
        .unwrap_err()
        .to_string();
    assert!(
        message.contains("not part of 'orders' primary key"),
        "got: {message}"
    );
    assert!(message.contains("id"), "got: {message}");
}

#[test]
fn composite_identity_with_the_full_key_is_accepted() {
    let columns = CachedColumns {
        columns: vec![
            column("tenant_id", true, false),
            column("order_no", true, false),
            column("amount", false, true),
        ],
        primary_keys: vec!["tenant_id".into(), "order_no".into()],
        table_name: "orders".into(),
        cached_at: std::time::Instant::now(),
    };
    let table = RowChangeTableContext {
        table: "orders".into(),
        ..table_context("w3-full", "w3-full-session")
    };
    let change = PendingRowChange {
        row_identity: BTreeMap::from([
            ("tenant_id".into(), Some(Value::Integer(7))),
            ("order_no".into(), Some(Value::Integer(3))),
        ]),
        original_values: BTreeMap::new(),
        current_values: BTreeMap::from([("amount".into(), Some(Value::String("9.99".into())))]),
        changed_columns: vec!["amount".into()],
        delete_marked: false,
        insert_marked: false,
    };

    let plan = plan_for(table, &[change], &columns).unwrap();
    assert_eq!(plan.updates.len(), 1);
}

// ---------------------------------------------------------------------------
// V-2: writes must name columns the table really has
// ---------------------------------------------------------------------------

#[test]
fn insert_writing_an_unknown_column_is_rejected_by_name() {
    let columns = users_columns();
    let table = table_context("v2-unknown", "v2-unknown-session");
    let error = plan_for(
        table,
        &[insert_change(&[(
            "nmae",
            Some(Value::String("Ada".into())),
        )])],
        &columns,
    )
    .unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("grid.insert.unknownColumn"),
        "got: {message}"
    );
    assert!(message.contains("nmae"), "got: {message}");
}

#[test]
fn insert_writing_a_known_nullable_column_is_accepted() {
    let columns = users_columns();
    let table = table_context("v2-known", "v2-known-session");
    let plan = plan_for(
        table,
        &[insert_change(&[
            ("name", Some(Value::String("Ada".into()))),
            ("note", None),
        ])],
        &columns,
    )
    .unwrap();

    assert_eq!(plan.inserts.len(), 1);
    assert_eq!(plan.inserts[0].changed_columns, vec!["name", "note"]);
}

// ---------------------------------------------------------------------------
// IB-D2: the insert mark and existing-row state are mutually exclusive
// ---------------------------------------------------------------------------

#[test]
fn insert_row_carrying_an_identity_is_rejected() {
    let columns = users_columns();
    let table = table_context("d2-identity", "d2-identity-session");
    let mut change = insert_change(&[("name", Some(Value::String("Ada".into())))]);
    change
        .row_identity
        .insert("id".into(), Some(Value::Integer(1)));

    let message = plan_for(table, &[change], &columns)
        .unwrap_err()
        .to_string();
    assert!(message.contains("conflictingIntents"), "got: {message}");
    assert!(
        message.contains("existing row") && message.contains("new row"),
        "got: {message}"
    );
}

#[test]
fn insert_row_carrying_original_values_is_rejected() {
    let columns = users_columns();
    let table = table_context("d2-original", "d2-original-session");
    let mut change = insert_change(&[("name", Some(Value::String("Ada".into())))]);
    change
        .original_values
        .insert("name".into(), Some(Value::String("Grace".into())));

    let message = plan_for(table, &[change], &columns)
        .unwrap_err()
        .to_string();
    assert!(message.contains("conflictingIntents"), "got: {message}");
}

#[test]
fn insert_row_with_empty_existing_row_state_is_accepted() {
    let columns = users_columns();
    let table = table_context("d2-empty", "d2-empty-session");
    let plan = plan_for(
        table,
        &[insert_change(&[(
            "name",
            Some(Value::String("Ada".into())),
        )])],
        &columns,
    )
    .unwrap();

    assert_eq!(plan.inserts.len(), 1);
    assert!(plan.inserts[0].row_identity.is_empty());
    assert!(plan.inserts[0].original_values.is_empty());
}

// ---------------------------------------------------------------------------
// IB-D4: say that a new row cannot be located after commit
// ---------------------------------------------------------------------------

#[test]
fn insert_without_returning_seam_warns_that_the_page_will_refresh() {
    let columns = users_columns();
    let table = table_context("d4-none", "d4-none-session");
    let plan = plan_for(
        table,
        &[insert_change(&[(
            "name",
            Some(Value::String("Ada".into())),
        )])],
        &columns,
    )
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

#[test]
fn insert_with_returning_seam_has_no_degradation_warning() {
    let columns = users_columns();
    let table = table_context("d4-returning", "d4-returning-session");
    let driver = MockDriver::new(
        "postgres",
        MockDriverOptions {
            execute_rows_affected: 1,
            parameterized_writes: true,
            insert_returning_clause: Some("RETURNING id".into()),
            ..MockDriverOptions::default()
        },
    );
    let plan = build_row_change_plan(
        driver.as_ref(),
        table,
        &[insert_change(&[(
            "name",
            Some(Value::String("Ada".into())),
        )])],
        &columns,
    )
    .unwrap();

    assert!(
        !warning_codes(&plan).contains(&"grid.insert.returningUnavailable"),
        "got warnings: {:?}",
        warning_codes(&plan)
    );
}

#[test]
fn plan_without_inserts_has_no_returning_warning() {
    let columns = users_columns();
    let table = table_context("d4-update-only", "d4-update-only-session");
    let plan = plan_for(table, &[update_change(1, "Alice", "Updated")], &columns).unwrap();

    assert!(
        !warning_codes(&plan).contains(&"grid.insert.returningUnavailable"),
        "got warnings: {:?}",
        warning_codes(&plan)
    );
}

fn warning_codes(plan: &RowChangePlan) -> Vec<&str> {
    plan.warnings
        .iter()
        .map(|warning| warning.code.as_str())
        .collect()
}

// ---------------------------------------------------------------------------
// Insert plan shape
// ---------------------------------------------------------------------------

#[test]
fn insert_all_unset_row_is_a_default_values_statement() {
    let columns = users_columns();
    let plan = plan_for(
        table_context("insert-defaults", "insert-defaults-session"),
        &[insert_change(&[])],
        &columns,
    )
    .unwrap();

    // An all-Unset new row is legal (DEFAULT VALUES), not "no change" to drop.
    assert_eq!(plan.inserts.len(), 1);
    assert_eq!(
        plan.inserts[0].sql_template,
        "INSERT INTO \"users\" DEFAULT VALUES"
    );
    assert!(plan.inserts[0].changed_columns.is_empty());
}

#[test]
fn insert_plan_keeps_request_order_between_rows() {
    let columns = users_columns();
    let plan = plan_for(
        table_context("insert-order", "insert-order-session"),
        &[
            insert_change(&[("name", Some(Value::String("first".into())))]),
            insert_change(&[("name", Some(Value::String("second".into())))]),
        ],
        &columns,
    )
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

#[test]
fn insert_rejects_a_delete_marker_on_a_new_row() {
    let columns = users_columns();
    let table = table_context("insert-conflict", "insert-conflict-session");
    let mut change = insert_change(&[("name", Some(Value::String("Ada".into())))]);
    change.delete_marked = true;

    let error = plan_for(table, &[change], &columns).unwrap_err();
    assert!(error.to_string().contains("conflictingIntents"));
}

#[test]
fn rebuild_from_plan_metadata_matches_the_original_fingerprint() {
    let columns = users_columns();
    let table = table_context("rebuild", "rebuild-session");
    let plan = plan_for(
        table.clone(),
        &[update_change(1, "Alice", "Updated")],
        &columns,
    )
    .unwrap();
    let validated = validate_immutable_plan(
        driver().as_ref(),
        "rebuild-session",
        &plan,
        &plan.fingerprint,
        &columns,
    )
    .unwrap();
    assert_eq!(validated.fingerprint, plan.fingerprint);
    assert_eq!(validated.plan_id, plan.plan_id);
}

// ---------------------------------------------------------------------------
// Round 2: error codes are part of the contract
// ---------------------------------------------------------------------------

#[test]
fn update_writing_an_unknown_column_gets_plain_validation_not_the_insert_code() {
    let columns = users_columns();
    let table = table_context("v2-update-unknown", "v2-update-unknown-session");
    let mut change = update_change(1, "Alice", "Updated");
    change.changed_columns = vec!["nmae".into()];
    change
        .current_values
        .insert("nmae".into(), Some(Value::String("Ada".into())));

    let message = plan_for(table, &[change], &columns)
        .unwrap_err()
        .to_string();
    assert!(
        !message.contains("grid.insert.unknownColumn"),
        "got: {message}"
    );
    assert!(
        message.contains("UPDATE writes column(s) that do not exist"),
        "got: {message}"
    );
    assert!(message.contains("nmae"), "got: {message}");
}

#[test]
fn delete_carrying_column_writes_is_refused_with_conflicting_intents() {
    let columns = users_columns();
    let table = table_context("d7-delete-write", "d7-delete-write-session");
    let mut change = delete_change(4);
    change.changed_columns = vec!["name".into()];
    change
        .current_values
        .insert("name".into(), Some(Value::String("Ada".into())));

    let message = plan_for(table, &[change], &columns)
        .unwrap_err()
        .to_string();
    assert!(
        message.starts_with("grid.commit.conflictingIntents: "),
        "got: {message}"
    );
}

#[test]
fn a_clean_delete_still_plans() {
    let columns = users_columns();
    let plan = plan_for(
        table_context("d7-clean-delete", "d7-clean-delete-session"),
        &[delete_change(4)],
        &columns,
    )
    .unwrap();

    assert_eq!(plan.deletes.len(), 1);
    assert!(
        !warning_codes(&plan).contains(&"delete-suppresses-update"),
        "the suppressing warning is gone: {:?}",
        warning_codes(&plan)
    );
}

#[test]
fn stale_plan_fingerprint_carries_the_contract_prefix() {
    let columns = users_columns();
    let plan = plan_for(
        table_context("stale-prefix", "stale-prefix-session"),
        &[insert_change(&[(
            "name",
            Some(Value::String("Ada".into())),
        )])],
        &columns,
    )
    .unwrap();

    let mut tampered = plan.clone();
    tampered.inserts[0]
        .current_values
        .insert("name".into(), Some(Value::String("Mallory".into())));

    let message = validate_immutable_plan(
        driver().as_ref(),
        &tampered.table.db_session_id,
        &tampered,
        &plan.fingerprint,
        &columns,
    )
    .unwrap_err()
    .to_string();
    assert!(
        message.starts_with("grid.commit.stalePlan: "),
        "got: {message}"
    );
    assert!(message.contains("stale or was modified"), "got: {message}");
}

#[test]
fn connection_facts_with_a_different_driver_type_are_refused() {
    let facts = ConnectionFacts {
        database_type: "mysql".into(),
        read_only: false,
        database: Some("app".into()),
        schema: None,
        config_revision: 1,
    };
    let table = table_context("facts-drift", "facts-drift-session");
    let message = validate_connection_facts(&facts, &table)
        .unwrap_err()
        .to_string();
    assert!(message.contains("context changed"), "got: {message}");

    let matching = ConnectionFacts {
        database_type: "postgres".into(),
        ..facts
    };
    assert!(validate_connection_facts(&matching, &table).is_ok());
}

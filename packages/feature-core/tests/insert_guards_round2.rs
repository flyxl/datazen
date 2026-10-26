//! Independent round-2 verification of the shared `data_browsing` layer.
//!
//! The layer moved out of the Tauri host in `7841b589c`. These tests call it
//! directly (no command, no host state) so the guards are proven to live here —
//! a web host that reuses this crate gets the same refusals — and they look for
//! the paths the shipped suite leaves open: a table with no primary key at all,
//! the insert exemption, the `grid.commit.conflictingIntents:` prefix and the
//! warning's inverse cases.
//!
//! Written by the test agent: it asserts behaviour, not the implementation, and
//! it does not import anything from `src-tauri`.

use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{ColumnSchema, Value};
use datazen_feature_core::data_browsing::change_set::validate_immutable_plan;
use datazen_feature_core::data_browsing::{
    build_row_change_plan, changes_fingerprint, plan_changes, PendingRowChange,
    RowChangeTableContext,
};
use datazen_modules::schema::CachedColumns;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

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

fn cached(columns: Vec<ColumnSchema>) -> CachedColumns {
    let primary_keys = columns
        .iter()
        .filter(|column| column.is_primary_key)
        .map(|column| column.name.clone())
        .collect();
    CachedColumns {
        columns,
        primary_keys,
        table_name: "users".into(),
        cached_at: Instant::now(),
    }
}

/// `users(id PK, name, note)` — the shape every other suite uses.
fn users() -> CachedColumns {
    cached(vec![
        column("id", true),
        column("name", false),
        column("note", false),
    ])
}

/// `orders(tenant_id, order_no)` composite key plus a payload column.
fn orders() -> CachedColumns {
    cached(vec![
        column("tenant_id", true),
        column("order_no", true),
        column("amount", false),
    ])
}

fn driver() -> Arc<MockDriver> {
    let mut options = MockDriverOptions::default();
    options.parameterized_writes = true;
    options.execute_rows_affected = 1;
    MockDriver::new("postgres", options)
}

fn driver_with_returning(clause: &str) -> Arc<MockDriver> {
    let mut options = MockDriverOptions::default();
    options.parameterized_writes = true;
    options.execute_rows_affected = 1;
    options.insert_returning_clause = Some(clause.into());
    MockDriver::new("postgres", options)
}

fn table(name: &str) -> RowChangeTableContext {
    RowChangeTableContext {
        connection_id: "conn-1".into(),
        db_session_id: "session-1".into(),
        driver_type: "postgres".into(),
        database: Some("app".into()),
        schema: None,
        table: name.into(),
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

// ---------------------------------------------------------------------------
// W-3: identity column set must equal the real primary key
// ---------------------------------------------------------------------------

#[test]
fn w3_missing_key_column_is_refused_and_names_the_real_key() {
    let driver = driver();
    let error = build_row_change_plan(
        driver.as_ref(),
        table("orders"),
        &[update(&[("tenant_id", 7)], "amount", "9.99")],
        &orders(),
    )
    .unwrap_err();

    let message = error.to_string();
    assert!(message.contains("missing primary-key column"), "{message}");
    assert!(message.contains("order_no"), "{message}");
    assert!(
        message.contains("tenant_id, order_no"),
        "the refusal must print the real primary key: {message}"
    );
}

#[test]
fn w3_extra_key_column_is_refused() {
    let driver = driver();
    let error = build_row_change_plan(
        driver.as_ref(),
        table("orders"),
        &[update(
            &[("tenant_id", 7), ("order_no", 3), ("id", 11)],
            "amount",
            "9.99",
        )],
        &orders(),
    )
    .unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("not part of 'orders' primary key"),
        "{message}"
    );
    assert!(message.contains("id"), "{message}");
}

#[test]
fn w3_the_full_key_and_a_single_key_table_both_pass() {
    let driver = driver();
    let composite = build_row_change_plan(
        driver.as_ref(),
        table("orders"),
        &[update(
            &[("tenant_id", 7), ("order_no", 3)],
            "amount",
            "9.99",
        )],
        &orders(),
    )
    .expect("a complete composite identity is addressable");
    assert_eq!(composite.updates.len(), 1);

    let single = build_row_change_plan(
        driver.as_ref(),
        table("users"),
        &[update(&[("id", 1)], "name", "Ada")],
        &users(),
    )
    .expect("a complete single-column identity is addressable");
    assert_eq!(single.updates.len(), 1);
}

/// The guard cannot be satisfied by a table it cannot key: refusing is the only
/// answer that is not a silent mass update.
#[test]
fn a_table_without_a_primary_key_is_refused_explicitly() {
    let driver = driver();
    let unkeyed = cached(vec![column("name", false), column("note", false)]);
    let error = build_row_change_plan(
        driver.as_ref(),
        table("users"),
        &[update(&[("name", 1)], "name", "Ada")],
        &unkeyed,
    )
    .unwrap_err();

    let message = error.to_string();
    assert!(message.contains("reports none"), "{message}");
    assert!(message.contains("cannot address one row"), "{message}");
}

/// A new row has no identity yet, so the key-set guard must not run on it.
#[test]
fn a_new_row_is_exempt_from_the_primary_key_set_guard() {
    let driver = driver();
    let plan = build_row_change_plan(
        driver.as_ref(),
        table("orders"),
        &[new_row(&[("amount", Some(Value::String("9.99".into())))])],
        &orders(),
    )
    .expect("an insert does not carry a row identity to verify");

    assert_eq!(plan.inserts.len(), 1);
    assert!(plan.inserts[0].row_identity.is_empty());
    assert_eq!(plan.inserts[0].changed_columns, vec!["amount"]);
}

// ---------------------------------------------------------------------------
// IB-D2: the insert mark and existing-row state are mutually exclusive
// ---------------------------------------------------------------------------

#[test]
fn a_new_row_carrying_an_identity_is_refused_with_the_contract_prefix() {
    let driver = driver();
    let mut change = new_row(&[("name", Some(Value::String("Ada".into())))]);
    change
        .row_identity
        .insert("id".into(), Some(Value::Integer(1)));

    let error =
        build_row_change_plan(driver.as_ref(), table("users"), &[change], &users()).unwrap_err();

    let message = error.to_string();
    assert!(
        message.starts_with("grid.commit.conflictingIntents: "),
        "the code must be machine-readable: {message}"
    );
    assert!(
        message.contains("existing row") && message.contains("new row"),
        "{message}"
    );
}

#[test]
fn a_new_row_carrying_original_values_is_refused_with_the_contract_prefix() {
    let driver = driver();
    let mut change = new_row(&[("name", Some(Value::String("Ada".into())))]);
    change
        .original_values
        .insert("name".into(), Some(Value::String("Grace".into())));

    let error =
        build_row_change_plan(driver.as_ref(), table("users"), &[change], &users()).unwrap_err();

    assert!(
        error
            .to_string()
            .starts_with("grid.commit.conflictingIntents: "),
        "{error}"
    );
}

/// A clean draft must still go through; the new rejection must not block it.
#[test]
fn a_new_row_without_existing_row_state_is_accepted() {
    let driver = driver();
    let plan = build_row_change_plan(
        driver.as_ref(),
        table("users"),
        &[new_row(&[
            ("name", Some(Value::String("Ada".into()))),
            ("note", None),
        ])],
        &users(),
    )
    .expect("a draft with no identity and no original values is valid");

    assert_eq!(plan.inserts.len(), 1);
    assert!(plan.inserts[0].row_identity.is_empty());
    assert!(plan.inserts[0].original_values.is_empty());
    assert_eq!(plan.inserts[0].changed_columns, vec!["name", "note"]);
}

// ---------------------------------------------------------------------------
// IB-D4: the degradation must be announced, and only when it applies
// ---------------------------------------------------------------------------

fn warning_codes(plan: &datazen_feature_core::data_browsing::RowChangePlan) -> Vec<&str> {
    plan.warnings
        .iter()
        .map(|warning| warning.code.as_str())
        .collect()
}

#[test]
fn without_a_returning_seam_new_rows_are_announced_as_unlocatable() {
    let driver = driver();
    let plan = build_row_change_plan(
        driver.as_ref(),
        table("users"),
        &[new_row(&[("name", Some(Value::String("Ada".into())))])],
        &users(),
    )
    .unwrap();

    let warning = plan
        .warnings
        .iter()
        .find(|warning| warning.code == "grid.insert.returningUnavailable")
        .expect("the degradation must be announced");
    assert_eq!(warning.severity, "warning");
    assert!(
        warning.message.contains("cannot return their primary keys"),
        "{}",
        warning.message
    );
    assert!(warning.message.contains("refreshed"), "{}", warning.message);
}

#[test]
fn a_returning_seam_suppresses_the_degradation_warning() {
    let driver = driver_with_returning("RETURNING id");
    let plan = build_row_change_plan(
        driver.as_ref(),
        table("users"),
        &[new_row(&[("name", Some(Value::String("Ada".into())))])],
        &users(),
    )
    .unwrap();

    assert!(
        !warning_codes(&plan).contains(&"grid.insert.returningUnavailable"),
        "got: {:?}",
        warning_codes(&plan)
    );
    // The ordinary preview notice must survive either way.
    assert!(warning_codes(&plan).contains(&"insert-rows"));
}

#[test]
fn an_update_only_plan_never_carries_the_returning_warning() {
    let driver = driver();
    let plan = build_row_change_plan(
        driver.as_ref(),
        table("users"),
        &[update(&[("id", 1)], "name", "Ada")],
        &users(),
    )
    .unwrap();

    assert!(
        !warning_codes(&plan).contains(&"grid.insert.returningUnavailable"),
        "got: {:?}",
        warning_codes(&plan)
    );
}

// ---------------------------------------------------------------------------
// Fingerprint and immutability, proven against a locally computed baseline
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
struct BaselinePayload<'a> {
    table: &'a RowChangeTableContext,
    changes: &'a [PendingRowChange],
}

#[test]
fn an_insert_free_fingerprint_is_the_sha256_of_table_and_changes_only() {
    let driver = driver();
    let context = table("users");
    let changes = vec![update(&[("id", 7)], "name", "Ada")];
    let plan = build_row_change_plan(driver.as_ref(), context.clone(), &changes, &users()).unwrap();

    let bytes = serde_json::to_vec(&BaselinePayload {
        table: &context,
        changes: &changes,
    })
    .unwrap();
    let expected = format!("{:x}", Sha256::digest(bytes));
    assert_eq!(
        plan.fingerprint, expected,
        "the fingerprint payload shape must stay {{table, changes}}"
    );
    assert_eq!(changes_fingerprint(&context, &changes).unwrap(), expected);
}

#[test]
fn a_tampered_insert_plan_is_rejected_before_anything_runs() {
    let driver = driver();
    let plan = build_row_change_plan(
        driver.as_ref(),
        table("users"),
        &[new_row(&[("name", Some(Value::String("Ada".into())))])],
        &users(),
    )
    .unwrap();

    let mut tampered = plan.clone();
    tampered.inserts[0]
        .current_values
        .insert("name".into(), Some(Value::String("Mallory".into())));

    let error = validate_immutable_plan(
        driver.as_ref(),
        &tampered.table.db_session_id,
        &tampered,
        &plan.fingerprint,
        &users(),
    )
    .unwrap_err();

    assert!(
        error.to_string().contains("stale or was modified"),
        "{error}"
    );
}

/// Rebuilding from the plan alone must reproduce the confirmed fingerprint.
#[test]
fn rebuilding_a_plan_from_its_own_metadata_is_a_fixed_point() {
    let driver = driver();
    let plan = build_row_change_plan(
        driver.as_ref(),
        table("users"),
        &[
            new_row(&[("name", Some(Value::String("Ada".into())))]),
            update(&[("id", 4)], "name", "Grace"),
        ],
        &users(),
    )
    .unwrap();

    let rebuilt = validate_immutable_plan(
        driver.as_ref(),
        &plan.table.db_session_id,
        &plan,
        &plan.fingerprint,
        &users(),
    )
    .expect("the plan must validate against its own fingerprint");
    assert_eq!(rebuilt.fingerprint, plan.fingerprint);
    assert_eq!(rebuilt.inserts.len(), 1);
    assert_eq!(rebuilt.updates.len(), 1);
    // `plan_changes` must not invent an identity for the new row.
    let changes = plan_changes(&rebuilt);
    let insert = changes
        .iter()
        .find(|change| change.insert_marked)
        .expect("the new row survives the rebuild");
    assert!(insert.row_identity.is_empty());
    assert!(insert.original_values.is_empty());
}

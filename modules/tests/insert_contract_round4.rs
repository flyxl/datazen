//! Round-4 contract tests: the ten "existing refusal points now carry a code"
//! sites that live in the shared layer.
//!
//! The round-4 claim is narrow — "we only prepended `grid.<domain>.<reason>: `,
//! the decision, the trigger and the descriptive text are untouched". These
//! tests attack exactly that claim from the outside: every coded refusal must
//! put the code at position zero, keep its descriptive tail, and produce the
//! same code regardless of which operation tripped it. The three
//! `commit_changes` sites and the two `execute_plan` sites need a live runtime
//! and are pinned in the host suite instead.

use datazen::data_browsing::change_set::validate_immutable_plan;
use datazen::data_browsing::row_identity::{
    validate_identity_against_primary_key, validate_identity_shape,
};
use datazen::data_browsing::{
    build_row_change_plan, canonicalize_changes, validate_connection_facts, ConnectionFacts,
    PendingRowChange, RowChangeTableContext,
};
use datazen::schema::CachedColumns;
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{ColumnSchema, Value};
use std::collections::BTreeMap;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

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

fn cached(primary_keys: &[&str]) -> CachedColumns {
    let mut columns: Vec<ColumnSchema> =
        primary_keys.iter().map(|name| column(name, true)).collect();
    columns.push(column("name", false));
    CachedColumns {
        columns,
        primary_keys: primary_keys.iter().map(|key| (*key).to_string()).collect(),
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

fn driver() -> std::sync::Arc<MockDriver> {
    MockDriver::new(
        "postgres",
        MockDriverOptions {
            parameterized_writes: true,
            ..MockDriverOptions::default()
        },
    )
}

fn identity(pairs: &[(&str, Option<Value>)]) -> BTreeMap<String, Option<Value>> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_string(), value.clone()))
        .collect()
}

fn update_with(
    row_identity: BTreeMap<String, Option<Value>>,
    current: BTreeMap<String, Option<Value>>,
    changed: &[&str],
) -> PendingRowChange {
    PendingRowChange {
        row_identity,
        original_values: BTreeMap::new(),
        current_values: current,
        changed_columns: changed.iter().map(|c| (*c).to_string()).collect(),
        delete_marked: false,
        insert_marked: false,
    }
}

fn delete_with(row_identity: BTreeMap<String, Option<Value>>) -> PendingRowChange {
    PendingRowChange {
        row_identity,
        original_values: BTreeMap::new(),
        current_values: BTreeMap::new(),
        changed_columns: Vec::new(),
        delete_marked: true,
        insert_marked: false,
    }
}

/// Assert the message carries `code` at position zero and still contains every
/// descriptive fragment the message had before the code was prepended.
fn assert_coded(message: &str, code: &str, tails: &[&str]) {
    assert!(
        message.starts_with(&format!("{code}: ")),
        "expected {code} at position zero, got: {message}"
    );
    for tail in tails {
        assert!(
            message.contains(tail),
            "descriptive fragment {tail:?} must survive, got: {message}"
        );
    }
}

// ---------------------------------------------------------------------------
// grid.cell.ambiguousIdentity — the two canonicalize paths
// ---------------------------------------------------------------------------

#[test]
fn duplicate_primary_key_identity_is_coded_ambiguous() {
    let duplicate = update_with(
        identity(&[("id", Some(Value::Integer(1)))]),
        identity(&[
            ("id", Some(Value::Integer(1))),
            ("name", Some(Value::String("A".into()))),
        ]),
        &["name"],
    );

    let message = canonicalize_changes(&[duplicate.clone(), duplicate])
        .unwrap_err()
        .to_string();

    assert_coded(
        &message,
        "grid.cell.ambiguousIdentity",
        &["duplicate primary-key identity"],
    );
}

#[test]
fn a_primary_key_identity_collision_is_coded_ambiguous() {
    // Two distinct existing rows, both edited to claim the same key: the
    // originals differ (so the duplicate check passes) while the effective
    // identities collide.
    let first = update_with(
        identity(&[("id", Some(Value::Integer(1)))]),
        identity(&[("id", Some(Value::Integer(2)))]),
        &["id"],
    );
    let second = update_with(
        identity(&[("id", Some(Value::Integer(2)))]),
        identity(&[("id", Some(Value::Integer(2)))]),
        &["id"],
    );

    let message = canonicalize_changes(&[first, second])
        .unwrap_err()
        .to_string();

    assert_coded(
        &message,
        "grid.cell.ambiguousIdentity",
        &["primary-key identity collision"],
    );
}

// ---------------------------------------------------------------------------
// grid.cell.noRowIdentity — all six sites, all three operations
// ---------------------------------------------------------------------------

#[test]
fn the_two_identity_key_refusals_are_coded_no_row_identity() {
    // Reachable through the public canonicalize entry point. Inside
    // `build_row_change_plan` the shape guard fires first with its own (equally
    // coded) message, so these two are the "building block" variants.
    let empty = canonicalize_changes(&[delete_with(BTreeMap::new())])
        .unwrap_err()
        .to_string();
    assert_coded(
        &empty,
        "grid.cell.noRowIdentity",
        &["require primary-key identity"],
    );

    let null = canonicalize_changes(&[delete_with(identity(&[("id", None)]))])
        .unwrap_err()
        .to_string();
    assert_coded(
        &null,
        "grid.cell.noRowIdentity",
        &["must contain non-NULL primary-key values"],
    );
}

#[test]
fn no_row_identity_is_coded_under_all_three_operations() {
    let columns = cached(&["id", "tenant"]);

    for operation in ["INSERT", "UPDATE", "DELETE"] {
        let empty = validate_identity_shape(operation, &delete_with(BTreeMap::new()))
            .unwrap_err()
            .to_string();
        assert_coded(
            &empty,
            "grid.cell.noRowIdentity",
            &[
                operation,
                "the row identity is empty",
                "no column can locate the row",
            ],
        );

        let null = validate_identity_shape(operation, &delete_with(identity(&[("id", None)])))
            .unwrap_err()
            .to_string();
        assert_coded(
            &null,
            "grid.cell.noRowIdentity",
            &[
                operation,
                "NULL primary-key values",
                "a NULL cannot address one row",
            ],
        );

        // Missing a key column: identity names only part of the primary key.
        let missing = validate_identity_against_primary_key(
            operation,
            &update_with(
                identity(&[("id", Some(Value::Integer(1)))]),
                BTreeMap::new(),
                &[],
            ),
            &table("missing"),
            &columns,
        )
        .unwrap_err()
        .to_string();
        assert_coded(
            &missing,
            "grid.cell.noRowIdentity",
            &[
                operation,
                "is missing primary-key column(s)",
                "the primary key is (",
            ],
        );

        // Extra non-key column: the shape W-3 expanded `noRowIdentity` to cover.
        let extra = validate_identity_against_primary_key(
            operation,
            &update_with(
                identity(&[
                    ("id", Some(Value::Integer(1))),
                    ("tenant", Some(Value::Integer(7))),
                    ("nmae", Some(Value::String("x".into()))),
                ]),
                BTreeMap::new(),
                &[],
            ),
            &table("extra"),
            &columns,
        )
        .unwrap_err()
        .to_string();
        assert_coded(
            &extra,
            "grid.cell.noRowIdentity",
            &[operation, "not part of", "the primary key is ("],
        );
    }
}

// ---------------------------------------------------------------------------
// grid.cell.readOnly — the table-reports-no-primary-key refusal
// ---------------------------------------------------------------------------

#[test]
fn a_table_without_a_primary_key_is_coded_read_only() {
    let no_keys = cached(&[]);
    for operation in ["INSERT", "UPDATE", "DELETE"] {
        let message = validate_identity_against_primary_key(
            operation,
            &update_with(
                identity(&[("id", Some(Value::Integer(1)))]),
                BTreeMap::new(),
                &[],
            ),
            &table("no-pk"),
            &no_keys,
        )
        .unwrap_err()
        .to_string();
        assert_coded(
            &message,
            "grid.cell.readOnly",
            &[operation, "reports none", "cannot address one row"],
        );
    }
}

// ---------------------------------------------------------------------------
// grid.commit.stalePlan — the session-drift refusal
// ---------------------------------------------------------------------------

#[test]
fn a_plan_from_another_session_is_coded_stale_plan() {
    let columns = cached(&["id"]);
    let table = table("session-drift");
    let plan = build_row_change_plan(
        driver().as_ref(),
        table.clone(),
        &[update_with(
            identity(&[("id", Some(Value::Integer(1)))]),
            identity(&[
                ("id", Some(Value::Integer(1))),
                ("name", Some(Value::String("A".into()))),
            ]),
            &["name"],
        )],
        &columns,
    )
    .unwrap();

    let message = validate_immutable_plan(
        driver().as_ref(),
        "some-other-session",
        &plan,
        &plan.fingerprint,
        &columns,
    )
    .unwrap_err()
    .to_string();

    assert_coded(
        &message,
        "grid.commit.stalePlan",
        &["belongs to a different database session"],
    );
}

// ---------------------------------------------------------------------------
// Ruled out on purpose: the caller-error message stays uncoded
// ---------------------------------------------------------------------------

#[test]
fn the_driver_type_mismatch_stays_uncoded() {
    // The coordinator's ruling: "Table connection context changed" is a caller
    // error (request does not match the profile), not an expired plan, so it
    // must NOT grow a `grid.` prefix.
    let facts = ConnectionFacts {
        database_type: "mysql".into(),
        read_only: false,
        database: Some("app".into()),
        schema: None,
        config_revision: 1,
    };
    let message = validate_connection_facts(&facts, &table("facts"))
        .unwrap_err()
        .to_string();

    assert!(
        !message.contains("grid."),
        "must stay uncoded by ruling, got: {message}"
    );
    assert_eq!(message, "Table connection context changed");
}

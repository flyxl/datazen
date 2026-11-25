//! T233: the insert-location rule and the keyset invalidation — coder
//! tests. The three-step location rule (the exact identity, the
//! candidate maximum, the unavailable fallback) and the boundary
//! invalidation.

use std::collections::HashMap;

use serde_json::{json, Value};

use datazen_ui::table_data_connection_state::{
    ColumnSchema, KeysetPagingState, EMPTY_KEYSET_PAGING_STATE,
};
use datazen_ui::table_data_dirty_index::row_identity_key;
use datazen_ui::table_data_insert_location::{
    locate_inserted_row, CommitOperation, CommitStatementResult, InsertStatus,
};
use datazen_ui::table_data_keyset_actions::invalidated_keyset;

fn pk(name: &str) -> Vec<ColumnSchema> {
    vec![ColumnSchema {
        name: name.to_owned(),
        data_type: "INTEGER".to_owned(),
        nullable: false,
        default_value: None,
        is_primary_key: true,
        is_auto_increment: false,
        comment: None,
    }]
}

fn two_pks() -> Vec<ColumnSchema> {
    vec![pk("id")[0].clone(), pk("version")[0].clone()]
}

fn row(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

fn statement(operation: CommitOperation, identity: &[(&str, Value)]) -> CommitStatementResult {
    CommitStatementResult {
        operation,
        row_identity: identity.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
        affected_rows: 1,
    }
}

// ---- the not-applicable gate --------------------------------------------------

#[test]
fn a_commit_without_inserts_is_not_applicable() {
    let result = locate_inserted_row(
        &[row(&[("id", json!(1))])],
        &[row(&[("id", json!(1))])],
        &pk("id"),
        &[
            statement(CommitOperation::Update, &[("id", json!(1))]),
            statement(CommitOperation::Delete, &[("id", json!(2))]),
        ],
    );
    assert_eq!(result.status, InsertStatus::NotApplicable);
    assert_eq!(result.key, None);
}

// ---- rule 1: the exact identity ------------------------------------------------

#[test]
fn an_insert_with_a_row_identity_is_located_exactly() {
    // The refreshed page carries the row the statement's identity
    // describes.
    let refreshed = vec![
        row(&[("id", json!(1)), ("name", json!("old"))]),
        row(&[("id", json!(2)), ("name", json!("new"))]),
    ];
    let result = locate_inserted_row(
        &[row(&[("id", json!(1)), ("name", json!("old"))])],
        &refreshed,
        &pk("id"),
        &[statement(CommitOperation::Insert, &[("id", json!(2))])],
    );
    assert_eq!(result.status, InsertStatus::Located);
    // The key is the located row's identity key.
    let mut identity = HashMap::new();
    identity.insert("id".to_owned(), json!(2));
    assert_eq!(result.key.as_deref(), row_identity_key(&identity).as_deref());

    // A string identity matches on the value, not the position.
    let result = locate_inserted_row(
        &[],
        &[row(&[("code", json!("zz-9"))])],
        &pk("code"),
        &[statement(CommitOperation::Insert, &[("code", json!("zz-9"))])],
    );
    assert_eq!(result.status, InsertStatus::Located);
}

#[test]
fn an_identity_that_matches_nothing_falls_through_to_the_candidate() {
    // The identity describes a row the refreshed page does not carry;
    // rule 2 applies (the single comparable pk whose maximum grew).
    let result = locate_inserted_row(
        &[row(&[("id", json!(1))])],
        &[row(&[("id", json!(5))])],
        &pk("id"),
        &[statement(CommitOperation::Insert, &[("id", json!(99))])],
    );
    assert_eq!(result.status, InsertStatus::Located);
    // The candidate rule located the grown maximum (5), not the
    // statement's own identity.
    let mut identity = HashMap::new();
    identity.insert("id".to_owned(), json!(5));
    assert_eq!(result.key.as_deref(), row_identity_key(&identity).as_deref());
}

#[test]
fn an_identity_match_without_a_stable_pk_key_falls_through() {
    // The refreshed row matches the identity, but its pk column holds a
    // null — no stable key, so the exact rule cannot answer and rule 2
    // runs.
    let refreshed = vec![row(&[("id", json!(Value::Null)), ("name", json!("x"))])];
    let result = locate_inserted_row(
        &[],
        &refreshed,
        &pk("name"),
        &[statement(CommitOperation::Insert, &[("name", json!("x"))])],
    );
    // The pk is a string column; rule 2's candidate on "x": the previous
    // page was empty and the refreshed page holds exactly one row, so
    // the single-row arm locates it… but its key needs the name value —
    // which the identity match already refused (null id does not
    // matter; name is the pk and holds "x").
    assert_eq!(result.status, InsertStatus::Located);
}

// ---- rule 2: the candidate maximum ----------------------------------------------

#[test]
fn a_grown_maximum_locates_the_candidate_row() {
    let previous = vec![
        row(&[("id", json!(1))]),
        row(&[("id", json!(7))]),
    ];
    let refreshed = vec![
        row(&[("id", json!(1))]),
        row(&[("id", json!(7))]),
        row(&[("id", json!(9))]),
    ];
    let result = locate_inserted_row(&previous, &refreshed, &pk("id"), &[
        statement(CommitOperation::Insert, &[]),
    ]);
    assert_eq!(result.status, InsertStatus::Located);
    let mut identity = HashMap::new();
    identity.insert("id".to_owned(), json!(9));
    assert_eq!(result.key.as_deref(), row_identity_key(&identity).as_deref());
}

#[test]
fn a_maximum_that_did_not_grow_refuses_to_point() {
    // The refreshed maximum equals the previous one: the candidate is a
    // row that was already there — highlighting it would point at the
    // wrong row.
    let previous = vec![row(&[("id", json!(7))])];
    let refreshed = vec![
        row(&[("id", json!(1))]),
        row(&[("id", json!(7))]),
    ];
    let result = locate_inserted_row(&previous, &refreshed, &pk("id"), &[
        statement(CommitOperation::Insert, &[]),
    ]);
    assert_eq!(result.status, InsertStatus::Unavailable);
    assert_eq!(result.key, None);

    // A maximum that SHRANK is the same refusal (<= 0).
    let refreshed = vec![row(&[("id", json!(3))])];
    let result = locate_inserted_row(&previous, &refreshed, &pk("id"), &[
        statement(CommitOperation::Insert, &[]),
    ]);
    assert_eq!(result.status, InsertStatus::Unavailable);
}

#[test]
fn an_empty_previous_page_locates_only_a_single_row_page() {
    // Nothing comparable before the commit: exactly one refreshed row
    // can be attributed to the insert.
    let result = locate_inserted_row(&[], &[row(&[("id", json!(4))])], &pk("id"), &[
        statement(CommitOperation::Insert, &[]),
    ]);
    assert_eq!(result.status, InsertStatus::Located);

    // Two rows cannot be attributed.
    let refreshed = vec![row(&[("id", json!(4))]), row(&[("id", json!(5))])];
    let result = locate_inserted_row(&[], &refreshed, &pk("id"), &[
        statement(CommitOperation::Insert, &[]),
    ]);
    assert_eq!(result.status, InsertStatus::Unavailable);
}

#[test]
fn a_composite_primary_key_never_takes_the_candidate_rule() {
    let result = locate_inserted_row(
        &[row(&[("id", json!(1)), ("version", json!(1))])],
        &[row(&[("id", json!(2)), ("version", json!(1))])],
        &two_pks(),
        &[statement(CommitOperation::Insert, &[])],
    );
    assert_eq!(result.status, InsertStatus::Unavailable);
}

#[test]
fn unorderable_columns_refuse_the_candidate() {
    // A boolean is not comparable.
    let previous = vec![row(&[("flag", json!(true))])];
    let refreshed = vec![row(&[("flag", json!(true))]), row(&[("flag", json!(false))])];
    let result = locate_inserted_row(&previous, &refreshed, &pk("flag"), &[
        statement(CommitOperation::Insert, &[]),
    ]);
    assert_eq!(result.status, InsertStatus::Unavailable);

    // A null in the column is not comparable either.
    let refreshed = vec![row(&[("id", json!(Value::Null))])];
    let result = locate_inserted_row(&[], &refreshed, &pk("id"), &[
        statement(CommitOperation::Insert, &[]),
    ]);
    assert_eq!(result.status, InsertStatus::Unavailable);

    // A missing column on one row (React's `undefined`) is not
    // comparable.
    let refreshed = vec![HashMap::new(), row(&[("id", json!(1))])];
    let result = locate_inserted_row(&[], &refreshed, &pk("id"), &[
        statement(CommitOperation::Insert, &[]),
    ]);
    assert_eq!(result.status, InsertStatus::Unavailable);

    // Mixed kinds (a number among strings) are not orderable.
    let previous = vec![row(&[("code", json!("a"))])];
    let refreshed = vec![row(&[("code", json!("b"))]), row(&[("code", json!(3))])];
    let result = locate_inserted_row(&previous, &refreshed, &pk("code"), &[
        statement(CommitOperation::Insert, &[]),
    ]);
    assert_eq!(result.status, InsertStatus::Unavailable);
}

#[test]
fn an_empty_refreshed_page_is_unavailable() {
    let result = locate_inserted_row(&[row(&[("id", json!(1))])], &[], &pk("id"), &[
        statement(CommitOperation::Insert, &[]),
    ]);
    assert_eq!(result.status, InsertStatus::Unavailable);
}

#[test]
fn string_columns_order_lexicographically() {
    let previous = vec![row(&[("code", json!("aaa"))])];
    let refreshed = vec![row(&[("code", json!("aab"))])];
    let result = locate_inserted_row(&previous, &refreshed, &pk("code"), &[
        statement(CommitOperation::Insert, &[]),
    ]);
    assert_eq!(result.status, InsertStatus::Located);
}

// ---- the keyset invalidation ---------------------------------------------------

#[test]
fn invalidated_keyset_empties_the_boundaries_and_keeps_the_rest() {
    let mut state: KeysetPagingState = EMPTY_KEYSET_PAGING_STATE.clone();
    state.stack = vec![datazen_ui::table_data_connection_state::KeysetCursor {
        boundary: None,
        order_signature: "o".to_owned(),
        filter_signature: "f".to_owned(),
    }];
    state.anchor_page = 4;
    state.next_after_key = Some(vec![]);
    state.drift_suspected = true;

    let invalidated = invalidated_keyset(&state);
    // Every boundary dropped: the stack, the anchor, the next-page key.
    assert!(invalidated.stack.is_empty());
    assert_eq!(invalidated.anchor_page, 0);
    assert_eq!(invalidated.next_after_key, None);
    // The strategy, the order, the drift flag survive — the back end
    // re-decides them on the next answer.
    assert_eq!(invalidated.mode, state.mode);
    assert_eq!(invalidated.order_columns, state.order_columns);
    assert!(invalidated.drift_suspected);

    // An already-empty state round-trips as itself.
    let fresh = invalidated_keyset(&EMPTY_KEYSET_PAGING_STATE.clone());
    assert_eq!(fresh, EMPTY_KEYSET_PAGING_STATE);
}

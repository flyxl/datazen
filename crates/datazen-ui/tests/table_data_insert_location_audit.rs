//! T233 independent audit: the insert-location rule and the keyset
//! invalidation, fresh vectors.
//!
//! The coder tests cover one vector per rule; this audit attacks from
//! the other side: the second insert's identity winning after the first
//! misses, a composite identity match, negative and float orderings, a
//! refreshed page that turned unorderable, and an invalidation over a
//! populated order-column set.

use std::collections::HashMap;

use serde_json::{json, Value};

use datazen_ui::table_data_connection_state::{
    ColumnSchema, KeysetPagingState, OrderColumn, PagingMode, EMPTY_KEYSET_PAGING_STATE,
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

fn identity_of(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

#[test]
fn the_second_inserts_identity_wins_when_the_first_misses() {
    // Two inserts: the first statement's identity (id 100) is not on the
    // refreshed page; the second statement's (id 20) is.
    let refreshed = vec![
        row(&[("id", json!(1))]),
        row(&[("id", json!(20)), ("name", json!("second"))]),
    ];
    let result = locate_inserted_row(
        &[row(&[("id", json!(1))])],
        &refreshed,
        &pk("id"),
        &[
            statement(CommitOperation::Insert, &[("id", json!(100))]),
            statement(CommitOperation::Insert, &[("id", json!(20))]),
        ],
    );
    assert_eq!(result.status, InsertStatus::Located);
    assert_eq!(result.key.as_deref(), row_identity_key(&identity_of(&[("id", json!(20))])).as_deref());
}

#[test]
fn a_composite_identity_matches_every_column() {
    let pks = vec![pk("tenant")[0].clone(), pk("seq")[0].clone()];
    let refreshed = vec![
        row(&[("tenant", json!(1)), ("seq", json!(9))]),
        row(&[("tenant", json!(2)), ("seq", json!(5))]),
    ];
    let result = locate_inserted_row(
        &[],
        &refreshed,
        &pks,
        &[statement(CommitOperation::Insert, &[("tenant", json!(2)), ("seq", json!(5))])],
    );
    assert_eq!(result.status, InsertStatus::Located);
    assert_eq!(
        result.key.as_deref(),
        row_identity_key(&identity_of(&[("tenant", json!(2)), ("seq", json!(5))])).as_deref()
    );

    // One column off is not a match.
    let result = locate_inserted_row(
        &[],
        &refreshed,
        &pks,
        &[statement(CommitOperation::Insert, &[("tenant", json!(2)), ("seq", json!(6))])],
    );
    // Falls to the candidate rule, which a composite pk refuses.
    assert_eq!(result.status, InsertStatus::Unavailable);
}

#[test]
fn negative_and_float_maxima_order_and_grow() {
    // A negative maximum grows toward zero.
    let previous = vec![row(&[("n", json!(-5))])];
    let refreshed = vec![row(&[("n", json!(-1))])];
    let result = locate_inserted_row(&previous, &refreshed, &pk("n"), &[
        statement(CommitOperation::Insert, &[]),
    ]);
    assert_eq!(result.status, InsertStatus::Located);

    // Floats compare numerically.
    let previous = vec![row(&[("n", json!(1.5))])];
    let refreshed = vec![row(&[("n", json!(2.5))])];
    let result = locate_inserted_row(&previous, &refreshed, &pk("n"), &[
        statement(CommitOperation::Insert, &[]),
    ]);
    assert_eq!(result.status, InsertStatus::Located);

    // A float maximum that did not grow is refused.
    let refreshed = vec![row(&[("n", json!(1.25))])];
    let result = locate_inserted_row(&previous, &refreshed, &pk("n"), &[
        statement(CommitOperation::Insert, &[]),
    ]);
    assert_eq!(result.status, InsertStatus::Unavailable);
}

#[test]
fn a_page_that_turned_unorderable_is_refused() {
    // The previous page was orderable (numbers), but the refreshed page
    // mixes a string into the column: the whole column became
    // unorderable.
    let previous = vec![row(&[("id", json!(1))])];
    let refreshed = vec![row(&[("id", json!(1))]), row(&[("id", json!("2"))])];
    let result = locate_inserted_row(&previous, &refreshed, &pk("id"), &[
        statement(CommitOperation::Insert, &[]),
    ]);
    assert_eq!(result.status, InsertStatus::Unavailable);
    assert_eq!(result.key, None);
}

#[test]
fn a_structurally_different_identity_value_is_not_a_match() {
    // The refreshed row carries 1 (a number) where the identity says
    // "1" (a string): JSON-structurally different, no match — the same
    // verdict React's stringify comparison gives.
    let refreshed = vec![row(&[("id", json!(1))])];
    let result = locate_inserted_row(
        &[],
        &refreshed,
        &pk("id"),
        &[statement(CommitOperation::Insert, &[("id", json!("1"))])],
    );
    // No exact match; the candidate rule takes over (single row page,
    // empty previous → located by the row itself).
    assert_eq!(result.status, InsertStatus::Located);
    assert_eq!(result.key.as_deref(), row_identity_key(&identity_of(&[("id", json!(1))])).as_deref());
}

#[test]
fn an_insert_statement_with_an_empty_identity_still_takes_the_candidate() {
    // An insert with an empty rowIdentity (the no-RETURNING path) is the
    // candidate rule's regular case.
    let previous = vec![row(&[("id", json!(3))])];
    let refreshed = vec![row(&[("id", json!(3))]), row(&[("id", json!(4))])];
    let result = locate_inserted_row(&previous, &refreshed, &pk("id"), &[
        statement(CommitOperation::Insert, &[]),
    ]);
    assert_eq!(result.status, InsertStatus::Located);
    assert_eq!(result.key.as_deref(), row_identity_key(&identity_of(&[("id", json!(4))])).as_deref());
}

#[test]
fn invalidated_keyset_over_a_populated_order_set() {
    let mut state: KeysetPagingState = EMPTY_KEYSET_PAGING_STATE.clone();
    state.mode = PagingMode::Keyset;
    state.order_columns = vec![
        OrderColumn { column: "id".to_owned(), descending: false },
        OrderColumn { column: "name".to_owned(), descending: true },
    ];
    state.stack = vec![
        datazen_ui::table_data_connection_state::KeysetCursor {
            boundary: Some(vec![]),
            order_signature: "id,name".to_owned(),
            filter_signature: "f1".to_owned(),
        };
        2
    ];
    state.anchor_page = 9;
    state.next_after_key = Some(vec![]);

    let invalidated = invalidated_keyset(&state);
    assert!(invalidated.stack.is_empty());
    assert_eq!(invalidated.anchor_page, 0);
    assert_eq!(invalidated.next_after_key, None);
    // The strategy and the full order set survive.
    assert_eq!(invalidated.mode, PagingMode::Keyset);
    assert_eq!(invalidated.order_columns.len(), 2);
    assert_eq!(invalidated.order_columns[1].column, "name");
    assert!(invalidated.order_columns[1].descending);
}

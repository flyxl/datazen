//! T127 independent audit: fresh vectors over the dirty-state index.
//!
//! Written against the ported behavior, not the coder's fixtures: a
//! composite two-column primary key, a staged change under an anchor for
//! a row whose recomputed identity is ambiguous, a NULL in a non-key
//! column, a whole-float PK edit, object/array values through the identity
//! serializer, and the structural sharing across a delete-mark flip.
//! Mutation evidence lives in the audit commit message (red-then-green,
//! restore verified).

use std::collections::HashMap;

use serde_json::{json, Value};

use datazen_ui::table_data_connection_state::{ColumnSchema, PendingRowChange, TableState, WriteExpressionId};
use datazen_ui::table_data_dirty_index::{
    build_dirty_index, dirty_cell_key, row_identity_key, select_dirty_row, values_equal,
    DirtyRowState,
};

fn column(name: &str, pk: bool) -> ColumnSchema {
    ColumnSchema {
        name: name.to_owned(),
        data_type: "integer".to_owned(),
        nullable: !pk,
        default_value: None,
        is_primary_key: pk,
        is_auto_increment: false,
        comment: None,
    }
}

fn row(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

fn change(changed: &[(&str, Value, Value)]) -> PendingRowChange {
    let mut original_values = HashMap::new();
    let mut current_values = HashMap::new();
    let mut changed_columns = Vec::new();
    for (column, original, next) in changed {
        original_values.insert(column.to_string(), original.clone());
        current_values.insert(column.to_string(), next.clone());
        changed_columns.push(column.to_string());
    }
    PendingRowChange {
        row_index: None,
        row_identity: HashMap::new(),
        original_values,
        current_values,
        changed_columns,
        delete_marked: false,
        insert_marked: false,
        expression_values: None,
    }
}

fn state(rows: Vec<HashMap<String, Value>>, pending: Vec<(&str, PendingRowChange)>) -> TableState {
    let mut ts = TableState::default();
    ts.columns = vec![column("tenant", true), column("seq", true), column("payload", false)];
    ts.rows = rows;
    ts.pending_changes = pending
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
    ts
}

// -----------------------------------------------------------------------
// Fresh identity vectors.
// -----------------------------------------------------------------------

#[test]
fn a_composite_key_sorts_its_columns_in_the_identity_key() {
    let mut identity = HashMap::new();
    identity.insert("seq".to_owned(), json!(7));
    identity.insert("tenant".to_owned(), json!("t1"));
    assert_eq!(
        row_identity_key(&identity).as_deref(),
        Some("[\"seq\":7,\"tenant\":\"t1\"]"),
        "seq < tenant by byte order"
    );
}

#[test]
fn the_identity_serializer_canonicalizes_nested_values() {
    let mut identity = HashMap::new();
    identity.insert("k".to_owned(), json!({"b": 1, "a": [1, 2]}));
    assert_eq!(
        row_identity_key(&identity).as_deref(),
        Some("[\"k\":{\"a\":[1,2],\"b\":1}]"),
        "object keys sorted, arrays kept in order"
    );
}

#[test]
fn whole_floats_and_integers_are_the_same_identity() {
    assert!(values_equal(&json!(3), &json!(3.0)));
    let mut identity = HashMap::new();
    identity.insert("id".to_owned(), json!(3.0));
    assert_eq!(
        row_identity_key(&identity).as_deref(),
        Some("[\"id\":3]"),
        "3.0 serializes as 3, the JSON.stringify rule"
    );
}

#[test]
fn dirty_cell_key_injects_column_names_with_special_characters() {
    let a = dirty_cell_key("[\"id\":1]", "user:name");
    let b = dirty_cell_key("[\"id\":1]", "name");
    assert_ne!(a, b, "the NUL separator keeps (row, column) injective");
    assert_eq!(
        a.matches('\u{0}').count(),
        1,
        "exactly one NUL, never inside the identity key"
    );
}

// -----------------------------------------------------------------------
// Fresh index vectors.
// ---------------------------------------------------------------------------

#[test]
fn a_composite_key_row_matches_its_staged_change() {
    let ts = state(
        vec![row(&[("tenant", json!("t1")), ("seq", json!(1)), ("payload", json!("p"))])],
        vec![(
            "[\"seq\":1,\"tenant\":\"t1\"]",
            change(&[("payload", json!("p"), json!("P"))]),
        )],
    );
    let index = build_dirty_index(&ts, None);
    let entry = select_dirty_row(Some(&index), 0).expect("the composite-key row is marked");
    assert_eq!(entry.identity_key, "[\"seq\":1,\"tenant\":\"t1\"]");
    assert_eq!(entry.changed_columns, vec!["payload"]);
    assert_eq!(index.counts.cells_on_page, 1);
    assert!(index.off_page_keys.is_empty());
}

#[test]
fn an_ambiguous_composite_key_never_marks_even_with_an_anchor() {
    // Two rows share the composite identity: neither may be marked — and
    // the ambiguity guard runs *before* the anchor in the React source's
    // own order, so an anchor cannot rescue an ambiguous row either.
    let ts = state(
        vec![
            row(&[("tenant", json!("t")), ("seq", json!(1)), ("payload", json!("p"))]),
            row(&[("tenant", json!("t")), ("seq", json!(1)), ("payload", json!("q"))]),
        ],
        vec![(
            "[\"seq\":1,\"tenant\":\"t\"]",
            change(&[("payload", json!("p"), json!("P"))]),
        )],
    );
    let index = build_dirty_index(&ts, None);
    assert!(index.by_row_index.is_empty(), "the ambiguous composite key marks nothing");
    assert_eq!(index.off_page_keys, vec!["[\"seq\":1,\"tenant\":\"t\"]"]);

    // An anchor on one of the duplicates still does not pin the change:
    // React skips the ambiguous row before the anchor is ever read.
    let mut ts = ts;
    ts.row_identity_anchors.insert(0, "[\"seq\":1,\"tenant\":\"t\"]".to_owned());
    let index = build_dirty_index(&ts, None);
    assert!(
        index.by_row_index.is_empty(),
        "the ambiguity guard precedes the anchor priority"
    );
    assert_eq!(index.off_page_keys, vec!["[\"seq\":1,\"tenant\":\"t\"]"]);
}

#[test]
fn a_null_non_key_column_value_is_fine_for_matching() {
    // The identity holds; a NULL payload cell still carries its change.
    let ts = state(
        vec![row(&[("tenant", json!("t1")), ("seq", json!(1)), ("payload", Value::Null)])],
        vec![(
            "[\"seq\":1,\"tenant\":\"t1\"]",
            change(&[("payload", Value::Null, json!("written"))]),
        )],
    );
    let index = build_dirty_index(&ts, None);
    let entry = select_dirty_row(Some(&index), 0).expect("the row is marked");
    assert_eq!(entry.cells[0].original, Value::Null, "null means SQL NULL");
    assert_eq!(entry.cells[0].next, json!("written"));
}

#[test]
fn a_staged_expression_marks_the_column_without_a_value_change() {
    let (key, mut staged) = (
        "[\"seq\":1,\"tenant\":\"t1\"]",
        change(&[("payload", json!("unchanged"), json!("unchanged"))]),
    );
    let mut expressions = HashMap::new();
    expressions.insert("payload".to_owned(), WriteExpressionId::CurrentTimestamp);
    staged.expression_values = Some(expressions);
    let ts = state(
        vec![row(&[("tenant", json!("t1")), ("seq", json!(1)), ("payload", json!("unchanged") )])],
        vec![(key, staged)],
    );
    let index = build_dirty_index(&ts, None);
    let entry = select_dirty_row(Some(&index), 0).expect("the expression column is marked");
    assert_eq!(entry.changed_columns, vec!["payload"], "the intent is the expression");
    assert_eq!(
        entry.cells[0].expression,
        Some(WriteExpressionId::CurrentTimestamp),
        "the cell carries the staged expression"
    );
    // The snapshot values still ride along for the tooltip.
    assert_eq!(entry.cells[0].original, json!("unchanged"));
}

#[test]
fn structural_sharing_survives_a_delete_mark_flip() {
    let key = "[\"seq\":1,\"tenant\":\"t1\"]";
    let rows = vec![row(&[("tenant", json!("t1")), ("seq", json!(1)), ("payload", json!("p"))])];
    let ts = state(rows.clone(), vec![(key, change(&[("payload", json!("p"), json!("P"))]))]);
    let first = build_dirty_index(&ts, None);

    // The same change, now delete-marked: a different state, so the
    // previous entry is NOT reused.
    let (k, mut marked) = (key.to_owned(), change(&[]));
    marked.delete_marked = true;
    let ts2 = state(rows, vec![(k.as_str(), marked)]);
    let second = build_dirty_index(&ts2, Some(&first));
    let entry = select_dirty_row(Some(&second), 0).expect("the row is marked");
    assert_eq!(entry.state, DirtyRowState::Deleted);
    assert!(entry.cells.is_empty());
}

#[test]
fn a_pk_edit_to_the_same_value_by_float_form_is_not_a_change() {
    // seq is staged 1 → 1.0: valuesEqual says equal, so the entry's
    // changed_columns (the really-changed filter) drops it — only
    // payload counts, even though the staging list still carries seq.
    let ts = state(
        vec![row(&[("tenant", json!("t1")), ("seq", json!(1)), ("payload", json!("p"))])],
        vec![(
            "[\"seq\":1,\"tenant\":\"t1\"]",
            change(&[("seq", json!(1), json!(1.0)), ("payload", json!("p"), json!("q"))]),
        )],
    );
    let index = build_dirty_index(&ts, None);
    let entry = select_dirty_row(Some(&index), 0).expect("the row matches");
    assert_eq!(
        entry.changed_columns,
        vec!["payload"],
        "the entry's columns are the really-changed filter, not the staging list: 1 equals 1.0"
    );
    assert_eq!(index.counts.total_cells, 1);
    assert_eq!(index.counts.cells_on_page, 1);
}

#[test]
fn an_anchored_row_with_a_unique_identity_takes_the_change_over_its_neighbor() {
    // Two rows with DIFFERENT identities; the change is staged under the
    // first one's identity and anchored there: only the anchored row
    // marks (the second row's own identity has no staged change).
    let mut ts = state(
        vec![
            row(&[("tenant", json!("t1")), ("seq", json!(1)), ("payload", json!("p"))]),
            row(&[("tenant", json!("t1")), ("seq", json!(2)), ("payload", json!("q"))]),
        ],
        vec![(
            "[\"seq\":1,\"tenant\":\"t1\"]",
            change(&[("payload", json!("p"), json!("P"))]),
        )],
    );
    ts.row_identity_anchors.insert(0, "[\"seq\":1,\"tenant\":\"t1\"]".to_owned());
    let index = build_dirty_index(&ts, None);
    let entry = select_dirty_row(Some(&index), 0).expect("the anchored row is marked");
    assert_eq!(entry.state, DirtyRowState::Dirty);
    assert!(
        select_dirty_row(Some(&index), 1).is_none(),
        "the neighbor has no staged change under its own identity"
    );
    assert!(index.off_page_keys.is_empty());
}

//! T127: the dirty-state index — the T629 placeholder identity subset and
//! the derived dirty index, probed as pure functions.
//!
//! Every assertion observes the returned index (row entries, off-page
//! keys, counts), the identity keys' exact shape, and the equivalence
//! rules the React source pins: the anchor priority, the ambiguity guard,
//! the really-changed filter (including the staged-expression arm), the
//! counts without a page, and the structural sharing of unchanged
//! entries.

use serde_json::{json, Value};

use datazen_ui::table_data_connection_state::{
    ColumnSchema, PendingRowChange, TableState, WriteExpressionId,
};
use datazen_ui::table_data_dirty_index::{
    build_dirty_index, dirty_cell_key, duplicate_row_identity_keys, empty_dirty_index,
    row_identity_key, select_dirty_counts, select_dirty_row, values_equal, DirtyCounts,
    DirtyRowState,
};

// -----------------------------------------------------------------------
// Fixtures.
// -----------------------------------------------------------------------

fn pk_column(name: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.to_owned(),
        data_type: "integer".to_owned(),
        nullable: false,
        default_value: None,
        is_primary_key: true,
        is_auto_increment: false,
        comment: None,
    }
}

fn plain_column(name: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.to_owned(),
        data_type: "text".to_owned(),
        nullable: true,
        default_value: None,
        is_primary_key: false,
        is_auto_increment: false,
        comment: None,
    }
}

fn row(id: i64, name: &str) -> std::collections::HashMap<String, Value> {
    let mut row = std::collections::HashMap::new();
    row.insert("id".to_owned(), json!(id));
    row.insert("name".to_owned(), json!(name));
    row
}

/// One staged change: `changed` pairs map original → next.
fn staged(key: &str, changed: &[(&str, Value, Value)]) -> (String, PendingRowChange) {
    let mut original_values = std::collections::HashMap::new();
    let mut current_values = std::collections::HashMap::new();
    let mut changed_columns = Vec::new();
    for (column, original, next) in changed {
        original_values.insert(column.to_string(), original.clone());
        current_values.insert(column.to_string(), next.clone());
        changed_columns.push(column.to_string());
    }
    (
        key.to_owned(),
        PendingRowChange {
            row_index: None,
            row_identity: std::collections::HashMap::new(),
            original_values,
            current_values,
            changed_columns,
            delete_marked: false,
            insert_marked: false,
            expression_values: None,
        },
    )
}

fn deleted(key: &str) -> (String, PendingRowChange) {
    let (k, mut change) = staged(key, &[]);
    change.delete_marked = true;
    (k, change)
}

fn table_state(
    rows: Vec<std::collections::HashMap<String, Value>>,
    pending: Vec<(String, PendingRowChange)>,
) -> TableState {
    let mut ts = TableState::default();
    ts.columns = vec![pk_column("id"), plain_column("name")];
    ts.rows = rows;
    ts.pending_changes = pending.into_iter().collect();
    ts
}

// -----------------------------------------------------------------------
// The T629 placeholder subset.
// -----------------------------------------------------------------------

#[test]
fn row_identity_key_is_the_sorted_json_shape() {
    let mut identity = std::collections::HashMap::new();
    identity.insert("b".to_owned(), json!(2));
    identity.insert("a".to_owned(), json!("x"));
    // Column order in a schema must not affect identity.
    let mut other = std::collections::HashMap::new();
    other.insert("a".to_owned(), json!("x"));
    other.insert("b".to_owned(), json!(2));
    assert_eq!(
        row_identity_key(&identity).as_deref(),
        Some("[\"a\":\"x\",\"b\":2]"),
        "sorted entries, JSON string keys, bare serialized values"
    );
    assert_eq!(row_identity_key(&identity), row_identity_key(&other));
}

#[test]
fn duplicate_keys_report_only_repeated_identities() {
    let rows = vec![row(1, "a"), row(1, "b"), row(2, "c")];
    let pk = vec![pk_column("id")];
    assert_eq!(
        duplicate_row_identity_keys(&rows, &pk),
        vec!["[\"id\":1]"],
        "only the repeated id 1"
    );
    let unique = vec![row(1, "a"), row(2, "b")];
    assert!(duplicate_row_identity_keys(&unique, &pk).is_empty());
}

#[test]
fn values_equal_follows_json_stringify() {
    assert!(values_equal(&json!(null), &json!(null)));
    assert!(values_equal(&json!(null), &Value::Null));
    assert!(values_equal(&json!(1), &json!(1.0)), "1 and 1.0 stringify the same");
    assert!(!values_equal(&json!(1), &json!("1")), "number and string differ");
    assert!(values_equal(&json!({"a":1,"b":2}), &json!({"b":2,"a":1})), "object key order is canonicalized");
    assert!(!values_equal(&json!([1, 2]), &json!([2, 1])), "array order is significant");
}

// -----------------------------------------------------------------------
// The contract symbol.
// -----------------------------------------------------------------------

#[test]
fn dirty_cell_key_joins_with_nul() {
    assert_eq!(
        dirty_cell_key("[\"id\":1]", "user:name"),
        "[\"id\":1]\u{0}user:name",
        "NUL separates identity key and column name"
    );
}

// -----------------------------------------------------------------------
// The counts.
// ---------------------------------------------------------------------------

#[test]
fn the_counts_know_nothing_about_any_page() {
    let pending: Vec<(String, PendingRowChange)> = vec![
        staged("[\"id\":1]", &[("name", json!("a"), json!("A"))]),
        deleted("[\"id\":2]"),
        // A staged change whose values came back equal: not really changed.
        staged("[\"id\":3]", &[("name", json!("same"), json!("same"))]),
    ];
    let counts = select_dirty_counts(&pending.into_iter().collect());
    assert_eq!(
        counts,
        DirtyCounts {
            update_rows: 1,
            delete_rows: 1,
            total_rows: 2,
            total_cells: 1,
            cells_on_page: 0,
            cells_off_page: 1,
        },
        "without a page every cell is off the page"
    );
}

#[test]
fn a_staged_expression_is_a_change_even_when_the_placeholder_matches() {
    let (key, mut change) = staged("[\"id\":1]", &[("updated_at", json!("placeholder"), json!("placeholder"))]);
    let mut expressions = std::collections::HashMap::new();
    expressions.insert("updated_at".to_owned(), WriteExpressionId::CurrentTimestamp);
    change.expression_values = Some(expressions);
    let counts = select_dirty_counts(
        &vec![(key, change)].into_iter().collect(),
    );
    assert_eq!(counts.update_rows, 1, "the intent is the expression");
    assert_eq!(counts.total_cells, 1);
}

// -----------------------------------------------------------------------
// The index build.
// ---------------------------------------------------------------------------

#[test]
fn an_empty_pending_map_returns_the_stable_empty_index() {
    let ts = table_state(vec![row(1, "a")], vec![]);
    let index = build_dirty_index(&ts, None);
    assert_eq!(index, *empty_dirty_index());
    assert_eq!(index.by_row_index.len(), 0);
    assert_eq!(index.off_page_keys, Vec::<String>::new());
}

#[test]
fn matching_rows_carry_their_changed_cells() {
    let ts = table_state(
        vec![row(1, "a"), row(2, "b")],
        vec![
            staged("[\"id\":1]", &[("name", json!("a"), json!("A"))]),
            staged("[\"id\":2]", &[("name", json!("b"), json!("B")), ("id", json!(2), json!(2.0))]),
        ],
    );
    let index = build_dirty_index(&ts, None);
    assert_eq!(index.by_row_index.len(), 2, "both rows matched");
    let entry1 = select_dirty_row(Some(&index), 0).expect("row 0 entry");
    assert_eq!(entry1.state, DirtyRowState::Dirty);
    assert_eq!(entry1.identity_key, "[\"id\":1]");
    assert_eq!(entry1.changed_columns, vec!["name"]);
    assert_eq!(entry1.cells.len(), 1);
    assert_eq!(entry1.cells[0].original, json!("a"));
    assert_eq!(entry1.cells[0].next, json!("A"));

    // 2 vs 2.0: `valuesEqual` follows JSON.stringify, so `id` is not really
    // changed — only `name` counts.
    let entry2 = select_dirty_row(Some(&index), 1).expect("row 1 entry");
    assert_eq!(entry2.changed_columns, vec!["name"], "2 equals 2.0");

    assert_eq!(
        index.counts,
        DirtyCounts {
            update_rows: 2,
            delete_rows: 0,
            total_rows: 2,
            total_cells: 2,
            cells_on_page: 2,
            cells_off_page: 0,
        }
    );
    assert!(index.off_page_keys.is_empty(), "every staged change matched a page row");
}

#[test]
fn deleted_rows_carry_no_cells_but_still_count() {
    let ts = table_state(vec![row(1, "a")], vec![deleted("[\"id\":1]")]);
    let index = build_dirty_index(&ts, None);
    let entry = select_dirty_row(Some(&index), 0).expect("the deleted row is marked");
    assert_eq!(entry.state, DirtyRowState::Deleted);
    assert!(entry.changed_columns.is_empty(), "the same fact is never expressed twice");
    assert_eq!(index.counts.delete_rows, 1);
    assert_eq!(index.counts.update_rows, 0);
    assert_eq!(index.counts.total_cells, 0);
}

#[test]
fn the_anchor_wins_over_the_recomputed_identity() {
    // The row's PK was edited: its recomputed identity is the NEW key, but
    // the anchor points at the original one the change is staged under.
    let mut ts = table_state(
        vec![row(9, "a")],
        vec![staged("[\"id\":1]", &[("name", json!("a"), json!("A"))])],
    );
    ts.row_identity_anchors.insert(0, "[\"id\":1]".to_owned());
    let index = build_dirty_index(&ts, None);
    let entry = select_dirty_row(Some(&index), 0).expect("the anchored row is marked");
    assert_eq!(entry.identity_key, "[\"id\":1]", "the anchor key, not the recomputed [\"id\":9]");
    assert!(index.off_page_keys.is_empty(), "the anchored key matched");
}

#[test]
fn ambiguous_identities_are_never_marked() {
    let ts = table_state(
        vec![row(1, "a"), row(1, "duplicate")],
        vec![staged("[\"id\":1]", &[("name", json!("a"), json!("A"))])],
    );
    let index = build_dirty_index(&ts, None);
    assert_eq!(index.by_row_index.len(), 0, "painting one of two rows with the same key would be a guess");
    // The staged change itself is never dropped: it lands off-page.
    assert_eq!(index.off_page_keys, vec!["[\"id\":1]"]);
    assert_eq!(index.counts.total_rows, 1, "the counts still know the staged change");
    assert_eq!(index.counts.cells_on_page, 0);
    assert_eq!(index.counts.cells_off_page, 1);
}

#[test]
fn staged_changes_without_a_matching_row_stay_off_page() {
    let ts = table_state(
        vec![row(1, "a")],
        vec![
            staged("[\"id\":1]", &[("name", json!("a"), json!("A"))]),
            staged("[\"id\":5]", &[("name", json!("x"), json!("X"))]),
        ],
    );
    let index = build_dirty_index(&ts, None);
    assert_eq!(index.off_page_keys, vec!["[\"id\":5]"], "never dropped, sorted");
    assert_eq!(index.counts.cells_on_page, 1);
    assert_eq!(index.counts.cells_off_page, 1);
}

#[test]
fn a_null_primary_key_matches_nothing() {
    let mut null_pk_row = std::collections::HashMap::<String, Value>::new();
    null_pk_row.insert("id".to_owned(), Value::Null);
    null_pk_row.insert("name".to_owned(), json!("a"));
    let ts = table_state(
        vec![null_pk_row],
        vec![staged("[\"id\":1]", &[("name", json!("a"), json!("A"))])],
    );
    let index = build_dirty_index(&ts, None);
    assert_eq!(index.by_row_index.len(), 0, "a NULL PK builds no identity");
    assert_eq!(index.off_page_keys, vec!["[\"id\":1]"]);
}

#[test]
fn without_a_primary_key_every_staged_change_is_off_page() {
    let mut ts = table_state(
        vec![row(1, "a")],
        vec![staged("[\"id\":1]", &[("name", json!("a"), json!("A"))])],
    );
    ts.columns = vec![plain_column("id"), plain_column("name")];
    let index = build_dirty_index(&ts, None);
    assert!(index.by_row_index.is_empty(), "nothing can be staged against a PK-less table");
    assert_eq!(index.off_page_keys, vec!["[\"id\":1]"]);
    assert_eq!(index.counts.cells_on_page, 0);
    assert_eq!(index.counts.cells_off_page, 1);
}

// -----------------------------------------------------------------------
// Structural sharing.
// ---------------------------------------------------------------------------

#[test]
fn unchanged_entries_reuse_the_previous_data() {
    let ts = table_state(
        vec![row(1, "a")],
        vec![staged("[\"id\":1]", &[("name", json!("a"), json!("A"))])],
    );
    let first = build_dirty_index(&ts, None);
    let second = build_dirty_index(&ts, Some(&first));
    let entry = select_dirty_row(Some(&second), 0).expect("the row stays marked");
    let previous = select_dirty_row(Some(&first), 0).expect("the previous entry");
    assert_eq!(entry, previous, "nothing about the row changed: the previous entry's data is reused");
}

#[test]
fn a_changed_value_replaces_the_previous_entry() {
    let ts = table_state(
        vec![row(1, "a")],
        vec![staged("[\"id\":1]", &[("name", json!("a"), json!("A"))])],
    );
    let first = build_dirty_index(&ts, None);
    let evolved = table_state(
        vec![row(1, "a")],
        vec![staged("[\"id\":1]", &[("name", json!("a"), json!("AB"))])],
    );
    let second = build_dirty_index(&evolved, Some(&first));
    let entry = select_dirty_row(Some(&second), 0).expect("the row stays marked");
    assert_eq!(entry.cells[0].next, json!("AB"), "the new value lands");
}

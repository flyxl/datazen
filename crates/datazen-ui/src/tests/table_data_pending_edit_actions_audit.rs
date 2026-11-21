//! T294 independent audit: fresh vectors — the expression placeholder
//! on an existing change, the delete-mark merge preserving the column
//! changes, the paste's identity-changed problem, the
//! expression-only row's unstage, and the revert's anchor write on an
//! ambiguous-free duplicate set.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::table_data_connection_state::{
    ColumnSchema, PendingRowChange, TableState, WriteExpressionId,
};
use crate::table_data_pending_edit_actions::{
    compute_stage_cell_value_result, compute_stage_expression_value_result,
    compute_unstage_delete_result, stage_paste_writes_into, CellPasteWrite, PendingEditActions,
    StageCellValueInput, StageExpressionValueInput,
};

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

fn row(id: i64, name: &str) -> HashMap<String, Value> {
    let mut row = HashMap::new();
    row.insert("id".to_owned(), json!(id));
    row.insert("name".to_owned(), json!(name));
    row
}

fn identity(id: i64) -> HashMap<String, Value> {
    [("id".to_owned(), json!(id))].into_iter().collect()
}

fn staged_change(id: i64, col: &str, original: Value, current: Value) -> PendingRowChange {
    let mut original_values = HashMap::new();
    original_values.insert(col.to_owned(), original);
    let mut current_values = HashMap::new();
    current_values.insert(col.to_owned(), current);
    PendingRowChange {
        row_index: None,
        row_identity: identity(id),
        original_values,
        current_values,
        changed_columns: vec![col.to_owned()],
        delete_marked: false,
        insert_marked: false,
        expression_values: None,
    }
}

fn table_state(rows: Vec<HashMap<String, Value>>) -> TableState {
    let mut ts = TableState::default();
    ts.columns = vec![pk_column("id"), plain_column("name")];
    ts.rows = rows;
    ts
}

/// An expression staged over an EXISTING change: the existing change's
/// other column survives, the expression's column joins, and the
/// placeholder takes the ORIGINAL for a column never written.
#[test]
fn the_expression_over_an_existing_change() {
    let existing = staged_change(1, "name", json!("a"), json!("b"));
    let result = compute_stage_expression_value_result(StageExpressionValueInput {
        existing: Some(existing),
        column: "other".to_owned(),
        original_value: json!("o"),
        row_identity: identity(1),
        row_index: 3,
    });
    let change = result.change.expect("the expression stages");
    // The existing column survives untouched.
    assert_eq!(change.current_values.get("name"), Some(&json!("b")));
    // The new column joins with the original as the placeholder.
    assert_eq!(change.current_values.get("other"), Some(&json!("o")));
    assert_eq!(change.original_values.get("other"), Some(&json!("o")));
    // Both columns are changed.
    assert!(change.changed_columns.contains(&"name".to_owned()));
    assert!(change.changed_columns.contains(&"other".to_owned()));
    // The row index hint takes the input's (the existing had none).
    assert_eq!(change.row_index, Some(3));
}

/// The delete-mark merge: a row with staged column changes keeps them
/// when the mark lands — the anchor points at the same key.
#[test]
fn the_delete_mark_merge_keeps_the_column_changes() {
    let key = "[\"id\":1]".to_owned();
    let mut ts = table_state(vec![row(1, "b")]);
    ts.pending_changes
        .insert(key.clone(), staged_change(1, "name", json!("a"), json!("b")));
    let outcome = PendingEditActions::stage_row_delete(&ts, &[0])
        .expect("the mark lands");
    let pending = outcome.patch.pending_changes.expect("the marks land");
    let change = pending.get(&key).expect("the existing key is reused");
    assert!(change.delete_marked);
    assert_eq!(change.changed_columns, vec!["name".to_owned()]);
    assert_eq!(change.original_values.get("name"), Some(&json!("a")));
    // The unstage brings the row back with its column changes intact.
    let next = compute_unstage_delete_result(change).expect("the changes survive");
    assert!(!next.delete_marked);
    assert_eq!(next.changed_columns, vec!["name".to_owned()]);
}

/// A paste whose write lands on the primary key: the row would no
/// longer be locatable — the paste's own identityChanged code.
#[test]
fn the_pk_paste_reports_identity_changed() {
    let ts = table_state(vec![row(1, "a"), row(2, "b")]);
    // React's caller hands the FILTERED primary-key columns (the TS
    // signature takes primaryKeyColumns).
    let pk_cols: Vec<ColumnSchema> = ts
        .columns
        .iter()
        .filter(|column| column.is_primary_key)
        .cloned()
        .collect();
    let writes = vec![CellPasteWrite {
        row_index: 0,
        column_name: "id".to_owned(),
        value: json!(2), // collides with row 1's id
    }];
    let outcome = stage_paste_writes_into(&ts, &writes, &pk_cols);
    assert_eq!(outcome.staged, 0);
    assert_eq!(outcome.problems.len(), 1);
    assert_eq!(outcome.problems[0].code, "grid.paste.identityChanged");
    assert_eq!(outcome.problems[0].row_index, 0);
    // The problem coordinates name the PK column.
    assert_eq!(outcome.problems[0].column_name, "id");
}

/// An expression-only row's unstage: the expression is a pending
/// change even when the placeholder equals the original, and the
/// revert clears it with the write it belongs to.
#[test]
fn the_expression_only_row_reverts() {
    let mut expression_only = staged_change(1, "name", json!("a"), json!("a"));
    expression_only.expression_values = Some([(
        "name".to_owned(),
        WriteExpressionId::CurrentTimestamp,
    )].into_iter().collect());
    // The revert path (value = None): the expression clears, and the
    // row — with no other changes and no delete mark — unstages.
    let result = compute_stage_cell_value_result(StageCellValueInput {
        existing: Some(expression_only),
        column: "name".to_owned(),
        value: None,
        original_value: json!("a"),
        row_identity: identity(1),
        row_index: 0,
    });
    assert!(
        result.change.is_none(),
        "the expression-only row unstages on revert (the delete mark absent)"
    );
}

/// The revert writes the row's anchor even when the staged change's
/// key came from the anchor (the PK-edit association's own round
/// trip).
#[test]
fn the_reverts_anchor_write_round_trips() {
    let key = "[\"id\":1]".to_owned();
    let mut ts = table_state(vec![row(1, "b")]);
    ts.pending_changes
        .insert(key.clone(), staged_change(1, "name", json!("a"), json!("b")));
    ts.row_identity_anchors.insert(0, key.clone());
    let outcome = PendingEditActions::stage_cell_value(&ts, 0, "name", None)
        .expect("the revert runs");
    let anchors = outcome.patch.row_identity_anchors.expect("the anchors land");
    assert_eq!(anchors.get(&0).map(String::as_str), Some("[\"id\":1]"));
    let rows = outcome.patch.rows.expect("the row is written");
    assert_eq!(rows[0].get("name"), Some(&json!("a")));
}

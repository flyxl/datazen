//! T294: the staged-edit actions — coder vectors: the guards (no row,
//! no PK, ambiguous identity), the three-state write intents, the
//! revert path, the expression staging, the delete mark and its undo,
//! the paste batch, the column loop, and the rollback.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::table_data_connection_state::{
    ColumnSchema, PendingRowChange, TableState,
};
use crate::table_data_pending_edit_actions::{
    apply_patch, compute_stage_cell_value_result, compute_stage_expression_value_result,
    compute_stage_result, compute_unstage_delete_result, restored_cell_value,
    stage_paste_writes_into, CellPasteWrite, PendingEditActions, StageCellValueInput,
    StageEditInput, StageExpressionValueInput,
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

fn table_state(rows: Vec<HashMap<String, Value>>) -> TableState {
    let mut ts = TableState::default();
    ts.columns = vec![pk_column("id"), plain_column("name")];
    ts.rows = rows;
    ts
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

fn with_staged(mut ts: TableState, key: &str, change: PendingRowChange) -> TableState {
    ts.pending_changes.insert(key.to_owned(), change);
    ts
}

// -- computeStageResult (the T341 placeholder core) --------------------------

#[test]
fn the_stage_edit_rule_stages_edits_and_unstages_reverts() {
    // Editing a cell to a differing value stages it.
    let staged = compute_stage_result(StageEditInput {
        existing: None,
        column: "name".to_owned(),
        original_value: json!("a"),
        current_value: json!("b"),
        row_identity: identity(1),
        row_index: 0,
    });
    let change = staged.change.expect("a differing value stages");
    assert_eq!(change.changed_columns, vec!["name".to_owned()]);
    assert_eq!(change.current_values.get("name"), Some(&json!("b")));

    // Editing it back to the original unstages the row entirely.
    let staged = compute_stage_result(StageEditInput {
        existing: Some(staged_change(1, "name", json!("a"), json!("b"))),
        column: "name".to_owned(),
        original_value: json!("a"),
        current_value: json!("a"),
        row_identity: identity(1),
        row_index: 0,
    });
    assert!(staged.change.is_none(), "reverting the only changed column unstages");

    // A delete-marked row keeps its entry even when every column was
    // reverted.
    let mut marked = staged_change(1, "name", json!("a"), json!("b"));
    marked.delete_marked = true;
    let staged = compute_stage_result(StageEditInput {
        existing: Some(marked),
        column: "name".to_owned(),
        original_value: json!("a"),
        current_value: json!("a"),
        row_identity: identity(1),
        row_index: 0,
    });
    let change = staged.change.expect("the delete mark keeps the entry");
    assert!(change.delete_marked);
    assert!(change.changed_columns.is_empty());
}

#[test]
fn the_unstage_delete_rule_drops_only_the_mark() {
    // A row with only the mark: nothing left staged.
    let mut marked = staged_change(1, "name", json!("a"), json!("b"));
    marked.changed_columns = vec!["name".to_owned()];
    marked.delete_marked = true;
    let next = compute_unstage_delete_result(&marked).expect("the column changes survive");
    assert!(!next.delete_marked);

    // A row that was ONLY marked: the key leaves the map.
    let mut only_marked = marked.clone();
    only_marked.changed_columns = Vec::new();
    assert!(compute_unstage_delete_result(&only_marked).is_none());
}

// -- computeStageCellValueResult (the T328 placeholder core) ------------------

#[test]
fn the_three_states_of_a_write() {
    // A value: written.
    let result = compute_stage_cell_value_result(StageCellValueInput {
        existing: None,
        column: "name".to_owned(),
        value: Some(json!("b")),
        original_value: json!("a"),
        row_identity: identity(1),
        row_index: 0,
    });
    assert!(result.change.is_some());

    // NULL is a value like any other.
    let result = compute_stage_cell_value_result(StageCellValueInput {
        existing: None,
        column: "name".to_owned(),
        value: Some(Value::Null),
        original_value: json!("a"),
        row_identity: identity(1),
        row_index: 0,
    });
    let change = result.change.expect("a NULL is written");
    assert_eq!(change.current_values.get("name"), Some(&Value::Null));

    // None (do not write) with nothing existing: no change at all.
    let result = compute_stage_cell_value_result(StageCellValueInput {
        existing: None,
        column: "name".to_owned(),
        value: None,
        original_value: json!("a"),
        row_identity: identity(1),
        row_index: 0,
    });
    assert!(result.change.is_none());

    // None with an existing change: the column leaves the change list.
    let result = compute_stage_cell_value_result(StageCellValueInput {
        existing: Some(staged_change(1, "name", json!("a"), json!("b"))),
        column: "name".to_owned(),
        value: None,
        original_value: json!("a"),
        row_identity: identity(1),
        row_index: 0,
    });
    assert!(result.change.is_none(), "the revert unstages the only column");
}

#[test]
fn a_literal_replaces_a_staged_expression() {
    let mut with_expression = staged_change(1, "name", json!("a"), json!("a"));
    with_expression.expression_values = Some([(
        "name".to_owned(),
        crate::table_data_connection_state::WriteExpressionId::CurrentTimestamp,
    )].into_iter().collect());

    // The expression staging itself.
    let result = compute_stage_expression_value_result(StageExpressionValueInput {
        existing: None,
        column: "name".to_owned(),
        original_value: json!("a"),
        row_identity: identity(1),
        row_index: 0,
    });
    let change = result.change.expect("the expression stages");
    assert_eq!(
        change.expression_values.as_ref().and_then(|values| values.get("name").copied()),
        Some(crate::table_data_connection_state::WriteExpressionId::CurrentTimestamp)
    );
    // The placeholder keeps the current value.
    assert_eq!(change.current_values.get("name"), Some(&json!("a")));
    assert_eq!(change.changed_columns, vec!["name".to_owned()]);

    // A literal on the same column replaces the expression.
    let result = compute_stage_cell_value_result(StageCellValueInput {
        existing: Some(change),
        column: "name".to_owned(),
        value: Some(json!("z")),
        original_value: json!("a"),
        row_identity: identity(1),
        row_index: 0,
    });
    let change = result.change.expect("the literal stages");
    assert!(
        change.expression_values.as_ref().map_or(true, |values| !values.contains_key("name")),
        "the literal replaces the expression"
    );
    assert_eq!(change.current_values.get("name"), Some(&json!("z")));

    // The revert clears both: a delete-marked entry keeps the row (the
    // mark is a fact of its own) with the expression gone.
    let mut marked_expression = with_expression;
    marked_expression.delete_marked = true;
    let result = compute_stage_cell_value_result(StageCellValueInput {
        existing: Some(marked_expression),
        column: "name".to_owned(),
        value: None,
        original_value: json!("a"),
        row_identity: identity(1),
        row_index: 0,
    });
    let change = result.change.expect("the delete-marked entry keeps the revert");
    assert!(change.expression_values.map_or(true, |values| values.is_empty()));
    assert!(change.delete_marked);
}

#[test]
fn restored_cell_value_reads_the_original() {
    // Never staged: null (the cell keeps whatever it holds).
    assert_eq!(restored_cell_value(None, "name"), Value::Null);
    // A column outside the change list: null.
    let change = staged_change(1, "name", json!("a"), json!("b"));
    assert_eq!(restored_cell_value(Some(&change), "other"), Value::Null);
    // A staged column: the original.
    assert_eq!(restored_cell_value(Some(&change), "name"), json!("a"));
}

// -- the actions ----------------------------------------------------------------

#[test]
fn the_guards_refuse_the_unstageable() {
    // No row: no write.
    let ts = table_state(vec![row(1, "a")]);
    assert!(PendingEditActions::stage_cell_value(&ts, 9, "name", Some(json!("x"))).is_none());

    // No primary key: the noPrimaryKey error.
    let mut ts = table_state(vec![row(1, "a")]);
    ts.columns = vec![plain_column("name")];
    let outcome = PendingEditActions::stage_cell_value(&ts, 0, "name", Some(json!("x")))
        .expect("the refusal is a patch");
    assert_eq!(
        outcome.patch.error.and_then(|error| error),
        Some(crate::i18n::t("tableData.noPrimaryKey").to_owned())
    );

    // An ambiguous identity (a duplicate id in the loaded set).
    let ts = table_state(vec![row(1, "a"), row(1, "b")]);
    let outcome = PendingEditActions::stage_cell_value(&ts, 0, "name", Some(json!("x")))
        .expect("the refusal is a patch");
    assert_eq!(
        outcome.patch.error.and_then(|error| error),
        Some(crate::table_data_pending_changes::AMBIGUOUS_ROW_IDENTITY_ERROR.to_owned())
    );
}

#[test]
fn a_stage_cell_change_lands_the_write_and_the_anchor() {
    let ts = table_state(vec![row(1, "a"), row(2, "b")]);
    let outcome = PendingEditActions::stage_cell_change(&ts, 0, "name", Some(json!("edited")))
        .expect("the healthy write lands");
    let patch = outcome.patch;
    let rows = patch.rows.expect("the row is written");
    assert_eq!(rows[0].get("name"), Some(&json!("edited")));
    assert_eq!(rows[1].get("name"), Some(&json!("b")));
    let pending = patch.pending_changes.expect("the change is staged");
    assert_eq!(pending.len(), 1);
    let anchors = patch.row_identity_anchors.expect("the anchor lands");
    assert_eq!(anchors.get(&0).map(String::as_str), Some("[\"id\":1]"));
    assert_eq!(patch.error.and_then(|error| error), None, "the error clears");
    assert!(patch.preview_plan.is_some(), "the preview resets");
}

#[test]
fn the_revert_path_restores_the_original() {
    // A staged change on row 0's name; the revert goes back to "a".
    let key = "[\"id\":1]".to_owned();
    let ts = with_staged(
        table_state(vec![row(1, "b")]),
        &key,
        staged_change(1, "name", json!("a"), json!("b")),
    );
    let outcome = PendingEditActions::stage_cell_value(&ts, 0, "name", None)
        .expect("the revert path runs");
    let patch = outcome.patch;
    let rows = patch.rows.expect("the row is written");
    assert_eq!(rows[0].get("name"), Some(&json!("a")), "the cell goes back to the original");
    let pending = patch.pending_changes.expect("the map is written");
    assert!(pending.is_empty(), "the only changed column unstages the row");
}

#[test]
fn the_stage_row_delete_marks_and_the_unstage_undo() {
    let ts = table_state(vec![row(1, "a"), row(2, "b"), row(3, "c")]);
    let outcome = PendingEditActions::stage_row_delete(&ts, &[2, 0, 2])
        .expect("the delete marks land");
    let patch = outcome.patch;
    let pending = patch.pending_changes.expect("the marks land");
    assert_eq!(pending.len(), 2, "the duplicate index dedupes");
    for change in pending.values() {
        assert!(change.delete_marked);
    }
    let anchors = patch.row_identity_anchors.expect("the anchors land");
    assert_eq!(anchors.len(), 2);

    // The unstage: a row that was only marked leaves the map.
    let key = "[\"id\":1]".to_owned();
    let mut marked = staged_change(1, "name", json!("a"), json!("a"));
    marked.changed_columns = Vec::new();
    marked.delete_marked = true;
    let ts = with_staged(table_state(vec![row(1, "a")]), &key, marked);
    let outcome = PendingEditActions::unstage_row_delete(&ts, 0)
        .expect("the unstage runs");
    let pending = outcome.patch.pending_changes.expect("the map is written");
    assert!(pending.is_empty(), "an only-marked row is fully unstaged");
}

#[test]
fn delete_rows_dedupes_then_marks() {
    let ts = table_state(vec![row(1, "a"), row(2, "b")]);
    let outcome = PendingEditActions::delete_rows(&ts, &[1, 1, 0])
        .expect("the delete lands");
    let pending = outcome.patch.pending_changes.expect("the marks land");
    assert_eq!(pending.len(), 2);

    // Empty: no write at all.
    assert!(PendingEditActions::delete_rows(&ts, &[]).is_none());
}

#[test]
fn the_paste_batch_lands_and_reports() {
    let ts = table_state(vec![row(1, "a"), row(2, "b")]);
    let writes = vec![
        CellPasteWrite { row_index: 0, column_name: "name".to_owned(), value: json!("x") },
        CellPasteWrite { row_index: 9, column_name: "name".to_owned(), value: json!("y") },
        CellPasteWrite { row_index: 1, column_name: "name".to_owned(), value: Value::Null },
    ];
    let (outcome, result) = PendingEditActions::stage_paste_writes(&ts, &writes);
    assert_eq!(result.staged, 2, "two good cells");
    assert_eq!(result.skipped, 1, "the out-of-bounds cell");
    assert_eq!(result.problems[0].code, "grid.paste.outOfBounds");
    let outcome = outcome.expect("the landing patch");
    let rows = outcome.patch.rows.expect("the rows land");
    assert_eq!(rows[0].get("name"), Some(&json!("x")));
    assert_eq!(rows[1].get("name"), Some(&Value::Null));

    // The out-of-bounds problem carries its coordinates.
    assert_eq!(result.problems[0].row_index, 9);
    assert_eq!(result.problems[0].column_name, "name");
}

#[test]
fn an_all_skipped_paste_leaves_the_state_untouched() {
    let ts = table_state(vec![row(1, "a")]);
    let writes = vec![CellPasteWrite {
        row_index: 9,
        column_name: "name".to_owned(),
        value: json!("x"),
    }];
    let (outcome, result) = PendingEditActions::stage_paste_writes(&ts, &writes);
    assert!(outcome.is_none(), "no patch when every cell was skipped");
    assert_eq!(result.staged, 0);
    assert_eq!(result.skipped, 1);
}

#[test]
fn the_no_pk_paste_reports_every_cell() {
    let mut ts = table_state(vec![row(1, "a")]);
    ts.columns = vec![plain_column("name")];
    let writes = vec![
        CellPasteWrite { row_index: 0, column_name: "name".to_owned(), value: json!("x") },
        CellPasteWrite { row_index: 1, column_name: "name".to_owned(), value: json!("y") },
    ];
    let outcome = stage_paste_writes_into(&ts, &writes, &[]);
    assert_eq!(outcome.staged, 0);
    assert_eq!(outcome.problems.len(), 2);
    assert_eq!(outcome.problems[0].code, "grid.cell.noRowIdentity");
}

#[test]
fn the_apply_column_loop_sees_each_previous_write() {
    // Two rows with the same id? No — the loop's working state: writing
    // row 0's name, then row 1's, both staged under their own keys.
    let ts = table_state(vec![row(1, "a"), row(2, "b")]);
    let outcomes = PendingEditActions::apply_column_to_rows(&ts, "name", Some(json!("z")), &[0, 1]);
    assert_eq!(outcomes.len(), 2);
    let last = outcomes.last().expect("the loop ran");
    let pending = last.patch.pending_changes.as_ref().expect("the write landed");
    assert_eq!(pending.len(), 2, "both rows staged");
}

#[test]
fn the_rollback_drops_everything_and_reloads() {
    let mut ts = table_state(vec![row(1, "a")]);
    ts.pending_changes
        .insert("[\"id\":1]".to_owned(), staged_change(1, "name", json!("a"), json!("b")));
    ts.highlighted_new_row_key = Some("draft-1".to_owned());
    let outcome = PendingEditActions::rollback_pending_changes(&ts)
        .expect("the rollback runs");
    let patch = outcome.patch;
    assert!(outcome.reload, "the rollback re-reads the page");
    assert!(patch.pending_changes.expect("the map is written").is_empty());
    assert!(patch.draft_inserts.expect("the drafts drop").is_empty());
    assert!(patch.draft_insert_order.expect("the order drops").is_empty());
    assert_eq!(patch.highlighted_new_row_key, Some(None));
}

#[test]
fn apply_patch_writes_the_landed_fields() {
    let mut ts = table_state(vec![row(1, "a")]);
    let outcome = PendingEditActions::stage_cell_change(&ts, 0, "name", Some(json!("edited")))
        .expect("the write lands");
    apply_patch(&mut ts, &outcome.patch);
    assert_eq!(ts.rows[0].get("name"), Some(&json!("edited")));
    assert_eq!(ts.pending_changes.len(), 1);
    assert_eq!(ts.row_identity_anchors.get(&0).map(String::as_str), Some("[\"id\":1]"));
    assert!(ts.preview_plan.is_none());
    assert_eq!(ts.error, None);
}

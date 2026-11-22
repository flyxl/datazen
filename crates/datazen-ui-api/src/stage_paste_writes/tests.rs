use serde_json::json;

use crate::{PendingRowChange, RowIdentity, ValueMap};

use super::*;

fn row(id: i64, name: &str) -> ValueMap {
    let mut value = ValueMap::new();
    value.insert("id".to_owned(), json!(id));
    value.insert("name".to_owned(), json!(name));
    value
}

fn state(rows: Vec<ValueMap>) -> StagedTableState {
    StagedTableState { rows, ..Default::default() }
}

fn pk() -> Vec<String> {
    vec!["id".to_owned()]
}

fn write(row_index: u64, column: &str, value: serde_json::Value) -> CellPasteWrite {
    CellPasteWrite { row_index, column_name: column.to_owned(), value }
}

fn pending(id: i64, current_id: Option<i64>) -> PendingRowChange {
    let mut row_identity = RowIdentity::new();
    row_identity.insert("id".to_owned(), json!(id));
    let mut current_values = ValueMap::new();
    if let Some(next) = current_id {
        current_values.insert("id".to_owned(), json!(next));
    }
    PendingRowChange {
        row_index: Some(0),
        row_identity,
        original_values: ValueMap::new(),
        current_values,
        changed_columns: Vec::new(),
        delete_marked: false,
        insert_marked: None,
        expression_values: None,
    }
}

#[test]
fn grid_error_codes_and_gap_are_pinned() {
    assert_eq!(GridErrorCode::CellNoRowIdentity.as_str(), "grid.cell.noRowIdentity");
    assert_eq!(GridErrorCode::CellAmbiguousIdentity.as_str(), "grid.cell.ambiguousIdentity");
    assert_eq!(GridErrorCode::PasteOutOfBounds.as_str(), "grid.paste.outOfBounds");
    assert_eq!(GridErrorCode::PasteIdentityChanged.as_str(), "grid.paste.identityChanged");
    assert_eq!(GridErrorCode::PasteCrossTable.as_str(), "grid.paste.crossTable");
    assert_eq!(GridErrorCode::Unknown.as_str(), "unknown");
    assert_eq!(STAGE_PASTE_WRITES_GAP.len(), 3);
}

#[test]
fn identity_helpers_handle_nulls_uniqueness_and_pending_overlay() {
    assert!(build_row_identity(&row(1, "a"), &pk()).is_some());
    assert!(build_row_identity(&row(1, "a"), &[]).is_none());
    let mut null_pk = row(1, "a");
    null_pk.insert("id".to_owned(), serde_json::Value::Null);
    assert!(build_row_identity(&null_pk, &pk()).is_none());
    let mut missing = row(1, "a");
    missing.remove("id");
    assert!(build_row_identity(&missing, &pk()).is_none());

    let identity = build_row_identity(&row(1, "a"), &pk()).expect("a stable identity");
    assert_eq!(row_identity_key(&identity).as_deref(), Some("[\"id\":1]"));
    assert_eq!(row_identity_key(&RowIdentity::new()), None);

    let two_rows = state(vec![row(1, "a"), row(2, "b")]);
    assert!(row_identity_is_unique(&two_rows, 0, &identity, &pk()));
    let duplicate = build_row_identity(&row(2, "b"), &pk()).expect("identity");
    assert!(!row_identity_is_unique(&two_rows, 0, &duplicate, &pk()));

    let overlaid = effective_pending_identity(&pending(1, Some(2)), &pk()).expect("overlay");
    assert_eq!(row_identity_key(&overlaid).as_deref(), Some("[\"id\":2]"));
    assert!(effective_pending_identity(&pending(1, None), &pk()).is_some());

    let mut with_pending = state(vec![row(1, "a")]);
    with_pending.pending_changes.insert("other".to_owned(), pending(1, Some(2)));
    let moved = build_row_identity(&row(2, "x"), &pk()).expect("identity");
    assert!(has_pending_identity_collision(&with_pending, None, &moved, &pk()));
    assert!(!has_pending_identity_collision(&with_pending, Some("other"), &moved, &pk()));
}

#[test]
fn find_pending_for_row_prefers_the_anchor_then_the_direct_key() {
    let mut with_pending = state(vec![row(1, "a")]);
    let change = pending(1, None);
    with_pending.pending_changes.insert("anchor-key".to_owned(), change.clone());
    with_pending.row_identity_anchors.insert(0, "anchor-key".to_owned());
    let found = find_pending_for_row(&with_pending, 0, &row(1, "a"), &pk()).expect("anchored");
    assert_eq!(found.0, "anchor-key");
    assert_eq!(found.1, change);

    with_pending.row_identity_anchors.clear();
    with_pending.pending_changes.insert("[\"id\":1]".to_owned(), pending(1, None));
    let direct = find_pending_for_row(&with_pending, 0, &row(1, "a"), &pk()).expect("direct");
    assert_eq!(direct.0, "[\"id\":1]");

    with_pending.pending_changes.clear();
    assert!(find_pending_for_row(&with_pending, 0, &row(1, "a"), &pk()).is_none());
}

#[test]
fn empty_writes_and_missing_pk_leave_the_state_untouched() {
    let original = state(vec![row(1, "a")]);
    let empty = stage_paste_writes_into(&original, &[], &pk());
    assert_eq!(empty.rows, original.rows);
    assert_eq!(empty.staged, 0);
    assert!(empty.problems.is_empty());

    let no_pk = stage_paste_writes_into(&original, &[write(0, "name", json!("b"))], &[]);
    assert_eq!(no_pk.rows, original.rows);
    assert_eq!(no_pk.staged, 0);
    assert_eq!(no_pk.problems.len(), 1);
    assert_eq!(no_pk.problems[0].code, GridErrorCode::CellNoRowIdentity);
}

#[test]
fn a_bad_cell_is_skipped_without_discarding_the_good_ones() {
    let original = state(vec![row(1, "a"), row(2, "b")]);
    let outcome = stage_paste_writes_into(
        &original,
        &[write(9, "name", json!("oops")), write(1, "name", json!("new"))],
        &pk(),
    );
    assert_eq!(outcome.staged, 1);
    assert_eq!(outcome.problems.len(), 1);
    assert_eq!(outcome.problems[0].code, GridErrorCode::PasteOutOfBounds);
    assert_eq!(outcome.rows[1].get("name"), Some(&json!("new")));
    assert_eq!(outcome.rows[0].get("name"), Some(&json!("a")));

    let duplicated = state(vec![row(1, "a"), row(1, "b")]);
    let outcome = stage_paste_writes_into(&duplicated, &[write(0, "name", json!("x"))], &pk());
    assert_eq!(outcome.staged, 0);
    assert!(outcome.rows == duplicated.rows, "nothing was applied");
    assert_eq!(outcome.problems[0].code, GridErrorCode::CellAmbiguousIdentity);
}

#[test]
fn a_successful_write_stages_metadata_and_advances_rows() {
    let original = state(vec![row(1, "a")]);
    let outcome = stage_paste_writes_into(&original, &[write(0, "name", json!("new"))], &pk());
    assert_eq!(outcome.staged, 1);
    assert!(outcome.problems.is_empty());
    assert_eq!(outcome.rows[0].get("name"), Some(&json!("new")));
    assert_eq!(outcome.row_identity_anchors.get(&0).map(String::as_str), Some("[\"id\":1]"));
    let change = outcome.pending_changes.get("[\"id\":1]").expect("a staged change");
    assert_eq!(change.changed_columns, vec!["name".to_owned()]);
    assert_eq!(change.original_values.get("name"), Some(&json!("a")));
    assert_eq!(change.current_values.get("name"), Some(&json!("new")));
    assert_eq!(change.row_index, Some(0));
}

#[test]
fn a_second_write_in_the_same_paste_sees_the_first_and_can_revert() {
    let original = state(vec![row(1, "old")]);
    let outcome = stage_paste_writes_into(
        &original,
        &[write(0, "name", json!("new")), write(0, "name", json!("old"))],
        &pk(),
    );
    assert_eq!(outcome.staged, 1, "only the first write really changed a cell");
    assert_eq!(outcome.rows[0].get("name"), Some(&json!("old")));
    assert!(outcome.pending_changes.is_empty(), "the reverted write removed the pending change");
    assert_eq!(outcome.row_identity_anchors.get(&0).map(String::as_str), Some("[\"id\":1]"));
}

#[test]
fn a_paste_that_moves_the_primary_key_is_refused() {
    let original = state(vec![row(1, "a"), row(2, "b")]);
    let outcome = stage_paste_writes_into(&original, &[write(0, "id", json!(2))], &pk());
    assert_eq!(outcome.staged, 0);
    assert_eq!(outcome.problems.len(), 1);
    assert_eq!(outcome.problems[0].code, GridErrorCode::PasteIdentityChanged);
    assert!(outcome.rows == original.rows);
}

#[test]
fn crate_root_surface_is_reexported() {
    let _ = crate::stage_paste_writes_into;
    let _ = crate::build_row_identity;
    let _ = crate::row_identity_key;
    let _ = crate::row_identity_is_unique;
    let _ = crate::effective_pending_identity;
    let _ = crate::has_pending_identity_collision;
    let _ = crate::find_pending_for_row;
    assert_eq!(crate::STAGE_PASTE_WRITES_GAP.len(), 3);
    let _: Option<crate::CellPasteWrite> = None;
    let _: Option<crate::PasteCellProblem> = None;
    let _: Option<crate::StagePasteWritesOutcome> = None;
    let _: Option<crate::StagedTableState> = None;
    let _: Option<crate::GridErrorCode> = None;
}

//! T626: pure `useTableDraftRows` / draftInserts logic — creation order,
//! three-state cell values, interaction fields, duplicate/copy and dirty
//! tracking over a host-owned VM. No store, IPC or timer.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::views::table_view::draft_rows::{
    draft_wire, DraftRowVm, DraftRowsVm, DraftWire,
};

fn vm() -> DraftRowsVm {
    DraftRowsVm {
        enabled: true,
        ..Default::default()
    }
}

fn row(id: &str, values: &[(&str, Value)]) -> DraftRowVm {
    DraftRowVm {
        id: id.to_owned(),
        values: values.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect(),
    }
}

#[test]
fn add_draft_appends_empty_row_and_tracks_active() {
    let mut state = vm();
    assert_eq!(state.add_draft_row("draft-1".to_owned()), Some("draft-1".to_owned()));
    assert_eq!(state.draft_insert_count(), 1);
    assert_eq!(state.active_draft_id.as_deref(), Some("draft-1"));
    assert!(DraftRowsVm::is_empty_draft(&state.ordered_drafts()[0]));
    assert!(state.preview_stale);
    assert_eq!(state.add_draft_row("draft-2".to_owned()), Some("draft-2".to_owned()));
    assert_eq!(
        state.ordered_drafts().iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
        vec!["draft-1", "draft-2"]
    );
    assert!(state.has_draft_work());

    let mut read_only = vm();
    read_only.enabled = false;
    assert!(!read_only.can_add());
    assert_eq!(read_only.add_draft_row("x".to_owned()), None, "read-only panel has no insert entry point");
    assert_eq!(read_only.draft_insert_count(), 0);
}

#[test]
fn set_value_distinguishes_null_from_absent_and_clears_editing() {
    let mut state = vm();
    state.rows.push(row("draft-1", &[]));
    state.start_draft_edit("draft-1".to_owned(), "note".to_owned());
    assert_eq!(state.editing_draft.as_ref().map(|(d, c)| (d.as_str(), c.as_str())), Some(("draft-1", "note")));

    assert!(state.set_draft_value("draft-1", "note", Some(Value::Null)));
    assert_eq!(state.ordered_drafts()[0].values.get("note"), Some(&Value::Null));
    assert_eq!(state.editing_draft, None, "a write leaves the cell editor");

    state.start_draft_edit("draft-1".to_owned(), "note".to_owned());
    assert!(state.set_draft_value("draft-1", "note", None));
    assert!(!state.ordered_drafts()[0].values.contains_key("note"), "undefined removes the key");

    assert!(!state.set_draft_value("missing", "x", Some(json!(1))), "unknown draft is a no-op");
    state.cancel_draft_edit();
    assert_eq!(state.editing_draft, None);
}

#[test]
fn remove_duplicate_and_activate_clear_the_right_fields() {
    let mut state = vm();
    state.rows.push(row("a", &[("name", json!("Alice"))]));
    state.rows.push(row("b", &[]));
    state.activate_draft_row("b".to_owned());
    state.start_draft_edit("b".to_owned(), "name".to_owned());

    assert!(state.duplicate_draft_row("a", "copy".to_owned()));
    let copy = state.rows.iter().find(|r| r.id == "copy").expect("copy exists");
    assert_eq!(copy.values.get("name"), Some(&json!("Alice")));
    assert_eq!(state.active_draft_id.as_deref(), Some("copy"));
    // Deep copy: changing the copy must not touch the source.
    let mut copy = copy.clone();
    copy.values.insert("name".to_owned(), json!("Bob"));
    let source = state.rows.iter().find(|r| r.id == "a").expect("source exists");
    assert_eq!(source.values.get("name"), Some(&json!("Alice")));

    assert!(state.remove_draft_row("b"));
    assert_eq!(state.editing_draft, None, "removing the edited draft clears editing");
    state.activate_draft_row("a".to_owned());
    assert!(state.remove_draft_row("b") == false, "a second removal is a no-op");
    assert_eq!(state.active_draft_id.as_deref(), Some("a"), "unrelated active draft kept");

    let mut read_only = vm();
    read_only.enabled = false;
    read_only.rows.push(row("a", &[]));
    assert!(!read_only.duplicate_draft_row("a", "x".to_owned()));
}

#[test]
fn escape_removes_only_empty_drafts_and_copy_preserves_order() {
    let mut state = vm();
    state.rows.push(row("empty-1", &[]));
    state.rows.push(row("filled", &[("name", json!("keep me"))]));
    state.rows.push(row("empty-2", &[]));
    state.activate_draft_row("empty-1".to_owned());
    assert_eq!(state.remove_empty_draft_rows(), 2);
    assert_eq!(state.ordered_drafts().len(), 1);
    assert_eq!(state.ordered_drafts()[0].id, "filled");
    assert_eq!(state.active_draft_id, None, "removed active draft is cleared");

    let mut pasted = vm();
    let mut values = BTreeMap::new();
    values.insert("name".to_owned(), json!("first"));
    values.insert("note".to_owned(), Value::Null);
    let ids = pasted.add_draft_rows_from_copy(&[("copy-1".to_owned(), values), ("copy-2".to_owned(), BTreeMap::new())]);
    assert_eq!(ids, vec!["copy-1", "copy-2"]);
    assert_eq!(pasted.draft_insert_count(), 2);
    assert!(DraftRowsVm::is_empty_draft(&pasted.ordered_drafts()[1]), "an empty copy is a valid draft");
    assert!(pasted.add_draft_rows_from_copy(&[]).is_empty(), "empty copy is a no-op");
}

#[test]
fn wire_shape_sorts_columns_and_keeps_null() {
    let draft = row("draft-1", &[("note", Value::Null), ("name", json!("ab"))]);
    let wire: DraftWire = draft_wire(&draft);
    assert!(wire.insert_marked);
    assert!(!wire.delete_marked);
    assert_eq!(wire.changed_columns, vec!["name", "note"]);
    assert_eq!(wire.current_values.get("name"), Some(&json!("ab")));
    assert_eq!(wire.current_values.get("note"), Some(&Value::Null));
    assert_eq!(wire.current_values.len(), 2);
}

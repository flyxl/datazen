//! Independent audit for T626: pure `useTableDraftRows` / draftInserts port.
//!
//! Pin dirty tracking and interaction-field edges the coder does not drive,
//! and three React contracts around Escape, paste-rows and stale interaction.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use datazen_ui::views::table_view::draft_rows::{draft_wire, DraftRowVm, DraftRowsVm};

fn vm() -> DraftRowsVm {
    DraftRowsVm { enabled: true, ..Default::default() }
}

fn row(id: &str, values: &[(&str, Value)]) -> DraftRowVm {
    DraftRowVm {
        id: id.to_owned(),
        values: values.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect(),
    }
}

#[test]
fn wire_shape_of_an_empty_draft_has_no_columns() {
    let wire = draft_wire(&row("d", &[]));
    assert!(wire.insert_marked);
    assert!(!wire.delete_marked);
    assert!(wire.changed_columns.is_empty());
    assert!(wire.current_values.is_empty());
}

#[test]
fn dirty_flag_tracks_every_mutating_transition() {
    let mut s = vm();
    s.preview_stale = false;
    s.add_draft_row("a".to_owned());
    assert!(s.preview_stale, "audit: add invalidates the preview");
    s.preview_stale = false;
    s.set_draft_value("a", "n", Some(json!(1)));
    assert!(s.preview_stale, "audit: write invalidates the preview");
    s.preview_stale = false;
    s.duplicate_draft_row("a", "b".to_owned());
    assert!(s.preview_stale, "audit: duplicate invalidates the preview");
    s.preview_stale = false;
    s.remove_draft_row("b");
    assert!(s.preview_stale, "audit: remove invalidates the preview");
    s.preview_stale = false;
    assert!(!s.remove_draft_row("missing"), "audit: removing an absent draft is a no-op");
    assert!(!s.preview_stale, "audit: a no-op removal keeps the preview fresh");
    s.preview_stale = false;
    assert!(s.add_draft_rows_from_copy(&[("c".to_owned(), BTreeMap::new())]).len() == 1);
    assert!(s.preview_stale, "audit: paste invalidates the preview");
}

#[test]
fn interaction_fields_follow_focus_and_cancel() {
    let mut s = vm();
    s.rows.push(row("a", &[]));
    s.rows.push(row("b", &[]));
    s.start_draft_edit("a".to_owned(), "n".to_owned());
    assert_eq!(s.active_draft_id.as_deref(), Some("a"));
    assert_eq!(s.editing_draft.as_ref().map(|(d, c)| (d.as_str(), c.as_str())), Some(("a", "n")));
    s.cancel_draft_edit();
    assert_eq!(s.editing_draft, None, "audit: cancel leaves the editor");
    assert_eq!(s.active_draft_id.as_deref(), Some("a"), "audit: cancel keeps the focused row");
    s.activate_draft_row("b".to_owned());
    assert_eq!(s.active_draft_id.as_deref(), Some("b"));
    s.start_draft_edit("b".to_owned(), "n".to_owned());
    assert!(s.editing_draft.is_some(), "audit setup: an editor is open before the failed write");
    assert!(!s.set_draft_value("ghost", "n", Some(json!(1))));
    assert_eq!(s.editing_draft, None, "audit: a failed write still clears editing");
}

// ----- React parity assertions -----

#[test]
fn react_escape_keeps_the_editing_target() {
    let mut s = vm();
    s.rows.push(row("empty", &[]));
    s.start_draft_edit("empty".to_owned(), "n".to_owned());
    assert_eq!(s.remove_empty_draft_rows(), 1);
    assert_eq!(
        s.editing_draft.as_ref().map(|(d, c)| (d.as_str(), c.as_str())),
        Some(("empty", "n")),
        "DEFECT: React removeEmptyDraftRows clears activeDraftId but never setEditingDraft (useTableDraftRows.ts:106-114)"
    );
}

#[test]
fn react_paste_rows_are_not_gated_by_enabled() {
    let mut s = vm();
    s.enabled = false;
    let values = BTreeMap::from([("name".to_owned(), json!("x"))]);
    let ids = s.add_draft_rows_from_copy(&[("copy-1".to_owned(), values)]);
    assert_eq!(
        ids,
        vec!["copy-1"],
        "DEFECT: React withDraftRowsFromCopy has no enabled gate (draftInserts.ts:156-174)"
    );
}

#[test]
fn react_removing_an_absent_draft_clears_stale_interaction() {
    let mut s = vm();
    s.activate_draft_row("ghost".to_owned());
    s.start_draft_edit("ghost".to_owned(), "n".to_owned());
    assert!(!s.remove_draft_row("ghost"));
    assert_eq!(
        s.active_draft_id,
        None,
        "DEFECT: React removeDraftRow always clears a matching activeDraftId even when the store draft is absent (useTableDraftRows.ts:86-93)"
    );
    assert_eq!(
        s.editing_draft,
        None,
        "DEFECT: React removeDraftRow always clears a matching editingDraft even when the store draft is absent (useTableDraftRows.ts:86-93)"
    );
}

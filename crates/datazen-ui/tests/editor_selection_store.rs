//! T209: the editor-selection store — the ephemeral per-panel text
//! selection of every mounted query editor: an empty write removes the
//! entry, an unchanged write is a no-op, clear drops it, and the
//! nullish-panel read is empty.

use datazen_ui::editor_selection_store::{
    clear_editor_selection, get_editor_selection, set_editor_selection,
    EditorSelectionStore,
};

#[test]
fn a_selection_lands_and_reads_back() {
    let mut store = EditorSelectionStore::default();
    store.set_selection("q1", "SELECT 1");
    assert_eq!(store.selection("q1"), "SELECT 1");
    assert_eq!(store.selection("q2"), "", "an unreported panel is empty");
}

#[test]
fn an_empty_selection_removes_the_entry() {
    let mut store = EditorSelectionStore::default();
    store.set_selection("q1", "SELECT 1");
    store.set_selection("q1", "");
    assert_eq!(store.selection("q1"), "", "the empty write removes the key");
}

#[test]
fn an_unchanged_selection_is_a_no_op() {
    let mut store = EditorSelectionStore::default();
    store.set_selection("q1", "SELECT 1");
    let before = store.clone();
    store.set_selection("q1", "SELECT 1");
    assert_eq!(store, before, "the unchanged write changes nothing");
}

#[test]
fn clear_drops_the_panel_entry_only() {
    let mut store = EditorSelectionStore::default();
    store.set_selection("q1", "SELECT 1");
    store.set_selection("q2", "SELECT 2");
    store.clear_selection("q1");
    assert_eq!(store.selection("q1"), "");
    assert_eq!(store.selection("q2"), "SELECT 2", "the sibling stays");
}

#[test]
fn clearing_a_missing_panel_is_a_no_op() {
    let mut store = EditorSelectionStore::default();
    let before = store.clone();
    store.clear_selection("nope");
    assert_eq!(store, before);
}

#[test]
fn the_global_entries_read_write_and_clear() {
    set_editor_selection("panel-a", "SELECT now()");
    set_editor_selection("panel-b", "SELECT 2");
    assert_eq!(get_editor_selection(Some("panel-a")), "SELECT now()");
    assert_eq!(get_editor_selection(None), "", "a nullish panel reads empty");
    assert_eq!(get_editor_selection(Some("panel-c")), "", "an unreported panel reads empty");
    clear_editor_selection("panel-a");
    assert_eq!(get_editor_selection(Some("panel-a")), "", "the clear dropped it");
    assert_eq!(get_editor_selection(Some("panel-b")), "SELECT 2", "the sibling survives");
    clear_editor_selection("panel-b");
}

#[test]
fn an_empty_global_write_removes_the_entry() {
    set_editor_selection("panel-d", "SELECT 3");
    set_editor_selection("panel-d", "");
    assert_eq!(get_editor_selection(Some("panel-d")), "");
}

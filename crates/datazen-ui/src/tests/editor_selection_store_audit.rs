//! T209 independent audit (tester): fresh vectors for the editor
//! selection store — whitespace-only selections (truthy in JS, so they
//! stay), replaced selections, panel-id isolation, and the global
//! entries measured through every combination.

use crate::editor_selection_store::{
    clear_editor_selection, get_editor_selection, set_editor_selection,
    EditorSelectionStore,
};

#[test]
fn audit_a_whitespace_selection_is_a_real_selection() {
    // React's `if (!selection)` only drops the empty string: whitespace
    // is truthy in JS, so a space-only selection stays an entry.
    let mut store = EditorSelectionStore::default();
    store.set_selection("w", " ");
    assert_eq!(store.selection("w"), " ", "whitespace is kept");
    // And it only goes away through clear or an empty write.
    store.clear_selection("w");
    assert_eq!(store.selection("w"), "");
}

#[test]
fn audit_replacing_a_selection_overwrites_not_appends() {
    let mut store = EditorSelectionStore::default();
    store.set_selection("q", "old");
    store.set_selection("q", "new");
    assert_eq!(store.selection("q"), "new", "the newer selection wins");
}

#[test]
fn audit_panel_ids_are_isolated() {
    let mut store = EditorSelectionStore::default();
    for (panel, selection) in [("a", "1"), ("b", "2"), ("c", "3")] {
        store.set_selection(panel, selection);
    }
    store.clear_selection("b");
    assert_eq!(store.selection("a"), "1");
    assert_eq!(store.selection("b"), "");
    assert_eq!(store.selection("c"), "3");
}

#[test]
fn audit_the_global_round_trip_through_every_combination() {
    for panel in ["x", "y"] {
        for (first, second, clear_first) in
            [("one", "two", true), ("one", "two", false), ("one", "", true)]
        {
            clear_editor_selection(panel);
            set_editor_selection(panel, first);
            if clear_first {
                clear_editor_selection(panel);
            }
            set_editor_selection(panel, second);
            let expected = if second.is_empty() { "" } else { second };
            assert_eq!(
                get_editor_selection(Some(panel)),
                expected,
                "panel {panel}: {first:?} then {second:?} (cleared: {clear_first})"
            );
        }
    }
    // The two panels never bleed into each other.
    set_editor_selection("x", "left");
    set_editor_selection("y", "right");
    assert_eq!(get_editor_selection(Some("x")), "left");
    assert_eq!(get_editor_selection(Some("y")), "right");
    clear_editor_selection("x");
    clear_editor_selection("y");
}

#[test]
fn audit_a_removed_then_recreated_entry_round_trips() {
    set_editor_selection("re", "first");
    set_editor_selection("re", "");
    assert_eq!(get_editor_selection(Some("re")), "");
    set_editor_selection("re", "second");
    assert_eq!(get_editor_selection(Some("re")), "second");
    clear_editor_selection("re");
}

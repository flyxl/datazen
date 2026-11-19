//! Editor-selection store ported from React
//! `packages/frontend/src/stores/editorSelectionStore.ts` (T209).
//!
//! The current text selection of every mounted query editor, keyed by
//! panel id. The selection is ephemeral: it exists only while at least
//! one editor for the panel is mounted, and the editor reports it on
//! every `selectionSet` / `docChanged` update; the AI drawer reads it
//! for the active query panel so a send can freeze the selection into
//! the chat snapshot.
//!
//! The React store is a zustand singleton with three actions and three
//! module-level entry points (`getEditorSelection`,
//! `setEditorSelection`, `clearEditorSelection`). The Rust port keeps
//! that shape: a `thread_local!` singleton over a plain map, the same
//! three transitions (an empty selection removes the key; an unchanged
//! write is a no-op; a missing key's clear is a no-op), and the same
//! three entries. The Rust editor port (the SqlEditor task) calls
//! [`set_editor_selection`] from its update listener; the AI drawer port
//! reads [`get_editor_selection`] when it freezes a snapshot.

use std::cell::RefCell;
use std::collections::HashMap;

/// The store (`useEditorSelectionStore`): the per-panel selections.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct EditorSelectionStore {
    by_panel: HashMap<String, String>,
}

impl EditorSelectionStore {
    /// `setSelection`: an empty selection removes the panel's entry (a
    /// selection that exists is never empty); an unchanged selection
    /// writes nothing; otherwise the new selection lands.
    pub(crate) fn set_selection(&mut self, panel_id: &str, selection: &str) {
        if selection.is_empty() {
            self.by_panel.remove(panel_id);
            return;
        }
        if self.by_panel.get(panel_id).map(String::as_str) == Some(selection) {
            return;
        }
        self.by_panel.insert(panel_id.to_owned(), selection.to_owned());
    }

    /// `clearSelection`: drop the panel's entry (unmounting the editor).
    pub(crate) fn clear_selection(&mut self, panel_id: &str) {
        self.by_panel.remove(panel_id);
    }

    /// The panel's current selection, or empty when nothing is reported.
    pub(crate) fn selection(&self, panel_id: &str) -> &str {
        self.by_panel.get(panel_id).map(String::as_str).unwrap_or("")
    }
}

thread_local! {
    /// The store singleton (`useEditorSelectionStore`).
    static EDITOR_SELECTION: RefCell<EditorSelectionStore> =
        RefCell::new(EditorSelectionStore::default());
}

/// `getEditorSelection`: the selection of the active query panel without
/// subscribing — empty for a nullish panel or an unreported one.
pub(crate) fn get_editor_selection(panel_id: Option<&str>) -> String {
    match panel_id {
        Some(panel_id) => EDITOR_SELECTION.with_borrow(|store| {
            store.selection(panel_id).to_owned()
        }),
        None => String::new(),
    }
}

/// `setEditorSelection`: the editor's update-listener write.
pub(crate) fn set_editor_selection(panel_id: &str, selection: &str) {
    EDITOR_SELECTION.with_borrow_mut(|store| store.set_selection(panel_id, selection));
}

/// `clearEditorSelection`: drop the entry when the editor unmounts.
pub(crate) fn clear_editor_selection(panel_id: &str) {
    EDITOR_SELECTION.with_borrow_mut(|store| store.clear_selection(panel_id));
}

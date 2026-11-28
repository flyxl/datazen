//! Give each open query tab its own editor without changing the native input
//! and rendering entry points. The selected draft lives in `AppState::sql`;
//! switching moves it, with its undo and redo history, into the parked map.

use std::{cell::Cell, collections::HashMap, rc::Rc};

use datazen_ui_api::{NativeExecuteShortcut, TabKind, WorkspaceApi};
use rune::Tx;

use crate::{state_def::AppState, types::Section, sql_editor::SqlEditor};

type EditorScroll = Rc<Cell<[f32; 2]>>;

/// The callback and its internal scroll anchor belong to the same draft.
#[derive(Clone, Debug)]
pub(crate) struct QueryEditorViewport {
    pub anchor: String,
    pub offset: EditorScroll,
}

impl QueryEditorViewport {
    /// A dock action built for another draft (including a reset/reused tab
    /// id) must never select ranges in the now-mounted editor.
    pub(crate) fn owns_current_query(&self, state: &AppState) -> bool {
        state.active == Section::Workspace
            && state.native_kv.is_none()
            && state.connect_loading.is_none()
            && state.query_editor_matches_connection()
            && state.tabs.selected().is_some_and(|tab| {
                tab.kind == TabKind::Query && state.query_editor_tabs.active == Some(tab.id)
            })
            && Rc::ptr_eq(&self.offset, &state.query_editor_tabs.scroll)
    }
}

fn scroll_anchor(tab: u32) -> String {
    format!("query-editor-draft-{tab}-scroll-anchor")
}

struct ParkedDraft {
    editor: SqlEditor,
    scroll: EditorScroll,
    connection: Option<String>,
}

#[derive(Default)]
pub(crate) struct QueryEditorTabs {
    active: Option<u32>,
    scroll: EditorScroll,
    connection: Option<String>,
    parked: HashMap<u32, ParkedDraft>,
    scroll_mount: Option<u32>,
    pending_scroll: Option<[f32; 2]>,
    snippets_menu: Option<u32>,
}

impl AppState {
    pub(crate) fn sync_query_editor_to_selected_tab(&mut self) {
        let next = self.tabs.selected().filter(|tab| tab.kind == TabKind::Query).map(|tab| tab.id);
        let previous = self.query_editor_tabs.active;
        if next == previous {
            return;
        }

        self.cancel_query_editor_input();
        if let Some(offset) = self.query_editor_tabs.pending_scroll.take() {
            // The new viewport can report its first layout before app_poll.
            // If it is left again that same frame, keep the intended offset.
            self.query_editor_tabs.scroll.set(offset);
        }
        let old = ParkedDraft {
            editor: std::mem::replace(&mut self.sql, SqlEditor::from_doc("")),
            scroll: std::mem::take(&mut self.query_editor_tabs.scroll),
            connection: self.query_editor_tabs.connection.take(),
        };
        if let Some(id) = previous {
            if self.tabs.tabs().iter().any(|tab| tab.id == id && tab.kind == TabKind::Query) {
                self.query_editor_tabs.parked.insert(id, old);
            }
        }

        self.query_editor_tabs.active = next;
        self.query_editor_tabs.scroll_mount = None;
        if let Some(id) = next {
            if let Some(draft) = self.query_editor_tabs.parked.remove(&id) {
                self.sql = draft.editor;
                self.query_editor_tabs.scroll = draft.scroll;
                self.query_editor_tabs.connection = draft.connection;
            } else {
                self.query_editor_tabs.connection = self.query_editor_current_connection();
            }
        }
        self.query_editor_tabs.pending_scroll = next.map(|_| self.query_editor_tabs.scroll.get());
        self.result_focused = false;
    }

    pub(crate) fn discard_query_editor_tab(&mut self, tab: u32) {
        self.query_editor_tabs.parked.remove(&tab);
    }

    pub(crate) fn reset_query_editor_tabs(&mut self) {
        self.cancel_native_query_format();
        self.cancel_query_regex();
        self.cancel_regex_marks();
        self.sql = SqlEditor::from_doc("");
        self.query_editor_tabs = QueryEditorTabs::default();
    }

    pub(crate) fn cancel_query_editor_input(&mut self) {
        self.cancel_native_query_format();
        self.cancel_query_regex();
        self.cancel_regex_marks();
        self.query_editor_tabs.snippets_menu = None;
        self.sql.preedit("");
        self.sql.dismiss_find();
        self.sql.goto_lease = None;
        self.sql.focused = false;
        self.query_editor_tabs.scroll_mount = None;
        if self.query_editor_tabs.active.is_some() && self.query_editor_tabs.pending_scroll.is_none() {
            self.query_editor_tabs.pending_scroll = Some(self.query_editor_tabs.scroll.get());
        }
    }

    pub(crate) fn query_snippet_menu_owner(&self) -> Option<u32> {
        self.query_editor_tabs.snippets_menu.filter(|owner| {
            self.snippet_edit_owner() == Some(*owner)
        })
    }

    pub(crate) fn snippet_edit_owner(&self) -> Option<u32> {
        if self.active != Section::Workspace || self.native_kv.is_some()
            || self.query_loading.is_some() || self.native_query_selected()
            || self.sql.composition().is_some()
        {
            return None;
        }
        let tab = self.tabs.selected().filter(|tab| tab.kind == TabKind::Query)?;
        (self.query_editor_tabs.active == Some(tab.id)).then_some(tab.id)
    }

    pub(crate) fn set_query_snippet_menu_owner(&mut self, owner: Option<u32>) {
        self.query_editor_tabs.snippets_menu = owner;
    }

    pub(crate) fn retire_invalid_query_snippet_menu(&mut self) -> bool {
        if self.query_editor_tabs.snippets_menu.is_some() && self.query_snippet_menu_owner().is_none() {
            self.query_editor_tabs.snippets_menu = None;
            return true;
        }
        false
    }

    pub(crate) fn query_editor_viewport(&self) -> Option<QueryEditorViewport> {
        Some(QueryEditorViewport {
            anchor: scroll_anchor(self.query_editor_tabs.active?),
            offset: self.query_editor_tabs.scroll.clone(),
        })
    }

    pub(crate) fn restore_query_editor_scroll(&mut self, tx: &mut Tx) -> bool {
        let mounted = if self.active == Section::Workspace && self.native_kv.is_none() {
            self.query_editor_tabs.active
        } else {
            None
        };
        if mounted == self.query_editor_tabs.scroll_mount {
            return false;
        }
        self.query_editor_tabs.scroll_mount = mounted;
        if let Some(tab) = mounted {
            // Defer the actual move until Rune commits the selected viewport.
            // Target a unique child: a queued click may switch tabs again after
            // poll but before commit; then a stale request must match nothing.
            let offset = self.query_editor_tabs.pending_scroll.take()
                .unwrap_or_else(|| self.query_editor_tabs.scroll.get());
            self.query_editor_tabs.scroll.set(offset);
            tx.scroll_to_instant(scroll_anchor(tab), offset[0], offset[1]);
            return true;
        }
        false
    }

    fn query_editor_current_connection(&self) -> Option<String> {
        self.desktop_connection.clone().or_else(|| {
            // Do not reinterpret the in-memory demo as the current connection
            // when a real live catalog is present without a profile identity.
            if self.desktop_catalog.is_some() {
                return None;
            }
            self.workspace.session().map(|session| session.connection_id.clone())
        })
    }

    pub(crate) fn query_editor_matches_connection(&self) -> bool {
        self.query_editor_tabs.connection.as_ref().is_none_or(|owner| {
            self.query_editor_current_connection().as_ref() == Some(owner)
        })
    }

    pub(crate) fn uses_alternate_execute_shortcut(&self) -> bool {
        self.desktop.native_execute_shortcut() == Some(NativeExecuteShortcut::ModR)
    }

    pub(crate) fn owns_query_execution_shortcut(&self, tx: &Tx) -> bool {
        tx.focused() == Some("sql-editor-scroll")
            && self.sql.focused()
            && !self.result_focused
            && self.connect_loading.is_none()
            && self.snippet_edit_owner().is_some()
            && self.query_editor_matches_connection()
    }

    pub(crate) fn owns_query_document_shortcut(&self, tx: &Tx) -> bool {
        self.active == Section::Workspace
            && self.native_kv.is_none()
            && tx.focused() == Some("sql-editor-scroll")
            && self.sql.focused()
            && self.sql.composition().is_none()
            && !self.result_focused
            && self.connect_loading.is_none()
            && self.tabs.selected().is_some_and(|tab| {
                tab.kind == TabKind::Query && self.query_editor_tabs.active == Some(tab.id)
            })
            && self.query_editor_matches_connection()
    }

    pub(crate) fn bind_query_editor_for_execution(&mut self) -> bool {
        if !self.query_editor_matches_connection() {
            self.query_action_notice("This query tab belongs to a different connection. Reopen that connection or start a new query.");
            return false;
        }
        if self.query_editor_tabs.active.is_some() && self.query_editor_tabs.connection.is_none() {
            self.query_editor_tabs.connection = self.query_editor_current_connection();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_dock_owner_never_targets_a_new_draft_even_with_the_same_tab_id() {
        let mut state = AppState::default();
        let a = state.open_query_tab("A").unwrap();
        let owner = state.query_editor_viewport().unwrap();
        assert!(owner.owns_current_query(&state));
        state.open_query_tab("B").unwrap();
        assert!(!owner.owns_current_query(&state));
        assert!(state.select_tab(a));
        assert!(owner.owns_current_query(&state), "the parked draft retains its identity");
        state.reset_query_editor_tabs();
        state.sync_query_editor_to_selected_tab();
        assert_eq!(state.tabs.selected().unwrap().id, a);
        assert!(!owner.owns_current_query(&state), "an old dock cannot hit a new editor");
    }

    #[test]
    fn first_layout_report_cannot_clobber_a_pending_offset_even_on_rapid_switch() {
        let mut s = AppState::default();
        let a = s.open_query_tab("A").unwrap();
        assert!(s.restore_query_editor_scroll(&mut Tx::default()));
        let a_frame = s.query_editor_viewport().unwrap();
        a_frame.offset.set([83.0, 414.0]);

        let b = s.open_query_tab("B").unwrap();
        assert!(s.restore_query_editor_scroll(&mut Tx::default()));
        let b_frame = s.query_editor_viewport().unwrap();
        b_frame.offset.set([19.0, 212.0]);
        assert_ne!(a_frame.anchor, b_frame.anchor, "queued A scroll cannot find B's viewport");

        assert!(s.select_tab(a));
        // In Rune the new view's first layout can run before the next poll.
        // Simulate a zero layout report and an immediate second tab click.
        let a_again = s.query_editor_viewport().unwrap();
        assert_eq!(a_again.anchor, a_frame.anchor);
        a_again.offset.set([0.0, 0.0]);
        assert!(s.select_tab(b));
        assert!(s.restore_query_editor_scroll(&mut Tx::default()));
        assert_eq!(b_frame.offset.get(), [19.0, 212.0]);

        assert!(s.select_tab(a));
        s.query_editor_viewport().unwrap().offset.set([0.0, 0.0]);
        assert!(s.restore_query_editor_scroll(&mut Tx::default()));
        assert_eq!(a_frame.offset.get(), [83.0, 414.0]);
    }
}

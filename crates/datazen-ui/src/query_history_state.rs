//! Connection-scoped history for the live query sidebar.
use crate::state_def::AppState;
use crate::types::{QuerySidebarPanel, Section};
use crate::views::query::query_sidebar_section::HistoryScope;
use rune::TextEdit;
use datazen_ui_api::{AdapterJob, QueryHistorySnapshot, TabKind};

#[derive(Default)]
pub(crate) struct QueryHistoryState {
    generation: u32,
    pub pending: bool,
    pub scope: HistoryScope,
    pub search: TextEdit,
    pub result: Option<Result<QueryHistorySnapshot, String>>,
}

impl QueryHistoryState {
    pub fn reset(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.pending = false;
        self.result = None;
    }

    pub fn reset_context(&mut self) {
        self.reset();
        self.scope = HistoryScope::Current;
        self.search.clear();
    }

    pub fn deliver(&mut self, generation: u32, result: Result<QueryHistorySnapshot, String>) {
        if self.pending && generation == self.generation {
            self.pending = false;
            self.result = Some(result);
        }
    }
}

impl AppState {
    fn current_query_accepts_library_action(&self) -> bool {
        self.active == Section::Workspace
            && self.tabs.selected().is_some_and(|tab| tab.kind == TabKind::Query)
    }

    pub(crate) fn query_sidebar_shows(&self, panel: QuerySidebarPanel) -> bool {
        self.query_sidebar_open && self.query_sidebar_panel == panel
    }

    pub(crate) fn toggle_query_sidebar(&mut self, panel: QuerySidebarPanel) {
        if !self.current_query_accepts_library_action() {
            return;
        }
        if self.query_sidebar_shows(panel) {
            self.close_query_sidebar();
        } else {
            self.select_query_sidebar(panel);
        }
    }

    pub(crate) fn close_query_sidebar(&mut self) {
        if !self.current_query_accepts_library_action() || !self.query_sidebar_open {
            return;
        }
        if self.query_sidebar_shows(QuerySidebarPanel::History) {
            self.query_history.reset();
        }
        self.query_sidebar_open = false;
        // Cancel only this region's transient drag if another input closes
        // its pane, leaving the user's saved width and other splits alone.
        if self.splitter_preview.as_ref()
            .is_some_and(|(key, _)| key == crate::constants::QUERY_SIDEBAR_WIDTH_KEY)
        {
            self.splitter_preview = None;
        }
    }

    pub(crate) fn select_query_sidebar(&mut self, panel: QuerySidebarPanel) {
        if !self.current_query_accepts_library_action() {
            return;
        }
        if self.query_sidebar_shows(QuerySidebarPanel::History) && panel != QuerySidebarPanel::History {
            self.query_history.reset();
        }
        self.query_sidebar_open = true;
        self.query_sidebar_panel = panel;
        self.refresh_query_history_if_open();
    }

    pub(crate) fn refresh_query_history_if_open(&mut self) {
        if !self.query_sidebar_shows(QuerySidebarPanel::History) {
            return;
        }
        self.query_history.reset();
        let Some(connection_id) = self.desktop_connection.clone() else {
            return;
        };
        self.query_history.pending = self.desktop.begin_job(AdapterJob::ReadQueryHistory {
            generation: self.query_history.generation,
            connection_id,
        });
        if !self.query_history.pending {
            self.query_history.result =
                Some(Err("Query history is unavailable in this host.".into()));
        }
    }

    /// Copy a historic statement into a new editor. Never execute it from a history click.
    pub(crate) fn open_history_statement(&mut self, sql: &str) {
        self.active = Section::Workspace;
        if self.open_query_tab("History").is_some() {
            self.sql = crate::sql_editor::SqlEditor::from_doc(sql.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_or_switching_the_visible_history_retires_its_pending_reply() {
        let mut state = AppState::default();
        state.active = Section::Workspace;
        state.open_query_tab("History scope").unwrap();
        state.toggle_query_sidebar(QuerySidebarPanel::History);
        assert!(state.query_sidebar_shows(QuerySidebarPanel::History));
        state.query_history.scope = HistoryScope::All;
        state.query_history.search = TextEdit::new("known statement".into());

        // Simulate the already-started read's reply arriving after the actual
        // close action; the current query/editor does not change.
        state.query_history.pending = true;
        let closed = state.query_history.generation;
        state.toggle_query_sidebar(QuerySidebarPanel::History);
        assert!(!state.query_sidebar_open);
        state.query_history.deliver(closed, Err("late close".into()));
        assert!(state.query_history.result.is_none());

        state.toggle_query_sidebar(QuerySidebarPanel::History);
        assert_ne!(state.query_history.generation, closed);
        assert_eq!(state.query_history.scope, HistoryScope::All);
        assert_eq!(state.query_history.search.text, "known statement");
        state.query_history.pending = true;
        let switched = state.query_history.generation;
        state.toggle_query_sidebar(QuerySidebarPanel::Snippets);
        assert!(state.query_sidebar_shows(QuerySidebarPanel::Snippets));
        state.query_history.deliver(switched, Err("late switch".into()));
        assert!(state.query_history.result.is_none());
    }
}

//! Connection-scoped history for the live query sidebar.
use crate::state_def::AppState;
use crate::types::{QuerySidebarPanel, Section};
use datazen_ui_api::{AdapterJob, QueryHistorySnapshot};

#[derive(Default)]
pub(crate) struct QueryHistoryState {
    generation: u32,
    pub pending: bool,
    pub result: Option<Result<QueryHistorySnapshot, String>>,
}

impl QueryHistoryState {
    pub fn reset(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.pending = false;
        self.result = None;
    }

    pub fn deliver(&mut self, generation: u32, result: Result<QueryHistorySnapshot, String>) {
        if self.pending && generation == self.generation {
            self.pending = false;
            self.result = Some(result);
        }
    }
}

impl AppState {
    pub(crate) fn select_query_sidebar(&mut self, panel: QuerySidebarPanel) {
        self.query_sidebar_panel = panel;
        self.refresh_query_history_if_open();
    }

    pub(crate) fn refresh_query_history_if_open(&mut self) {
        if self.query_sidebar_panel != QuerySidebarPanel::History {
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

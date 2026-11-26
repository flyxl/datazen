//! Bind close confirmation and precise host cancellation to one running query.
use crate::{state_def::AppState, views::RunningExecutionCloseRequest};

pub(crate) struct NativeQuery {
    pub generation: u32,
    tab: u32,
    sql: String,
    stop_requested: bool,
    close_after_stop: bool,
}

impl NativeQuery {
    pub fn new(generation: u32, tab: u32, sql: String) -> Self {
        Self {
            generation,
            tab,
            sql,
            stop_requested: false,
            close_after_stop: false,
        }
    }
}

impl AppState {
    pub(crate) fn native_query_selected(&self) -> bool {
        self.native_query
            .as_ref()
            .is_some_and(|query| self.tabs.selected().is_some_and(|tab| tab.id == query.tab))
    }

    pub(crate) fn maybe_confirm_running_tab_close(&mut self, id: u32) -> bool {
        let Some(query) = self.native_query.as_ref().filter(|q| q.tab == id) else {
            return false;
        };
        if self.running_close_request.is_none() && !query.close_after_stop {
            let title = self
                .tabs
                .tabs()
                .iter()
                .find(|tab| tab.id == id)
                .map(|tab| tab.title.clone())
                .unwrap_or_default();
            self.ask_running_execution_close(RunningExecutionCloseRequest {
                panel_id: query.tab.to_string(),
                title,
                pane_id: query.generation.to_string(),
                sql: query.sql.clone(),
                running_panes: 1,
            });
        }
        true
    }

    pub(crate) fn stop_confirmed_tab(&mut self, request: &RunningExecutionCloseRequest) {
        let Some(query) = self.native_query.as_ref() else {
            return;
        };
        if request.panel_id == query.tab.to_string()
            && request.pane_id == query.generation.to_string()
        {
            self.request_running_query_stop(query.generation, true);
        }
    }

    pub(crate) fn request_running_query_stop(&mut self, generation: u32, close_after: bool) {
        let Some(query) = self
            .native_query
            .as_mut()
            .filter(|q| q.generation == generation)
        else {
            return;
        };
        if query.stop_requested || self.desktop.request_native_query_stop(generation) {
            query.stop_requested = true;
            query.close_after_stop |= close_after;
        } else {
            self.query_action_notice("Unable to request a stop for this query.");
        }
    }

    pub(crate) fn note_query_stop_rejected(&mut self, generation: u32) {
        if let Some(query) = self
            .native_query
            .as_mut()
            .filter(|q| q.generation == generation)
        {
            query.stop_requested = false;
            query.close_after_stop = false;
            self.query_action_notice("The database did not accept the query stop.");
        }
    }

    pub(crate) fn finish_running_query(&mut self, generation: u32, terminal: bool) {
        if self
            .native_query
            .as_ref()
            .is_none_or(|q| q.generation != generation)
        {
            return;
        }
        let Some(query) = self.native_query.take() else {
            return;
        };
        if self
            .running_close_request
            .as_ref()
            .is_some_and(|request| request.pane_id == generation.to_string())
        {
            self.running_close_request = None;
        }
        if query.close_after_stop {
            if terminal {
                self.next_query_generation();
                self.query_loading = None;
                self.close_tab_now(query.tab);
            } else {
                self.query_action_notice(
                    "Unable to confirm the query stopped. The tab is still open.",
                );
            }
        }
    }
}

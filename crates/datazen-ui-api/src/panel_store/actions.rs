use super::focus::pane_key;
use super::host::{PanelStoreHost, RunQueryRequest};
use super::machine::PanelStore;
use super::result_edit::{apply_result_edits, mark_error};
use super::types::{BindParams, ResultEdit, StorePanelType};
use crate::{
    panel_target_database, panel_target_schema, QueryCancelState, QueryPanelContext,
    SchemaStateForPanel,
};

fn query_target(panel: &super::types::StorePanel, sql: &str) -> (Option<String>, Option<String>) {
    let context = QueryPanelContext {
        db_session_id: panel.connection_id.clone(),
        database_type: String::new(),
        database: panel.database.clone(),
        schema: panel.schema.clone(),
        namespace_path: Vec::new(),
    };
    let schema_state = SchemaStateForPanel::default();
    (
        panel_target_database(&context, &schema_state, false, Some(sql)),
        panel_target_schema(&context, &schema_state),
    )
}

impl PanelStore {
    pub fn update_sql(&mut self, panel_id: &str, sql: String, pane_id: Option<&str>) {
        let key = pane_key(panel_id, pane_id);
        if let Some(exec) = self.state.query_exec.get_mut(&key) {
            exec.sql = sql;
            exec.error = None;
        }
    }

    fn dispatch_query(
        &mut self,
        host: &dyn PanelStoreHost,
        panel_id: &str,
        sql: String,
        params: Option<BindParams>,
        pane_id: Option<&str>,
    ) -> bool {
        let panel = match self.state.panels.iter().find(|entry| entry.id == panel_id) {
            Some(panel) => panel.clone(),
            None => return false,
        };
        if panel.panel_type != StorePanelType::Query {
            return false;
        }
        let key = pane_key(panel_id, pane_id);
        let exec = match self.state.query_exec.get(&key) {
            Some(exec) => exec,
            None => return false,
        };
        let trimmed = exec.sql.trim().to_owned();
        if trimmed.is_empty() {
            return false;
        }
        let (database, schema) = query_target(&panel, &trimmed);
        let request = RunQueryRequest {
            key,
            db_session_id: panel.connection_id.clone(),
            sql: trimmed,
            params: params.filter(|values| !values.is_empty()),
            database,
            schema,
            connection_id: panel.connection_id.clone(),
        };
        host.run_query(&request);
        let connection_id = panel.connection_id.clone();
        self.load_history(host, Some(&connection_id));
        true
    }

    pub fn execute_query(
        &mut self,
        host: &dyn PanelStoreHost,
        panel_id: &str,
        params: Option<BindParams>,
        pane_id: Option<&str>,
    ) -> bool {
        let sql = self
            .state
            .query_exec
            .get(&pane_key(panel_id, pane_id))
            .map(|exec| exec.sql.clone())
            .unwrap_or_default();
        self.dispatch_query(host, panel_id, sql, params, pane_id)
    }

    pub fn execute_selection(
        &mut self,
        host: &dyn PanelStoreHost,
        panel_id: &str,
        sql: String,
        params: Option<BindParams>,
        pane_id: Option<&str>,
    ) -> bool {
        self.dispatch_query(host, panel_id, sql, params, pane_id)
    }

    pub fn cancel_query(
        &mut self,
        host: &dyn PanelStoreHost,
        panel_id: &str,
        pane_id: Option<&str>,
    ) -> bool {
        let key = pane_key(panel_id, pane_id);
        let admissible = self
            .state
            .query_exec
            .get(&key)
            .map(|exec| {
                exec.running
                    && exec.cancel_state != QueryCancelState::Requested
                    && exec.execution_id.is_some()
            })
            .unwrap_or(false);
        if !admissible || !host.can_cancel(panel_id) {
            return false;
        }
        if let Some(exec) = self.state.query_exec.get_mut(&key) {
            exec.cancel_state = QueryCancelState::Requested;
        }
        if !host.request_cancel(panel_id, super::focus::pane_id_of_pane_key(&key)) {
            if let Some(exec) = self.state.query_exec.get_mut(&key) {
                exec.cancel_state = QueryCancelState::Failed;
                exec.error = Some(host.cancel_failed_message());
            }
            return false;
        }
        true
    }

    pub fn set_active_result(&mut self, panel_id: &str, idx: usize, pane_id: Option<&str>) {
        if let Some(exec) = self.state.query_exec.get_mut(&pane_key(panel_id, pane_id)) {
            exec.active_result_idx = idx;
        }
    }

    pub fn toggle_pin_result(&mut self, panel_id: &str, idx: usize, pane_id: Option<&str>) -> bool {
        let key = pane_key(panel_id, pane_id);
        match self.state.query_exec.get_mut(&key) {
            Some(exec) => match exec.results.get_mut(idx) {
                Some(result) => {
                    result.pinned = !result.pinned;
                    true
                }
                None => false,
            },
            None => false,
        }
    }

    pub fn set_result_detail_row(
        &mut self,
        panel_id: &str,
        index: Option<u64>,
        pane_id: Option<&str>,
    ) {
        if let Some(exec) = self.state.query_exec.get_mut(&pane_key(panel_id, pane_id)) {
            exec.result_detail_row_index = index;
        }
    }

    pub fn update_result_cells(
        &mut self,
        host: &dyn PanelStoreHost,
        panel_id: &str,
        result_idx: usize,
        edits: Vec<ResultEdit>,
        pane_id: Option<&str>,
    ) -> Result<(), String> {
        let key = pane_key(panel_id, pane_id);
        match apply_result_edits(
            &mut self.state.query_exec,
            host,
            &key,
            result_idx,
            &edits,
            &mut self.in_flight,
        ) {
            Ok(()) => Ok(()),
            Err(message) => {
                mark_error(&mut self.state.query_exec, &key, message.clone());
                Err(message)
            }
        }
    }

    pub fn update_result_cell(
        &mut self,
        host: &dyn PanelStoreHost,
        panel_id: &str,
        result_idx: usize,
        row: u64,
        col: &str,
        value: crate::Value,
        pane_id: Option<&str>,
    ) -> Result<(), String> {
        self.update_result_cells(
            host,
            panel_id,
            result_idx,
            vec![ResultEdit {
                row,
                col: col.to_owned(),
                value,
            }],
            pane_id,
        )
    }

    pub fn set_chart_config(
        &mut self,
        panel_id: &str,
        config: crate::ChartConfig,
        pane_id: Option<&str>,
    ) {
        if let Some(exec) = self.state.query_exec.get_mut(&pane_key(panel_id, pane_id)) {
            exec.chart_config = Some(config);
        }
    }

    pub fn set_result_view_mode(&mut self, panel_id: &str, mode: &str, pane_id: Option<&str>) {
        if let Some(exec) = self.state.query_exec.get_mut(&pane_key(panel_id, pane_id)) {
            exec.result_view_mode = Some(mode.to_owned());
        }
    }

    pub fn load_history(&mut self, host: &dyn PanelStoreHost, connection_id: Option<&str>) {
        self.state.query_history = host.load_history(connection_id);
    }

    pub fn open_query_history(&mut self, host: &dyn PanelStoreHost, connection_id: Option<&str>) {
        self.state.history_visible = true;
        self.state.favorites_visible = false;
        self.load_history(host, connection_id);
    }

    pub fn set_pending_query_history(&mut self, connection_id: Option<String>) {
        self.state.pending_query_history_connection_id = connection_id;
    }

    pub fn set_pending_history_query(&mut self, query: Option<super::types::PendingHistoryQuery>) {
        self.state.pending_history_query = query;
    }

    pub fn toggle_history(&mut self) {
        self.state.history_visible = !self.state.history_visible;
    }

    pub fn load_favorites(&mut self, host: &dyn PanelStoreHost, connection_id: Option<&str>) {
        self.state.query_favorites = host.load_favorites(connection_id);
    }

    pub fn refresh_favorites(&mut self, host: &dyn PanelStoreHost, connection_id: Option<&str>) {
        let (favorites, root) = host.refresh_favorites(connection_id);
        self.state.query_favorites = favorites;
        self.state.favorites_root = root;
    }

    pub fn add_favorite(
        &mut self,
        host: &dyn PanelStoreHost,
        title: &str,
        sql: &str,
        connection_id: &str,
    ) {
        host.add_favorite(connection_id, title, sql);
        self.load_favorites(host, Some(connection_id));
    }

    pub fn delete_favorite(&mut self, host: &dyn PanelStoreHost, id: &str) {
        host.delete_favorite(id);
        let connection_id = self
            .state
            .panels
            .iter()
            .find(|panel| Some(&panel.id) == self.state.active_panel_id.as_ref())
            .map(|panel| panel.connection_id.clone());
        self.load_favorites(host, connection_id.as_deref());
    }

    pub fn toggle_favorites(&mut self) {
        self.state.favorites_visible = !self.state.favorites_visible;
    }
}

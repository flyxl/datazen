use crate::{FavoriteQuery, QueryHistoryEntry, Value};

use super::types::BindParams;

#[derive(Clone, Debug, PartialEq)]
pub struct RunQueryRequest {
    pub key: String,
    pub db_session_id: String,
    pub sql: String,
    pub params: Option<BindParams>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub connection_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommitCellRequest {
    pub binding: String,
    pub statement_index: u32,
    pub row_index: u64,
    pub column: String,
    pub value: Value,
}

pub trait PanelStoreHost {
    fn confirm_close(&self, panel_id: &str, pane_ids: &[String]) -> bool;

    fn has_session(&self, key: &str) -> bool;

    fn session_keys_of_panel(&self, panel_id: &str) -> Vec<String>;

    fn remove_session(&self, key: &str) -> Result<(), String>;

    fn mark_blocked(&self, key: &str);

    fn can_cancel(&self, panel_id: &str) -> bool;

    fn request_cancel(&self, panel_id: &str, pane_id: &str) -> bool;

    fn close_blocked_message(&self) -> String;

    fn cancel_failed_message(&self) -> String;

    fn run_query(&self, request: &RunQueryRequest);

    fn load_history(&self, connection_id: Option<&str>) -> Vec<QueryHistoryEntry>;

    fn load_favorites(&self, connection_id: Option<&str>) -> Vec<FavoriteQuery>;

    fn refresh_favorites(
        &self,
        connection_id: Option<&str>,
    ) -> (Vec<FavoriteQuery>, Option<String>);

    fn add_favorite(&self, connection_id: &str, title: &str, sql: &str);

    fn delete_favorite(&self, id: &str);

    fn commit_cell(&self, request: &CommitCellRequest) -> Result<(), String>;
}

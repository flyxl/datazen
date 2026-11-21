use serde_json::Value;

use super::types::{ConfirmKind, ConnectionSummary, ExecuteParams, WorkflowStoreView};
use crate::{HistoryListItem, WorkflowRunRecord};

pub trait WorkflowPanelHost {
    fn load_workflows(&mut self) -> WorkflowStoreView;

    fn workflow_get_dir(&self) -> String;

    fn get_connections(&self) -> Vec<ConnectionSummary>;

    fn workflow_history_list(&self) -> Vec<HistoryListItem>;

    fn clear_workflow_result(&mut self);

    fn execute_workflow(&mut self, params: &ExecuteParams);

    fn workflow_get(&self, id: &str) -> Result<Value, String>;

    fn workflow_delete(&self, id: &str) -> Result<(), String>;

    fn workflow_save(&self, definition: &Value) -> Result<(), String>;

    fn workflow_reload(&self) -> Result<(), String>;

    fn workflow_history_get(&self, id: &str) -> Result<WorkflowRunRecord, String>;

    fn workflow_history_clear(&self) -> Result<(), String>;

    fn confirm(&self, kind: ConfirmKind, title: &str, message: &str) -> bool;

    fn open_docs(&self, topic: &str);
}

use std::collections::BTreeMap;

use serde_json::Value;

use crate::{HistoryListItem, WorkflowExecutionResult, WorkflowListItem};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelTab {
    Workflows,
    History,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmKind {
    DeleteWorkflow,
    ClearHistory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackTone {
    Error,
    Info,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionSummary {
    pub id: String,
    pub name: String,
    pub database_type: String,
    pub database: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExecuteParams {
    pub workflow_id: String,
    pub variables: BTreeMap<String, String>,
    pub connection_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowStoreView {
    pub workflows: Vec<WorkflowListItem>,
    pub loading: bool,
    pub executing: bool,
    pub error: Option<String>,
    pub result: Option<WorkflowExecutionResult>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowPanelOptions {
    pub db_session_id: Option<String>,
    pub locales_ready: bool,
    pub feedback_delay_ms: u64,
}

impl Default for WorkflowPanelOptions {
    fn default() -> Self {
        Self {
            db_session_id: None,
            locales_ready: true,
            feedback_delay_ms: 2_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowPanelLabels {
    pub title: String,
    pub history_title: String,
    pub create: String,
    pub reload: String,
    pub docs_help: String,
    pub storage_dir: String,
    pub storage_dir_hint: String,
    pub empty: String,
    pub edit: String,
    pub delete: String,
    pub delete_confirm: String,
    pub saved: String,
    pub loading: String,
    pub running: String,
    pub run: String,
    pub select_connection: String,
    pub var_type_connection: String,
    pub history_clear: String,
    pub history_clear_confirm: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowPanelState {
    pub workflows: Vec<WorkflowListItem>,
    pub workflows_loading: bool,
    pub is_executing: bool,
    pub workflow_error: Option<String>,
    pub result: Option<WorkflowExecutionResult>,
    pub selected_workflow: Option<WorkflowListItem>,
    pub variables: BTreeMap<String, String>,
    pub workflows_dir: String,
    pub show_form: bool,
    pub editing_id: Option<String>,
    pub draft: Value,
    pub feedback: String,
    pub tab: PanelTab,
    pub history_items: Vec<HistoryListItem>,
    pub history_detail: Option<WorkflowExecutionResult>,
    pub saved_connections: Vec<ConnectionSummary>,
}

impl Default for WorkflowPanelState {
    fn default() -> Self {
        Self {
            workflows: Vec::new(),
            workflows_loading: false,
            is_executing: false,
            workflow_error: None,
            result: None,
            selected_workflow: None,
            variables: BTreeMap::new(),
            workflows_dir: String::new(),
            show_form: false,
            editing_id: None,
            draft: Value::Null,
            feedback: String::new(),
            tab: PanelTab::Workflows,
            history_items: Vec::new(),
            history_detail: None,
            saved_connections: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowSelectOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VariableInput {
    Connection {
        options: Vec<WorkflowSelectOption>,
        disabled: bool,
    },
    Text {
        placeholder: String,
        disabled: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorkflowPanelEffect {
    ScheduleFeedbackClear { delay_ms: u64 },
    DocsOpened { topic: String },
}

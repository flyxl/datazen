use serde_json::Value;

use crate::SendWorkflowChatMessageParams;

pub const COPIED_FEEDBACK_MS: u64 = 1500;
pub const SAVE_OK_RESET_MS: u64 = 3000;
pub const WORKFLOW_CHAT_PANEL_GAP: [&str; 5] = [
    "useAiStore workflowChat subscription",
    "useActiveConnectionStore snapshot",
    "openSettingsWindow",
    "AiInput / QuestionBlock rendering",
    "setTimeout save-reset",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowPanelConnection {
    pub id: String,
    pub name: String,
    pub database_type: String,
    pub database: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowValidationConnection {
    pub id: String,
    pub requires_explicit_database: bool,
}

pub type ParsedWorkflowDefinition = Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkflowSaveError {
    MissingField(String),
    Invalid(String),
    Command(String),
}

impl WorkflowSaveError {
    pub fn message(&self) -> &str {
        match self {
            WorkflowSaveError::MissingField(field) => field,
            WorkflowSaveError::Invalid(message) | WorkflowSaveError::Command(message) => message,
        }
    }
}

pub type ActiveSessionSnapshot = std::collections::HashMap<String, Option<String>>;

pub struct WorkflowChatPanelPorts {
    pub yaml_blocks: Box<dyn Fn(&str) -> Vec<String>>,
    pub parse_and_validate:
        Box<dyn Fn(&str, &[WorkflowValidationConnection]) -> Result<ParsedWorkflowDefinition, WorkflowSaveError>>,
    pub save_workflow: Box<dyn Fn(&ParsedWorkflowDefinition) -> Result<(), String>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkflowChatState {
    pub messages: Vec<crate::AiChatMessage>,
    pub is_streaming: bool,
    pub stream_content: String,
    pub stream_reasoning: String,
    pub request_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SendRequest {
    pub should_send: bool,
    pub params: SendWorkflowChatMessageParams,
}

use crate::{AiChatMessage, AiMessageRole, SendWorkflowChatMessageParams};

use super::types::{
    WorkflowChatPanelPorts, WorkflowChatState, ParsedWorkflowDefinition, WorkflowSaveError,
    WorkflowValidationConnection, SAVE_OK_RESET_MS,
};

pub fn handle_stop(state: &mut WorkflowChatState) {
    let collected = state.stream_content.clone();
    let reasoning = state.stream_reasoning.clone();
    if !collected.is_empty() || !reasoning.is_empty() {
        let mut message = AiChatMessage::new(AiMessageRole::Assistant, collected);
        message.reasoning = if reasoning.is_empty() { None } else { Some(reasoning) };
        state.messages.push(message);
    }
    state.is_streaming = false;
    state.stream_content.clear();
    state.stream_reasoning.clear();
    state.request_id = None;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NewChatReset {
    pub clear_input: bool,
    pub clear_save_ok: bool,
    pub clear_save_error: bool,
}

pub fn handle_new_chat_reset() -> NewChatReset {
    NewChatReset { clear_input: true, clear_save_ok: true, clear_save_error: true }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveSuccess {
    pub reset_after_ms: u64,
}

pub fn save_yaml(
    yaml: &str,
    validation_connections: &[WorkflowValidationConnection],
    ports: &WorkflowChatPanelPorts,
) -> Result<(ParsedWorkflowDefinition, SaveSuccess), WorkflowSaveError> {
    let parsed = (ports.parse_and_validate)(yaml, validation_connections)?;
    (ports.save_workflow)(&parsed).map_err(WorkflowSaveError::Command)?;
    Ok((parsed, SaveSuccess { reset_after_ms: SAVE_OK_RESET_MS }))
}

pub fn answer_questions_params(
    answers: &str,
    db_session_id: Option<String>,
) -> SendWorkflowChatMessageParams {
    let include_schema = db_session_id.is_some();
    SendWorkflowChatMessageParams {
        db_session_id,
        database: None,
        content: answers.to_owned(),
        include_schema: Some(include_schema),
        context_files: None,
        context_tables: None,
    }
}

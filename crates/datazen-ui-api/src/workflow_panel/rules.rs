use std::collections::BTreeMap;

use serde_json::Value;

use super::types::{ConnectionSummary, FeedbackTone, VariableInput, WorkflowSelectOption};
use crate::WorkflowListItem;

fn display_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(boolean) => boolean.to_string(),
        other => other.to_string(),
    }
}

pub fn variable_defaults(workflow: &WorkflowListItem) -> BTreeMap<String, String> {
    workflow
        .variables
        .iter()
        .map(|variable| {
            let value = variable
                .default
                .as_ref()
                .map(display_string)
                .unwrap_or_default();
            (variable.name.clone(), value)
        })
        .collect()
}

pub fn feedback_tone(feedback: &str) -> FeedbackTone {
    if feedback.starts_with("Error") || feedback.starts_with("error") {
        FeedbackTone::Error
    } else {
        FeedbackTone::Info
    }
}

pub fn is_loading(locales_ready: bool, workflows_loading: bool) -> bool {
    !locales_ready || workflows_loading
}

pub fn is_connection_variable(variable_type: &str) -> bool {
    variable_type == "connection"
}

pub fn connection_options(
    saved_connections: &[ConnectionSummary],
    select_label: &str,
) -> Vec<WorkflowSelectOption> {
    let mut options = vec![WorkflowSelectOption {
        value: String::new(),
        label: select_label.to_owned(),
    }];
    options.extend(
        saved_connections
            .iter()
            .map(|connection| WorkflowSelectOption {
                value: connection.id.clone(),
                label: connection.name.clone(),
            }),
    );
    options
}

pub fn variable_input(
    variable_type: &str,
    saved_connections: &[ConnectionSummary],
    select_label: &str,
    disabled: bool,
) -> VariableInput {
    if is_connection_variable(variable_type) {
        VariableInput::Connection {
            options: connection_options(saved_connections, select_label),
            disabled,
        }
    } else {
        VariableInput::Text {
            placeholder: String::new(),
            disabled,
        }
    }
}

pub fn error_text(message: &str) -> String {
    format!("Error: {message}")
}

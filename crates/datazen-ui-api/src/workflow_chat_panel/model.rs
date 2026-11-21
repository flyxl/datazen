use crate::{split_context_items, ContextItem, SendWorkflowChatMessageParams};

use super::types::{
    ActiveSessionSnapshot, SendRequest, WorkflowPanelConnection, WorkflowValidationConnection,
};

pub type MultiDbProvider = dyn Fn(&WorkflowPanelConnection) -> bool;

pub fn selected_db_session_id(
    selected_connection: Option<&str>,
    active: &ActiveSessionSnapshot,
) -> Option<String> {
    let connection = selected_connection?;
    if connection.is_empty() {
        return None;
    }
    match active.get(connection) {
        Some(Some(session)) if !session.is_empty() => Some(session.clone()),
        _ => None,
    }
}

pub fn connection_options(connections: &[WorkflowPanelConnection], no_connection_label: &str) -> Vec<(String, String)> {
    let mut options = vec![(String::new(), no_connection_label.to_owned())];
    options.extend(connections.iter().map(|connection| {
        (
            connection.id.clone(),
            format!("{} ({})", connection.name, connection.database_type),
        )
    }));
    options
}

pub fn validation_connections(
    connections: &[WorkflowPanelConnection],
    provider: &MultiDbProvider,
) -> Vec<WorkflowValidationConnection> {
    connections
        .iter()
        .map(|connection| WorkflowValidationConnection {
            id: connection.id.clone(),
            requires_explicit_database: provider(connection),
        })
        .collect()
}

pub fn selected_database(
    connections: &[WorkflowPanelConnection],
    selected_connection: Option<&str>,
) -> Option<String> {
    let connection = selected_connection?;
    if connection.is_empty() {
        return None;
    }
    connections
        .iter()
        .find(|item| item.id == connection)
        .and_then(|item| item.database.clone())
}

pub fn build_send_request(
    input: &str,
    is_streaming: bool,
    db_session_id: Option<String>,
    context_items: &[ContextItem],
) -> SendRequest {
    let content = input.trim();
    if content.is_empty() || is_streaming {
        return SendRequest::default();
    }
    let split = split_context_items(context_items);
    let include_schema = db_session_id.is_some();
    SendRequest {
        should_send: true,
        params: SendWorkflowChatMessageParams {
            db_session_id,
            database: None,
            content: content.to_owned(),
            include_schema: Some(include_schema),
            context_files: if split.context_files.is_empty() {
                None
            } else {
                Some(split.context_files)
            },
            context_tables: if split.context_tables.is_empty() {
                None
            } else {
                Some(split.context_tables)
            },
        },
    }
}

pub fn is_configured_gate(is_configured: bool) -> bool {
    is_configured
}

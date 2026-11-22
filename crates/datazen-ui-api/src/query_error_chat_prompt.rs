#[cfg(test)]
mod tests;

use crate::{AiChatDraftRequest, AiChatDraftSource, ContentViewCallbacks};

pub const MAX_PROMPT_SQL_LENGTH: usize = 4_000;
pub const MAX_PROMPT_ERROR_LENGTH: usize = 4_000;

pub const QUERY_ERROR_CHAT_PROMPT_SURFACE: [&str; 3] = [
    "buildQueryErrorChatPrompt",
    "sendQueryErrorChatDraft",
    "QUERY_ERROR_CHAT_PROMPT_GAP",
];

pub const QUERY_ERROR_CHAT_PROMPT_GAP: [&str; 3] = [
    "buildQueryDiagnosisContext redaction and sanitisation",
    "crypto.randomUUID request ids",
    "React callback dispatch through ContentView",
];

#[derive(Clone, Debug, PartialEq)]
pub struct QueryErrorChatContext {
    pub database_type: String,
    pub database: String,
    pub schema: Option<String>,
    pub safe_sql: String,
    pub safe_error_message: String,
    pub context_fingerprint: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueryErrorDraftInput {
    pub panel_id: String,
    pub connection_id: String,
    pub db_session_id: String,
    pub database: Option<String>,
    pub schema: Option<String>,
}

fn js_slice(text: &str, limit: usize) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    if units.len() <= limit {
        return text.to_owned();
    }
    String::from_utf16_lossy(&units[..limit])
}

pub fn build_query_error_chat_prompt(context: &QueryErrorChatContext) -> String {
    let mut lines = vec![
        "I need help understanding and fixing a database query error.".to_owned(),
        String::new(),
        format!("Database type: {}", context.database_type),
        format!("Database: {}", context.database),
    ];
    if let Some(schema) = &context.schema {
        lines.push(format!("Schema: {schema}"));
    }
    lines.extend([
        String::new(),
        "SQL:".to_owned(),
        "```sql".to_owned(),
        js_slice(&context.safe_sql, MAX_PROMPT_SQL_LENGTH),
        "```".to_owned(),
        String::new(),
        "Error:".to_owned(),
        "```".to_owned(),
        js_slice(&context.safe_error_message, MAX_PROMPT_ERROR_LENGTH),
        "```".to_owned(),
    ]);
    lines.join("\n")
}

#[allow(clippy::too_many_arguments)]
pub fn send_query_error_chat_draft(
    input: &QueryErrorDraftInput,
    context: Option<&QueryErrorChatContext>,
    request_id: String,
    callbacks: Option<&ContentViewCallbacks<'_>>,
) -> Option<AiChatDraftRequest> {
    let context = context?;
    let content = build_query_error_chat_prompt(context);
    let request = AiChatDraftRequest::new(
        request_id,
        AiChatDraftSource::QueryError,
        input.panel_id.clone(),
        input.connection_id.clone(),
        input.db_session_id.clone(),
        input.database.clone(),
        input.schema.clone(),
        context.context_fingerprint.clone(),
        content,
    );
    if let Some(callbacks) = callbacks {
        (callbacks.open_ai_chat_draft)(&request);
    }
    Some(request)
}

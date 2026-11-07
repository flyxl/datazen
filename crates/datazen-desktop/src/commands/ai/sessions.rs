//! T15 AI chat session IPC: list/get/upsert/delete/export.
//!
//! Every read and write is scoped by the host organization + principal; the
//! frontend's `session_key` is an additional filter, never an authorization
//! boundary. Upsert replaces messages by default and can append via `append`.

use crate::commands::error::CommandError;
use crate::commands::AppState;
use crate::store::ai_sessions::{AiSessionMessageRecord, AiSessionRecord, AiSessionSummary};
use crate::store::AppDbError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

fn scope(state: &AppState) -> Result<(String, String), CommandError> {
    let organization_id = state
        .organization_scope()?
        .organization_id()
        .as_str()
        .to_string();
    let principal_id = crate::platform::local_principal()
        .map(|principal| principal.as_str().to_string())
        .unwrap_or_else(|| "datazen-local-user".to_string());
    Ok((organization_id, principal_id))
}

fn now() -> DateTime<Utc> {
    Utc::now()
}

fn map_storage_error(error: AppDbError) -> CommandError {
    match error {
        AppDbError::Validation(message) => CommandError::Validation(message),
        AppDbError::NotFound(message) => CommandError::NotFound(message),
        other => CommandError::Internal(other.to_string()),
    }
}

fn parse_metadata(raw: Option<&str>) -> Option<Value> {
    raw.and_then(|value| serde_json::from_str(value).ok())
}

fn metadata_json(metadata: Option<&Value>) -> Result<Option<String>, CommandError> {
    metadata
        .map(|value| serde_json::to_string(value).map_err(CommandError::Json))
        .transpose()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSessionMessageDto {
    pub id: String,
    pub role: String,
    pub content: String,
    pub status: String,
    pub metadata: Option<Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSessionSummaryDto {
    pub id: String,
    pub session_key: String,
    pub connection_id: Option<String>,
    pub database: Option<String>,
    pub title: String,
    pub metadata: Option<Value>,
    pub message_count: i64,
    pub preview: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_access_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSessionDto {
    pub id: String,
    pub session_key: String,
    pub connection_id: Option<String>,
    pub database: Option<String>,
    pub title: String,
    pub metadata: Option<Value>,
    /// Summary-compatible fields so list and get share one frontend DTO.
    pub message_count: i64,
    pub preview: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_access_at: String,
    pub messages: Vec<AiSessionMessageDto>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSessionMessageInput {
    pub id: Option<String>,
    pub role: String,
    pub content: String,
    pub status: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSessionUpsertInput {
    pub id: Option<String>,
    pub session_key: String,
    #[serde(alias = "connection")]
    pub connection_id: Option<String>,
    #[serde(alias = "databaseName")]
    pub database: Option<String>,
    pub title: Option<String>,
    pub metadata: Option<Value>,
    pub last_access_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub messages: Vec<AiSessionMessageInput>,
    /// Explicit write mode: replace (default) or append. Re-running a replace
    /// with the same id is idempotent; append de-duplicates message ids.
    #[serde(default)]
    pub append: bool,
}

fn message_dto(message: AiSessionMessageRecord) -> AiSessionMessageDto {
    AiSessionMessageDto {
        id: message.id,
        role: message.role,
        content: message.content,
        status: message.status,
        metadata: parse_metadata(message.metadata_json.as_deref()),
        created_at: message.created_at,
    }
}

fn summary_dto(summary: AiSessionSummary) -> AiSessionSummaryDto {
    AiSessionSummaryDto {
        id: summary.session.id,
        session_key: summary.session.session_key,
        connection_id: summary.session.connection_id,
        database: summary.session.database_name,
        title: summary.session.title,
        metadata: parse_metadata(summary.session.metadata_json.as_deref()),
        message_count: summary.message_count,
        preview: summary.preview,
        created_at: summary.session.created_at,
        updated_at: summary.session.updated_at,
        last_access_at: summary.session.last_access_at,
    }
}

fn session_dto(session: AiSessionRecord, messages: Vec<AiSessionMessageRecord>) -> AiSessionDto {
    let message_count = messages.len() as i64;
    let preview = messages
        .iter()
        .find(|message| message.role == "user")
        .map(|message| message.content.trim().chars().take(120).collect())
        .unwrap_or_default();
    AiSessionDto {
        id: session.id,
        session_key: session.session_key,
        connection_id: session.connection_id,
        database: session.database_name,
        title: session.title,
        metadata: parse_metadata(session.metadata_json.as_deref()),
        message_count,
        preview,
        created_at: session.created_at,
        updated_at: session.updated_at,
        last_access_at: session.last_access_at,
        messages: messages.into_iter().map(message_dto).collect(),
    }
}

fn normalize_scope(value: Option<String>) -> Option<String> {
    value.map(|value| value.trim().to_string())
}

fn derive_title(messages: &[AiSessionMessageInput], provided: Option<&str>) -> String {
    if let Some(title) = provided.map(str::trim).filter(|title| !title.is_empty()) {
        return title.chars().take(120).collect();
    }
    messages
        .iter()
        .find(|message| message.role == "user")
        .map(|message| message.content.trim().chars().take(80).collect())
        .unwrap_or_default()
}

#[tauri::command]
pub async fn ai_sessions_list(
    state: State<'_, AppState>,
    connection_id: Option<String>,
    database: Option<String>,
    session_key: Option<String>,
) -> Result<Vec<AiSessionSummaryDto>, CommandError> {
    let (organization_id, principal_id) = scope(&state)?;
    let rows = state
        .store
        .app_db()
        .list_ai_sessions(
            &organization_id,
            &principal_id,
            normalize_scope(connection_id).as_deref(),
            normalize_scope(database).as_deref(),
            normalize_scope(session_key).as_deref(),
        )
        .map_err(map_storage_error)?;
    Ok(rows.into_iter().map(summary_dto).collect())
}

#[tauri::command]
pub async fn ai_sessions_get(
    state: State<'_, AppState>,
    id: String,
) -> Result<AiSessionDto, CommandError> {
    let (organization_id, principal_id) = scope(&state)?;
    let db = state.store.app_db();
    let session = db
        .get_ai_session(&organization_id, &principal_id, &id)
        .map_err(map_storage_error)?
        .ok_or_else(|| CommandError::NotFound("AI session not found".into()))?;
    let messages = db
        .list_ai_session_messages(&organization_id, &principal_id, &id)
        .map_err(map_storage_error)?;
    Ok(session_dto(session, messages))
}

#[tauri::command]
pub async fn ai_sessions_upsert(
    state: State<'_, AppState>,
    input: AiSessionUpsertInput,
) -> Result<AiSessionDto, CommandError> {
    if input.session_key.trim().is_empty() {
        return Err(CommandError::Validation("sessionKey is required".into()));
    }
    let (organization_id, principal_id) = scope(&state)?;
    let session_id = input
        .id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let timestamp = now().to_rfc3339();
    let session = AiSessionRecord {
        id: session_id.clone(),
        organization_id,
        owner_principal_id: principal_id,
        session_key: input.session_key,
        connection_id: normalize_scope(input.connection_id),
        database_name: normalize_scope(input.database),
        title: derive_title(&input.messages, input.title.as_deref()),
        metadata_json: metadata_json(input.metadata.as_ref())?,
        created_at: timestamp.clone(),
        updated_at: timestamp.clone(),
        last_access_at: input
            .last_access_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_else(|| timestamp.clone()),
    };
    let mut messages = Vec::with_capacity(input.messages.len());
    for message in input.messages {
        if message.content.trim().is_empty() && message.role != "assistant" {
            return Err(CommandError::Validation(
                "AI message content is required".into(),
            ));
        }
        let status = message.status.unwrap_or_else(|| "complete".into());
        messages.push(AiSessionMessageRecord {
            id: message
                .id
                .filter(|id| !id.trim().is_empty())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            session_id: session_id.clone(),
            position: 0,
            role: message.role,
            content: message.content,
            status,
            metadata_json: metadata_json(message.metadata.as_ref())?,
            created_at: message
                .created_at
                .map(|value| value.to_rfc3339())
                .unwrap_or_else(|| timestamp.clone()),
        });
    }
    let db = state.store.app_db();
    let saved = db
        .upsert_ai_session(session, &messages, input.append)
        .map_err(map_storage_error)?;
    let all_messages = db
        .list_ai_session_messages(&saved.organization_id, &saved.owner_principal_id, &saved.id)
        .map_err(map_storage_error)?;
    Ok(session_dto(saved, all_messages))
}

#[tauri::command]
pub async fn ai_sessions_delete(
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, CommandError> {
    let (organization_id, principal_id) = scope(&state)?;
    state
        .store
        .app_db()
        .delete_ai_session(&organization_id, &principal_id, &id)
        .map_err(map_storage_error)
}

fn markdown_session(session: &AiSessionDto) -> String {
    let mut out = String::new();
    let title = if session.title.is_empty() {
        "AI conversation"
    } else {
        session.title.as_str()
    };
    out.push_str(&format!("# {title}\n\n"));
    out.push_str(&format!("- Session: `{}`\n", session.session_key));
    if let Some(connection_id) = &session.connection_id {
        out.push_str(&format!("- Connection: `{connection_id}`\n"));
    }
    if let Some(database) = &session.database {
        out.push_str(&format!("- Database: `{database}`\n"));
    }
    out.push_str(&format!("- Created: {}\n", session.created_at));
    out.push_str(&format!("- Updated: {}\n\n", session.updated_at));
    for message in &session.messages {
        let role = match message.role.as_str() {
            "user" => "User",
            "assistant" => "Assistant",
            "tool" => "Tool",
            "system" => "System",
            _ => "Message",
        };
        let suffix = if message.status == "complete" {
            String::new()
        } else {
            format!(" ({})", message.status)
        };
        out.push_str(&format!("## {role}{suffix}\n\n{}\n\n", message.content));
    }
    out
}

#[tauri::command]
pub async fn ai_sessions_export(
    state: State<'_, AppState>,
    id: String,
    format: String,
) -> Result<String, CommandError> {
    let (organization_id, principal_id) = scope(&state)?;
    let db = state.store.app_db();
    let session = db
        .get_ai_session(&organization_id, &principal_id, &id)
        .map_err(map_storage_error)?
        .ok_or_else(|| CommandError::NotFound("AI session not found".into()))?;
    let messages = db
        .list_ai_session_messages(&organization_id, &principal_id, &id)
        .map_err(map_storage_error)?;
    let dto = session_dto(session, messages);
    match format.trim().to_ascii_lowercase().as_str() {
        "json" => serde_json::to_string_pretty(&dto).map_err(CommandError::Json),
        "markdown" | "md" => Ok(markdown_session(&dto)),
        other => Err(CommandError::Validation(format!(
            "unsupported AI session export format: {other}"
        ))),
    }
}

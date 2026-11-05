//! AI memory IPC, the desktop `MemoryRepository`, and chat retrieval helpers.
//!
//! Every command takes organization and principal from the host scope gate,
//! never from the request body. `secret` memories are rejected before they
//! reach AppDb; the T09 store enforces the same rule a second time.

use crate::commands::error::CommandError;
use crate::commands::AppState;
use crate::store::ai_memory::AiMemoryRecord;
use crate::store::{AppDb, AppDbError};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use datazen::ai::domain::types::AiDataEgressLevel;
use datazen::ai::memory::{
    MemoryContext, MemoryEntry, MemoryError, MemoryInjection, MemoryInjectionPolicy,
    MemoryInjector, MemoryKind, MemoryMode, MemoryQuery, MemoryReference, MemoryRepository,
    MemoryRetrievalRequest, MemoryRetriever, MemoryScope, MemoryScopeKind, MemorySensitivity,
    MemorySource, MemoryStatus,
};
use datazen::ai::safety::provider_allows_unredacted_credentials;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;
use tauri::{Emitter, State, WebviewWindow};

const DEFAULT_PAGE_LIMIT: u32 = 50;
const MAX_PAGE_LIMIT: u32 = 200;

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

fn map_storage_error(error: AppDbError) -> CommandError {
    match error {
        AppDbError::Validation(message) => CommandError::Validation(message),
        other => CommandError::Internal(other.to_string()),
    }
}

fn now() -> DateTime<Utc> {
    Utc::now()
}

fn parse_time(value: &str, field: &str) -> Result<DateTime<Utc>, CommandError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| CommandError::Validation(format!("invalid memory {field}")))
}

fn content_hash(content: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(content.as_bytes()))
}

/// Convert arbitrary user text into a safe FTS5 query. Raw punctuation such
/// as `:` or `?` is FTS syntax and would otherwise make retrieval fail.
fn fts_query(text: &str) -> String {
    let terms: Vec<String> = text
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .filter(|term| !term.is_empty())
        .take(16)
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect();
    terms.join(" OR ")
}

// ── IPC DTOs ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemoryDto {
    pub id: String,
    pub scope_type: MemoryScopeKind,
    pub scope_id: String,
    pub connection_id: Option<String>,
    pub database_name: Option<String>,
    pub schema_name: Option<String>,
    pub table_name: Option<String>,
    pub column_name: Option<String>,
    pub kind: MemoryKind,
    pub subject: String,
    pub content: String,
    pub structured_json: Option<serde_json::Value>,
    pub source: MemorySource,
    pub source_ref: Option<String>,
    pub confidence: f64,
    pub sensitivity: MemorySensitivity,
    pub status: MemoryStatus,
    pub pinned: bool,
    pub use_count: u32,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemoryPage {
    pub entries: Vec<AiMemoryDto>,
    pub total: usize,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemoryStats {
    pub total: usize,
    pub by_status: BTreeMap<String, usize>,
    pub by_kind: BTreeMap<String, usize>,
    pub by_scope: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemoryCreateInput {
    pub id: Option<String>,
    pub scope_type: MemoryScopeKind,
    pub scope_id: Option<String>,
    pub connection_id: Option<String>,
    pub database_name: Option<String>,
    pub schema_name: Option<String>,
    pub table_name: Option<String>,
    pub column_name: Option<String>,
    pub kind: MemoryKind,
    pub subject: String,
    pub content: String,
    pub structured_json: Option<serde_json::Value>,
    pub source: Option<MemorySource>,
    pub source_ref: Option<String>,
    pub confidence: Option<f64>,
    pub sensitivity: Option<MemorySensitivity>,
    pub status: Option<MemoryStatus>,
    pub pinned: Option<bool>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemoryUpdateInput {
    pub subject: Option<String>,
    pub content: Option<String>,
    pub kind: Option<MemoryKind>,
    pub structured_json: Option<serde_json::Value>,
    pub source_ref: Option<String>,
    pub confidence: Option<f64>,
    pub sensitivity: Option<MemorySensitivity>,
    pub status: Option<MemoryStatus>,
    pub pinned: Option<bool>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MemoryFilter<'a> {
    scope_type: Option<MemoryScopeKind>,
    scope_id: Option<&'a str>,
    status: Option<MemoryStatus>,
    kind: Option<MemoryKind>,
    connection_id: Option<&'a str>,
    text: Option<&'a str>,
}

impl MemoryFilter<'_> {
    fn matches(&self, record: &AiMemoryRecord) -> bool {
        let Some(scope_type) = MemoryScopeKind::parse(&record.scope_type) else {
            return false;
        };
        if self
            .scope_type
            .is_some_and(|expected| expected != scope_type)
        {
            return false;
        }
        if self
            .scope_id
            .is_some_and(|expected| expected != record.scope_id)
        {
            return false;
        }
        if self
            .status
            .is_some_and(|expected| expected.as_str() != record.status)
        {
            return false;
        }
        if self
            .kind
            .is_some_and(|expected| expected.as_str() != record.kind)
        {
            return false;
        }
        if self
            .connection_id
            .is_some_and(|expected| record.connection_id.as_deref() != Some(expected))
        {
            return false;
        }
        if let Some(text) = self.text {
            let needle = text.trim().to_lowercase();
            if !needle.is_empty()
                && !record.subject.to_lowercase().contains(&needle)
                && !record.content.to_lowercase().contains(&needle)
            {
                return false;
            }
        }
        true
    }
}

fn record_to_dto(record: AiMemoryRecord) -> Result<AiMemoryDto, CommandError> {
    let scope_type = MemoryScopeKind::parse(&record.scope_type)
        .ok_or_else(|| CommandError::Internal("invalid persisted memory scope".into()))?;
    let kind = MemoryKind::parse(&record.kind)
        .ok_or_else(|| CommandError::Internal("invalid persisted memory kind".into()))?;
    let source = MemorySource::parse(&record.source)
        .ok_or_else(|| CommandError::Internal("invalid persisted memory source".into()))?;
    let sensitivity = MemorySensitivity::parse(&record.sensitivity)
        .ok_or_else(|| CommandError::Internal("invalid persisted memory sensitivity".into()))?;
    let status = MemoryStatus::parse(&record.status)
        .ok_or_else(|| CommandError::Internal("invalid persisted memory status".into()))?;
    let structured_json = record
        .structured_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|error| {
            CommandError::Internal(format!("invalid persisted memory JSON: {error}"))
        })?;
    Ok(AiMemoryDto {
        id: record.id,
        scope_type,
        scope_id: record.scope_id,
        connection_id: record.connection_id,
        database_name: record.database_name,
        schema_name: record.schema_name,
        table_name: record.table_name,
        column_name: record.column_name,
        kind,
        subject: record.subject,
        content: record.content,
        structured_json,
        source,
        source_ref: record.source_ref,
        confidence: record.confidence,
        sensitivity,
        status,
        pinned: record.pinned,
        use_count: record.use_count.max(0) as u32,
        last_used_at: record
            .last_used_at
            .as_deref()
            .map(|value| parse_time(value, "lastUsedAt"))
            .transpose()?,
        expires_at: record
            .expires_at
            .as_deref()
            .map(|value| parse_time(value, "expiresAt"))
            .transpose()?,
        created_at: parse_time(&record.created_at, "createdAt")?,
        updated_at: parse_time(&record.updated_at, "updatedAt")?,
    })
}

fn records_for_filter(
    state: &AppState,
    filter: MemoryFilter<'_>,
) -> Result<Vec<AiMemoryRecord>, CommandError> {
    let (organization_id, principal_id) = scope(state)?;
    let records = state
        .store
        .app_db()
        .list_ai_memories(&organization_id, &principal_id)
        .map_err(map_storage_error)?;
    Ok(records
        .into_iter()
        .filter(|record| filter.matches(record))
        .collect())
}

fn page(
    records: Vec<AiMemoryRecord>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<AiMemoryPage, CommandError> {
    let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT).clamp(1, MAX_PAGE_LIMIT);
    let offset = offset.unwrap_or(0);
    let total = records.len();
    let entries = records
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(record_to_dto)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(AiMemoryPage {
        entries,
        total,
        limit,
        offset,
    })
}

fn required_scope_component<'a>(
    value: Option<&'a str>,
    field: &str,
) -> Result<&'a str, CommandError> {
    value.ok_or_else(|| CommandError::Validation(format!("{field} is required for this scope")))
}

fn derive_scope_id(
    input: &AiMemoryCreateInput,
    organization_id: &str,
    principal_id: &str,
) -> Result<String, CommandError> {
    if let Some(scope_id) = input
        .scope_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(scope_id.to_string());
    }
    let connection = input
        .connection_id
        .as_deref()
        .filter(|v| !v.trim().is_empty());
    let database = input
        .database_name
        .as_deref()
        .filter(|v| !v.trim().is_empty());
    let schema = input
        .schema_name
        .as_deref()
        .filter(|v| !v.trim().is_empty());
    let table = input.table_name.as_deref().filter(|v| !v.trim().is_empty());
    let column = input
        .column_name
        .as_deref()
        .filter(|v| !v.trim().is_empty());
    let qualify = |parts: &[Option<&str>]| {
        parts
            .iter()
            .flatten()
            .copied()
            .filter(|value| !value.trim().is_empty())
            .collect::<Vec<_>>()
            .join("/")
    };
    let id = match input.scope_type {
        MemoryScopeKind::Organization => organization_id.to_string(),
        MemoryScopeKind::User => principal_id.to_string(),
        MemoryScopeKind::Connection => {
            required_scope_component(connection, "connectionId")?.to_string()
        }
        MemoryScopeKind::Database => qualify(&[
            connection,
            Some(required_scope_component(database, "databaseName")?),
        ]),
        MemoryScopeKind::Schema => qualify(&[
            connection,
            Some(required_scope_component(database, "databaseName")?),
            Some(required_scope_component(schema, "schemaName")?),
        ]),
        MemoryScopeKind::Table => qualify(&[
            connection,
            Some(required_scope_component(database, "databaseName")?),
            schema,
            Some(required_scope_component(table, "tableName")?),
        ]),
        MemoryScopeKind::Column => qualify(&[
            connection,
            database,
            schema,
            Some(required_scope_component(table, "tableName")?),
            Some(required_scope_component(column, "columnName")?),
        ]),
    };
    if id.is_empty() {
        return Err(CommandError::Validation(
            "a scope id or enough scope components are required".into(),
        ));
    }
    Ok(id)
}

fn validate_sensitivity(sensitivity: MemorySensitivity) -> Result<(), CommandError> {
    if sensitivity == MemorySensitivity::Secret {
        return Err(CommandError::Validation(
            "secret AI memories are rejected at the IPC boundary".into(),
        ));
    }
    Ok(())
}

fn validate_confidence(confidence: f64) -> Result<(), CommandError> {
    if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
        return Err(CommandError::Validation(format!(
            "confidence must be finite and between 0 and 1, got {confidence}"
        )));
    }
    Ok(())
}

fn append_audit(
    state: &AppState,
    memory_id: &str,
    action: &str,
    actor: &str,
    before_hash: Option<String>,
    after_hash: Option<String>,
) -> Result<(), CommandError> {
    state
        .store
        .app_db()
        .append_ai_memory_audit(&crate::store::ai_memory::AiMemoryAuditRecord {
            id: uuid::Uuid::new_v4().to_string(),
            memory_id: memory_id.to_string(),
            action: action.to_string(),
            actor_principal_id: actor.to_string(),
            before_hash,
            after_hash,
            detail_json: None,
            created_at: now().to_rfc3339(),
        })
        .map_err(map_storage_error)
}

fn append_inject_audit(state: &AppState, principal_id: &str, references: &[MemoryReference]) {
    let ids: Vec<&str> = references.iter().map(|entry| entry.id.as_str()).collect();
    let detail = serde_json::json!({
        "memoryIds": ids,
        "references": references,
    })
    .to_string();
    for reference in references {
        let audit = crate::store::ai_memory::AiMemoryAuditRecord {
            id: uuid::Uuid::new_v4().to_string(),
            memory_id: reference.id.clone(),
            action: "inject".to_string(),
            actor_principal_id: principal_id.to_string(),
            before_hash: None,
            after_hash: None,
            detail_json: Some(detail.clone()),
            created_at: now().to_rfc3339(),
        };
        if let Err(error) = state.store.app_db().append_ai_memory_audit(&audit) {
            tracing::warn!(error = %error, memory_id = %reference.id, "failed to audit AI memory injection");
        }
    }
}

// ── IPC commands ───────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn ai_memory_list(
    state: State<'_, AppState>,
    scope_type: Option<MemoryScopeKind>,
    scope_id: Option<String>,
    status: Option<MemoryStatus>,
    kind: Option<MemoryKind>,
    query: Option<String>,
    connection_id: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<AiMemoryPage, CommandError> {
    let filter = MemoryFilter {
        scope_type,
        scope_id: scope_id.as_deref(),
        status,
        kind,
        connection_id: connection_id.as_deref(),
        text: query.as_deref(),
    };
    let records = records_for_filter(&state, filter)?;
    page(records, limit, offset)
}

#[tauri::command]
pub async fn ai_memory_create(
    state: State<'_, AppState>,
    input: AiMemoryCreateInput,
) -> Result<AiMemoryDto, CommandError> {
    let (organization_id, principal_id) = scope(&state)?;
    let sensitivity = input.sensitivity.unwrap_or(MemorySensitivity::Normal);
    validate_sensitivity(sensitivity)?;
    let confidence = input.confidence.unwrap_or(0.5);
    validate_confidence(confidence)?;
    if input.subject.trim().is_empty() {
        return Err(CommandError::Validation(
            "memory subject is required".into(),
        ));
    }
    if input.content.trim().is_empty() {
        return Err(CommandError::Validation(
            "memory content is required".into(),
        ));
    }
    let scope_id = derive_scope_id(&input, &organization_id, &principal_id)?;
    let id = input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let timestamp = now().to_rfc3339();
    let structured_json = input
        .structured_json
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(CommandError::Json)?;
    let content_hash = content_hash(&input.content);
    let record = AiMemoryRecord {
        id: id.clone(),
        organization_id,
        owner_principal_id: principal_id.clone(),
        scope_type: input.scope_type.as_str().to_string(),
        scope_id,
        connection_id: input.connection_id,
        database_name: input.database_name,
        schema_name: input.schema_name,
        table_name: input.table_name,
        column_name: input.column_name,
        kind: input.kind.as_str().to_string(),
        subject: input.subject.trim().to_string(),
        content: input.content,
        structured_json,
        source: input.source.unwrap_or_default().as_str().to_string(),
        source_ref: input.source_ref,
        confidence,
        sensitivity: sensitivity.as_str().to_string(),
        status: input
            .status
            .unwrap_or(MemoryStatus::Active)
            .as_str()
            .to_string(),
        pinned: input.pinned.unwrap_or(false),
        use_count: 0,
        last_used_at: None,
        expires_at: input.expires_at.map(|value| value.to_rfc3339()),
        content_hash,
        created_at: timestamp.clone(),
        updated_at: timestamp,
    };
    state
        .store
        .app_db()
        .upsert_ai_memory(&record)
        .map_err(map_storage_error)?;
    append_audit(
        &state,
        &id,
        "create",
        &principal_id,
        None,
        Some(record.content_hash.clone()),
    )?;
    record_to_dto(record)
}

#[tauri::command]
pub async fn ai_memory_update(
    state: State<'_, AppState>,
    id: String,
    patch: AiMemoryUpdateInput,
) -> Result<AiMemoryDto, CommandError> {
    let (organization_id, principal_id) = scope(&state)?;
    let db = state.store.app_db();
    let existing = db
        .get_ai_memory(&organization_id, &principal_id, &id)
        .map_err(map_storage_error)?
        .ok_or_else(|| CommandError::NotFound("memory not found".into()))?;
    let mut record = existing.clone();
    if let Some(subject) = patch.subject {
        if subject.trim().is_empty() {
            return Err(CommandError::Validation(
                "memory subject is required".into(),
            ));
        }
        record.subject = subject.trim().to_string();
    }
    if let Some(content) = patch.content {
        if content.trim().is_empty() {
            return Err(CommandError::Validation(
                "memory content is required".into(),
            ));
        }
        record.content_hash = content_hash(&content);
        record.content = content;
    }
    if let Some(kind) = patch.kind {
        record.kind = kind.as_str().to_string();
    }
    if let Some(structured_json) = patch.structured_json {
        record.structured_json =
            Some(serde_json::to_string(&structured_json).map_err(CommandError::Json)?);
    }
    if let Some(source_ref) = patch.source_ref {
        record.source_ref = Some(source_ref);
    }
    if let Some(confidence) = patch.confidence {
        validate_confidence(confidence)?;
        record.confidence = confidence;
    }
    if let Some(sensitivity) = patch.sensitivity {
        validate_sensitivity(sensitivity)?;
        record.sensitivity = sensitivity.as_str().to_string();
    }
    if let Some(status) = patch.status {
        record.status = status.as_str().to_string();
    }
    if let Some(pinned) = patch.pinned {
        record.pinned = pinned;
    }
    if let Some(expires_at) = patch.expires_at {
        record.expires_at = Some(expires_at.to_rfc3339());
    }
    record.updated_at = now().to_rfc3339();
    db.upsert_ai_memory(&record).map_err(map_storage_error)?;
    append_audit(
        &state,
        &id,
        "update",
        &principal_id,
        Some(existing.content_hash),
        Some(record.content_hash.clone()),
    )?;
    record_to_dto(record)
}

async fn set_status(
    state: &State<'_, AppState>,
    id: &str,
    status: MemoryStatus,
    action: &str,
) -> Result<AiMemoryDto, CommandError> {
    let (organization_id, principal_id) = scope(state)?;
    let db = state.store.app_db();
    let mut record = db
        .get_ai_memory(&organization_id, &principal_id, id)
        .map_err(map_storage_error)?
        .ok_or_else(|| CommandError::NotFound("memory not found".into()))?;
    record.status = status.as_str().to_string();
    record.updated_at = now().to_rfc3339();
    db.upsert_ai_memory(&record).map_err(map_storage_error)?;
    append_audit(
        state,
        id,
        action,
        &principal_id,
        Some(record.content_hash.clone()),
        Some(record.content_hash.clone()),
    )?;
    record_to_dto(record)
}

#[tauri::command]
pub async fn ai_memory_confirm(
    state: State<'_, AppState>,
    id: String,
) -> Result<AiMemoryDto, CommandError> {
    set_status(&state, &id, MemoryStatus::Active, "confirm").await
}

#[tauri::command]
pub async fn ai_memory_reject(
    state: State<'_, AppState>,
    id: String,
) -> Result<AiMemoryDto, CommandError> {
    set_status(&state, &id, MemoryStatus::Rejected, "reject").await
}

#[tauri::command]
pub async fn ai_memory_delete(
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, CommandError> {
    let (organization_id, principal_id) = scope(&state)?;
    let db = state.store.app_db();
    let existing = db
        .get_ai_memory(&organization_id, &principal_id, &id)
        .map_err(map_storage_error)?;
    let deleted = db
        .delete_ai_memory(&organization_id, &principal_id, &id)
        .map_err(map_storage_error)?;
    if deleted {
        append_audit(
            &state,
            &id,
            "delete",
            &principal_id,
            existing.map(|record| record.content_hash),
            None,
        )?;
    }
    Ok(deleted)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn ai_memory_clear(
    state: State<'_, AppState>,
    scope_type: Option<MemoryScopeKind>,
    scope_id: Option<String>,
    status: Option<MemoryStatus>,
) -> Result<usize, CommandError> {
    let (organization_id, principal_id) = scope(&state)?;
    let filter = MemoryFilter {
        scope_type,
        scope_id: scope_id.as_deref(),
        status,
        kind: None,
        connection_id: None,
        text: None,
    };
    let records = records_for_filter(&state, filter)?;
    let db = state.store.app_db();
    let mut deleted = 0usize;
    for record in records {
        if db
            .delete_ai_memory(&organization_id, &principal_id, &record.id)
            .map_err(map_storage_error)?
        {
            deleted += 1;
            append_audit(
                &state,
                &record.id,
                "delete",
                &principal_id,
                Some(record.content_hash),
                None,
            )?;
        }
    }
    Ok(deleted)
}

#[tauri::command]
pub async fn ai_memory_search(
    state: State<'_, AppState>,
    query: String,
    scope_type: Option<MemoryScopeKind>,
    scope_id: Option<String>,
    status: Option<MemoryStatus>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<AiMemoryPage, CommandError> {
    let query = query.trim();
    if query.is_empty() {
        return page(Vec::new(), limit, offset);
    }
    let (organization_id, principal_id) = scope(&state)?;
    let fts = fts_query(query);
    if fts.is_empty() {
        return page(Vec::new(), limit, offset);
    }
    let records = state
        .store
        .app_db()
        .search_ai_memories(&organization_id, &principal_id, &fts, MAX_PAGE_LIMIT)
        .map_err(map_storage_error)?;
    let filter = MemoryFilter {
        scope_type,
        scope_id: scope_id.as_deref(),
        status,
        kind: None,
        connection_id: None,
        text: None,
    };
    page(
        records
            .into_iter()
            .filter(|record| filter.matches(record))
            .collect(),
        limit,
        offset,
    )
}

#[tauri::command]
pub async fn ai_memory_export(
    state: State<'_, AppState>,
    format: String,
    scope_type: Option<MemoryScopeKind>,
    status: Option<MemoryStatus>,
) -> Result<String, CommandError> {
    let filter = MemoryFilter {
        scope_type,
        scope_id: None,
        status,
        kind: None,
        connection_id: None,
        text: None,
    };
    let records = records_for_filter(&state, filter)?;
    let entries = records
        .into_iter()
        .map(record_to_dto)
        .collect::<Result<Vec<_>, _>>()?;
    match format.trim().to_lowercase().as_str() {
        "json" => serde_json::to_string_pretty(&entries).map_err(CommandError::Json),
        "markdown" | "md" => Ok(render_markdown(&entries)),
        _ => Err(CommandError::Validation(
            "memory export format must be json or markdown".into(),
        )),
    }
}

#[tauri::command]
pub async fn ai_memory_stats(state: State<'_, AppState>) -> Result<AiMemoryStats, CommandError> {
    let records = records_for_filter(
        &state,
        MemoryFilter {
            scope_type: None,
            scope_id: None,
            status: None,
            kind: None,
            connection_id: None,
            text: None,
        },
    )?;
    let mut by_status = BTreeMap::new();
    let mut by_kind = BTreeMap::new();
    let mut by_scope = BTreeMap::new();
    for record in &records {
        *by_status.entry(record.status.clone()).or_insert(0) += 1;
        *by_kind.entry(record.kind.clone()).or_insert(0) += 1;
        *by_scope.entry(record.scope_type.clone()).or_insert(0) += 1;
    }
    Ok(AiMemoryStats {
        total: records.len(),
        by_status,
        by_kind,
        by_scope,
    })
}

fn render_markdown(entries: &[AiMemoryDto]) -> String {
    let mut out = format!("# DataZen AI Memory\n\n{} memories\n", entries.len());
    for entry in entries {
        out.push_str(&format!(
            "\n## {} — {} (`{}`)\n\n- status: {}\n- scope: {}:{}\n- confidence: {:.2}\n\n{}\n",
            entry.subject,
            entry.kind.as_str(),
            entry.id,
            entry.status.as_str(),
            entry.scope_type.as_str(),
            entry.scope_id,
            entry.confidence,
            entry.content
        ));
    }
    out
}

// ── Desktop MemoryRepository (T11 port over AppDb/FTS) ─────────────────────

pub(crate) struct DesktopMemoryRepository {
    db: Arc<AppDb>,
    organization_id: String,
    principal_id: String,
}

impl DesktopMemoryRepository {
    pub(crate) fn new(state: &AppState) -> Result<Self, CommandError> {
        let (organization_id, principal_id) = scope(state)?;
        Ok(Self {
            db: state.store.app_db(),
            organization_id,
            principal_id,
        })
    }

    fn load(&self, text: &str, limit: usize) -> Result<Vec<AiMemoryRecord>, MemoryError> {
        let limit = limit.clamp(1, MAX_PAGE_LIMIT as usize) as u32;
        let rows = if text.trim().is_empty() {
            self.db
                .list_ai_memories(&self.organization_id, &self.principal_id)
                .map_err(|error| MemoryError::Repository(error.to_string()))?
        } else {
            let query = fts_query(text);
            if query.is_empty() {
                return Ok(Vec::new());
            }
            self.db
                .search_ai_memories(&self.organization_id, &self.principal_id, &query, limit)
                .map_err(|error| MemoryError::Repository(error.to_string()))?
        };
        Ok(rows.into_iter().take(limit as usize).collect())
    }
}

fn memory_entry(record: AiMemoryRecord) -> Result<MemoryEntry, MemoryError> {
    let invalid =
        |field: &str| MemoryError::Repository(format!("invalid persisted memory {field}"));
    let scope_kind = MemoryScopeKind::parse(&record.scope_type).ok_or_else(|| invalid("scope"))?;
    let kind = MemoryKind::parse(&record.kind).ok_or_else(|| invalid("kind"))?;
    let source = MemorySource::parse(&record.source).ok_or_else(|| invalid("source"))?;
    let sensitivity =
        MemorySensitivity::parse(&record.sensitivity).ok_or_else(|| invalid("sensitivity"))?;
    let status = MemoryStatus::parse(&record.status).ok_or_else(|| invalid("status"))?;
    let parse = |value: &str, field: &str| {
        DateTime::parse_from_rfc3339(value)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|_| invalid(field))
    };
    let structured_json = record
        .structured_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|_| invalid("JSON"))?;
    Ok(MemoryEntry {
        id: record.id,
        organization_id: Some(record.organization_id),
        owner_principal_id: Some(record.owner_principal_id),
        scope: MemoryScope::new(scope_kind, record.scope_id),
        kind,
        subject: record.subject,
        content: record.content,
        structured_json,
        source,
        source_ref: record.source_ref,
        confidence: record.confidence,
        sensitivity,
        status,
        pinned: record.pinned,
        conflict: false,
        use_count: record.use_count.max(0) as u32,
        last_used_at: record
            .last_used_at
            .as_deref()
            .map(|value| parse(value, "lastUsedAt"))
            .transpose()?,
        expires_at: record
            .expires_at
            .as_deref()
            .map(|value| parse(value, "expiresAt"))
            .transpose()?,
        content_hash: Some(record.content_hash),
        created_at: parse(&record.created_at, "createdAt")?,
        updated_at: parse(&record.updated_at, "updatedAt")?,
    })
}

#[async_trait]
impl MemoryRepository for DesktopMemoryRepository {
    async fn candidates(&self, query: &MemoryQuery) -> Result<Vec<MemoryEntry>, MemoryError> {
        let rows = self.load(&query.text, query.limit)?;
        let scopes: Vec<(MemoryScopeKind, &str)> = query
            .scopes
            .iter()
            .map(|scope| (scope.kind, scope.id.as_str()))
            .collect();
        let statuses: Vec<&str> = query
            .statuses
            .iter()
            .map(|status| status.as_str())
            .collect();
        rows.into_iter()
            .filter(|record| {
                MemoryScopeKind::parse(&record.scope_type).is_some_and(|kind| {
                    scopes
                        .iter()
                        .any(|(k, id)| *k == kind && *id == record.scope_id)
                }) && statuses.contains(&record.status.as_str())
            })
            .map(memory_entry)
            .collect()
    }

    async fn list_by_scope(
        &self,
        scope: &MemoryScope,
        status: Option<MemoryStatus>,
    ) -> Result<Vec<MemoryEntry>, MemoryError> {
        let rows = self.load("", MAX_PAGE_LIMIT as usize)?;
        rows.into_iter()
            .filter(|record| {
                MemoryScopeKind::parse(&record.scope_type) == Some(scope.kind)
                    && record.scope_id == scope.id
                    && status.is_none_or(|status| record.status == status.as_str())
            })
            .map(memory_entry)
            .collect()
    }
}

// ── Chat retrieval ─────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub(crate) struct ChatMemory {
    pub injection: Option<MemoryInjection>,
    pub used: Vec<MemoryReference>,
    pub candidates: Vec<MemoryReference>,
}

fn database_scope_id(connection_id: Option<&str>, database: &str) -> String {
    match connection_id.filter(|value| !value.trim().is_empty()) {
        Some(connection_id) if !database.trim().is_empty() => {
            format!("{connection_id}/{database}")
        }
        _ => database.to_string(),
    }
}

pub(crate) fn build_memory_context(
    principal_id: &str,
    organization_id: &str,
    connection_id: Option<&str>,
    database: Option<&str>,
    requested_scope: Option<MemoryScopeKind>,
) -> MemoryContext {
    let mut context = MemoryContext::new(principal_id).with_organization(organization_id);
    let scope = requested_scope.unwrap_or(MemoryScopeKind::Database);
    if matches!(scope, MemoryScopeKind::User | MemoryScopeKind::Organization) {
        return context;
    }
    if let Some(connection_id) = connection_id.filter(|value| !value.trim().is_empty()) {
        context = context.with_connection(connection_id);
    }
    if matches!(
        scope,
        MemoryScopeKind::Database
            | MemoryScopeKind::Schema
            | MemoryScopeKind::Table
            | MemoryScopeKind::Column
    ) {
        if let Some(database) = database.filter(|value| !value.trim().is_empty()) {
            context = context.with_database(database_scope_id(connection_id, database));
        }
    }
    context
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn resolve_chat_memory(
    state: &AppState,
    mode: MemoryMode,
    requested_scope: Option<MemoryScopeKind>,
    connection_id: Option<&str>,
    database: Option<&str>,
    query_text: &str,
    provider_is_cloud: bool,
    egress_level: AiDataEgressLevel,
) -> Result<ChatMemory, CommandError> {
    if mode == MemoryMode::Off {
        return Ok(ChatMemory::default());
    }
    let (organization_id, principal_id) = scope(state)?;
    let repository = DesktopMemoryRepository::new(state)?;
    let context = build_memory_context(
        &principal_id,
        &organization_id,
        connection_id,
        database,
        requested_scope,
    );
    let request = MemoryRetrievalRequest {
        text: query_text.to_string(),
        include_candidates: mode == MemoryMode::Ask,
        ..Default::default()
    };
    let scored = MemoryRetriever::new(&repository)
        .retrieve(&context, &request)
        .await
        .map_err(|error| CommandError::Internal(error.to_string()))?;
    if mode == MemoryMode::Ask {
        return Ok(ChatMemory {
            injection: None,
            used: Vec::new(),
            candidates: scored
                .iter()
                .enumerate()
                .map(|(index, scored)| MemoryReference {
                    reference: format!("M{}", index + 1),
                    id: scored.entry.id.clone(),
                    subject: scored.entry.subject.clone(),
                    kind: scored.entry.kind,
                    scope: scored.entry.scope.label(),
                    score: scored.score.total,
                })
                .collect(),
        });
    }
    let policy = MemoryInjectionPolicy {
        provider_is_cloud,
        data_egress_level: egress_level,
    };
    let injection = MemoryInjector::default().inject(&scored, &policy);
    let used = injection.references();
    if !used.is_empty() {
        append_inject_audit(state, &principal_id, &used);
    }
    Ok(ChatMemory {
        injection: (!injection.is_empty()).then_some(injection),
        used,
        candidates: Vec::new(),
    })
}

pub(crate) fn provider_is_cloud(
    provider: &dyn datazen::ai::domain::traits::AiProvider,
    config: &datazen::ai::AiProviderConfig,
) -> bool {
    !provider_allows_unredacted_credentials(provider.provider_type(), config.endpoint.as_deref())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn emit_memory_candidates<R: tauri::Runtime>(
    window: &WebviewWindow<R>,
    request_id: &str,
    candidates: &[MemoryReference],
) {
    if candidates.is_empty() {
        return;
    }
    let _ = window.emit(
        "ai:memory-candidates",
        serde_json::json!({ "requestId": request_id, "candidates": candidates }),
    );
}

#[cfg(test)]
#[path = "memory_tests.rs"]
mod tests;

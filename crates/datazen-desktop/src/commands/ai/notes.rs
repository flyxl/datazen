//! AI connection-note IPC and delete-confirmation counts.
//!
//! Notes live in AppDb (not `connections.json`) and are scoped by the desktop
//! organization plus the current local principal. The command layer only
//! resolves identity and maps storage errors; sizing/empty-delete semantics
//! live in the store so every caller gets the same validation.

use crate::commands::error::CommandError;
use crate::commands::AppState;
use crate::store::ai_memory::AiConnectionContextCounts;
use crate::store::AppDbError;
use datazen::ai::MAX_CONNECTION_NOTES_BYTES;
use tauri::State;

fn scope(state: &AppState) -> Result<(String, String), CommandError> {
    let organization_id = state
        .organization_scope()?
        .organization_id()
        .as_str()
        .to_string();
    // The desktop principal is process-stable; the same fallback as the
    // startup contexts keeps notes readable even before identity assembly.
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

#[tauri::command]
pub async fn ai_connection_notes_get(
    connection_id: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, CommandError> {
    let (organization_id, principal_id) = scope(&state)?;
    state
        .store
        .app_db()
        .get_ai_connection_note(&organization_id, &principal_id, &connection_id)
        .map(|note| note.map(|note| note.content))
        .map_err(map_storage_error)
}

#[tauri::command]
pub async fn ai_connection_notes_set(
    connection_id: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    if content.len() > MAX_CONNECTION_NOTES_BYTES {
        return Err(CommandError::Validation(format!(
            "connection notes exceed {MAX_CONNECTION_NOTES_BYTES} bytes"
        )));
    }
    let (organization_id, principal_id) = scope(&state)?;
    state
        .store
        .app_db()
        .set_ai_connection_note(&organization_id, &principal_id, &connection_id, &content)
        .map_err(map_storage_error)
}

#[tauri::command]
pub async fn ai_connection_context_counts(
    connection_id: String,
    state: State<'_, AppState>,
) -> Result<AiConnectionContextCounts, CommandError> {
    let (organization_id, principal_id) = scope(&state)?;
    state
        .store
        .app_db()
        .count_ai_connection_context(&organization_id, &principal_id, &connection_id)
        .map_err(map_storage_error)
}

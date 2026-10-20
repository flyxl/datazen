//! Schema Diff profile IPC commands (list / save / delete).

use super::{AppState, CommandError, SchemaDiffProfile, State};

#[tauri::command]
pub async fn get_schema_diff_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<SchemaDiffProfile>, CommandError> {
    Ok(state.store.get_schema_diff_profiles().await)
}

#[tauri::command]
pub async fn save_schema_diff_profile(
    state: State<'_, AppState>,
    mut profile: SchemaDiffProfile,
) -> Result<(), CommandError> {
    profile.validate().map_err(CommandError::Validation)?;
    validate_schema_diff_profile_connections(&state, &profile).await?;
    profile.updated_at = chrono::Utc::now();
    state
        .store
        .save_schema_diff_profile(profile)
        .await
        .map_err(|error| CommandError::Internal(error.to_string()))
}

pub(super) async fn validate_schema_diff_profile_connections(
    state: &AppState,
    profile: &SchemaDiffProfile,
) -> Result<(), CommandError> {
    if state
        .store
        .get_connection(&profile.source_connection_id)
        .await
        .is_none()
    {
        return Err(CommandError::Validation(
            "schema diff profile source connection no longer exists".into(),
        ));
    }
    if state
        .store
        .get_connection(&profile.target_connection_id)
        .await
        .is_none()
    {
        return Err(CommandError::Validation(
            "schema diff profile target connection no longer exists".into(),
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_schema_diff_profile(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<(), CommandError> {
    state
        .store
        .delete_schema_diff_profile(&profile_id)
        .await
        .map_err(|error| CommandError::Internal(error.to_string()))
}

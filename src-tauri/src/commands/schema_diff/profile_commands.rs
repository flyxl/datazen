//! Schema Diff profile IPC commands (list / save / delete).

use super::{AppState, CommandError, SchemaDiffProfile, State};
use crate::commands::port_call::port_cmd;
use crate::platform::ipc::require_application_services;
use datazen_schema_diff::SchemaDiffProfileRepositoryError;

fn profile_repository_error(error: SchemaDiffProfileRepositoryError) -> CommandError {
    match error {
        SchemaDiffProfileRepositoryError::Unavailable(detail) => CommandError::Internal(detail),
        SchemaDiffProfileRepositoryError::NotFound(entity) => CommandError::NotFound(entity),
        SchemaDiffProfileRepositoryError::Invalid(detail) => CommandError::Validation(detail),
    }
}

#[tauri::command]
pub async fn get_schema_diff_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<SchemaDiffProfile>, CommandError> {
    let scope = state.organization_scope()?;
    let services = require_application_services(&state)?;
    services
        .schema_diff_profiles
        .list(&scope)
        .await
        .map_err(profile_repository_error)
}

#[tauri::command]
pub async fn save_schema_diff_profile(
    state: State<'_, AppState>,
    mut profile: SchemaDiffProfile,
) -> Result<(), CommandError> {
    profile.validate().map_err(CommandError::Validation)?;
    validate_schema_diff_profile_connections(&state, &profile).await?;
    profile.updated_at = chrono::Utc::now();
    let scope = state.organization_scope()?;
    let services = require_application_services(&state)?;
    services
        .schema_diff_profiles
        .save(&scope, profile)
        .await
        .map_err(profile_repository_error)
}

pub(super) async fn validate_schema_diff_profile_connections(
    state: &AppState,
    profile: &SchemaDiffProfile,
) -> Result<(), CommandError> {
    let scope = state.organization_scope()?;
    let services = require_application_services(state)?;
    if port_cmd(
        services
            .connections
            .get(&scope, &profile.source_connection_id)
            .await,
        "save_schema_diff_profile",
    )?
    .is_none()
    {
        return Err(CommandError::Validation(
            "schema diff profile source connection no longer exists".into(),
        ));
    }
    if port_cmd(
        services
            .connections
            .get(&scope, &profile.target_connection_id)
            .await,
        "save_schema_diff_profile",
    )?
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
    let scope = state.organization_scope()?;
    let services = require_application_services(&state)?;
    services
        .schema_diff_profiles
        .delete(&scope, &profile_id)
        .await
        .map_err(profile_repository_error)
}

#[cfg(test)]
mod tests {
    use super::{CommandError, SchemaDiffProfileRepositoryError};

    const SOURCE: &str = include_str!("profile_commands.rs");

    #[test]
    fn profile_commands_use_application_services_instead_of_the_host_store() {
        let production = SOURCE.split("#[cfg(test)]").next().unwrap_or_default();
        assert!(
            !production.contains("state.store"),
            "Schema Diff profile commands must not read the host store directly"
        );
        assert_eq!(
            production.matches("schema_diff_profiles").count(),
            4,
            "list/save/delete must all use the injected Schema Diff profile port"
        );
        assert!(
            production.contains(".connections") && production.contains(".get("),
            "profile validation must use the injected connection catalog"
        );
    }

    #[test]
    fn profile_repository_errors_keep_transport_categories() {
        assert!(matches!(
            super::profile_repository_error(SchemaDiffProfileRepositoryError::NotFound(
                "schema diff profile".into()
            )),
            CommandError::NotFound(_)
        ));
        assert!(matches!(
            super::profile_repository_error(SchemaDiffProfileRepositoryError::Invalid(
                "bad profile".into()
            )),
            CommandError::Validation(_)
        ));
        assert!(matches!(
            super::profile_repository_error(SchemaDiffProfileRepositoryError::Unavailable(
                "disk".into()
            )),
            CommandError::Internal(_)
        ));
    }
}

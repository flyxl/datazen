use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use datazen_driver_api::DriverCommandDefinition;

pub(crate) async fn get_connection_commands_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<Vec<DriverCommandDefinition>, CommandError> {
    let (driver, _) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("get_connection_commands")?;
    let mut definitions = driver.command_definitions();
    definitions.retain(|d| !crate::services::schema_metadata::is_metadata_command(&d.id));
    definitions.extend(crate::services::schema_metadata::command_definitions());
    Ok(definitions)
}

pub(crate) async fn get_driver_type_commands_impl(
    state: &AppState,
    driver_type: String,
) -> Result<Vec<DriverCommandDefinition>, CommandError> {
    let driver = state
        .driver_registry
        .get(&driver_type)
        .await
        .ok_or_else(|| CommandError::NotFound(format!("Driver not found: {driver_type}")))?;
    let mut definitions = driver.command_definitions();
    definitions.retain(|d| !crate::services::schema_metadata::is_metadata_command(&d.id));
    definitions.extend(crate::services::schema_metadata::command_definitions());
    Ok(definitions)
}

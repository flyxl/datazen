use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use datazen_driver_api::DriverCommandDefinition;

pub(crate) async fn get_connection_commands_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<Vec<DriverCommandDefinition>, CommandError> {
    let resolved =
        super::resolve::resolve_command_driver(state, Some(&db_session_id), None).await?;
    Ok(
        datazen_feature_core::services::command_policy::discovery_definitions(
            resolved.driver.as_ref(),
        ),
    )
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
    Ok(datazen_feature_core::services::command_policy::discovery_definitions(driver.as_ref()))
}

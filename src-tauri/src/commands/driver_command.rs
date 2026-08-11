use super::error::{CmdExt, CommandError};
use super::AppState;
use datazen_driver_api::{CommandResult, DriverCommandDefinition};
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteDriverCommandRequest {
    pub connection_id: String,
    pub command: String,
    #[serde(default)]
    pub input: serde_json::Value,
}

pub(crate) async fn get_driver_commands_impl(
    state: &AppState,
    connection_id: String,
) -> Result<Vec<DriverCommandDefinition>, CommandError> {
    let (driver, _) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("get_driver_commands")?;
    Ok(driver.command_definitions())
}

pub(crate) async fn execute_driver_command_impl(
    state: &AppState,
    request: ExecuteDriverCommandRequest,
) -> Result<CommandResult, CommandError> {
    let (driver, handle) = state
        .connection_manager
        .get_connection(&request.connection_id)
        .await
        .cmd_err("execute_driver_command")?;

    let definitions = driver.command_definitions();
    if !definitions.iter().any(|definition| definition.id == request.command) {
        return Err(CommandError::Message(format!(
            "Unsupported driver command: {}",
            request.command
        )));
    }

    driver
        .execute_command(&handle, &request.command, request.input)
        .await
        .cmd_err("execute_driver_command")
}

#[tauri::command]
pub async fn get_driver_commands(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<DriverCommandDefinition>, CommandError> {
    get_driver_commands_impl(&state, connection_id).await
}

#[tauri::command]
pub async fn execute_driver_command(
    state: State<'_, AppState>,
    request: ExecuteDriverCommandRequest,
) -> Result<CommandResult, CommandError> {
    execute_driver_command_impl(&state, request).await
}

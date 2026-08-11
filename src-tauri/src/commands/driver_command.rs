use super::error::{CmdExt, CommandError};
use super::AppState;
use datazen_driver_api::{validate_command_input, CommandResult, DriverCommandDefinition};
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

    let definition = driver
        .command_definitions()
        .into_iter()
        .find(|definition| definition.id == request.command)
        .ok_or_else(|| {
            CommandError::Message(format!(
                "Unsupported driver command: {}",
                request.command
            ))
        })?;

    validate_command_input(&definition, &request.input)
        .map_err(CommandError::Message)?;

    // Command definitions are deliberately the single capability gate. This
    // prevents IPC callers from reaching an arbitrary method that a driver did
    // not expose in its manifest. Permission enforcement is kept in the same
    // command-definition path so a future permission backend can consume the
    // declared `definition.permissions` without adding another dispatch API.
    let _permissions = definition.permissions;

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

/// Discover commands from a concrete Connection rather than from a Driver type.
///
/// This is the canonical discovery API for Workflow UI and future command
/// clients. `get_driver_commands` remains as a compatibility alias.
#[tauri::command]
pub async fn get_connection_commands(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_uses_camel_case_wire_format() {
        let request = ExecuteDriverCommandRequest {
            connection_id: "mysql-prod".into(),
            command: "query".into(),
            input: serde_json::json!({ "sql": "SELECT 1" }),
        };

        let encoded = serde_json::to_value(request).unwrap();
        assert_eq!(encoded["connectionId"], "mysql-prod");
        assert_eq!(encoded["command"], "query");
        assert_eq!(encoded["input"]["sql"], "SELECT 1");
        assert!(encoded.get("connection_id").is_none());
    }

    #[test]
    fn request_defaults_input_to_null_when_omitted() {
        let request: ExecuteDriverCommandRequest = serde_json::from_value(serde_json::json!({
            "connectionId": "mysql-prod",
            "command": "query"
        }))
        .unwrap();

        assert_eq!(request.connection_id, "mysql-prod");
        assert_eq!(request.command, "query");
        assert_eq!(request.input, serde_json::Value::Null);
    }
}

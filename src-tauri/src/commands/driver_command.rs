use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::mcp::permission::McpPermissionMode;
use datazen_driver_api::{
    check_command_access, validate_command_input, CommandAccessLevel, CommandResult,
    DriverCommandDefinition,
};
use serde::{Deserialize, Serialize};
use tauri::State;

pub(crate) fn access_level_for_mode(mode: Option<McpPermissionMode>) -> CommandAccessLevel {
    match mode {
        None | Some(McpPermissionMode::HighRiskWrite) => CommandAccessLevel::HighRisk,
        Some(McpPermissionMode::SafeWrite) => CommandAccessLevel::Write,
        Some(McpPermissionMode::ReadOnly) => CommandAccessLevel::Read,
    }
}

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
    let (_runtime_id, driver, _) = state
        .connection_manager
        .resolve_session(&connection_id)
        .await
        .cmd_err("get_driver_commands")?;
    Ok(driver.command_definitions())
}

pub(crate) async fn execute_driver_command_impl(
    state: &AppState,
    request: ExecuteDriverCommandRequest,
) -> Result<CommandResult, CommandError> {
    execute_driver_command_with_mode(state, request, None).await
}

pub(crate) async fn execute_driver_command_with_mode(
    state: &AppState,
    request: ExecuteDriverCommandRequest,
    permission_mode: Option<McpPermissionMode>,
) -> Result<CommandResult, CommandError> {
    let (_runtime_id, driver, handle) = state
        .connection_manager
        .resolve_session(&request.connection_id)
        .await
        .cmd_err("execute_driver_command")?;

    let definition = driver
        .command_definitions()
        .into_iter()
        .find(|definition| definition.id == request.command)
        .ok_or_else(|| {
            CommandError::Validation(format!(
                "Unsupported driver command: {}",
                request.command
            ))
        })?;

    validate_command_input(&definition, &request.input)
        .map_err(CommandError::Validation)?;
    check_command_access(&definition, access_level_for_mode(permission_mode))
        .map_err(CommandError::Validation)?;

    if matches!(definition.id.as_str(), "query" | "execute") {
        if let Some(mode) = permission_mode {
            if let Some(sql) = request.input.get("sql").and_then(|v| v.as_str()) {
                crate::mcp::permission::check_sql_allowed(sql, mode)
                    .map_err(CommandError::Validation)?;
            }
        }
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

    #[tokio::test]
    async fn discovers_standard_commands_from_connection() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let (_, conn_id) = test.save_and_connect("cmd-discover").await;
        let definitions = get_driver_commands_impl(&test.state, conn_id).await.unwrap();
        let ids: Vec<_> = definitions.iter().map(|d| d.id.as_str()).collect();
        assert!(ids.contains(&"query"));
        assert!(ids.contains(&"execute"));
    }

    #[tokio::test]
    async fn executes_query_command_through_ipc() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let (_, conn_id) = test.save_and_connect("cmd-exec").await;
        let result = execute_driver_command_impl(
            &test.state,
            ExecuteDriverCommandRequest {
                connection_id: conn_id,
                command: "query".into(),
                input: serde_json::json!({ "sql": "SELECT 1" }),
            },
        )
        .await
        .unwrap();
        assert!(result.data.get("results").is_some() || result.data.get("columns").is_some() || result.data.is_object());
    }

    #[tokio::test]
    async fn rejects_unknown_command() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let (_, conn_id) = test.save_and_connect("cmd-unknown").await;
        let err = execute_driver_command_impl(
            &test.state,
            ExecuteDriverCommandRequest {
                connection_id: conn_id,
                command: "not-a-command".into(),
                input: serde_json::json!({}),
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("Unsupported driver command"));
    }

    #[tokio::test]
    async fn rejects_invalid_query_input() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let (_, conn_id) = test.save_and_connect("cmd-invalid").await;
        let err = execute_driver_command_impl(
            &test.state,
            ExecuteDriverCommandRequest {
                connection_id: conn_id,
                command: "query".into(),
                input: serde_json::json!({}),
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("missing required field"));
    }

    #[tokio::test]
    async fn read_only_mode_denies_execute() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let (_, conn_id) = test.save_and_connect("cmd-ro").await;
        let err = execute_driver_command_with_mode(
            &test.state,
            ExecuteDriverCommandRequest {
                connection_id: conn_id,
                command: "execute".into(),
                input: serde_json::json!({ "sql": "DELETE FROM t" }),
            },
            Some(crate::mcp::permission::McpPermissionMode::ReadOnly),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("not allowed"));
    }

    #[tokio::test]
    async fn discovers_commands_from_config_id() {
        let test = crate::testing::app_state::TestAppState::new().await;
        test.save_connection("cfg-discover").await;
        let definitions = get_driver_commands_impl(&test.state, "cfg-discover".into())
            .await
            .unwrap();
        assert!(definitions.iter().any(|d| d.id == "query"));
    }
}

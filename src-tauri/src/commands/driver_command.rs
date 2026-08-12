use std::sync::Arc;

use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::mcp::permission::McpPermissionMode;
use datazen_driver_api::{
    check_command_access, validate_command_input, CommandAccessLevel, CommandResult,
    ConnectionHandle, DatabaseDriver, DriverCommandDefinition,
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
    #[serde(default)]
    pub connection_id: Option<String>,
    #[serde(default)]
    pub driver_type: Option<String>,
    pub command: String,
    #[serde(default)]
    pub input: serde_json::Value,
}

fn unbound_handle() -> ConnectionHandle {
    ConnectionHandle {
        id: String::new(),
        pool_id: String::new(),
    }
}

fn nonempty(value: Option<&String>) -> Option<&str> {
    value.map(String::as_str).map(str::trim).filter(|s| !s.is_empty())
}

/// SQL SELECT row cap from settings. `None` when the "limit SELECT results"
/// switch is off — streaming must not invent a cap from batch size.
pub(crate) async fn query_result_limit_from_settings(state: &AppState) -> Option<u32> {
    let settings = state.store.get_settings().await;
    if settings.limit_select_results && settings.query_result_limit > 0 {
        Some(settings.query_result_limit)
    } else {
        None
    }
}

async fn apply_query_result_limit(state: &AppState, input: &mut serde_json::Value) {
    if input.get("limit").is_some() {
        return;
    }
    if let Some(limit) = query_result_limit_from_settings(state).await {
        input["limit"] = serde_json::json!(limit);
    }
}

fn sql_from_input(input: &serde_json::Value) -> Option<String> {
    input
        .get("sql")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

async fn record_sql_command_outcome(
    state: &AppState,
    connection_id: Option<&str>,
    sql: &str,
    success: bool,
    execution_time_ms: u64,
    rows_affected: Option<u64>,
    error_message: Option<String>,
) {
    let Some(connection_id) = connection_id else {
        return;
    };
    let entry = crate::store::QueryHistoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        connection_id: connection_id.to_string(),
        database: String::new(),
        sql: sql.to_string(),
        executed_at: chrono::Utc::now(),
        execution_time_ms,
        rows_affected,
        success,
        error_message,
    };
    let _ = state.store.add_query_history(entry).await;
}

fn query_rows_affected(data: &serde_json::Value) -> Option<u64> {
    if let Some(rows) = data.get("rowsAffected").and_then(|v| v.as_u64()) {
        return Some(rows);
    }
    let results = data.get("results")?.as_array()?;
    Some(
        results
            .iter()
            .filter_map(|r| r.get("rowsAffected").and_then(|v| v.as_u64()))
            .sum(),
    )
}

async fn resolve_command_driver(
    state: &AppState,
    connection_id: Option<&String>,
    driver_type: Option<&String>,
) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle, bool), CommandError> {
    if let Some(id) = nonempty(connection_id) {
        let (_runtime_id, driver, handle) = state
            .connection_manager
            .resolve_session(id)
            .await
            .cmd_err("execute_driver_command")?;
        return Ok((driver, handle, true));
    }
    let driver_type = nonempty(driver_type).ok_or_else(|| {
        CommandError::Validation("connectionId or driverType is required".into())
    })?;
    let driver = state
        .driver_registry
        .get(&driver_type.to_string())
        .await
        .ok_or_else(|| CommandError::NotFound(format!("Driver not found: {driver_type}")))?;
    Ok((driver, unbound_handle(), false))
}

pub(crate) async fn get_connection_commands_impl(
    state: &AppState,
    connection_id: String,
) -> Result<Vec<DriverCommandDefinition>, CommandError> {
    let (_runtime_id, driver, _) = state
        .connection_manager
        .resolve_session(&connection_id)
        .await
        .cmd_err("get_connection_commands")?;
    Ok(driver.command_definitions())
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
    mut request: ExecuteDriverCommandRequest,
    permission_mode: Option<McpPermissionMode>,
) -> Result<CommandResult, CommandError> {
    let (driver, handle, bound) = resolve_command_driver(
        state,
        request.connection_id.as_ref(),
        request.driver_type.as_ref(),
    )
    .await?;

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

    if !bound && definition.metadata.requires_connection {
        return Err(CommandError::Validation(format!(
            "Command '{}' requires a connection",
            request.command
        )));
    }

    let is_sql_command = matches!(definition.id.as_str(), "query" | "execute");
    if definition.id == "query" {
        apply_query_result_limit(state, &mut request.input).await;
    }

    validate_command_input(&definition, &request.input)
        .map_err(CommandError::Validation)?;
    check_command_access(&definition, access_level_for_mode(permission_mode))
        .map_err(CommandError::Validation)?;

    if is_sql_command {
        if let Some(mode) = permission_mode {
            if let Some(sql) = request.input.get("sql").and_then(|v| v.as_str()) {
                crate::mcp::permission::check_sql_allowed(sql, mode)
                    .map_err(CommandError::Validation)?;
            }
        }
    }

    let sql = sql_from_input(&request.input);
    let connection_id = nonempty(request.connection_id.as_ref()).map(str::to_string);
    tracing::info!(
        command = %request.command,
        connection_id = connection_id.as_deref().unwrap_or(""),
        sql_len = sql.as_ref().map(|s| s.len()).unwrap_or(0),
        "execute_driver_command"
    );
    if let Some(sql) = sql.as_ref() {
        tracing::debug!(sql_preview = %sql.chars().take(500).collect::<String>(), "execute_driver_command sql");
    }

    match driver
        .execute_command(&handle, &request.command, request.input)
        .await
    {
        Ok(result) => {
            if is_sql_command {
                if let Some(sql) = sql.as_deref() {
                    let execution_time_ms = result
                        .data
                        .get("totalTimeMs")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    record_sql_command_outcome(
                        state,
                        connection_id.as_deref(),
                        sql,
                        true,
                        execution_time_ms,
                        query_rows_affected(&result.data),
                        None,
                    )
                    .await;
                    if crate::cache::sql_may_mutate_schema(sql) {
                        if let Some(id) = connection_id.as_deref() {
                            state.schema_cache.clear_connection(id).await;
                        }
                    }
                }
            }
            Ok(result)
        }
        Err(err) => {
            if is_sql_command {
                if let Some(sql) = sql.as_deref() {
                    record_sql_command_outcome(
                        state,
                        connection_id.as_deref(),
                        sql,
                        false,
                        0,
                        None,
                        Some(err.to_string()),
                    )
                    .await;
                }
            }
            Err(err).cmd_err("execute_driver_command")
        }
    }
}

/// Discover commands from a Driver type. No live Connection is required.
#[tauri::command]
pub async fn get_driver_commands(
    state: State<'_, AppState>,
    driver_type: String,
) -> Result<Vec<DriverCommandDefinition>, CommandError> {
    get_driver_type_commands_impl(&state, driver_type).await
}

/// Discover commands from a concrete Connection.
#[tauri::command]
pub async fn get_connection_commands(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<DriverCommandDefinition>, CommandError> {
    get_connection_commands_impl(&state, connection_id).await
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
            connection_id: Some("mysql-prod".into()),
            driver_type: None,
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

        assert_eq!(request.connection_id.as_deref(), Some("mysql-prod"));
        assert_eq!(request.command, "query");
        assert_eq!(request.input, serde_json::Value::Null);
        assert!(request.driver_type.is_none());
    }

    #[tokio::test]
    async fn discovers_standard_commands_from_connection() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let (_, conn_id) = test.save_and_connect("cmd-discover").await;
        let definitions = get_connection_commands_impl(&test.state, conn_id).await.unwrap();
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
                connection_id: Some(conn_id.clone()),
                driver_type: None,
                command: "query".into(),
                input: serde_json::json!({ "sql": "SELECT 1" }),
            },
        )
        .await
        .unwrap();
        assert!(result.data.get("results").is_some() || result.data.get("columns").is_some() || result.data.is_object());
        let history = test.state.store.get_query_history(10).await;
        assert!(history.iter().any(|e| e.connection_id == conn_id && e.success));
    }

    #[tokio::test]
    async fn rejects_unknown_command() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let (_, conn_id) = test.save_and_connect("cmd-unknown").await;
        let err = execute_driver_command_impl(
            &test.state,
            ExecuteDriverCommandRequest {
                connection_id: Some(conn_id),
                driver_type: None,
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
                connection_id: Some(conn_id),
                driver_type: None,
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
                connection_id: Some(conn_id),
                driver_type: None,
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
        let definitions = get_connection_commands_impl(&test.state, "cfg-discover".into())
            .await
            .unwrap();
        assert!(definitions.iter().any(|d| d.id == "query"));
    }

    #[tokio::test]
    async fn discovers_commands_from_driver_type_without_connection() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let definitions = get_driver_type_commands_impl(&test.state, "postgres".into())
            .await
            .unwrap();
        assert!(definitions.iter().any(|d| d.id == "query"));
    }

    #[tokio::test]
    async fn rejects_connection_required_command_without_connection() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let err = execute_driver_command_impl(
            &test.state,
            ExecuteDriverCommandRequest {
                connection_id: None,
                driver_type: Some("postgres".into()),
                command: "query".into(),
                input: serde_json::json!({ "sql": "SELECT 1" }),
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("requires a connection"));
    }

    #[tokio::test]
    async fn executes_unbound_driver_command_by_type() {
        use datazen_driver_api::{CommandCategory, DriverCommandDefinition, DriverCommandMetadata};

        let ping = DriverCommandDefinition {
            id: "ping".into(),
            name: "Ping".into(),
            description: None,
            input_schema: serde_json::json!({ "type": "object" }),
            output_schema: None,
            permissions: vec![],
            metadata: DriverCommandMetadata::new(CommandCategory::Observe, CommandAccessLevel::Read)
                .unbound()
                .hide_from_workflow(),
        };
        let test = crate::testing::app_state::TestAppState::with_options(
            crate::testing::mock_driver::MockDriverOptions {
                extra_commands: vec![ping],
                ..Default::default()
            },
        )
        .await;
        let result = execute_driver_command_impl(
            &test.state,
            ExecuteDriverCommandRequest {
                connection_id: None,
                driver_type: Some("postgres".into()),
                command: "ping".into(),
                input: serde_json::json!({}),
            },
        )
        .await
        .unwrap();
        assert_eq!(result.data["command"], "ping");
    }
}

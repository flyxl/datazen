use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::error::{CmdExt, CommandError};
use super::query::ensure_session_database;
use super::AppState;
use crate::mcp::permission::McpPermissionMode;
use datazen_driver_api::{
    check_command_access, validate_command_input, CommandAccessLevel, CommandResult,
    ConnectionHandle, DatabaseDriver, DriverCommandDefinition, DriverSaveDialogSpec,
    QueryStreamCallback, QueryStreamEvent,
};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
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
pub struct ExecuteDriverCommandStreamRequest {
    #[serde(default)]
    pub db_session_id: Option<String>,
    pub command: String,
    #[serde(default)]
    pub input: serde_json::Value,
    /// F1: optional explicit database pin — the session is switched to this
    /// logical database before the command runs (same mechanism as the
    /// query-family commands; `None`/blank keeps the current active database).
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub apply_result_limit: Option<bool>,
    #[serde(default)]
    pub record_history: Option<bool>,
}

#[derive(Clone, Copy)]
pub struct ExecuteDriverCommandStreamOpts {
    pub apply_result_limit: bool,
    pub record_history: bool,
}

impl Default for ExecuteDriverCommandStreamOpts {
    fn default() -> Self {
        Self {
            apply_result_limit: true,
            record_history: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteDriverCommandRequest {
    #[serde(default)]
    pub db_session_id: Option<String>,
    #[serde(default)]
    pub driver_type: Option<String>,
    pub command: String,
    #[serde(default)]
    pub input: serde_json::Value,
    /// F1: optional explicit database pin (session-bound commands only —
    /// ignored for unbound `driverType` requests). See
    /// `ensure_session_database` for the switching semantics.
    #[serde(default)]
    pub database: Option<String>,
}

fn unbound_handle() -> ConnectionHandle {
    ConnectionHandle {
        id: String::new(),
        pool_id: String::new(),
    }
}

fn nonempty(value: Option<&String>) -> Option<&str> {
    value
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
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
    db_session_id: Option<&str>,
    sql: &str,
    success: bool,
    execution_time_ms: u64,
    rows_affected: Option<u64>,
    error_message: Option<String>,
) {
    let Some(db_session_id) = db_session_id else {
        return;
    };
    // Resolve the persisted connection config that owns this runtime session.
    let Some(connection_id) = state
        .connection_manager
        .owner_connection_id(db_session_id)
        .await
    else {
        tracing::warn!(
            db_session_id,
            "Skipping history: no owning connectionId for this dbSessionId"
        );
        return;
    };
    // Record the session-active logical database so history can be grouped /
    // filtered per panel context (empty string when the driver is single-db).
    let database = state
        .connection_manager
        .get_session_config(db_session_id)
        .await
        .ok()
        .and_then(|config| config.database)
        .unwrap_or_default();
    let entry = crate::store::QueryHistoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        connection_id,
        database,
        schema: None,
        sql: sql.to_string(),
        executed_at: chrono::Utc::now(),
        execution_time_ms,
        rows_affected,
        success,
        error_message,
    };
    // Persistence failures must not break query execution, but silently
    // dropping history (disk full / locked db) is undebuggable — log it.
    if let Err(e) = state.store.add_query_history(entry).await {
        tracing::warn!(error = %e, "add_query_history failed");
    }
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
    db_session_id: Option<&String>,
    driver_type: Option<&String>,
) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle, bool), CommandError> {
    // resolve_session is dual-mode: it accepts a runtime db_session_id and
    // falls back to the owning persisted connection_id (legacy callers, e.g.
    // the extension bridge until W3 adds an explicit target parameter).
    if let Some(id) = nonempty(db_session_id) {
        let (_runtime_id, driver, handle) = state
            .connection_manager
            .resolve_session(id)
            .await
            .cmd_err("execute_driver_command")?;
        return Ok((driver, handle, true));
    }
    let driver_type = nonempty(driver_type)
        .ok_or_else(|| CommandError::Validation("dbSessionId or driverType is required".into()))?;
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
    // Internal reuse (query/schema IPCs, MCP tools): never shows a native
    // dialog — commands declaring a save dialog are rejected here.
    execute_driver_command_with_mode(state, request, None, None).await
}

pub(crate) async fn execute_driver_command_stream_impl(
    state: &AppState,
    mut request: ExecuteDriverCommandStreamRequest,
    on_event: QueryStreamCallback,
    opts: ExecuteDriverCommandStreamOpts,
) -> Result<(), CommandError> {
    if request.command != "query_stream" {
        return Err(CommandError::Validation(format!(
            "Streaming is only supported for command 'query_stream', got '{}'",
            request.command
        )));
    }

    let db_session_id = request
        .db_session_id
        .as_ref()
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| CommandError::Validation("dbSessionId is required".into()))?;

    let (driver, handle, _bound) =
        resolve_command_driver(state, request.db_session_id.as_ref(), None).await?;

    // F1: pin the session's active database before streaming so unqualified
    // SQL lands on the caller-selected database (no-op without a pin).
    ensure_session_database(
        state,
        &handle.id,
        request.database.as_deref(),
        "execute_driver_command_stream",
    )
    .await?;

    let definition = driver
        .command_definitions()
        .into_iter()
        .find(|definition| definition.id == request.command)
        .ok_or_else(|| {
            CommandError::Validation(format!(
                "Unsupported streaming driver command: {}",
                request.command
            ))
        })?;

    if opts.apply_result_limit {
        apply_query_result_limit(state, &mut request.input).await;
    }

    validate_command_input(&definition, &request.input).map_err(CommandError::Validation)?;
    check_command_access(&definition, CommandAccessLevel::Read)
        .map_err(CommandError::Validation)?;

    let sql = sql_from_input(&request.input).ok_or_else(|| {
        CommandError::Validation("command 'query_stream' requires string input 'sql'".into())
    })?;

    if let Some(params) = request.input.get("params").cloned() {
        let bound_sql =
            crate::sql_guard::apply_params(&sql, &params).map_err(CommandError::Validation)?;
        request.input["sql"] = serde_json::Value::String(bound_sql);
    }
    let sql = request
        .input
        .get("sql")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            CommandError::Validation("command 'query_stream' requires string input 'sql'".into())
        })?;

    tracing::info!(
        db_session_id,
        sql_len = sql.len(),
        "execute_driver_command_stream"
    );
    tracing::debug!(
        db_session_id,
        sql_preview = %sql.chars().take(500).collect::<String>(),
        "execute_driver_command_stream sql"
    );

    let read_only = state
        .connection_manager
        .get_session_config(&handle.id)
        .await
        .map(|c| c.read_only)
        .unwrap_or(false);
    let safe_mode = state.store.get_settings().await.safe_mode;
    crate::sql_guard::check_sql(&sql, read_only, safe_mode).map_err(CommandError::Validation)?;

    let limit = if opts.apply_result_limit {
        query_result_limit_from_settings(state).await
    } else {
        None
    };
    let rows_affected = Arc::new(AtomicU64::new(0));
    let total_ms = Arc::new(AtomicU64::new(0));
    let rows_cb = Arc::clone(&rows_affected);
    let ms_cb = Arc::clone(&total_ms);
    let user_cb = Arc::clone(&on_event);
    let wrapped: QueryStreamCallback = Arc::new(move |event| {
        match &event {
            QueryStreamEvent::StatementEnd {
                rows_affected: Some(n),
                ..
            } => {
                rows_cb.fetch_add(*n, Ordering::Relaxed);
            }
            QueryStreamEvent::Done { total_time_ms } => {
                ms_cb.store(*total_time_ms, Ordering::Relaxed);
            }
            _ => {}
        }
        user_cb(event);
    });

    match driver.query_stream(&handle, &sql, limit, wrapped).await {
        Ok(()) => {
            if opts.record_history {
                record_sql_command_outcome(
                    state,
                    Some(db_session_id),
                    &sql,
                    true,
                    total_ms.load(Ordering::Relaxed),
                    Some(rows_affected.load(Ordering::Relaxed)),
                    None,
                )
                .await;
            }
            if crate::cache::sql_may_mutate_schema(&sql) {
                state.schema_cache.clear_connection(db_session_id).await;
            }
            Ok(())
        }
        Err(err) => {
            if opts.record_history {
                record_sql_command_outcome(
                    state,
                    Some(db_session_id),
                    &sql,
                    false,
                    0,
                    None,
                    Some(err.to_string()),
                )
                .await;
            }
            Err(err).cmd_err("execute_driver_command_stream")
        }
    }
}

pub(crate) async fn execute_driver_command_with_mode(
    state: &AppState,
    mut request: ExecuteDriverCommandRequest,
    permission_mode: Option<McpPermissionMode>,
    dialog: Option<&tauri::AppHandle>,
) -> Result<CommandResult, CommandError> {
    let (driver, handle, bound) = resolve_command_driver(
        state,
        request.db_session_id.as_ref(),
        request.driver_type.as_ref(),
    )
    .await?;

    let definition = driver
        .command_definitions()
        .into_iter()
        .find(|definition| definition.id == request.command)
        .ok_or_else(|| {
            CommandError::Validation(format!("Unsupported driver command: {}", request.command))
        })?;

    if !bound && definition.metadata.requires_connection {
        return Err(CommandError::Validation(format!(
            "Command '{}' requires a connection",
            request.command
        )));
    }

    // Save-dialog commands are interactive-only: the host thin shell pops the
    // native dialog after execution, which is impossible headless (MCP /
    // workflow / internal reuse). Reject before doing any work.
    if definition.metadata.save_dialog.is_some() {
        if permission_mode.is_some() || dialog.is_none() {
            return Err(CommandError::Validation(format!(
                "Command '{}' requires an interactive session to show its save dialog",
                request.command
            )));
        }
    }

    // F1: session-bound commands honor an optional explicit database pin and
    // switch the live session before execution; unbound driverType requests
    // have no session to switch and ignore it.
    if bound {
        ensure_session_database(
            state,
            &handle.id,
            request.database.as_deref(),
            "execute_driver_command",
        )
        .await?;
    }

    let is_sql_command = matches!(definition.id.as_str(), "query" | "execute");
    if definition.id == "query" {
        apply_query_result_limit(state, &mut request.input).await;
    }

    validate_command_input(&definition, &request.input).map_err(CommandError::Validation)?;
    check_command_access(&definition, access_level_for_mode(permission_mode))
        .map_err(CommandError::Validation)?;

    if is_sql_command {
        if let Some(params) = request.input.get("params").cloned() {
            if let Some(sql) = request.input.get("sql").and_then(|v| v.as_str()) {
                let bound_sql = crate::sql_guard::apply_params(sql, &params)
                    .map_err(CommandError::Validation)?;
                request.input["sql"] = serde_json::Value::String(bound_sql);
            }
        }
        if bound {
            if let Some(sql) = request.input.get("sql").and_then(|v| v.as_str()) {
                let read_only = state
                    .connection_manager
                    .get_session_config(&handle.id)
                    .await
                    .map(|c| c.read_only)
                    .unwrap_or(false);
                let safe_mode = state.store.get_settings().await.safe_mode;
                crate::sql_guard::check_sql(sql, read_only, safe_mode)
                    .map_err(CommandError::Validation)?;
            }
        }
        if let Some(mode) = permission_mode {
            if let Some(sql) = request.input.get("sql").and_then(|v| v.as_str()) {
                crate::mcp::permission::check_sql_allowed(sql, mode)
                    .map_err(CommandError::Validation)?;
            }
        }
    }

    let sql = sql_from_input(&request.input);
    let db_session_id = nonempty(request.db_session_id.as_ref()).map(str::to_string);
    tracing::info!(
        command = %request.command,
        db_session_id = db_session_id.as_deref().unwrap_or(""),
        sql_len = sql.as_ref().map(|s| s.len()).unwrap_or(0),
        "execute_driver_command"
    );
    if let Some(sql) = sql.as_ref() {
        let preview: String = sql.chars().take(500).collect();
        tracing::debug!(
            sql_preview = %crate::log_redact::redact_secrets_for_log(&preview),
            "execute_driver_command sql"
        );
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
                        db_session_id.as_deref(),
                        sql,
                        true,
                        execution_time_ms,
                        query_rows_affected(&result.data),
                        None,
                    )
                    .await;
                    if crate::cache::sql_may_mutate_schema(sql) {
                        if let Some(id) = db_session_id.as_deref() {
                            state.schema_cache.clear_connection(id).await;
                        }
                    }
                }
            }
            // Generic host thin shell for metadata-declared save dialogs: no
            // driver-type branching — any driver opting in gets the same flow.
            if let Some(spec) = definition.metadata.save_dialog.as_ref() {
                return finish_save_dialog(
                    dialog.expect("save dialog checked above"),
                    spec,
                    result,
                )
                .await;
            }
            Ok(result)
        }
        Err(err) => {
            if is_sql_command {
                if let Some(sql) = sql.as_deref() {
                    record_sql_command_outcome(
                        state,
                        db_session_id.as_deref(),
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

/// Thin shell shared by every command whose metadata declares a save dialog:
/// decode the command's byte payload, ask the user where to store it through a
/// native save dialog, write the bytes and replace the result data with
/// `{ <resultPathField>: savedPath | null }` (`null` on cancel). The absolute
/// path is user-chosen in an OS dialog, mirroring the former dedicated
/// `*_with_dialog` IPCs.
async fn finish_save_dialog(
    app: &tauri::AppHandle,
    spec: &DriverSaveDialogSpec,
    mut result: CommandResult,
) -> Result<CommandResult, CommandError> {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use tauri_plugin_dialog::DialogExt;

    if spec.extensions.is_empty() {
        return Err(CommandError::Validation(
            "Save-dialog command must declare at least one file extension".into(),
        ));
    }
    let ext_list: Vec<&str> = spec.extensions.iter().map(String::as_str).collect();

    let data_base64 = result
        .data
        .get(&spec.data_base64_field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            CommandError::Internal(format!(
                "Save-dialog command did not return '{}' byte data",
                spec.data_base64_field
            ))
        })?
        .to_string();
    let bytes = BASE64
        .decode(data_base64.trim())
        .map_err(|e| CommandError::Internal(format!("Command returned invalid base64: {e}")))?;
    let file_name = result
        .data
        .get(&spec.file_name_field)
        .and_then(serde_json::Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("download")
        .to_string();

    let picked = app
        .dialog()
        .file()
        .add_filter(&spec.filter_name, &ext_list)
        .set_file_name(&file_name)
        .blocking_save_file();
    let saved = match picked {
        None => None,
        Some(fp) => {
            let path = super::file::dialog_path_to_buf(fp)?;
            super::file::validate_extension(&path, &ext_list)?;
            let byte_count = bytes.len();
            tokio::fs::write(&path, bytes)
                .await
                .cmd_err("execute_driver_command")?;
            tracing::info!(
                bytes = byte_count,
                saved = %path.display(),
                "driver command payload saved via native dialog"
            );
            Some(path.to_string_lossy().into_owned())
        }
    };

    let mut data = serde_json::Map::new();
    data.insert(
        spec.result_path_field.clone(),
        saved.map(serde_json::Value::String).unwrap_or_default(),
    );
    result.data = serde_json::Value::Object(data);
    Ok(result)
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
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: ExecuteDriverCommandRequest,
) -> Result<CommandResult, CommandError> {
    // GUI IPC passes the AppHandle so metadata-declared save dialogs can run.
    execute_driver_command_with_mode(&state, request, None, Some(&app)).await
}

#[tauri::command]
pub async fn execute_driver_command_stream(
    state: State<'_, AppState>,
    request: ExecuteDriverCommandStreamRequest,
    on_event: Channel<QueryStreamEvent>,
    apply_result_limit: Option<bool>,
    record_history: Option<bool>,
) -> Result<(), CommandError> {
    let callback: QueryStreamCallback = Arc::new(move |event| {
        let _ = on_event.send(event);
    });
    execute_driver_command_stream_impl(
        &state,
        request,
        callback,
        ExecuteDriverCommandStreamOpts {
            apply_result_limit: apply_result_limit.unwrap_or(true),
            record_history: record_history.unwrap_or(true),
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_uses_camel_case_wire_format() {
        let request = ExecuteDriverCommandRequest {
            db_session_id: Some("mysql-prod".into()),
            driver_type: None,
            command: "query".into(),
            input: serde_json::json!({ "sql": "SELECT 1" }),
            database: Some("db_b".into()),
        };

        let encoded = serde_json::to_value(request).unwrap();
        assert_eq!(encoded["dbSessionId"], "mysql-prod");
        assert_eq!(encoded["command"], "query");
        assert_eq!(encoded["input"]["sql"], "SELECT 1");
        assert_eq!(encoded["database"], "db_b");
        assert!(encoded.get("db_session_id").is_none());
    }

    #[test]
    fn request_defaults_input_to_null_when_omitted() {
        let request: ExecuteDriverCommandRequest = serde_json::from_value(serde_json::json!({
            "dbSessionId": "mysql-prod",
            "command": "query"
        }))
        .unwrap();

        assert_eq!(request.db_session_id.as_deref(), Some("mysql-prod"));
        assert_eq!(request.command, "query");
        assert_eq!(request.input, serde_json::Value::Null);
        assert!(request.driver_type.is_none());
    }

    #[test]
    fn debug_sql_preview_redacts_secrets() {
        let sql = "SELECT * FROM t WHERE url = 'mysql://root:hunter2@127.0.0.1/app'";
        let preview: String = sql.chars().take(500).collect();
        let redacted = crate::log_redact::redact_secrets_for_log(&preview);
        assert!(!redacted.contains("hunter2"), "{redacted}");
        assert!(redacted.contains("mysql://***@"), "{redacted}");
    }

    #[tokio::test]
    async fn discovers_standard_commands_from_connection() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let (_, conn_id) = test.save_and_connect("cmd-discover").await;
        let definitions = get_connection_commands_impl(&test.state, conn_id)
            .await
            .unwrap();
        let ids: Vec<_> = definitions.iter().map(|d| d.id.as_str()).collect();
        assert!(ids.contains(&"query"));
        assert!(ids.contains(&"execute"));
    }

    #[tokio::test]
    async fn execute_driver_command_pins_session_database_before_execution() {
        let test = crate::testing::app_state::TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("cmd-pin-db").await;
        // Sample config pins database = "app"; an explicit different pin must
        // switch the live session before the command runs (BUG-001 fix).
        execute_driver_command_impl(
            &test.state,
            ExecuteDriverCommandRequest {
                db_session_id: Some(conn_id.clone()),
                driver_type: None,
                command: "query".into(),
                input: serde_json::json!({ "sql": "SELECT 1" }),
                database: Some("analytics".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            test.mock.use_database_calls(),
            vec!["analytics".to_string()]
        );
        let config = test
            .state
            .connection_manager
            .get_session_config(&conn_id)
            .await
            .unwrap();
        assert_eq!(config.database.as_deref(), Some("analytics"));
    }

    #[tokio::test]
    async fn execute_driver_command_skips_switch_when_pin_missing_or_same() {
        let test = crate::testing::app_state::TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("cmd-no-pin").await;
        for database in [None, Some("app".into()), Some("   ".into())] {
            execute_driver_command_impl(
                &test.state,
                ExecuteDriverCommandRequest {
                    db_session_id: Some(conn_id.clone()),
                    driver_type: None,
                    command: "query".into(),
                    input: serde_json::json!({ "sql": "SELECT 1" }),
                    database: database.clone(),
                },
            )
            .await
            .unwrap();
        }
        assert!(test.mock.use_database_calls().is_empty());
    }

    #[tokio::test]
    async fn stream_pins_session_database_before_query_stream() {
        let test = crate::testing::app_state::TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("stream-pin-db").await;
        let callback: QueryStreamCallback = Arc::new(|_event| {});
        execute_driver_command_stream_impl(
            &test.state,
            ExecuteDriverCommandStreamRequest {
                db_session_id: Some(conn_id.clone()),
                command: "query_stream".into(),
                input: serde_json::json!({ "sql": "SELECT 1" }),
                database: Some("analytics".into()),
                apply_result_limit: Some(true),
                record_history: Some(false),
            },
            callback,
            ExecuteDriverCommandStreamOpts::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            test.mock.use_database_calls(),
            vec!["analytics".to_string()]
        );
        let config = test
            .state
            .connection_manager
            .get_session_config(&conn_id)
            .await
            .unwrap();
        assert_eq!(config.database.as_deref(), Some("analytics"));
    }

    #[tokio::test]
    async fn executes_query_command_through_ipc() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let (_, conn_id) = test.save_and_connect("cmd-exec").await;
        let result = execute_driver_command_impl(
            &test.state,
            ExecuteDriverCommandRequest {
                db_session_id: Some(conn_id.clone()),
                driver_type: None,
                command: "query".into(),
                input: serde_json::json!({ "sql": "SELECT 1" }),
                database: None,
            },
        )
        .await
        .unwrap();
        assert!(
            result.data.get("results").is_some()
                || result.data.get("columns").is_some()
                || result.data.is_object()
        );
        let history = test
            .state
            .store
            .get_query_history(10, None, None, None)
            .await;
        assert!(history.iter().any(|e| e.success));
    }

    #[tokio::test]
    async fn rejects_unknown_command() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let (_, conn_id) = test.save_and_connect("cmd-unknown").await;
        let err = execute_driver_command_impl(
            &test.state,
            ExecuteDriverCommandRequest {
                db_session_id: Some(conn_id),
                driver_type: None,
                command: "not-a-command".into(),
                input: serde_json::json!({}),
                database: None,
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
                db_session_id: Some(conn_id),
                driver_type: None,
                command: "query".into(),
                input: serde_json::json!({}),
                database: None,
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
                db_session_id: Some(conn_id),
                driver_type: None,
                command: "execute".into(),
                input: serde_json::json!({ "sql": "DELETE FROM t" }),
                database: None,
            },
            Some(crate::mcp::permission::McpPermissionMode::ReadOnly),
            None,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("not allowed"));
    }

    #[tokio::test]
    async fn discovers_commands_from_connection_id() {
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
                db_session_id: None,
                driver_type: Some("postgres".into()),
                command: "query".into(),
                input: serde_json::json!({ "sql": "SELECT 1" }),
                database: None,
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
            metadata: DriverCommandMetadata::new(
                CommandCategory::Observe,
                CommandAccessLevel::Read,
            )
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
                db_session_id: None,
                driver_type: Some("postgres".into()),
                command: "ping".into(),
                input: serde_json::json!({}),
                database: Some("ignored-for-unbound".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(result.data["command"], "ping");
        // Unbound driverType requests have no session to pin — the explicit
        // database must be ignored, not error.
        assert!(test.mock.use_database_calls().is_empty());
    }

    #[tokio::test]
    async fn save_dialog_commands_rejected_without_interactive_handle() {
        use datazen_driver_api::{
            CommandCategory, DriverCommandDefinition, DriverCommandMetadata, DriverSaveDialogSpec,
        };

        let pull = DriverCommandDefinition {
            id: "pull_payload".into(),
            name: "Pull Payload".into(),
            description: None,
            input_schema: serde_json::json!({ "type": "object" }),
            output_schema: None,
            permissions: vec![],
            metadata: DriverCommandMetadata::new(CommandCategory::Io, CommandAccessLevel::Write)
                .unbound()
                .hide_from_workflow()
                .save_dialog(DriverSaveDialogSpec {
                    file_name_field: "fileName".into(),
                    data_base64_field: "dataBase64".into(),
                    filter_name: "SQLite Database".into(),
                    extensions: vec!["db".into()],
                    result_path_field: "savedPath".into(),
                }),
        };
        let test = crate::testing::app_state::TestAppState::with_options(
            crate::testing::mock_driver::MockDriverOptions {
                extra_commands: vec![pull],
                ..Default::default()
            },
        )
        .await;

        // Internal reuse path (`execute_driver_command_impl`): no AppHandle →
        // the command must be rejected before any execution happens.
        let err = execute_driver_command_impl(
            &test.state,
            ExecuteDriverCommandRequest {
                db_session_id: None,
                driver_type: Some("postgres".into()),
                command: "pull_payload".into(),
                input: serde_json::json!({}),
                database: None,
            },
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string().contains("interactive session"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn safe_mode_blocks_update_without_where() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let (_, conn_id) = test.save_and_connect("cmd-safe").await;
        let err = execute_driver_command_impl(
            &test.state,
            ExecuteDriverCommandRequest {
                db_session_id: Some(conn_id),
                driver_type: None,
                command: "query".into(),
                input: serde_json::json!({ "sql": "UPDATE t SET x = 1" }),
                database: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("WHERE"));
    }

    #[tokio::test]
    async fn connection_read_only_blocks_write_sql() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let mut config = crate::testing::app_state::sample_postgres_config("cmd-conn-ro");
        config.read_only = true;
        test.store.save_connection(config).await.unwrap();
        let conn_id = test.connect_config("cmd-conn-ro").await;
        let err = execute_driver_command_impl(
            &test.state,
            ExecuteDriverCommandRequest {
                db_session_id: Some(conn_id),
                driver_type: None,
                command: "query".into(),
                input: serde_json::json!({ "sql": "UPDATE t SET x = 1 WHERE id = 1" }),
                database: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("read-only"));
    }

    #[tokio::test]
    async fn bind_params_are_substituted_before_execution() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let (_, conn_id) = test.save_and_connect("cmd-params").await;
        let result = execute_driver_command_impl(
            &test.state,
            ExecuteDriverCommandRequest {
                db_session_id: Some(conn_id),
                driver_type: None,
                command: "query".into(),
                input: serde_json::json!({
                    "sql": "SELECT * FROM t WHERE id = :id",
                    "params": { "id": 7 }
                }),
                database: None,
            },
        )
        .await
        .unwrap();
        assert!(result.data.is_object());
    }

    #[tokio::test]
    async fn disabling_safe_mode_allows_update_without_where() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let mut settings = test.store.get_settings().await;
        settings.safe_mode = false;
        test.store.save_settings(settings).await.unwrap();
        let (_, conn_id) = test.save_and_connect("cmd-unsafe").await;
        let result = execute_driver_command_impl(
            &test.state,
            ExecuteDriverCommandRequest {
                db_session_id: Some(conn_id),
                driver_type: None,
                command: "query".into(),
                input: serde_json::json!({ "sql": "UPDATE t SET x = 1" }),
                database: None,
            },
        )
        .await
        .unwrap();
        assert!(result.data.is_object());
    }
}

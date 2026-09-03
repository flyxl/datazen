use super::access::access_level_for_mode;
use super::helpers::{
    apply_query_result_limit, inject_sql_target_fields, nonempty, query_rows_affected,
    record_sql_command_outcome, sql_from_input,
};
use super::resolve::resolve_command_driver;
use super::super::error::{CmdExt, CommandError};
use super::super::query::ensure_session_database;
use super::super::AppState;
use super::types::ExecuteDriverCommandRequest;
use crate::mcp::permission::McpPermissionMode;
use datazen_driver_api::{
    check_command_access, validate_command_input, CommandResult, DriverSaveDialogSpec,
};

pub(crate) async fn execute_driver_command_impl(
    state: &AppState,
    request: ExecuteDriverCommandRequest,
) -> Result<CommandResult, CommandError> {
    // Internal reuse (query/schema IPCs, MCP tools): never shows a native
    // dialog — commands declaring a save dialog are rejected here.
    execute_driver_command_with_mode(state, request, None, None).await
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
    // F7: pass envelope targeting into the command input so drivers that
    // implement dialect-aware qualification (`qualify_sql_target`) can inline
    // the target. Drivers without the capability ignore the extra fields and
    // keep relying on the host session pin (ensure_session_database).
    if is_sql_command {
        inject_sql_target_fields(
            &mut request.input,
            request.database.as_deref(),
            request.schema.as_deref(),
        );
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
        tracing::debug!(
            sql_preview = %crate::log_redact::sql_preview_for_log(sql),
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

    // Central dialog gateway: webdriver builds may replace this save dialog
    // with an injected result (commands/dialog.rs).
    let saved = match super::super::dialog::save_file(
        app,
        (spec.filter_name.clone(), spec.extensions.clone()),
        file_name,
    )
    .await?
    {
        None => None,
        Some(path) => {
            super::super::file::validate_extension(&path, &ext_list)?;
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

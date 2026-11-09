//! 驱动命令执行编排（B2 域刀后的分层）。
//!
//! * [`execute_driver_command_impl`] —— **跨域共享入口**（保留，调用方清单
//!   见其文档注释）：内部复用路径（无 AppHandle、无权限档位），声明保存
//!   对话框的命令在此被拒。
//! * [`run_driver_command`] —— 域内私有编排核心：`execute_driver_command`
//!   命令（GUI，AppHandle 可用）与上面的共享入口共用同一份接线。原
//!   `pub(crate) fn execute_driver_command_with_mode` 转发 API 已收编于此，
//!   域外不可再直达。
//!
//! 编排里的形态中立规则（定义解析、调用校验、访问分级、SQL 参数绑定、
//! 信封目标注入）早已是 `datazen::execution::application::command_policy`
//! 的用例（错误文案的钉子测试在那边）；这里持有的是宿主接线：活会话解析
//! （resolve.rs）、执行设置读取、schema 缓存失效、历史记录、原生保存
//! 对话框（finish_save_dialog）。

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use super::helpers::{
    apply_query_result_limit, inject_sql_target_fields, nonempty, query_rows_affected,
    record_sql_command_outcome, sql_from_input,
};
use super::resolve::resolve_command_driver;
use super::types::ExecuteDriverCommandRequest;
use datazen::mcp::application::command_policy::access_level_for_mode;
use datazen::mcp::permission::McpPermissionMode;
use datazen_driver_api::{
    check_command_access, validate_command_input, CommandResult, DriverSaveDialogSpec,
};

/// 跨域共享入口（B2 driver_command 域刀保留）：内部复用路径的驱动命令执行
/// ——无 AppHandle、无权限档位；命令声明保存对话框时在此被拒（见
/// `command_policy::validate_invocation` 的冻结文案）。
///
/// 调用方（待其域刀把编排收编后可回收本入口）：
/// * 生产：`commands/query.rs`（execute_query）、`commands/schema.rs`
///   （run_schema_object_command）；
/// * 测试：`commands/export/live_acceptance_tests.rs`、`commands/schema/tests.rs`、
///   `platform/session_backend/tests.rs`、`services/schema_metadata/tests.rs`。
///
/// 不是命令转发体：GUI 命令 [`super::execute_driver_command`] 与它共享
/// 同一编排核心 [`run_driver_command`]，区别只在对话框句柄的有无。
pub(crate) async fn execute_driver_command_impl(
    state: &AppState,
    request: ExecuteDriverCommandRequest,
) -> Result<CommandResult, CommandError> {
    run_driver_command(state, request, None, None).await
}

/// 域内私有编排核心（原 `execute_driver_command_with_mode`，B2 域刀收编）：
/// 解析驱动 → 策略校验（连接要求 / 保存对话框交互要求）→ 限额与信封目标
/// 注入 → 输入校验与访问分级 → SQL 守卫（只读连接 / 安全模式 / 档位）→
/// 执行（元数据命令走 schema 缓存服务）→ 历史记录与缓存失效 →
/// （声明保存对话框且交互可用时）`finish_save_dialog`。
///
/// `permission_mode` 当前生产入口恒为 `None`（GUI 与内部复用都不带档位；
/// 带档位的 MCP / workflow 工具路径在 modules 侧自带管线，见
/// `mcp::application::tools::query`）。参数保留：域内测试用它钉
/// 「档位 → 访问分级 → 拒绝」的组合行为，未来内部复用若需要档位也不必
/// 再开新面。
///
/// 可见性是 `pub(super)`：域外（含测试宿主）不得直达，一律走命令入口或
/// 上面的跨域共享入口。
pub(super) async fn run_driver_command(
    state: &AppState,
    mut request: ExecuteDriverCommandRequest,
    permission_mode: Option<McpPermissionMode>,
    dialog: Option<&tauri::AppHandle>,
) -> Result<CommandResult, CommandError> {
    let resolved = resolve_command_driver(
        state,
        request.db_session_id.as_ref(),
        request.driver_type.as_ref(),
    )
    .await?;
    let driver = &resolved.driver;
    let handle = &resolved.handle;
    let bound = resolved.bound;

    let metadata_command = crate::services::schema_metadata::is_metadata_command(&request.command);
    let definition = datazen::mcp::application::command_policy::resolve_definition(
        driver.as_ref(),
        &request.command,
        request.database.is_some() || request.schema.is_some(),
    )
    .map_err(CommandError::Validation)?;
    datazen::mcp::application::command_policy::validate_invocation(
        &definition,
        bound,
        permission_mode,
        dialog.is_some(),
    )
    .map_err(CommandError::Validation)?;

    let is_sql_command = matches!(definition.id.as_str(), "query" | "execute");
    if definition.id == "query" {
        apply_query_result_limit(state, &mut request.input).await;
    }
    // F7: pass envelope targeting into the command input so drivers that
    // implement dialect-aware qualification (`qualify_sql_target`) inline the
    // target. Drivers without the capability ignore the extra fields; the host
    // never switches the session's database for them.
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
        datazen::mcp::application::command_policy::bind_sql_params(&mut request.input)
            .map_err(CommandError::Validation)?;
        if bound {
            if let Some(sql) = request.input.get("sql").and_then(|v| v.as_str()) {
                let read_only = if let Some(session) = resolved.session.as_ref() {
                    state
                        .store
                        .get_connection(session.connection_id.as_str())
                        .await
                        .map(|config| config.read_only)
                        .unwrap_or(false)
                } else {
                    false
                };
                let safe_mode = state.store.execution_settings().await.safe_mode;
                datazen::execution::domain::sql_guard::check_sql(sql, read_only, safe_mode)
                    .map_err(CommandError::Validation)?;
            }
        }
        if let Some(mode) = permission_mode {
            if let Some(sql) = request.input.get("sql").and_then(|v| v.as_str()) {
                datazen::mcp::permission::check_sql_allowed(sql, mode)
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
            sql_preview = %datazen_infra::redaction::sql_preview_for_log(sql),
            "execute_driver_command sql"
        );
    }

    let outcome = if metadata_command {
        let id = db_session_id.as_deref().ok_or_else(|| {
            CommandError::Validation("Metadata commands require a session".into())
        })?;
        crate::services::schema_metadata::execute(
            state,
            id,
            &driver,
            &handle,
            &request.command,
            request.input,
        )
        .await
    } else {
        driver
            .execute_command(&handle, &request.command, request.input)
            .await
            .map_err(CommandError::from)
    };
    match outcome {
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
                    if datazen::schema::sql_may_mutate_schema(sql) {
                        if let Some(id) = db_session_id.as_deref() {
                            state.schema_cache.clear_connection(id).await;
                        }
                    }
                }
            }
            // Generic host thin shell for metadata-declared save dialogs: no
            // driver-type branching — any driver opting in gets the same flow.
            if let Some(spec) = definition.metadata.save_dialog.as_ref() {
                let app = dialog.ok_or_else(|| {
                    CommandError::Internal("save dialog AppHandle missing after preflight".into())
                })?;
                return finish_save_dialog(app, spec, result).await;
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
            // Same write policy as every other save path (DBR-06): write to the
            // `.datazen-part` sibling and rename into place, so a failure keeps
            // a pre-existing file intact.
            super::super::file::write_all_via_part(&path, &bytes)
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

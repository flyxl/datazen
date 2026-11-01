//! 流式驱动命令编排（B2 域刀后的分层）。
//!
//! [`execute_driver_command_stream_impl`] 是**跨域共享入口**（保留）：
//! `commands/query_tests.rs` 的五个 `query_stream_command_*` 测试仍驱动它
//! （它们随 query 域刀登记为“跟随 driver_command 刀”，本刀保签名不动）；
//! `commands/query.rs` 的日志卫生守卫还逐字扫它的函数体。待那五个测试
//! 转真命令入口（需要可构造的 tauri Channel 测试面）后可回收。
//!
//! 形态中立的遥测聚合（受影响行数 / 总耗时的先累计后转发包装）已下沉
//! `datazen::execution::application::stream_telemetry`（B2 域刀）；事件
//! **发射**（tauri Channel）留在宿主命令壳 `execute_driver_command_stream`。

use std::sync::Arc;

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use super::helpers::{
    apply_query_result_limit, nonempty, query_result_limit_from_settings,
    record_sql_command_outcome, sql_from_input,
};
use super::resolve::resolve_command_driver;
use super::types::{ExecuteDriverCommandStreamOpts, ExecuteDriverCommandStreamRequest};
use datazen::execution::application::stream_telemetry::StreamTelemetry;
use datazen_driver_api::{
    check_command_access, validate_command_input, CommandAccessLevel, CommandCatalog,
    QueryExecutionId, QueryStreamCallback, QueryStreamEvent,
};

/// 跨域共享入口（B2 driver_command 域刀保留）：流式驱动命令编排。
///
/// 调用方（待其转真命令入口后可回收）：
/// * 测试：`commands/query_tests.rs`（五个 `query_stream_command_*`）；
/// * 源码守卫：`commands/query.rs` 的 `log_hygiene_tests` 逐字扫本函数体
///   （签名与 `sql_preview_for_log` 的 debug 位置都不得动）。
///
/// 不是命令转发体：宿主命令 `execute_driver_command_stream` 只做
/// Channel→回调端口的适配与开关默认值，编排本体在这里。
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

    let resolved = resolve_command_driver(state, request.db_session_id.as_ref(), None).await?;
    let driver = &resolved.driver;
    let handle = &resolved.handle;

    let catalog = datazen_driver_api::LegacyCommandCatalog::new(driver.as_ref());
    let definition = catalog
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

    // Presence-only validation: the actual string is re-read below after
    // `bind_sql_params` has mutated `request.input` in place.
    sql_from_input(&request.input).ok_or_else(|| {
        CommandError::Validation("command 'query_stream' requires string input 'sql'".into())
    })?;

    datazen::mcp::application::command_policy::bind_sql_params(&mut request.input)
        .map_err(CommandError::Validation)?;
    let mut sql = request
        .input
        .get("sql")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            CommandError::Validation("command 'query_stream' requires string input 'sql'".into())
        })?;

    // F7: give rewrite-capable drivers the chance to inline the SQL target
    // (dialect-qualified names, no session switch). Drivers without the
    // capability execute the SQL as-is.
    let target_database = nonempty(request.database.as_ref()).map(str::to_string);
    let target_schema = nonempty(request.schema.as_ref()).map(str::to_string);
    if target_database.is_some() || target_schema.is_some() {
        match driver.qualify_sql_target(&sql, target_database.as_deref(), target_schema.as_deref())
        {
            Some(qualified) => {
                if qualified != sql {
                    tracing::info!(
                        db_session_id,
                        "driver rewrote SQL target qualification for stream"
                    );
                }
                sql = qualified;
            }
            None => {
                tracing::debug!(
                    db_session_id,
                    database = target_database.as_deref().unwrap_or(""),
                    schema = target_schema.as_deref().unwrap_or(""),
                    "driver has no SQL target rewrite capability; streaming SQL as-is"
                );
            }
        }
    }

    tracing::info!(
        db_session_id,
        sql_len = sql.len(),
        "execute_driver_command_stream"
    );
    tracing::debug!(
        db_session_id,
        sql_preview = %datazen_infra::redaction::sql_preview_for_log(&sql),
        "execute_driver_command_stream sql"
    );

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
    datazen::execution::domain::sql_guard::check_sql(&sql, read_only, safe_mode)
        .map_err(CommandError::Validation)?;

    // Every stream gets a fresh opaque execution identity. The driver's
    // prepare hook registers a pending entry before the started event is
    // published, so cancel can safely win the race before a backend target is
    // acquired. Legacy drivers use the API's no-op/stream compatibility
    // defaults, but never their old session-wide cancel method.
    let execution_id = QueryExecutionId::new(uuid::Uuid::new_v4().to_string());
    if let Err(error) = driver.prepare_query_execution(&handle, &execution_id).await {
        let _ = driver.cleanup_query_execution(&handle, &execution_id).await;
        return Err(error).cmd_err("execute_driver_command_stream");
    }
    if let Err(error) = state
        .query_executions
        .register(execution_id.clone(), handle.id.clone())
        .await
    {
        let _ = driver.cleanup_query_execution(&handle, &execution_id).await;
        return Err(CommandError::Validation(error));
    }
    on_event(QueryStreamEvent::ExecutionStarted {
        execution_id: execution_id.as_str().to_string(),
    });

    let limit = if opts.apply_result_limit {
        query_result_limit_from_settings(state).await
    } else {
        None
    };
    // 形态中立的遥测包装（modules）：在把事件转交用户回调之前累计
    // `StatementEnd.rows_affected` 与 `Done.total_time_ms`，流结束后供
    // 历史记录读取。
    let telemetry = StreamTelemetry::new();
    let wrapped = telemetry.wrap(Arc::clone(&on_event));

    let stream_result = driver
        .query_stream_with_execution_at(
            &handle,
            &execution_id,
            &sql,
            limit,
            datazen_driver_api::SqlTarget::new(
                target_database.as_deref(),
                target_schema.as_deref(),
            ),
            wrapped,
        )
        .await;
    if let Err(error) = driver.cleanup_query_execution(&handle, &execution_id).await {
        tracing::warn!(
            db_session_id,
            execution_id = %execution_id.as_str(),
            error = %error,
            "query execution driver cleanup failed"
        );
    }
    state.query_executions.remove(&execution_id).await;

    match stream_result {
        Ok(()) => {
            if opts.record_history {
                record_sql_command_outcome(
                    state,
                    Some(db_session_id),
                    &sql,
                    true,
                    telemetry.total_time_ms(),
                    Some(telemetry.rows_affected()),
                    None,
                )
                .await;
            }
            if datazen::schema::sql_may_mutate_schema(&sql) {
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

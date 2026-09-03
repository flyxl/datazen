use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::super::error::{CmdExt, CommandError};
use super::super::query::ensure_session_database;
use super::super::AppState;
use super::helpers::{
    apply_query_result_limit, nonempty, query_result_limit_from_settings,
    record_sql_command_outcome, sql_from_input,
};
use super::resolve::resolve_command_driver;
use super::types::{ExecuteDriverCommandStreamOpts, ExecuteDriverCommandStreamRequest};
use datazen_driver_api::{
    check_command_access, validate_command_input, CommandAccessLevel, QueryExecutionId,
    QueryStreamCallback, QueryStreamEvent,
};

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
    // capability execute the SQL as-is and the ensure_session_database pin
    // above remains the fallback for the database dimension.
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
        sql_preview = %crate::log_redact::sql_preview_for_log(&sql),
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

    let stream_result = driver
        .query_stream_with_execution(&handle, &execution_id, &sql, limit, wrapped)
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

use super::super::AppState;
use datazen_driver_api::ConnectionHandle;

pub(crate) fn unbound_handle() -> ConnectionHandle {
    ConnectionHandle {
        id: String::new(),
        pool_id: String::new(),
    }
}

pub(crate) fn nonempty(value: Option<&String>) -> Option<&str> {
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

pub(crate) async fn apply_query_result_limit(state: &AppState, input: &mut serde_json::Value) {
    if input.get("limit").is_some() {
        return;
    }
    if let Some(limit) = query_result_limit_from_settings(state).await {
        input["limit"] = serde_json::json!(limit);
    }
}

pub(crate) fn sql_from_input(input: &serde_json::Value) -> Option<String> {
    input
        .get("sql")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// F7: copy non-blank envelope targeting fields into the command input object
/// (no-op for blank values or non-object inputs).
pub(crate) fn inject_sql_target_fields(
    input: &mut serde_json::Value,
    database: Option<&str>,
    schema: Option<&str>,
) {
    let Some(map) = input.as_object_mut() else {
        return;
    };
    let database = database.map(str::trim).filter(|s| !s.is_empty());
    let schema = schema.map(str::trim).filter(|s| !s.is_empty());
    if let Some(database) = database {
        map.insert(
            "database".into(),
            serde_json::Value::String(database.into()),
        );
    }
    if let Some(schema) = schema {
        map.insert("schema".into(), serde_json::Value::String(schema.into()));
    }
}

/// Persist a query/execute outcome to `{appData}/history.sqlite`.
///
/// The full SQL string is written **in plaintext** (see `history_db` module
/// docs). Logging paths use `log_redact`; history storage does not yet.
pub(crate) async fn record_sql_command_outcome(
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

pub(crate) fn query_rows_affected(data: &serde_json::Value) -> Option<u64> {
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

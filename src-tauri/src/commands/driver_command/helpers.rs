use super::super::AppState;
use datazen_driver_api::ConnectionHandle;
pub(crate) use datazen_feature_core::services::command_policy::{
    inject_sql_target_fields, nonempty, query_rows_affected, sql_from_input,
};

pub(crate) fn unbound_handle() -> ConnectionHandle {
    ConnectionHandle {
        id: String::new(),
        pool_id: String::new(),
    }
}

/// SQL SELECT row cap from settings. `None` when the "limit SELECT results"
/// switch is off — streaming must not invent a cap from batch size.
pub(crate) async fn query_result_limit_from_settings(state: &AppState) -> Option<u32> {
    state.store.execution_settings().await.result_limit()
}

pub(crate) async fn apply_query_result_limit(state: &AppState, input: &mut serde_json::Value) {
    state
        .store
        .execution_settings()
        .await
        .apply_result_limit(input);
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

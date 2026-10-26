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
    let Some(adapter) = state.platform_require().ok() else {
        tracing::warn!(
            db_session_id,
            "Skipping history: runtime unavailable for dbSessionId"
        );
        return;
    };
    let Ok(runtime) = adapter.runtime() else {
        tracing::warn!(db_session_id, "Skipping history: runtime unavailable");
        return;
    };
    let Some(connection_id) = runtime.connection_id_for_session(
        &datazen_runtime::connection::DbSessionId::new(db_session_id),
    ) else {
        tracing::warn!(db_session_id, "Skipping history: no owning connectionId");
        return;
    };
    // Record the session-active logical database so history can be grouped /
    // filtered per panel context (empty string when the driver is single-db).
    let database = state
        .store
        .get_connection(connection_id.as_str())
        .await
        .and_then(|config| config.database)
        .unwrap_or_default();
    let entry = crate::store::QueryHistoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        connection_id: connection_id.as_str().to_owned(),
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
    // D6 item 1：写入经模块服务（服务内部同样按告警降级，不向上抛错）。
    match crate::platform::ipc::require_history_service(state) {
        Ok(service) => service.add_query_history(entry).await,
        Err(error) => {
            tracing::warn!(%error, "add_query_history failed: history service not injected")
        }
    }
}

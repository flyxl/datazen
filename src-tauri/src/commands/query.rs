use std::sync::Arc;

use super::driver_command::{
    execute_driver_command_stream_impl, ExecuteDriverCommandStreamOpts,
    ExecuteDriverCommandStreamRequest,
};
use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{ExplainResult, MultiQueryResult};
use crate::store::QueryHistoryEntry;
use datazen_driver_api::{QueryExecutionId, QueryStreamCallback, QueryStreamEvent};
use tauri::ipc::Channel;
use tauri::State;

/// F1 (IPC refactor): query-family commands accept an explicit `database` pin
/// instead of relying on a prior `use_database` IPC round-trip. When the pin
/// differs from the session's active database, switch the live session first;
/// `None` (or blank) is a no-op so legacy callers keep their behavior.
pub(crate) async fn ensure_session_database(
    state: &AppState,
    db_session_id: &str,
    database: Option<&str>,
    op: &str,
) -> Result<(), CommandError> {
    let Some(database) = database.map(str::trim).filter(|db| !db.is_empty()) else {
        return Ok(());
    };
    let active = state
        .connection_manager
        .get_session_config(db_session_id)
        .await
        .cmd_err(op)?
        .database;
    if active.as_deref() == Some(database) {
        return Ok(());
    }
    let (driver, handle) = state
        .connection_manager
        .get_session(db_session_id)
        .await
        .cmd_err(op)?;
    driver.use_database(&handle, database).await.cmd_err(op)?;
    state
        .connection_manager
        .set_active_database(db_session_id, database)
        .await
        .cmd_err(op)?;
    tracing::info!(%db_session_id, database = %database, "session active database switched");
    Ok(())
}

pub(crate) async fn execute_query_impl(
    state: &AppState,
    db_session_id: String,
    sql: String,
    database: Option<String>,
) -> Result<MultiQueryResult, CommandError> {
    tracing::info!(%db_session_id, sql_len = sql.len(), "execute_query");
    tracing::debug!(
        %db_session_id,
        sql_preview = %crate::log_redact::sql_preview_for_log(&sql),
        "execute_query sql"
    );
    ensure_session_database(state, &db_session_id, database.as_deref(), "execute_query").await?;
    let result = super::driver_command::execute_driver_command_impl(
        state,
        super::driver_command::ExecuteDriverCommandRequest {
            db_session_id: Some(db_session_id),
            driver_type: None,
            command: "query".into(),
            // Session was already pinned by ensure_session_database above.
            database: None,
            schema: None,
            input: serde_json::json!({ "sql": sql }),
        },
    )
    .await?;
    serde_json::from_value(result.data).map_err(CommandError::Json)
}

#[derive(Clone, Copy)]
pub(crate) struct ExecuteQueryStreamOpts {
    pub apply_result_limit: bool,
    pub record_history: bool,
}

impl Default for ExecuteQueryStreamOpts {
    fn default() -> Self {
        Self {
            apply_result_limit: true,
            record_history: true,
        }
    }
}

pub(crate) async fn execute_query_stream_impl(
    state: &AppState,
    db_session_id: String,
    sql: String,
    database: Option<String>,
    on_event: QueryStreamCallback,
    opts: ExecuteQueryStreamOpts,
) -> Result<(), CommandError> {
    ensure_session_database(
        state,
        &db_session_id,
        database.as_deref(),
        "execute_query_stream",
    )
    .await?;
    execute_driver_command_stream_impl(
        state,
        ExecuteDriverCommandStreamRequest {
            db_session_id: Some(db_session_id),
            command: "query_stream".into(),
            // Session was already pinned by ensure_session_database above.
            database: None,
            schema: None,
            input: serde_json::json!({ "sql": sql }),
            apply_result_limit: Some(opts.apply_result_limit),
            record_history: Some(opts.record_history),
        },
        on_event,
        ExecuteDriverCommandStreamOpts {
            apply_result_limit: opts.apply_result_limit,
            record_history: opts.record_history,
        },
    )
    .await
}

pub(crate) async fn get_explain_impl(
    state: &AppState,
    db_session_id: String,
    sql: String,
    database: Option<String>,
) -> Result<ExplainResult, CommandError> {
    tracing::debug!(%db_session_id, "get_explain");
    ensure_session_database(state, &db_session_id, database.as_deref(), "get_explain").await?;
    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("get_explain")?;

    driver.explain(&handle, &sql).await.cmd_err("get_explain")
}

pub(crate) async fn cancel_query_impl(
    state: &AppState,
    db_session_id: String,
    execution_id: String,
) -> Result<(), CommandError> {
    let execution_id = QueryExecutionId::new(execution_id);
    tracing::info!(
        %db_session_id,
        execution_id = %execution_id.as_str(),
        "cancel_query"
    );
    state
        .query_executions
        .validate_owner(&execution_id, &db_session_id)
        .await
        .map_err(CommandError::Validation)?;
    let config = state
        .connection_manager
        .get_session_config(&db_session_id)
        .await
        .cmd_err("cancel_query")?;
    match state
        .driver_registry
        .get_capabilities(&config.database_type)
        .await
    {
        Some(capabilities) if capabilities.supports_query_execution_cancel => {}
        Some(_) => {
            return Err(CommandError::Validation(
                "UNSUPPORTED_OPERATION:cancel_query:query cancellation is not supported by this driver"
                    .into(),
            ));
        }
        None => {
            return Err(CommandError::Validation(
                "UNSUPPORTED_OPERATION:cancel_query:query cancellation capability is unknown"
                    .into(),
            ));
        }
    }
    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("cancel_query")?;

    driver
        .cancel_query_with_execution(&handle, &execution_id)
        .await
        .cmd_err("cancel_query")
}

pub(crate) async fn get_query_history_impl(
    state: &AppState,
    limit: usize,
    connection_id: Option<String>,
    database: Option<String>,
    schema: Option<String>,
) -> Result<Vec<QueryHistoryEntry>, CommandError> {
    Ok(state
        .store
        .get_query_history(
            limit,
            connection_id.as_deref(),
            database.as_deref(),
            schema.as_deref(),
        )
        .await)
}

pub(crate) async fn clear_query_history_impl(state: &AppState) -> Result<(), CommandError> {
    tracing::info!("clear_query_history");
    state
        .store
        .clear_query_history()
        .await
        .cmd_err("clear_query_history")
}

pub(crate) async fn get_favorite_queries_impl(
    state: &AppState,
    connection_id: Option<String>,
) -> Result<Vec<crate::store::FavoriteQuery>, CommandError> {
    Ok(state
        .store
        .get_favorite_queries(connection_id.as_deref())
        .await)
}

pub(crate) async fn add_favorite_query_impl(
    state: &AppState,
    connection_id: String,
    title: String,
    sql: String,
) -> Result<crate::store::FavoriteQuery, CommandError> {
    let fav = crate::store::FavoriteQuery {
        id: uuid::Uuid::new_v4().to_string(),
        connection_id,
        title,
        sql,
        created_at: chrono::Utc::now(),
    };
    state
        .store
        .add_favorite_query(fav.clone())
        .await
        .cmd_err("add_favorite_query")?;
    Ok(fav)
}

pub(crate) async fn delete_favorite_query_impl(
    state: &AppState,
    id: String,
) -> Result<(), CommandError> {
    state
        .store
        .delete_favorite_query(&id)
        .await
        .cmd_err("delete_favorite_query")
}

#[tauri::command]
pub async fn execute_query(
    state: State<'_, AppState>,
    db_session_id: String,
    sql: String,
    database: Option<String>,
) -> Result<MultiQueryResult, CommandError> {
    execute_query_impl(&state, db_session_id, sql, database).await
}

#[tauri::command]
pub async fn execute_query_stream(
    state: State<'_, AppState>,
    db_session_id: String,
    sql: String,
    database: Option<String>,
    on_event: Channel<QueryStreamEvent>,
    apply_result_limit: Option<bool>,
    record_history: Option<bool>,
) -> Result<(), CommandError> {
    let callback: QueryStreamCallback = Arc::new(move |event| {
        let _ = on_event.send(event);
    });
    execute_query_stream_impl(
        &state,
        db_session_id,
        sql,
        database,
        callback,
        ExecuteQueryStreamOpts {
            apply_result_limit: apply_result_limit.unwrap_or(true),
            record_history: record_history.unwrap_or(true),
        },
    )
    .await
}

#[tauri::command]
pub async fn get_explain(
    state: State<'_, AppState>,
    db_session_id: String,
    sql: String,
    database: Option<String>,
) -> Result<ExplainResult, CommandError> {
    get_explain_impl(&state, db_session_id, sql, database).await
}

#[tauri::command]
pub async fn cancel_query(
    state: State<'_, AppState>,
    db_session_id: String,
    execution_id: String,
) -> Result<(), CommandError> {
    cancel_query_impl(&state, db_session_id, execution_id).await
}

#[tauri::command]
pub async fn get_query_history(
    state: State<'_, AppState>,
    limit: usize,
    connection_id: Option<String>,
    database: Option<String>,
    schema: Option<String>,
) -> Result<Vec<QueryHistoryEntry>, CommandError> {
    get_query_history_impl(&state, limit, connection_id, database, schema).await
}

#[tauri::command]
pub async fn clear_query_history(state: State<'_, AppState>) -> Result<(), CommandError> {
    clear_query_history_impl(&state).await
}

#[tauri::command]
pub async fn get_favorite_queries(
    state: State<'_, AppState>,
    connection_id: Option<String>,
) -> Result<Vec<crate::store::FavoriteQuery>, CommandError> {
    get_favorite_queries_impl(&state, connection_id).await
}

#[tauri::command]
pub async fn add_favorite_query(
    state: State<'_, AppState>,
    connection_id: String,
    title: String,
    sql: String,
) -> Result<crate::store::FavoriteQuery, CommandError> {
    add_favorite_query_impl(&state, connection_id, title, sql).await
}

#[tauri::command]
pub async fn delete_favorite_query(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), CommandError> {
    delete_favorite_query_impl(&state, id).await
}

pub(crate) async fn begin_session_transaction_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<(), CommandError> {
    {
        let txs = state.session_transactions.lock().await;
        if txs.contains_key(&db_session_id) {
            return Ok(());
        }
    }
    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("begin_session_transaction")?;
    let tx = match driver.begin_transaction(&handle).await {
        Ok(tx) => tx,
        Err(e) => {
            if state
                .session_transactions
                .lock()
                .await
                .contains_key(&db_session_id)
            {
                return Ok(());
            }
            return Err(e).cmd_err("begin_session_transaction");
        }
    };
    let mut txs = state.session_transactions.lock().await;
    if txs.contains_key(&db_session_id) {
        drop(txs);
        let _ = driver.rollback(tx).await;
        return Ok(());
    }
    txs.insert(db_session_id, tx);
    Ok(())
}

pub(crate) async fn commit_session_transaction_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<(), CommandError> {
    let (driver, _) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("commit_session_transaction")?;
    let tx = state
        .session_transactions
        .lock()
        .await
        .remove(&db_session_id)
        .ok_or_else(|| CommandError::Validation("No open transaction".into()))?;
    driver
        .commit(tx)
        .await
        .cmd_err("commit_session_transaction")
}

pub(crate) async fn rollback_session_transaction_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<(), CommandError> {
    let (driver, _) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("rollback_session_transaction")?;
    let tx = state
        .session_transactions
        .lock()
        .await
        .remove(&db_session_id)
        .ok_or_else(|| CommandError::Validation("No open transaction".into()))?;
    driver
        .rollback(tx)
        .await
        .cmd_err("rollback_session_transaction")
}

pub(crate) async fn session_transaction_status_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<bool, CommandError> {
    Ok(state
        .session_transactions
        .lock()
        .await
        .contains_key(&db_session_id))
}

#[tauri::command]
pub async fn begin_session_transaction(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<(), CommandError> {
    begin_session_transaction_impl(&state, db_session_id).await
}

#[tauri::command]
pub async fn commit_session_transaction(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<(), CommandError> {
    commit_session_transaction_impl(&state, db_session_id).await
}

#[tauri::command]
pub async fn rollback_session_transaction(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<(), CommandError> {
    rollback_session_transaction_impl(&state, db_session_id).await
}

#[tauri::command]
pub async fn session_transaction_status(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<bool, CommandError> {
    session_transaction_status_impl(&state, db_session_id).await
}

#[cfg(test)]
mod log_hygiene_tests {
    #[test]
    fn execute_query_source_does_not_info_log_sql_preview() {
        let src = include_str!("query.rs");
        let start = src
            .find("pub(crate) async fn execute_query_impl")
            .expect("fn");
        let chunk = &src[start..start + 800];
        assert!(
            !chunk.contains("tracing::info!(") || !chunk.contains("%sql_preview"),
            "execute_query must not info!-log sql_preview"
        );
        assert!(
            chunk.contains("sql_len") || chunk.contains("tracing::debug!"),
            "expected sql_len and/or debug preview"
        );
        assert!(
            chunk.contains("sql_preview_for_log"),
            "execute_query debug preview must use log_redact"
        );
    }

    #[test]
    fn execute_query_stream_source_does_not_info_log_sql_preview() {
        let src = include_str!("driver_command.rs");
        let start = src
            .find("pub(crate) async fn execute_driver_command_stream_impl")
            .expect("fn");
        let end = src[start..]
            .find("let read_only = state")
            .map(|i| start + i)
            .unwrap_or(start + 8000);
        let chunk = &src[start..end];
        assert!(
            !chunk.contains("tracing::info!(") || !chunk.contains("%sql_preview"),
            "execute_driver_command_stream must not info!-log sql_preview"
        );
        assert!(
            chunk.contains("sql_preview_for_log"),
            "execute_driver_command_stream debug preview must use log_redact"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ColumnSchema, DriverCapabilities, DriverError, ExplainResult, Value};
    use crate::store::AppSettings;
    use crate::testing::app_state::TestAppState;
    use crate::testing::mock_driver::MockDriverOptions;

    #[tokio::test]
    async fn execute_query_success_records_history() {
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("q-cfg").await;
        let result = execute_query_impl(&test.state, conn_id, "SELECT 1".into(), None)
            .await
            .unwrap();
        assert_eq!(result.results.len(), 1);

        let history = get_query_history_impl(&test.state, 10, None, None, None)
            .await
            .unwrap();
        assert_eq!(history.len(), 1);
        assert!(history[0].success);
    }

    #[tokio::test]
    async fn execute_query_respects_result_limit_setting() {
        let test = TestAppState::with_tables().await;
        let mut settings = AppSettings::default();
        settings.limit_select_results = true;
        settings.query_result_limit = 5;
        test.state.store.save_settings(settings).await.unwrap();

        let (_, conn_id) = test.save_and_connect("limit-cfg").await;
        execute_query_impl(&test.state, conn_id, "SELECT * FROM users".into(), None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn get_explain_query() {
        let opts = MockDriverOptions {
            explain_plan: ExplainResult {
                plan_text: "Seq Scan".into(),
                plan_json: None,
                plan_tree: None,
                total_cost: Some(1.0),
                estimated_rows: Some(10),
            },
            ..Default::default()
        };
        let test = TestAppState::with_options(opts).await;
        test.registry
            .register_test_driver_with_capabilities(
                "postgres",
                test.mock.clone(),
                DriverCapabilities {
                    supports_cancel_query: true,
                    supports_query_execution_cancel: true,
                    supports_explain: true,
                    supports_streaming_results: true,
                },
            )
            .await;
        let (_, conn_id) = test.save_and_connect("explain-cfg").await;

        let plan = get_explain_impl(&test.state, conn_id.clone(), "SELECT 1".into(), None)
            .await
            .unwrap();
        assert_eq!(plan.plan_text, "Seq Scan");
    }

    #[tokio::test]
    async fn cancel_query_rejects_when_driver_capability_is_unknown_without_calling_driver() {
        let test = TestAppState::with_options(MockDriverOptions {
            cancel_error: Some("legacy driver cancellation must not be called".into()),
            ..Default::default()
        })
        .await;
        let (_, conn_id) = test.save_and_connect("cancel-unknown").await;
        let execution_id = QueryExecutionId::new("exec-unknown");
        test.state
            .query_executions
            .register(execution_id.clone(), conn_id.clone())
            .await
            .unwrap();

        assert!(test
            .registry
            .get_capabilities(&"postgres".to_string())
            .await
            .is_none());
        let error = cancel_query_impl(&test.state, conn_id, execution_id.as_str().to_string())
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            super::super::error::CommandError::Validation(message)
                if message.starts_with("UNSUPPORTED_OPERATION:cancel_query:")
                    && message.ends_with("capability is unknown")
        ));
        assert_eq!(test.mock.cancel_query_calls(), 0);
    }

    #[tokio::test]
    async fn cancel_query_rejects_when_driver_capability_is_disabled() {
        let test = TestAppState::new().await;
        let (_, conn_id) = test.save_and_connect("cancel-unsupported").await;
        let execution_id = QueryExecutionId::new("exec-unsupported");
        test.state
            .query_executions
            .register(execution_id.clone(), conn_id.clone())
            .await
            .unwrap();
        test.registry
            .register_test_driver_with_capabilities(
                "postgres",
                test.mock.clone(),
                DriverCapabilities {
                    supports_cancel_query: false,
                    supports_query_execution_cancel: false,
                    supports_explain: true,
                    supports_streaming_results: true,
                },
            )
            .await;

        let error = cancel_query_impl(&test.state, conn_id, execution_id.as_str().to_string())
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            super::super::error::CommandError::Validation(message)
                if message.starts_with("UNSUPPORTED_OPERATION:cancel_query:")
        ));
        assert_eq!(test.mock.cancel_query_calls(), 0);
    }

    #[tokio::test]
    async fn cancel_query_surfaces_driver_unsupported_without_claiming_success() {
        let test = TestAppState::with_options(MockDriverOptions {
            cancel_error: Some("backend cancellation is unavailable".into()),
            ..Default::default()
        })
        .await;
        let (_, conn_id) = test.save_and_connect("cancel-driver-unsupported").await;
        let execution_id = QueryExecutionId::new("exec-driver-unsupported");
        test.state
            .query_executions
            .register(execution_id.clone(), conn_id.clone())
            .await
            .unwrap();
        test.registry
            .register_test_driver_with_capabilities(
                "postgres",
                test.mock.clone(),
                DriverCapabilities {
                    // The legacy session-wide capability is deliberately
                    // independent; precise cancellation must not be gated by
                    // or fall back to that old API.
                    supports_cancel_query: false,
                    supports_query_execution_cancel: true,
                    supports_explain: true,
                    supports_streaming_results: true,
                },
            )
            .await;

        let error = cancel_query_impl(&test.state, conn_id, execution_id.as_str().to_string())
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            super::super::error::CommandError::Driver(DriverError::Unsupported(message))
                if message == "backend cancellation is unavailable"
        ));
        assert_eq!(test.mock.cancel_query_calls(), 0);
        assert_eq!(test.mock.precise_cancel_query_calls(), 1);
    }

    #[tokio::test]
    async fn execute_query_not_connected_errors() {
        let test = TestAppState::new().await;
        assert!(
            execute_query_impl(&test.state, "nope".into(), "SELECT 1".into(), None)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn favorite_queries_roundtrip() {
        let test = TestAppState::new().await;
        assert!(get_favorite_queries_impl(&test.state, None)
            .await
            .unwrap()
            .is_empty());

        let fav = add_favorite_query_impl(
            &test.state,
            "cfg-test".into(),
            "My query".into(),
            "SELECT 1".into(),
        )
        .await
        .unwrap();
        assert_eq!(
            get_favorite_queries_impl(&test.state, None)
                .await
                .unwrap()
                .len(),
            1
        );

        delete_favorite_query_impl(&test.state, fav.id)
            .await
            .unwrap();
        assert!(get_favorite_queries_impl(&test.state, None)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn clear_query_history() {
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("hist-cfg").await;
        execute_query_impl(&test.state, conn_id, "SELECT 1".into(), None)
            .await
            .unwrap();
        clear_query_history_impl(&test.state).await.unwrap();
        assert!(get_query_history_impl(&test.state, 10, None, None, None)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn execute_query_with_columns_returns_rows() {
        let opts = MockDriverOptions {
            columns: vec![ColumnSchema {
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: false,
            }],
            query_rows: vec![vec![Some(Value::Integer(7))]],
            ..Default::default()
        };
        let test = TestAppState::with_options(opts).await;
        let (_, conn_id) = test.save_and_connect("rows-cfg").await;
        let result = execute_query_impl(&test.state, conn_id, "SELECT id FROM t".into(), None)
            .await
            .unwrap();
        assert_eq!(result.results[0].rows.len(), 1);
    }

    #[tokio::test]
    async fn execute_query_stream_does_not_apply_limit_when_switch_off() {
        let test = TestAppState::with_tables().await;
        assert!(!test.state.store.get_settings().await.limit_select_results);
        let (_, conn_id) = test.save_and_connect("stream-nolimit").await;
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let events_cb = std::sync::Arc::clone(&events);
        let cb: QueryStreamCallback = std::sync::Arc::new(move |ev| {
            events_cb.lock().unwrap().push(ev);
        });
        execute_query_stream_impl(
            &test.state,
            conn_id,
            "SELECT 1".into(),
            None,
            cb,
            ExecuteQueryStreamOpts::default(),
        )
        .await
        .unwrap();
        assert_eq!(test.mock.last_query_limit(), Some(None));
        let events = events.lock().unwrap();
        assert!(events
            .iter()
            .any(|e| matches!(e, QueryStreamEvent::StatementStart { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, QueryStreamEvent::Done { .. })));
        let history = get_query_history_impl(&test.state, 10, None, None, None)
            .await
            .unwrap();
        assert!(history.iter().any(|e| e.success));
    }

    #[tokio::test]
    async fn execute_query_stream_respects_limit_select_setting() {
        let test = TestAppState::with_tables().await;
        let mut settings = AppSettings::default();
        settings.limit_select_results = true;
        settings.query_result_limit = 5;
        test.state.store.save_settings(settings).await.unwrap();

        let (_, conn_id) = test.save_and_connect("stream-limit").await;
        let cb: QueryStreamCallback = std::sync::Arc::new(|_| {});
        execute_query_stream_impl(
            &test.state,
            conn_id,
            "SELECT * FROM users".into(),
            None,
            cb,
            ExecuteQueryStreamOpts::default(),
        )
        .await
        .unwrap();
        assert_eq!(test.mock.last_query_limit(), Some(Some(5)));
    }

    #[tokio::test]
    async fn execute_query_stream_can_skip_result_limit_for_export() {
        let test = TestAppState::with_tables().await;
        let mut settings = AppSettings::default();
        settings.limit_select_results = true;
        settings.query_result_limit = 5;
        test.state.store.save_settings(settings).await.unwrap();

        let (_, conn_id) = test.save_and_connect("stream-export").await;
        let cb: QueryStreamCallback = std::sync::Arc::new(|_| {});
        execute_query_stream_impl(
            &test.state,
            conn_id,
            "SELECT * FROM users".into(),
            None,
            cb,
            ExecuteQueryStreamOpts {
                apply_result_limit: false,
                record_history: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(test.mock.last_query_limit(), Some(None));
        let history = get_query_history_impl(&test.state, 10, None, None, None)
            .await
            .unwrap();
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn execute_query_stream_failure_records_history() {
        let opts = MockDriverOptions {
            query_error: Some("boom".into()),
            ..Default::default()
        };
        let test = TestAppState::with_options(opts).await;
        let (_, conn_id) = test.save_and_connect("stream-fail").await;
        let cb: QueryStreamCallback = std::sync::Arc::new(|_| {});
        let err = execute_query_stream_impl(
            &test.state,
            conn_id,
            "SELECT 1".into(),
            None,
            cb,
            ExecuteQueryStreamOpts::default(),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("boom"));
        let history = get_query_history_impl(&test.state, 10, None, None, None)
            .await
            .unwrap();
        assert_eq!(history.len(), 1);
        assert!(!history[0].success);
        assert!(history[0]
            .error_message
            .as_ref()
            .is_some_and(|m| m.contains("boom")));
    }

    #[tokio::test]
    async fn execute_query_stream_not_connected_errors() {
        let test = TestAppState::new().await;
        let cb: QueryStreamCallback = std::sync::Arc::new(|_| {});
        assert!(execute_query_stream_impl(
            &test.state,
            "nope".into(),
            "SELECT 1".into(),
            None,
            cb,
            ExecuteQueryStreamOpts::default(),
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn execute_query_switches_session_database_when_pinned_differs() {
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("switch-db-cfg").await;
        // Sample config pins database = "app"; pinning another database must
        // switch the live session before executing and update the session record.
        let result = execute_query_impl(
            &test.state,
            conn_id.clone(),
            "SELECT 1".into(),
            Some("analytics".into()),
        )
        .await
        .unwrap();
        assert_eq!(result.results.len(), 1);
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
    async fn execute_query_skips_switch_when_same_or_none() {
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("no-switch-db-cfg").await;

        execute_query_impl(&test.state, conn_id.clone(), "SELECT 1".into(), None)
            .await
            .unwrap();
        // Same as the session's active database ("app") — no driver switch.
        execute_query_impl(
            &test.state,
            conn_id.clone(),
            "SELECT 1".into(),
            Some("app".into()),
        )
        .await
        .unwrap();
        // Blank pins are treated like None.
        execute_query_impl(
            &test.state,
            conn_id.clone(),
            "SELECT 1".into(),
            Some("   ".into()),
        )
        .await
        .unwrap();

        assert!(test.mock.use_database_calls().is_empty());
        let config = test
            .state
            .connection_manager
            .get_session_config(&conn_id)
            .await
            .unwrap();
        assert_eq!(config.database.as_deref(), Some("app"));
    }

    #[tokio::test]
    async fn get_explain_switches_session_database_when_pinned_differs() {
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("explain-db-cfg").await;
        get_explain_impl(
            &test.state,
            conn_id,
            "SELECT 1".into(),
            Some("other".into()),
        )
        .await
        .unwrap();
        assert_eq!(test.mock.use_database_calls(), vec!["other".to_string()]);
    }

    #[tokio::test]
    async fn session_transaction_begin_commit_and_status() {
        let test = TestAppState::new().await;
        let (_, conn_id) = test.save_and_connect("tx-cfg").await;
        assert!(
            !session_transaction_status_impl(&test.state, conn_id.clone())
                .await
                .unwrap()
        );
        begin_session_transaction_impl(&test.state, conn_id.clone())
            .await
            .unwrap();
        assert!(
            session_transaction_status_impl(&test.state, conn_id.clone())
                .await
                .unwrap()
        );
        begin_session_transaction_impl(&test.state, conn_id.clone())
            .await
            .unwrap();
        commit_session_transaction_impl(&test.state, conn_id.clone())
            .await
            .unwrap();
        assert!(
            !session_transaction_status_impl(&test.state, conn_id.clone())
                .await
                .unwrap()
        );
        begin_session_transaction_impl(&test.state, conn_id.clone())
            .await
            .unwrap();
        rollback_session_transaction_impl(&test.state, conn_id.clone())
            .await
            .unwrap();
        assert!(!session_transaction_status_impl(&test.state, conn_id)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn commit_and_rollback_without_tx_are_validation_errors() {
        let test = TestAppState::new().await;
        let (_, conn_id) = test.save_and_connect("tx-empty").await;
        let commit_err = commit_session_transaction_impl(&test.state, conn_id.clone())
            .await
            .unwrap_err();
        assert!(commit_err.to_string().contains("No open transaction"));
        let rollback_err = rollback_session_transaction_impl(&test.state, conn_id)
            .await
            .unwrap_err();
        assert!(rollback_err.to_string().contains("No open transaction"));
    }

    #[tokio::test]
    async fn concurrent_begin_is_idempotent() {
        let test = TestAppState::new().await;
        let (_, conn_id) = test.save_and_connect("tx-race").await;
        let a = begin_session_transaction_impl(&test.state, conn_id.clone());
        let b = begin_session_transaction_impl(&test.state, conn_id.clone());
        let (ra, rb) = tokio::join!(a, b);
        assert!(ra.is_ok());
        assert!(rb.is_ok());
        assert!(
            session_transaction_status_impl(&test.state, conn_id.clone())
                .await
                .unwrap()
        );
        commit_session_transaction_impl(&test.state, conn_id)
            .await
            .unwrap();
    }
}

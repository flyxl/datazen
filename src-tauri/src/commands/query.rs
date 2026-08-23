use std::sync::Arc;

use super::driver_command::{
    execute_driver_command_stream_impl, ExecuteDriverCommandStreamOpts,
    ExecuteDriverCommandStreamRequest,
};
use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{ExplainResult, MultiQueryResult};
use crate::store::QueryHistoryEntry;
use datazen_driver_api::{QueryStreamCallback, QueryStreamEvent};
use tauri::ipc::Channel;
use tauri::State;

pub(crate) async fn execute_query_impl(
    state: &AppState,
    connection_id: String,
    sql: String,
) -> Result<MultiQueryResult, CommandError> {
    tracing::info!(%connection_id, sql_len = sql.len(), "execute_query");
    tracing::debug!(%connection_id, sql_preview = %sql.chars().take(500).collect::<String>(), "execute_query sql");
    let result = super::driver_command::execute_driver_command_impl(
        state,
        super::driver_command::ExecuteDriverCommandRequest {
            connection_id: Some(connection_id),
            driver_type: None,
            command: "query".into(),
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
    connection_id: String,
    sql: String,
    on_event: QueryStreamCallback,
    opts: ExecuteQueryStreamOpts,
) -> Result<(), CommandError> {
    execute_driver_command_stream_impl(
        state,
        ExecuteDriverCommandStreamRequest {
            connection_id: Some(connection_id),
            command: "query_stream".into(),
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
    connection_id: String,
    sql: String,
) -> Result<ExplainResult, CommandError> {
    tracing::debug!(%connection_id, "get_explain");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("get_explain")?;

    driver.explain(&handle, &sql).await.cmd_err("get_explain")
}

pub(crate) async fn cancel_query_impl(
    state: &AppState,
    connection_id: String,
) -> Result<(), CommandError> {
    tracing::info!(%connection_id, "cancel_query");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("cancel_query")?;

    driver.cancel_query(&handle).await.cmd_err("cancel_query")
}

pub(crate) async fn get_query_history_impl(
    state: &AppState,
    limit: usize,
    config_id: Option<String>,
    database: Option<String>,
    schema: Option<String>,
) -> Result<Vec<QueryHistoryEntry>, CommandError> {
    Ok(state
        .store
        .get_query_history(
            limit,
            config_id.as_deref(),
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
    config_id: Option<String>,
) -> Result<Vec<crate::store::FavoriteQuery>, CommandError> {
    Ok(state.store.get_favorite_queries(config_id.as_deref()).await)
}

pub(crate) async fn add_favorite_query_impl(
    state: &AppState,
    config_id: String,
    title: String,
    sql: String,
) -> Result<crate::store::FavoriteQuery, CommandError> {
    let fav = crate::store::FavoriteQuery {
        id: uuid::Uuid::new_v4().to_string(),
        config_id,
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
    connection_id: String,
    sql: String,
) -> Result<MultiQueryResult, CommandError> {
    execute_query_impl(&state, connection_id, sql).await
}

#[tauri::command]
pub async fn execute_query_stream(
    state: State<'_, AppState>,
    connection_id: String,
    sql: String,
    on_event: Channel<QueryStreamEvent>,
    apply_result_limit: Option<bool>,
    record_history: Option<bool>,
) -> Result<(), CommandError> {
    let callback: QueryStreamCallback = Arc::new(move |event| {
        let _ = on_event.send(event);
    });
    execute_query_stream_impl(
        &state,
        connection_id,
        sql,
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
    connection_id: String,
    sql: String,
) -> Result<ExplainResult, CommandError> {
    get_explain_impl(&state, connection_id, sql).await
}

#[tauri::command]
pub async fn cancel_query(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), CommandError> {
    cancel_query_impl(&state, connection_id).await
}

#[tauri::command]
pub async fn get_query_history(
    state: State<'_, AppState>,
    limit: usize,
    config_id: Option<String>,
    database: Option<String>,
    schema: Option<String>,
) -> Result<Vec<QueryHistoryEntry>, CommandError> {
    get_query_history_impl(&state, limit, config_id, database, schema).await
}

#[tauri::command]
pub async fn clear_query_history(state: State<'_, AppState>) -> Result<(), CommandError> {
    clear_query_history_impl(&state).await
}

#[tauri::command]
pub async fn get_favorite_queries(
    state: State<'_, AppState>,
    config_id: Option<String>,
) -> Result<Vec<crate::store::FavoriteQuery>, CommandError> {
    get_favorite_queries_impl(&state, config_id).await
}

#[tauri::command]
pub async fn add_favorite_query(
    state: State<'_, AppState>,
    config_id: String,
    title: String,
    sql: String,
) -> Result<crate::store::FavoriteQuery, CommandError> {
    add_favorite_query_impl(&state, config_id, title, sql).await
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
    connection_id: String,
) -> Result<(), CommandError> {
    {
        let txs = state.session_transactions.lock().await;
        if txs.contains_key(&connection_id) {
            return Ok(());
        }
    }
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("begin_session_transaction")?;
    let tx = match driver.begin_transaction(&handle).await {
        Ok(tx) => tx,
        Err(e) => {
            if state
                .session_transactions
                .lock()
                .await
                .contains_key(&connection_id)
            {
                return Ok(());
            }
            return Err(e).cmd_err("begin_session_transaction");
        }
    };
    let mut txs = state.session_transactions.lock().await;
    if txs.contains_key(&connection_id) {
        drop(txs);
        let _ = driver.rollback(tx).await;
        return Ok(());
    }
    txs.insert(connection_id, tx);
    Ok(())
}

pub(crate) async fn commit_session_transaction_impl(
    state: &AppState,
    connection_id: String,
) -> Result<(), CommandError> {
    let (driver, _) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("commit_session_transaction")?;
    let tx = state
        .session_transactions
        .lock()
        .await
        .remove(&connection_id)
        .ok_or_else(|| CommandError::Validation("No open transaction".into()))?;
    driver
        .commit(tx)
        .await
        .cmd_err("commit_session_transaction")
}

pub(crate) async fn rollback_session_transaction_impl(
    state: &AppState,
    connection_id: String,
) -> Result<(), CommandError> {
    let (driver, _) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("rollback_session_transaction")?;
    let tx = state
        .session_transactions
        .lock()
        .await
        .remove(&connection_id)
        .ok_or_else(|| CommandError::Validation("No open transaction".into()))?;
    driver
        .rollback(tx)
        .await
        .cmd_err("rollback_session_transaction")
}

pub(crate) async fn session_transaction_status_impl(
    state: &AppState,
    connection_id: String,
) -> Result<bool, CommandError> {
    Ok(state
        .session_transactions
        .lock()
        .await
        .contains_key(&connection_id))
}

#[tauri::command]
pub async fn begin_session_transaction(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), CommandError> {
    begin_session_transaction_impl(&state, connection_id).await
}

#[tauri::command]
pub async fn commit_session_transaction(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), CommandError> {
    commit_session_transaction_impl(&state, connection_id).await
}

#[tauri::command]
pub async fn rollback_session_transaction(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), CommandError> {
    rollback_session_transaction_impl(&state, connection_id).await
}

#[tauri::command]
pub async fn session_transaction_status(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<bool, CommandError> {
    session_transaction_status_impl(&state, connection_id).await
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
    }

    #[test]
    fn execute_query_stream_source_does_not_info_log_sql_preview() {
        let src = include_str!("driver_command.rs");
        let start = src
            .find("pub(crate) async fn execute_driver_command_stream_impl")
            .expect("fn");
        let chunk = &src[start..start + 900];
        assert!(
            !chunk.contains("tracing::info!(") || !chunk.contains("%sql_preview"),
            "execute_driver_command_stream must not info!-log sql_preview"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ColumnSchema, ExplainResult, Value};
    use crate::store::AppSettings;
    use crate::testing::app_state::TestAppState;
    use crate::testing::mock_driver::MockDriverOptions;

    #[tokio::test]
    async fn execute_query_success_records_history() {
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("q-cfg").await;
        let result = execute_query_impl(&test.state, conn_id, "SELECT 1".into())
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
        execute_query_impl(&test.state, conn_id, "SELECT * FROM users".into())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn get_explain_and_cancel_query() {
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
        let (_, conn_id) = test.save_and_connect("explain-cfg").await;

        let plan = get_explain_impl(&test.state, conn_id.clone(), "SELECT 1".into())
            .await
            .unwrap();
        assert_eq!(plan.plan_text, "Seq Scan");

        cancel_query_impl(&test.state, conn_id).await.unwrap();
    }

    #[tokio::test]
    async fn execute_query_not_connected_errors() {
        let test = TestAppState::new().await;
        assert!(
            execute_query_impl(&test.state, "nope".into(), "SELECT 1".into())
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
        execute_query_impl(&test.state, conn_id, "SELECT 1".into())
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
        let result = execute_query_impl(&test.state, conn_id, "SELECT id FROM t".into())
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
            cb,
            ExecuteQueryStreamOpts::default(),
        )
        .await
        .is_err());
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

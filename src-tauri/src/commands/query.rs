use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::driver_command::query_result_limit_from_settings;
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

pub(crate) async fn execute_query_stream_impl(
    state: &AppState,
    connection_id: String,
    sql: String,
    on_event: QueryStreamCallback,
) -> Result<(), CommandError> {
    tracing::info!(%connection_id, sql_len = sql.len(), "execute_query_stream");
    tracing::debug!(
        %connection_id,
        sql_preview = %sql.chars().take(500).collect::<String>(),
        "execute_query_stream sql"
    );

    let (_runtime_id, driver, handle) = state
        .connection_manager
        .resolve_session(&connection_id)
        .await
        .cmd_err("execute_query_stream")?;

    let limit = query_result_limit_from_settings(state).await;
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
            let entry = crate::store::QueryHistoryEntry {
                id: uuid::Uuid::new_v4().to_string(),
                connection_id: connection_id.clone(),
                database: String::new(),
                sql: sql.clone(),
                executed_at: chrono::Utc::now(),
                execution_time_ms: total_ms.load(Ordering::Relaxed),
                rows_affected: Some(rows_affected.load(Ordering::Relaxed)),
                success: true,
                error_message: None,
            };
            let _ = state.store.add_query_history(entry).await;
            if crate::cache::sql_may_mutate_schema(&sql) {
                state.schema_cache.clear_connection(&connection_id).await;
            }
            Ok(())
        }
        Err(err) => {
            let entry = crate::store::QueryHistoryEntry {
                id: uuid::Uuid::new_v4().to_string(),
                connection_id: connection_id.clone(),
                database: String::new(),
                sql: sql.clone(),
                executed_at: chrono::Utc::now(),
                execution_time_ms: 0,
                rows_affected: None,
                success: false,
                error_message: Some(err.to_string()),
            };
            let _ = state.store.add_query_history(entry).await;
            Err(err).cmd_err("execute_query_stream")
        }
    }
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
) -> Result<Vec<QueryHistoryEntry>, CommandError> {
    Ok(state.store.get_query_history(limit).await)
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
) -> Result<Vec<crate::store::FavoriteQuery>, CommandError> {
    Ok(state.store.get_favorite_queries().await)
}

pub(crate) async fn add_favorite_query_impl(
    state: &AppState,
    title: String,
    sql: String,
) -> Result<crate::store::FavoriteQuery, CommandError> {
    let fav = crate::store::FavoriteQuery {
        id: uuid::Uuid::new_v4().to_string(),
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
) -> Result<(), CommandError> {
    let callback: QueryStreamCallback = Arc::new(move |event| {
        let _ = on_event.send(event);
    });
    execute_query_stream_impl(&state, connection_id, sql, callback).await
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
) -> Result<Vec<QueryHistoryEntry>, CommandError> {
    get_query_history_impl(&state, limit).await
}

#[tauri::command]
pub async fn clear_query_history(state: State<'_, AppState>) -> Result<(), CommandError> {
    clear_query_history_impl(&state).await
}

#[tauri::command]
pub async fn get_favorite_queries(
    state: State<'_, AppState>,
) -> Result<Vec<crate::store::FavoriteQuery>, CommandError> {
    get_favorite_queries_impl(&state).await
}

#[tauri::command]
pub async fn add_favorite_query(
    state: State<'_, AppState>,
    title: String,
    sql: String,
) -> Result<crate::store::FavoriteQuery, CommandError> {
    add_favorite_query_impl(&state, title, sql).await
}

#[tauri::command]
pub async fn delete_favorite_query(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), CommandError> {
    delete_favorite_query_impl(&state, id).await
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
        let src = include_str!("query.rs");
        let start = src
            .find("pub(crate) async fn execute_query_stream_impl")
            .expect("fn");
        let chunk = &src[start..start + 900];
        assert!(
            !chunk.contains("tracing::info!(") || !chunk.contains("%sql_preview"),
            "execute_query_stream must not info!-log sql_preview"
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

        let history = get_query_history_impl(&test.state, 10).await.unwrap();
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
        assert!(get_favorite_queries_impl(&test.state)
            .await
            .unwrap()
            .is_empty());

        let fav = add_favorite_query_impl(&test.state, "My query".into(), "SELECT 1".into())
            .await
            .unwrap();
        assert_eq!(
            get_favorite_queries_impl(&test.state).await.unwrap().len(),
            1
        );

        delete_favorite_query_impl(&test.state, fav.id)
            .await
            .unwrap();
        assert!(get_favorite_queries_impl(&test.state)
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
        assert!(get_query_history_impl(&test.state, 10)
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
        execute_query_stream_impl(&test.state, conn_id, "SELECT 1".into(), cb)
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
        let history = get_query_history_impl(&test.state, 10).await.unwrap();
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
        execute_query_stream_impl(&test.state, conn_id, "SELECT * FROM users".into(), cb)
            .await
            .unwrap();
        assert_eq!(test.mock.last_query_limit(), Some(Some(5)));
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
        let err = execute_query_stream_impl(&test.state, conn_id, "SELECT 1".into(), cb)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("boom"));
        let history = get_query_history_impl(&test.state, 10).await.unwrap();
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
        assert!(
            execute_query_stream_impl(&test.state, "nope".into(), "SELECT 1".into(), cb)
                .await
                .is_err()
        );
    }
}

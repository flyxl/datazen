use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{ExplainResult, MultiQueryResult};
use crate::store::QueryHistoryEntry;
use tauri::State;
use uuid::Uuid;

#[tauri::command]
pub async fn execute_query(
    state: State<'_, AppState>,
    connection_id: String,
    sql: String,
) -> Result<MultiQueryResult, CommandError> {
    tracing::info!(%connection_id, sql_len = sql.len(), "execute_query");
    tracing::debug!(%connection_id, sql_preview = %sql.chars().take(500).collect::<String>(), "execute_query sql");
    let settings = state.store.get_settings().await;
    let limit = if settings.limit_select_results && settings.query_result_limit > 0 {
        Some(settings.query_result_limit)
    } else {
        None
    };

    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("execute_query")?;

    match driver.query_multi(&handle, &sql, limit).await {
        Ok(result) => {
            tracing::info!(
                %connection_id,
                statements = result.results.len(),
                ms = result.total_time_ms,
                "execute_query OK"
            );
            let total_rows: u64 = result
                .results
                .iter()
                .filter_map(|r| r.rows_affected)
                .sum();
            let entry = QueryHistoryEntry {
                id: Uuid::new_v4().to_string(),
                connection_id: connection_id.clone(),
                database: String::new(),
                sql: sql.clone(),
                executed_at: chrono::Utc::now(),
                execution_time_ms: result.total_time_ms,
                rows_affected: Some(total_rows),
                success: true,
                error_message: None,
            };
            let _ = state.store.add_query_history(entry).await;
            Ok(result)
        }
        Err(err) => {
            tracing::error!(%connection_id, error = %err, "execute_query failed");
            let entry = QueryHistoryEntry {
                id: Uuid::new_v4().to_string(),
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
            Err(CommandError::Driver(err))
        }
    }
}

#[tauri::command]
pub async fn get_explain(
    state: State<'_, AppState>,
    connection_id: String,
    sql: String,
) -> Result<ExplainResult, CommandError> {
    tracing::debug!(%connection_id, "get_explain");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("get_explain")?;

    driver
        .explain(&handle, &sql)
        .await
        .cmd_err("get_explain")
}

#[tauri::command]
pub async fn cancel_query(state: State<'_, AppState>, connection_id: String) -> Result<(), CommandError> {
    tracing::info!(%connection_id, "cancel_query");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("cancel_query")?;

    driver
        .cancel_query(&handle)
        .await
        .cmd_err("cancel_query")
}

#[tauri::command]
pub async fn get_query_history(
    state: State<'_, AppState>,
    limit: usize,
) -> Result<Vec<QueryHistoryEntry>, CommandError> {
    Ok(state.store.get_query_history(limit).await)
}

#[tauri::command]
pub async fn clear_query_history(state: State<'_, AppState>) -> Result<(), CommandError> {
    tracing::info!("clear_query_history");
    state
        .store
        .clear_query_history()
        .await
        .cmd_err("clear_query_history")
}

#[tauri::command]
pub async fn get_favorite_queries(state: State<'_, AppState>) -> Result<Vec<crate::store::FavoriteQuery>, CommandError> {
    Ok(state.store.get_favorite_queries().await)
}

#[tauri::command]
pub async fn add_favorite_query(
    state: State<'_, AppState>,
    title: String,
    sql: String,
) -> Result<crate::store::FavoriteQuery, CommandError> {
    let fav = crate::store::FavoriteQuery {
        id: uuid::Uuid::new_v4().to_string(),
        title,
        sql,
        created_at: chrono::Utc::now(),
    };
    state.store.add_favorite_query(fav.clone()).await
        .cmd_err("add_favorite_query")?;
    Ok(fav)
}

#[tauri::command]
pub async fn delete_favorite_query(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), CommandError> {
    state.store.delete_favorite_query(&id).await
        .cmd_err("delete_favorite_query")
}

#[cfg(test)]
mod log_hygiene_tests {
    #[test]
    fn execute_query_source_does_not_info_log_sql_preview() {
        let src = include_str!("query.rs");
        // Crude but effective: the info! macro block for execute_query must not bind sql_preview
        let start = src.find("pub async fn execute_query").expect("fn");
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
}

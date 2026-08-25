use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::store::HistoryScope;
use tauri::State;

pub(crate) async fn purge_history_impl(
    state: &AppState,
    scope: String,
    retain_days: Option<u32>,
) -> Result<u64, CommandError> {
    let scope = HistoryScope::parse(&scope)
        .ok_or_else(|| CommandError::Validation(format!("Invalid history scope: {scope}")))?;
    tracing::info!(?scope, ?retain_days, "purge_history");
    state
        .store
        .purge_history(scope, retain_days)
        .await
        .cmd_err("purge_history")
}

#[tauri::command]
pub async fn purge_history(
    state: State<'_, AppState>,
    scope: String,
    retain_days: Option<u32>,
) -> Result<u64, CommandError> {
    purge_history_impl(&state, scope, retain_days).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::QueryHistoryEntry;
    use crate::testing::app_state::TestAppState;
    use chrono::Utc;
    use uuid::Uuid;

    fn sample_entry(sql: &str) -> QueryHistoryEntry {
        QueryHistoryEntry {
            id: Uuid::new_v4().to_string(),
            connection_id: "cfg1".into(),
            database: "app".into(),
            schema: None,
            sql: sql.into(),
            executed_at: Utc::now(),
            execution_time_ms: 1,
            rows_affected: None,
            success: true,
            error_message: None,
        }
    }

    #[tokio::test]
    async fn purge_history_clear_all_query_scope() {
        let test = TestAppState::new().await;
        test.store
            .add_query_history(sample_entry("SELECT 1"))
            .await
            .unwrap();

        let deleted = purge_history_impl(&test.state, "query".into(), None)
            .await
            .unwrap();
        assert_eq!(deleted, 1);
        assert!(test
            .store
            .get_query_history(10, None, None, None)
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn purge_history_rejects_invalid_scope() {
        let test = TestAppState::new().await;
        let err = purge_history_impl(&test.state, "invalid".into(), Some(7))
            .await
            .unwrap_err();
        assert!(matches!(err, CommandError::Validation(_)));
    }
}

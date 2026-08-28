use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use super::compare::count_rows;
use crate::store::SyncTask;

pub(crate) async fn get_sync_tasks_impl(state: &AppState) -> Result<Vec<SyncTask>, CommandError> {
    Ok(state.store.get_sync_tasks().await)
}

pub(crate) async fn save_sync_task_direct_impl(
    state: &AppState,
    task: SyncTask,
) -> Result<(), CommandError> {
    state
        .store
        .save_sync_task(task)
        .await
        .cmd_err("save_sync_task_direct")
}

pub(crate) async fn delete_sync_task_impl(
    state: &AppState,
    task_id: String,
) -> Result<(), CommandError> {
    state
        .store
        .delete_sync_task(&task_id)
        .await
        .cmd_err("delete_sync_task")
}

pub(crate) async fn check_sync_conflicts_impl(
    state: &AppState,
    task_id: String,
) -> Result<serde_json::Value, CommandError> {
    let tasks = state.store.get_sync_tasks().await;
    let task = tasks
        .iter()
        .find(|t| t.id == task_id)
        .ok_or_else(|| CommandError::NotFound("Sync task not found".into()))?;

    let (src_driver, src_handle) = state
        .connection_manager
        .get_session(&task.source_db_session_id)
        .await
        .cmd_err("check_sync_conflicts")?;

    let src_config = state
        .connection_manager
        .get_session_config(&task.source_db_session_id)
        .await
        .cmd_err("check_sync_conflicts")?;

    let mut conflicts = Vec::<serde_json::Value>::new();

    for table in &task.tables {
        if task.completed_tables.contains(table) {
            continue;
        }

        let original_count = task.source_row_counts.get(table).copied().unwrap_or(0);
        let current_count = count_rows(
            src_driver.as_ref(),
            &src_handle,
            &src_config.database_type,
            src_config.database.as_deref(),
            None,
            table,
        )
        .await?;

        if current_count != original_count {
            conflicts.push(serde_json::json!({
                "table": table,
                "originalRows": original_count,
                "currentRows": current_count,
            }));
        }
    }

    Ok(serde_json::json!({
        "hasConflicts": !conflicts.is_empty(),
        "conflicts": conflicts,
    }))
}

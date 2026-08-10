//! Data sync IPC commands (compare, table sync, task persistence).

mod compare;
mod table_sync;
mod tasks;
mod types;

#[cfg(test)]
mod tests;

use super::error::CommandError;
use super::AppState;
use crate::store::SyncTask;
pub(crate) use compare::{
    compare_databases_impl, compare_table_data_impl, compare_table_schemas_impl,
};
pub(crate) use table_sync::{sync_table_impl, sync_tables_impl};
pub(crate) use tasks::{
    check_sync_conflicts_impl, delete_sync_task_impl, get_sync_tasks_impl,
    save_sync_task_direct_impl,
};
use tauri::State;

#[tauri::command]
pub async fn compare_databases(
    state: State<'_, AppState>,
    source_connection_id: String,
    target_connection_id: String,
) -> Result<Vec<serde_json::Value>, CommandError> {
    compare_databases_impl(&state, source_connection_id, target_connection_id).await
}

#[tauri::command]
pub async fn compare_table_schemas(
    state: State<'_, AppState>,
    source_connection_id: String,
    target_connection_id: String,
    table_name: String,
) -> Result<serde_json::Value, CommandError> {
    compare_table_schemas_impl(&state, source_connection_id, target_connection_id, table_name).await
}

#[tauri::command]
pub async fn compare_table_data(
    state: State<'_, AppState>,
    source_connection_id: String,
    target_connection_id: String,
    table_name: String,
) -> Result<serde_json::Value, CommandError> {
    compare_table_data_impl(&state, source_connection_id, target_connection_id, table_name).await
}

#[tauri::command]
pub async fn sync_table(
    state: State<'_, AppState>,
    source_connection_id: String,
    target_connection_id: String,
    table_name: String,
) -> Result<u64, CommandError> {
    sync_table_impl(&state, source_connection_id, target_connection_id, table_name).await
}

#[tauri::command]
pub async fn sync_tables(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    task_id: String,
    source_connection_id: String,
    target_connection_id: String,
    source_config_id: String,
    target_config_id: String,
    tables: Vec<String>,
    skip_tables: Vec<String>,
    strategy: String,
) -> Result<serde_json::Value, CommandError> {
    sync_tables_impl(
        &state,
        app_handle,
        task_id,
        source_connection_id,
        target_connection_id,
        source_config_id,
        target_config_id,
        tables,
        skip_tables,
        strategy,
    )
    .await
}

#[tauri::command]
pub async fn get_sync_tasks(state: State<'_, AppState>) -> Result<Vec<SyncTask>, CommandError> {
    get_sync_tasks_impl(&state).await
}

#[tauri::command]
pub async fn save_sync_task_direct(
    state: State<'_, AppState>,
    task: SyncTask,
) -> Result<(), CommandError> {
    save_sync_task_direct_impl(&state, task).await
}

#[tauri::command]
pub async fn delete_sync_task(state: State<'_, AppState>, task_id: String) -> Result<(), CommandError> {
    delete_sync_task_impl(&state, task_id).await
}

#[tauri::command]
pub async fn check_sync_conflicts(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<serde_json::Value, CommandError> {
    check_sync_conflicts_impl(&state, task_id).await
}

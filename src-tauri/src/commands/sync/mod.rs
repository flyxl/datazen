//! Data sync IPC commands (task persistence, inspect/compare/apply/generate).

mod apply;
pub(crate) mod compare;
mod exec;
mod inspect;
mod jobs;
mod keyset_source;
mod tasks;
pub(crate) mod types;

#[cfg(test)]
mod tests;

use super::error::CommandError;
use super::AppState;
use crate::store::SyncTask;
pub(crate) use apply::{
    apply_data_sync_impl, compare_data_sync_impl, generate_data_sync_sql_impl,
    revalidate_data_sync_impl,
};
pub(crate) use exec::execute_data_sync_impl;
pub(crate) use inspect::inspect_data_sync_impl;
pub(crate) use jobs::cancel_job;
pub(crate) use tasks::{
    check_sync_conflicts_impl, delete_sync_task_impl, get_sync_tasks_impl,
    save_sync_task_direct_impl,
};
use tauri::State;
use types::{resolve_options, SyncOptionsInput};

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
pub async fn delete_sync_task(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<(), CommandError> {
    delete_sync_task_impl(&state, task_id).await
}

#[tauri::command]
pub async fn inspect_data_sync(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    source_database: Option<String>,
    target_database: Option<String>,
    source_schema: Option<String>,
    target_schema: Option<String>,
) -> Result<Vec<crate::data_sync::TableResult>, CommandError> {
    inspect_data_sync_impl(
        &state,
        source_db_session_id,
        target_db_session_id,
        source_database,
        target_database,
        source_schema,
        target_schema,
        &[],
    )
    .await
}

#[tauri::command]
pub async fn execute_data_sync(
    state: State<'_, AppState>,
    target_db_session_id: String,
    statements: Vec<crate::data_sync::SqlStatement>,
    job_id: Option<String>,
    target_database: Option<String>,
) -> Result<crate::data_sync::ExecutionResult, CommandError> {
    execute_data_sync_impl(
        &state,
        target_db_session_id,
        statements,
        job_id,
        target_database,
    )
    .await
}

#[tauri::command]
pub async fn cancel_data_sync(job_id: String) -> Result<bool, CommandError> {
    Ok(cancel_job(&job_id).await)
}

#[tauri::command]
pub async fn compare_data_sync(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    tables: Option<Vec<String>>,
    job_id: Option<String>,
    source_database: Option<String>,
    target_database: Option<String>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    options: Option<SyncOptionsInput>,
) -> Result<Vec<crate::data_sync::TableResult>, CommandError> {
    compare_data_sync_impl(
        &state,
        source_db_session_id,
        target_db_session_id,
        tables.unwrap_or_default(),
        job_id,
        source_database,
        target_database,
        source_schema,
        target_schema,
        resolve_options(options),
        &[],
    )
    .await
}

#[tauri::command]
pub async fn apply_data_sync(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    tables: Vec<String>,
    job_id: Option<String>,
    source_database: Option<String>,
    target_database: Option<String>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    options: Option<SyncOptionsInput>,
) -> Result<crate::data_sync::ExecutionResult, CommandError> {
    apply_data_sync_impl(
        &state,
        source_db_session_id,
        target_db_session_id,
        tables,
        job_id,
        source_database,
        target_database,
        source_schema,
        target_schema,
        resolve_options(options),
    )
    .await
}

#[tauri::command]
pub async fn generate_data_sync_sql(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    tables: Vec<crate::data_sync::TableResult>,
    options: SyncOptionsInput,
    source_database: Option<String>,
    target_database: Option<String>,
    source_schema: Option<String>,
    target_schema: Option<String>,
) -> Result<Vec<crate::data_sync::SqlStatement>, CommandError> {
    let _ = (source_db_session_id, source_database, source_schema);
    generate_data_sync_sql_impl(
        &state,
        target_db_session_id,
        tables,
        resolve_options(Some(options)),
        target_database,
        target_schema,
    )
    .await
}

#[tauri::command]
pub async fn revalidate_data_sync(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    tables: Option<Vec<String>>,
    source_database: Option<String>,
    target_database: Option<String>,
    source_schema: Option<String>,
    target_schema: Option<String>,
) -> Result<serde_json::Value, CommandError> {
    revalidate_data_sync_impl(
        &state,
        source_db_session_id,
        target_db_session_id,
        tables.unwrap_or_default(),
        source_database,
        target_database,
        source_schema,
        target_schema,
    )
    .await
}

#[tauri::command]
pub async fn check_sync_conflicts(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<serde_json::Value, CommandError> {
    check_sync_conflicts_impl(&state, task_id).await
}

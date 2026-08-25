//! Data Transfer IPC commands.

mod exec;
mod inspect;
mod jobs;
mod preview;
mod types;

#[cfg(test)]
mod tests;

use super::error::CommandError;
use super::AppState;
use crate::data_transfer::{
    classify_transfer_pair as classify_transfer_pair_impl, TableInspectResult,
    TransferExecutionResult, TransferJob, TransferMode, TransferPreview,
};
pub(crate) use exec::execute_data_transfer_impl;
pub(crate) use inspect::inspect_data_transfer_impl;
pub(crate) use jobs::cancel_job;
pub(crate) use preview::preview_data_transfer_impl;
use tauri::State;

#[tauri::command]
pub fn classify_transfer_pair(
    source_database_type: String,
    target_database_type: String,
) -> Result<crate::data_transfer::TransferPairingView, CommandError> {
    Ok(classify_transfer_pair_impl(
        &source_database_type,
        &target_database_type,
    ))
}

#[tauri::command]
pub async fn inspect_data_transfer(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    source_database: Option<String>,
    target_database: Option<String>,
    mode: TransferMode,
    tables: Option<Vec<crate::data_transfer::TableMapping>>,
) -> Result<Vec<TableInspectResult>, CommandError> {
    inspect_data_transfer_impl(
        &state,
        source_db_session_id,
        target_db_session_id,
        source_database,
        target_database,
        mode,
        &tables.unwrap_or_default(),
    )
    .await
}

#[tauri::command]
pub async fn preview_data_transfer(
    state: State<'_, AppState>,
    job: TransferJob,
) -> Result<TransferPreview, CommandError> {
    preview_data_transfer_impl(&state, job).await
}

#[tauri::command]
pub async fn execute_data_transfer(
    state: State<'_, AppState>,
    job: TransferJob,
    job_id: Option<String>,
) -> Result<TransferExecutionResult, CommandError> {
    execute_data_transfer_impl(&state, job, job_id).await
}

#[tauri::command]
pub async fn cancel_data_transfer(job_id: String) -> Result<bool, CommandError> {
    Ok(cancel_job(&job_id).await)
}

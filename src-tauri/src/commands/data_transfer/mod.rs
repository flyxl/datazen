//! Data Transfer IPC commands.

use std::sync::atomic::{AtomicBool, Ordering};

mod exec;
mod inspect;
mod job_api;
mod jobs;
mod plans;
mod preview;
mod types;

#[cfg(test)]
mod resume_preflight_tests;
#[cfg(test)]
mod tests;

use super::error::CommandError;
use super::AppState;
use crate::data_transfer::model::TableExecutionOutcome;
use crate::data_transfer::{
    classify_transfer_pair as classify_transfer_pair_impl, TableInspectResult,
    TransferExecutionResult, TransferJob, TransferMode, TransferPreview, TransferProfile,
    TransferRunRequest,
};
pub(crate) use exec::execute_data_transfer_impl;
pub(crate) use exec::execute_data_transfer_impl_with_write_observer;
pub(crate) use inspect::{inspect_data_transfer_impl, inspect_sql_file_transfer_impl};
pub use job_api::*;
pub(crate) use jobs::cancel_job;
pub(crate) use preview::preview_data_transfer_impl;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn get_transfer_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<TransferProfile>, CommandError> {
    Ok(state.store.get_transfer_profiles().await)
}

#[tauri::command]
pub async fn save_transfer_profile(
    state: State<'_, AppState>,
    mut profile: TransferProfile,
) -> Result<(), CommandError> {
    profile.validate().map_err(CommandError::Validation)?;
    if state
        .store
        .get_connection(&profile.source_connection_id)
        .await
        .is_none()
    {
        return Err(CommandError::Validation(
            "transfer profile source connection no longer exists".into(),
        ));
    }
    if let Some(target) = profile.target_connection_id.as_deref() {
        if state.store.get_connection(target).await.is_none() {
            return Err(CommandError::Validation(
                "transfer profile target connection no longer exists".into(),
            ));
        }
    }
    profile.updated_at = chrono::Utc::now();
    state
        .store
        .save_transfer_profile(profile)
        .await
        .map_err(|error| CommandError::Internal(error.to_string()))
}

#[tauri::command]
pub async fn delete_transfer_profile(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<(), CommandError> {
    state
        .store
        .delete_transfer_profile(&profile_id)
        .await
        .map_err(|error| CommandError::Internal(error.to_string()))
}

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
        None,
        None,
        mode,
        &tables.unwrap_or_default(),
    )
    .await
}

/// Inspect source tables for a SQL-file destination. The file target has no
/// live database session, so this command deliberately receives only the
/// source session plus the optional output dialect.
#[tauri::command]
pub async fn inspect_sql_file_transfer(
    state: State<'_, AppState>,
    source_db_session_id: String,
    source_database: Option<String>,
    source_schema: Option<String>,
    target_database_type: Option<String>,
    mode: TransferMode,
    tables: Option<Vec<crate::data_transfer::TableMapping>>,
) -> Result<Vec<TableInspectResult>, CommandError> {
    inspect_sql_file_transfer_impl(
        &state,
        source_db_session_id,
        source_database,
        source_schema,
        mode,
        target_database_type,
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

/// Pick a SQL destination through the native dialog. Only the opaque token is
/// returned to the webview; the selected path remains in the host registry.
#[tauri::command]
pub async fn pick_data_transfer_sql_file(
    app: AppHandle,
) -> Result<Option<crate::data_transfer::SqlFileTarget>, CommandError> {
    let picked = super::dialog::save_file(
        &app,
        ("SQL".into(), vec!["sql".into(), "sql.gz".into()]),
        "datazen-transfer.sql".into(),
    )
    .await?;
    let Some(path) = picked else {
        return Ok(None);
    };
    let token = crate::data_transfer::sql_file::register_path(path).map_err(CommandError::from)?;
    Ok(Some(crate::data_transfer::SqlFileTarget {
        file_token: token,
        database_type: None,
        database: None,
        schema: None,
        encoding: None,
        compression: None,
    }))
}

#[tauri::command]
pub async fn execute_data_transfer(
    state: State<'_, AppState>,
    request: TransferRunRequest,
    profile: Option<crate::store::MigrationProfileRef>,
) -> Result<TransferExecutionResult, CommandError> {
    crate::commands::history::validate_migration_profile_ref(
        &state,
        "dataTransfer",
        profile.as_ref(),
    )
    .await?;
    let mut run =
        crate::commands::history::start_migration_run(&state, "dataTransfer", profile.as_ref())
            .await;
    run.selected_count = request
        .selection
        .source_tables
        .as_ref()
        .map_or(0, |tables| tables.len() as u64);
    if let Ok(plan) = plans::peek_plan(&request.plan_id) {
        run.source_connection_id = state
            .connection_manager
            .owner_connection_id(&plan.job.source.db_session_id)
            .await;
        if let Some(target) = plan.job.target.as_ref() {
            run.target_connection_id = state
                .connection_manager
                .owner_connection_id(&target.db_session_id)
                .await;
        }
    }
    let write_started = AtomicBool::new(false);
    let result =
        execute_data_transfer_impl_with_write_observer(&state, request, Some(&write_started)).await;
    match &result {
        Ok(value) => {
            crate::commands::history::finish_migration_run(
                &state,
                run,
                !value.partial && !value.cancelled,
                value.cancelled,
                value.rows_inserted,
                value.tables.iter().filter(|table| !table.success).count() as u64,
                0,
                transfer_rollback_history_outcome(value),
            )
            .await
        }
        Err(_) => {
            crate::commands::history::finish_migration_run(
                &state,
                run,
                false,
                false,
                0,
                1,
                0,
                transfer_error_history_outcome(write_started.load(Ordering::SeqCst)),
            )
            .await
        }
    }
    result
}

/// Arm a one-shot Data Transfer commit acknowledgement loss for one exact
/// target table. This IPC exists only in debug webdriver builds; execution
/// calls the real driver commit first and consumes the arm only after success.
#[cfg(all(debug_assertions, feature = "webdriver"))]
#[tauri::command]
pub fn arm_data_transfer_test_commit_ack_loss(target_table: String) -> Result<(), CommandError> {
    crate::data_transfer::execute::arm_test_commit_ack_loss(&target_table)
        .map_err(CommandError::Validation)
}

/// Clear a still-armed Data Transfer test fault during WDIO cleanup.
#[cfg(all(debug_assertions, feature = "webdriver"))]
#[tauri::command]
pub fn reset_data_transfer_test_commit_ack_loss() -> bool {
    crate::data_transfer::execute::clear_test_commit_ack_loss()
}

pub(crate) fn transfer_rollback_history_outcome(result: &TransferExecutionResult) -> &'static str {
    // SQL-file output does not use target database transactions or typed
    // per-table outcomes. Preserve its existing run-history contract here.
    if result.tables.iter().all(|table| table.outcome.is_none()) {
        return if result.partial {
            "unknown"
        } else {
            "notRequired"
        };
    }

    if result
        .tables
        .iter()
        .any(|table| table.outcome == Some(TableExecutionOutcome::Unknown))
    {
        "unknown"
    } else if result
        .tables
        .iter()
        .any(|table| table.outcome == Some(TableExecutionOutcome::PartiallyApplied))
    {
        "partiallyApplied"
    } else if result
        .tables
        .iter()
        .any(|table| table.outcome == Some(TableExecutionOutcome::RolledBack))
    {
        "rolledBack"
    } else if result.partial
        && result
            .tables
            .iter()
            .all(|table| table.outcome == Some(TableExecutionOutcome::NotStarted))
    {
        "notStarted"
    } else {
        "notRequired"
    }
}

pub(crate) fn transfer_error_history_outcome(write_started: bool) -> &'static str {
    if write_started {
        "unknown"
    } else {
        "notStarted"
    }
}

#[tauri::command]
pub async fn cancel_data_transfer(job_id: String) -> Result<bool, CommandError> {
    // P5 Jobs are cancelled through the Job repository: `request_cancel`
    // records the request, the run watch flips the handler's flag and the
    // runtime closes the Job as `Cancelled`. The legacy registry only knows
    // `services::job_registry` jobs, so it stays the fallback for an id this
    // client never accepted.
    if job_api::cancel_data_transfer_job(&job_id).await? {
        // A P5 Job answers with the recorded request itself, so the caller
        // learns whether the cancel actually reached the Job.
        return job_api::job_cancel_requested(&job_id).await;
    }
    Ok(cancel_job(&job_id).await)
}

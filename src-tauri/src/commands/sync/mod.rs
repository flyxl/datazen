//! Data sync IPC commands (task persistence, inspect/compare/apply/generate).

mod apply;
mod artifact_view;
pub(crate) mod compare;
mod comparison_store;
mod exec;
mod filter_validation;
#[cfg(test)]
mod filter_validation_tests;
mod host;
mod inspect;
mod jobs;
#[cfg(test)]
mod jobs_contract;
mod keyset_source;
mod plans;
mod tasks;
pub(crate) mod types;

#[cfg(test)]
mod tests;

use super::error::CommandError;
use super::AppState;
use crate::data_sync::{
    classify_data_sync_pair as classify_data_sync_pair_impl, DataSyncExecutionResponse,
    DataSyncPairingView, SyncProfile, SyncSourceFilter, TableMapping,
};
use crate::store::SyncTask;
#[cfg(test)]
pub(crate) use apply::generate_data_sync_sql_impl;
pub(crate) use apply::{apply_data_sync_impl, compare_data_sync_impl, revalidate_data_sync_impl};
#[cfg(test)]
pub(crate) use exec::execute_data_sync_impl;
pub(crate) use exec::{execute_data_sync_plan_impl, generate_data_sync_sql_for_plan_impl};
pub(crate) use filter_validation::{
    resolve_key_contracts, validate_filter_endpoints, validate_filter_schemas,
};
pub(crate) use inspect::inspect_data_sync_impl;
pub(crate) use jobs::cancel_job;
use plans::{SyncRunRequest, SyncRunSelection};
use std::collections::HashMap;
pub(crate) use tasks::{
    check_sync_conflicts_impl, delete_sync_task_impl, get_sync_tasks_impl,
    save_sync_task_direct_impl,
};
use tauri::State;
use types::{resolve_options, SyncOptionsInput};

/// Execute the default reviewed selection of a freshly-created Sync plan.
/// Workflow migration steps use this host-owned bridge so they cannot submit
/// client-authored rows or SQL while still sharing the normal immutable-plan
/// executor.
pub(crate) async fn execute_data_sync_profile_plan_impl(
    state: &AppState,
    plan_id: String,
    selection_revision: u64,
    options: crate::data_sync::SyncOptions,
) -> Result<crate::data_sync::ExecutionResult, CommandError> {
    let selection = plans::default_profile_selection(&plan_id, selection_revision, &options)
        .map_err(CommandError::Validation)?;
    execute_data_sync_plan_impl(
        state,
        plans::SyncRunRequest {
            plan_id,
            selection,
            options,
            job_id: None,
        },
    )
    .await
}

#[tauri::command]
pub fn classify_data_sync_pair(
    source_database_type: String,
    target_database_type: String,
) -> Result<DataSyncPairingView, CommandError> {
    Ok(classify_data_sync_pair_impl(
        &source_database_type,
        &target_database_type,
    ))
}

#[tauri::command]
pub async fn get_sync_tasks(state: State<'_, AppState>) -> Result<Vec<SyncTask>, CommandError> {
    get_sync_tasks_impl(&state).await
}

#[tauri::command]
pub async fn get_sync_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<SyncProfile>, CommandError> {
    get_sync_profiles_impl(&state).await
}

pub(crate) async fn get_sync_profiles_impl(
    state: &AppState,
) -> Result<Vec<SyncProfile>, CommandError> {
    Ok(state.store.get_sync_profiles().await)
}

#[tauri::command]
pub async fn save_sync_profile(
    state: State<'_, AppState>,
    profile: SyncProfile,
) -> Result<(), CommandError> {
    save_sync_profile_impl(&state, profile).await
}

pub(crate) async fn save_sync_profile_impl(
    state: &AppState,
    mut profile: SyncProfile,
) -> Result<(), CommandError> {
    profile.validate().map_err(CommandError::Validation)?;
    if state
        .store
        .get_connection(&profile.source_connection_id)
        .await
        .is_none()
    {
        return Err(CommandError::Validation(
            "sync profile source connection no longer exists".into(),
        ));
    }
    if state
        .store
        .get_connection(&profile.target_connection_id)
        .await
        .is_none()
    {
        return Err(CommandError::Validation(
            "sync profile target connection no longer exists".into(),
        ));
    }
    profile.updated_at = chrono::Utc::now();
    state
        .store
        .save_sync_profile(profile)
        .await
        .map_err(|error| CommandError::Internal(error.to_string()))
}

#[tauri::command]
pub async fn delete_sync_profile(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<(), CommandError> {
    delete_sync_profile_impl(&state, &profile_id).await
}

pub(crate) async fn delete_sync_profile_impl(
    state: &AppState,
    profile_id: &str,
) -> Result<(), CommandError> {
    state
        .store
        .delete_sync_profile(profile_id)
        .await
        .map_err(|error| CommandError::Internal(error.to_string()))
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
    tables: Option<Vec<TableMapping>>,
) -> Result<Vec<crate::data_sync::TableResult>, CommandError> {
    inspect_data_sync_impl(
        &state,
        source_db_session_id,
        target_db_session_id,
        source_database,
        target_database,
        source_schema,
        target_schema,
        &tables.unwrap_or_default(),
    )
    .await
}

#[tauri::command]
pub async fn execute_data_sync(
    state: State<'_, AppState>,
    request: SyncRunRequest,
    profile: Option<crate::store::MigrationProfileRef>,
) -> Result<DataSyncExecutionResponse, CommandError> {
    let mut run =
        crate::commands::history::start_migration_run(&state, "dataSync", profile.as_ref()).await;
    run.selected_count = request.selection.rows.len() as u64;
    if let Ok(plan) = plans::peek_plan(&request.plan_id) {
        run.source_connection_id = state
            .connection_manager
            .owner_connection_id(&plan.source_db_session_id)
            .await;
        run.target_connection_id = state
            .connection_manager
            .owner_connection_id(&plan.target_db_session_id)
            .await;
    }
    let (response, cancelled) = match crate::commands::history::validate_migration_profile_ref(
        &state,
        "dataSync",
        profile.as_ref(),
    )
    .await
    {
        Err(error) => (
            DataSyncExecutionResponse::failed_before_start(error.to_string()),
            false,
        ),
        Ok(()) => {
            execution_response_and_cancelled(execute_data_sync_plan_impl(&state, request).await)
        }
    };
    crate::commands::history::finish_data_sync_migration_run(&state, run, &response, cancelled)
        .await;
    Ok(response)
}

pub(crate) fn execution_response_from_result(
    result: Result<crate::data_sync::ExecutionResult, CommandError>,
) -> DataSyncExecutionResponse {
    match result {
        Ok(result) => DataSyncExecutionResponse::from_result(result),
        Err(CommandError::DataSyncOutcomeUnknown(error)) => {
            DataSyncExecutionResponse::unknown(error)
        }
        Err(error) => DataSyncExecutionResponse::failed_before_start(error.to_string()),
    }
}

fn execution_response_and_cancelled(
    result: Result<crate::data_sync::ExecutionResult, CommandError>,
) -> (DataSyncExecutionResponse, bool) {
    let cancelled = match &result {
        Err(CommandError::DataSyncNotStarted(message)) => message.starts_with("execute cancelled"),
        Ok(value) => value
            .rollback_reason
            .as_deref()
            .is_some_and(|message| message.starts_with("execute cancelled")),
        _ => false,
    };
    (execution_response_from_result(result), cancelled)
}

#[cfg(feature = "webdriver")]
#[tauri::command]
pub fn set_data_sync_test_commit_fault(fault: String) -> Result<(), CommandError> {
    exec::set_e2e_commit_fault(&fault).map_err(CommandError::Validation)
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
    filters: Option<HashMap<String, SyncSourceFilter>>,
) -> Result<plans::SyncComparisonPreview, CommandError> {
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
        &filters.unwrap_or_default(),
    )
    .await
}

/// Return one bounded page of the immutable, server-owned comparison. The
/// page request carries no SQL, credentials or row values from the client.
#[tauri::command]
pub fn get_data_sync_comparison_page(
    request: plans::SyncComparisonPageRequest,
) -> Result<plans::SyncComparisonPage, CommandError> {
    plans::get_comparison_page(request).map_err(CommandError::Validation)
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
    plan_id: String,
    selection: Option<SyncRunSelection>,
    options: SyncOptionsInput,
) -> Result<Vec<crate::data_sync::SqlStatement>, CommandError> {
    generate_data_sync_sql_for_plan_impl(
        &state,
        plan_id,
        selection.unwrap_or_default(),
        resolve_options(Some(options)),
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

//! Table ChangeSet IPC adapter.
//!
//! The plan, its guards, the fingerprint and the transaction orchestration live
//! in `datazen_feature_core::data_browsing`, shared with any future web host.
//! This module only deserializes IPC payloads, fetches the connection profile,
//! resolves the driver, lends the feature layer its catalog and runtime handles,
//! and maps the result back to `CommandError`.

use super::error::CommandError;
use super::AppState;
use datazen_driver_api::Value;
use datazen_feature_core::data_browsing::{
    commit_changes, preview_changes, validate_table_context, ConnectionFacts,
};
use tauri::State;

pub use datazen_feature_core::data_browsing::{
    CommitPendingChangesRequest, CommitPendingChangesResponse, PendingRowChange, RowChangePlan,
    RowChangeTableContext,
};

// Legacy row-write payload shapes. `commit_row_updates` / `commit_row_deletes`
// are registered as IPC commands but always reject ("legacy row writes are
// disabled"); the fields are kept so old clients still deserialize before the
// documented rejection, which is why rustc sees them as never-read.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct CellUpdate {
    pub column: String,
    pub value: Option<Value>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct RowUpdateBatch {
    pub set_columns: Vec<CellUpdate>,
    pub pk_columns: Vec<CellUpdate>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct RowDeleteBatch {
    pub pk_columns: Vec<CellUpdate>,
}

pub(crate) async fn preview_pending_changes_impl(
    state: &AppState,
    table: RowChangeTableContext,
    changes: Vec<PendingRowChange>,
) -> Result<RowChangePlan, CommandError> {
    tracing::info!(db_session_id = %table.db_session_id, table = %table.table, change_count = changes.len(), "preview_pending_changes");
    let (config, metadata) = state
        .store
        .platform_profile(&table.connection_id)
        .await
        .ok_or_else(|| CommandError::NotFound("Connection profile unavailable".into()))?;
    if !metadata.enabled || config.database_type != table.driver_type {
        return Err(CommandError::Validation(
            "Table connection context changed".into(),
        ));
    }
    let driver = state
        .driver_registry
        .get_sql_driver_by_name(config.database_type.as_str())
        .await
        .ok_or_else(|| CommandError::NotConfigured("Driver unavailable".into()))?;
    let columns = table_columns::HostTableColumns::new(state);
    preview_changes(
        driver.as_ref(),
        table,
        &changes,
        &columns,
        metadata.config_revision,
    )
    .await
    .map_err(CommandError::from)
}

#[tauri::command]
pub async fn preview_pending_changes(
    state: State<'_, AppState>,
    context: RowChangeTableContext,
    changes: Vec<PendingRowChange>,
) -> Result<RowChangePlan, CommandError> {
    preview_pending_changes_impl(&state, context, changes).await
}

pub(crate) async fn commit_pending_changes_impl(
    state: &AppState,
    request: CommitPendingChangesRequest,
) -> Result<CommitPendingChangesResponse, CommandError> {
    commit_pending_changes_with_handle(state, request, None).await
}

async fn commit_pending_changes_with_handle(
    state: &AppState,
    request: CommitPendingChangesRequest,
    manual_handle: Option<datazen_platform_api::dto::session::SessionHandle>,
) -> Result<CommitPendingChangesResponse, CommandError> {
    tracing::info!(
        db_session_id = %request.db_session_id,
        plan_id = %request.plan.plan_id,
        "commit_pending_changes"
    );
    // Input validation only: the feature layer re-checks the context because it
    // must also hold when a second transport calls it.
    validate_table_context(&request.plan.table).map_err(CommandError::from)?;
    let (config, metadata) = state
        .store
        .platform_profile(&request.plan.table.connection_id)
        .await
        .ok_or_else(|| CommandError::NotFound("Connection profile unavailable".into()))?;
    if !metadata.enabled || config.database_type != request.plan.table.driver_type {
        return Err(CommandError::Validation(
            "Table connection context changed".into(),
        ));
    }
    let driver = state
        .driver_registry
        .get_sql_driver_by_name(config.database_type.as_str())
        .await
        .ok_or_else(|| CommandError::NotConfigured("Driver unavailable".into()))?;
    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let context = adapter.begin_call().context().clone();
    let runtime = adapter
        .runtime()
        .map_err(crate::platform::into_command_error)?;
    let facts = ConnectionFacts {
        database_type: config.database_type.clone(),
        read_only: config.read_only,
        database: config.database.clone(),
        schema: config.schema.clone(),
        config_revision: metadata.config_revision,
    };
    let columns = table_columns::HostTableColumns::new(state);
    commit_changes(
        driver.as_ref(),
        request,
        &facts,
        &columns,
        manual_handle,
        runtime,
        context,
    )
    .await
    .map_err(CommandError::from)
}

#[tauri::command]
pub async fn commit_pending_changes(
    state: State<'_, AppState>,
    db_session_id: String,
    manual_handle: Option<datazen_platform_api::dto::session::SessionHandle>,
    plan: RowChangePlan,
    fingerprint: String,
) -> Result<CommitPendingChangesResponse, CommandError> {
    commit_pending_changes_with_handle(
        &state,
        CommitPendingChangesRequest {
            db_session_id,
            plan,
            fingerprint,
        },
        manual_handle,
    )
    .await
}

pub(crate) async fn commit_row_updates_impl(
    _state: &AppState,
    _db_session_id: String,
    _table: String,
    _updates: Vec<RowUpdateBatch>,
) -> Result<(), CommandError> {
    // Legacy writes contain no provenance, config revision or runtime binding.
    // Reject rather than silently joining a connection-tree transaction.
    Err(CommandError::Validation(
        "Legacy row writes are disabled; preview and commit a target-bound ChangeSet".into(),
    ))
}

#[tauri::command]
pub async fn commit_row_updates(
    state: State<'_, AppState>,
    db_session_id: String,
    table: String,
    updates: Vec<RowUpdateBatch>,
) -> Result<(), CommandError> {
    commit_row_updates_impl(&state, db_session_id, table, updates).await
}

pub(crate) async fn commit_row_deletes_impl(
    _state: &AppState,
    _db_session_id: String,
    _table: String,
    _deletes: Vec<RowDeleteBatch>,
) -> Result<(), CommandError> {
    // Legacy writes contain no provenance, config revision or runtime binding.
    // Reject rather than silently joining a connection-tree transaction.
    Err(CommandError::Validation(
        "Legacy row writes are disabled; preview and commit a target-bound ChangeSet".into(),
    ))
}

#[tauri::command]
pub async fn commit_row_deletes(
    state: State<'_, AppState>,
    db_session_id: String,
    table: String,
    deletes: Vec<RowDeleteBatch>,
) -> Result<(), CommandError> {
    commit_row_deletes_impl(&state, db_session_id, table, deletes).await
}

#[path = "data/table_columns.rs"]
mod table_columns;

#[cfg(test)]
#[path = "data/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "data/target_read_tests.rs"]
mod target_read_tests;

#[cfg(test)]
#[path = "data/insert_tests.rs"]
mod insert_tests;

#[cfg(test)]
#[path = "data/test_fixtures.rs"]
mod test_fixtures;

#[cfg(test)]
#[path = "data/insert_guard_tests.rs"]
mod insert_guard_tests;

#[cfg(test)]
#[path = "data/result_write_tests.rs"]
mod result_write_tests;

#[path = "table_target.rs"]
mod table_target;

#[tauri::command]
pub async fn get_table_data_at_target(
    state: State<'_, AppState>,
    request: table_target::TableReadRequest,
) -> Result<datazen_driver_api::TableDataResult, CommandError> {
    table_target::read(&state, request).await
}

#[path = "result_mapping.rs"]
mod result_mapping;
pub(crate) use result_mapping::{result_relation_candidate, verify_result_mapping};

#[path = "result_write.rs"]
mod result_write;
pub use result_write::ResultCellWrite;
#[tauri::command]
pub async fn commit_query_result_cell(
    state: State<'_, AppState>,
    request: ResultCellWrite,
) -> Result<(), CommandError> {
    result_write::commit_query_result_cell(state, request).await
}

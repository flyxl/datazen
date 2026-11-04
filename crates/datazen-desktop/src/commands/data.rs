//! Table ChangeSet IPC adapter.
//!
//! The plan, its guards, the fingerprint and the transaction orchestration live
//! in `datazen::data_browsing`, shared with any future web host.
//! This module only deserializes IPC payloads, fetches the connection profile,
//! resolves the driver, lends the feature layer its catalog and runtime handles,
//! and maps the result back to `CommandError`.

use super::error::CommandError;
use super::AppState;
use datazen::data_browsing::{
    commit_changes, preview_changes, validate_table_context, ConnectionFacts,
};
use tauri::State;

pub use datazen::data_browsing::{
    CommitPendingChangesRequest, CommitPendingChangesResponse, PendingRowChange, RowChangePlan,
    RowChangeTableContext,
};

#[tauri::command]
pub async fn preview_pending_changes(
    state: State<'_, AppState>,
    context: RowChangeTableContext,
    changes: Vec<PendingRowChange>,
) -> Result<RowChangePlan, CommandError> {
    tracing::info!(db_session_id = %context.db_session_id, table = %context.table, change_count = changes.len(), "preview_pending_changes");
    let (config, metadata) = state
        .store
        .platform_profile(&context.connection_id)
        .await
        .ok_or_else(|| CommandError::NotFound("Connection profile unavailable".into()))?;
    if !metadata.enabled || config.database_type != context.driver_type {
        return Err(CommandError::Validation(
            "Table connection context changed".into(),
        ));
    }
    let driver = state
        .driver_registry
        .get_sql_driver_by_name(config.database_type.as_str())
        .await
        .ok_or_else(|| CommandError::NotConfigured("Driver unavailable".into()))?;
    let columns = table_columns::HostTableColumns::new(&state);
    preview_changes(
        driver.as_ref(),
        context,
        &changes,
        &columns,
        metadata.config_revision,
    )
    .await
    .map_err(CommandError::from)
}

async fn commit_pending_changes_with_handle(
    state: &AppState,
    request: CommitPendingChangesRequest,
    manual_handle: Option<datazen::execution::SessionHandle>,
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
    let artifacts =
        crate::platform::feature_ports::HostArtifactChunks::new(runtime.clone()).shared();
    commit_changes(
        driver.as_ref(),
        request,
        &facts,
        &columns,
        manual_handle,
        runtime,
        artifacts,
        context,
    )
    .await
    .map_err(CommandError::from)
}

#[tauri::command]
pub async fn commit_pending_changes(
    state: State<'_, AppState>,
    db_session_id: String,
    manual_handle: Option<datazen::execution::SessionHandle>,
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

#[cfg(test)]
#[path = "data/result_source_tests.rs"]
mod result_source_tests;

// 测试宿主（cfg(test)）：query 刀移除 `session_transaction_*_impl` 后，data
// 测试的事务断言经真实 `#[tauri::command]` 入口驱动（与 query/dashboard/
// tunnel/connection 各域宿主同形）。
#[cfg(test)]
#[path = "data_command_test_host.rs"]
mod data_command_test_host;

#[path = "table_target.rs"]
mod table_target;

/// Target-bound table read (form-split F4: renamed from
/// `get_table_data_at_target` — the neutral kernel name, 1:1 with the facade
/// `getTableData`; the retired session-scoped predecessor freed the name).
#[tauri::command]
pub async fn get_table_data(
    state: State<'_, AppState>,
    request: table_target::TableReadRequest,
) -> Result<datazen_driver_api::TableDataResult, CommandError> {
    table_target::read(&state, request).await
}

/// Show the statements a table read would send, without sending them (DB-15).
#[tauri::command]
pub async fn preview_table_read_sql(
    state: State<'_, AppState>,
    request: table_target::TableReadPreviewRequest,
) -> Result<table_target::TableReadPreviewSql, CommandError> {
    table_target::preview(&state, request).await
}

// B4：保守结果映射整体下沉 `datazen::execution::application::result_mapping`
// （SQL 识别 + 驱动确认纯形态中立）；调用方直连 modules，宿主不再转发。
#[path = "result_write.rs"]
mod result_write;
pub use result_write::ResultCellWrite;
#[tauri::command]
pub async fn commit_query_result_cell(
    state: State<'_, AppState>,
    request: ResultCellWrite,
) -> Result<(), CommandError> {
    result_write::commit_query_result_cell(&state, request).await
}

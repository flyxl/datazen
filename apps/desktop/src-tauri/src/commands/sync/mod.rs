//! Data sync IPC commands (task persistence, inspect/compare/apply/generate).
//!
//! B2 sync 域刀（commands-layer-review §3.5）：`*_impl` 转发层已拆除——
//! 简单接线（任务/档案 CRUD、legacy apply 拒绝、revalidate 投影、
//! check_sync_conflicts 编排）直接住在 `#[tauri::command]` 处理函数里；
//! 重编排（inspect 管线、compare/prepare/apply 的 durable Job 流水）
//! 保留为域内核心，但不再有与命令同名的 `X → X_impl` 镜像：
//! [`inspect::run_inspect`]、[`apply::run_comparison`] /
//! [`apply::start_prepare_job`]、[`exec::start_apply_job`] /
//! [`exec::execute_data_sync_plan`] /
//! [`exec::generate_data_sync_sql_for_plan`]（jobs_contract 等域内契约测试
//! 直驱这些核心）。形态中立的投影随刀下沉 modules：检视收尾/复核报告/
//! 活动库校验/预览限幅在 `data_sync::api`，Artifact→Comparison 重建在
//! `data_sync::artifact_view`，keyset 页源在 `data_sync::keyset_source`
//! （宿主 `keyset_source.rs`/`artifact_view.rs` 整删）。
//!
//! `tauri::async_runtime` 的作业 spawn、`host::state` 的进程内 stores 是
//! 宿主形态，留宿主（B3 对照 WR-F5 服务端实现复核）。
//!
//! IPC 契约逐字节不变：命令名、参数（serde camelCase）、返回 JSON 与错误
//! 文案（见 `sync/tests.rs` 的 C1 钉子与 `jobs_contract*` 契约测试）。

mod apply;
/// Row-count delegate shared with the data_transfer domain (cross-domain
/// entry, B2 knife keeps it; the count assembly itself is
/// `data_sync::api::count_table_rows`).
pub(crate) mod compare;
mod exec;
mod host;
mod inspect;
mod job_api;
mod jobs;
#[cfg(test)]
mod jobs_cancel_contract;
#[cfg(test)]
mod jobs_contract;
mod tasks;

#[cfg(test)]
mod command_test_host;

#[cfg(test)]
mod tests;

use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::data_sync::api::SyncComparisonRequest;
use crate::data_sync::api::{resolve_options, SyncOptionsInput};
use crate::data_sync::plan_store as plans;
use crate::data_sync::plan_store::{SyncRunRequest, SyncRunSelection};
use crate::data_sync::{
    DataSyncExecutionResponse, DataSyncPairingView, SyncProfile, SyncSourceFilter, TableMapping,
};
use crate::store::SyncTask;
use std::collections::HashMap;
use tauri::State;

pub use job_api::*;

#[tauri::command]
pub fn classify_data_sync_pair(
    source_database_type: String,
    target_database_type: String,
) -> Result<DataSyncPairingView, CommandError> {
    Ok(crate::data_sync::classify_data_sync_pair(
        &source_database_type,
        &target_database_type,
    ))
}

#[tauri::command]
pub async fn get_sync_tasks(state: State<'_, AppState>) -> Result<Vec<SyncTask>, CommandError> {
    Ok(state.store.get_sync_tasks().await)
}

#[tauri::command]
pub async fn get_sync_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<SyncProfile>, CommandError> {
    Ok(state.store.get_sync_profiles().await)
}

#[tauri::command]
pub async fn save_sync_profile(
    state: State<'_, AppState>,
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
    state
        .store
        .delete_sync_profile(&profile_id)
        .await
        .map_err(|error| CommandError::Internal(error.to_string()))
}

#[tauri::command]
pub async fn save_sync_task_direct(
    state: State<'_, AppState>,
    task: SyncTask,
) -> Result<(), CommandError> {
    state
        .store
        .save_sync_task(task)
        .await
        .cmd_err("save_sync_task_direct")
}

#[tauri::command]
pub async fn delete_sync_task(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<(), CommandError> {
    state
        .store
        .delete_sync_task(&task_id)
        .await
        .cmd_err("delete_sync_task")
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
    inspect::run_inspect(
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
    // A consumed-plan receipt identifies an existing execution. Replaying it
    // must neither write again nor invent a second migration-history run.
    let replay = jobs::find_apply_receipt_for_plan(&state, &request.plan_id)
        .await
        .is_ok_and(|receipt| receipt.is_some());
    let mut run = if replay {
        None
    } else {
        Some(
            crate::commands::history::start_migration_run(&state, "dataSync", profile.as_ref())
                .await,
        )
    };
    if let Some(run) = run.as_mut() {
        run.selected_count = request.selection.rows.len() as u64;
        let scope = state.organization_scope()?;
        if let Ok(plan) = state.sync_stores.plan_store.peek(&scope, &request.plan_id) {
            run.source_connection_id = plan
                .source_endpoint_identity
                .as_ref()
                .map(|identity| identity.connection_id.as_str().to_owned());
            run.target_connection_id = plan
                .target_endpoint_identity
                .as_ref()
                .map(|identity| identity.connection_id.as_str().to_owned());
        }
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
            execution_response_and_cancelled(exec::execute_data_sync_plan(&state, request).await)
        }
    };
    if let Some(run) = run {
        crate::commands::history::finish_data_sync_migration_run(&state, run, &response, cancelled)
            .await;
    }
    Ok(response)
}

pub(crate) fn execution_response_from_result(
    result: Result<crate::data_sync::ExecutionResult, CommandError>,
) -> DataSyncExecutionResponse {
    match result {
        Ok(result) => DataSyncExecutionResponse::from_result(result),
        Err(CommandError::DataSyncPartiallyApplied(committed)) => {
            DataSyncExecutionResponse::partially_applied(committed)
        }
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
pub async fn cancel_data_sync(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<bool, CommandError> {
    jobs::cancel_durable_job(&state, &job_id).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn start_data_sync_prepare_job(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    tables: Option<Vec<String>>,
    job_id: String,
    source_database: Option<String>,
    target_database: Option<String>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    options: Option<SyncOptionsInput>,
    filters: Option<HashMap<String, SyncSourceFilter>>,
) -> Result<datazen::jobs::domain::job::JobView, CommandError> {
    apply::start_prepare_job(
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
        filters.unwrap_or_default(),
        None,
    )
    .await
}

#[tauri::command]
pub async fn start_data_sync_apply_job(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    request: SyncRunRequest,
) -> Result<datazen::jobs::domain::job::JobView, CommandError> {
    exec::start_apply_job(&state, source_db_session_id, target_db_session_id, request).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
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
    apply::run_comparison(
        &state,
        SyncComparisonRequest {
            source_db_session_id,
            target_db_session_id,
            tables: tables.unwrap_or_default(),
            job_id,
            source_database,
            target_database,
            source_schema,
            target_schema,
            options: resolve_options(options),
            mappings: Vec::new(),
            source_filters: filters.unwrap_or_default(),
        },
    )
    .await
}

/// Return one bounded page of the immutable, server-owned comparison. The
/// page request carries no SQL, credentials or row values from the client.
#[tauri::command]
pub fn get_data_sync_comparison_page(
    state: State<'_, AppState>,
    request: plans::SyncComparisonPageRequest,
) -> Result<plans::SyncComparisonPage, CommandError> {
    let scope = state.organization_scope()?;
    plans::get_comparison_page(state.sync_stores.plan_store.as_ref(), &scope, request)
        .map_err(CommandError::Validation)
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
    // Legacy apply had no reviewed plan: every entry is refused here and the
    // caller is pointed at the durable reviewed-selection path (原
    // `apply_data_sync_impl` 的拒绝体，B2 域刀内联)。
    let _ = (
        &state,
        &source_db_session_id,
        &target_db_session_id,
        &tables,
        &job_id,
        &source_database,
        &target_database,
        &source_schema,
        &target_schema,
        resolve_options(options),
    );
    Err(CommandError::Validation(
        "legacy apply has no reviewed plan; compare and submit the reviewed selection as a durable Data Sync job".into(),
    ))
}

#[tauri::command]
pub async fn generate_data_sync_sql(
    state: State<'_, AppState>,
    plan_id: String,
    selection: Option<SyncRunSelection>,
    options: SyncOptionsInput,
) -> Result<Vec<crate::data_sync::SqlStatement>, CommandError> {
    exec::generate_data_sync_sql_for_plan(
        &state,
        plan_id,
        selection.unwrap_or_default(),
        resolve_options(Some(options)),
    )
    .await
}

/// Re-run inspect gates for selected tables; returns stale table names when
/// structure/PK drifted. The stale-table projection itself is
/// `data_sync::api::revalidation_report` (B2 sink).
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
    let inspected = inspect::run_inspect(
        &state,
        source_db_session_id,
        target_db_session_id,
        source_database,
        target_database,
        source_schema,
        target_schema,
        &[],
    )
    .await?;
    Ok(crate::data_sync::api::revalidation_report(
        tables.unwrap_or_default(),
        inspected,
    ))
}

#[tauri::command]
pub async fn check_sync_conflicts(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<serde_json::Value, CommandError> {
    // 冲突检查编排（原 `check_sync_conflicts_impl`，B2 域刀内联）：持久化
    // 任务按 connectionId 重开专用会话，源侧行数与冻结计数比对；端点解析
    // 与释放的细节在 `tasks` 域内核心。
    let tasks = state.store.get_sync_tasks().await;
    let task = tasks
        .iter()
        .find(|t| t.id == task_id)
        .ok_or_else(|| CommandError::NotFound("Sync task not found".into()))?;

    let source = tasks::resolve_task_endpoint(
        &state,
        &task.source_connection_id,
        task.source_database.as_deref(),
        task.source_schema.as_deref(),
        "source",
    )
    .await?;
    // Resolve the target as well so a stale or deleted target cannot make the
    // task appear safe to resume. No target rows are read by this command.
    let target = match tasks::resolve_task_endpoint(
        &state,
        &task.target_connection_id,
        task.target_database.as_deref(),
        task.target_schema.as_deref(),
        "target",
    )
    .await
    {
        Ok(target) => target,
        Err(error) => {
            tasks::release_task_endpoint(&state, &source).await;
            return Err(error);
        }
    };

    let result = async {
        let mut conflicts = Vec::<serde_json::Value>::new();

        for table in &task.tables {
            if task.completed_tables.contains(table) {
                continue;
            }

            let original_count = task.source_row_counts.get(table).copied().unwrap_or(0);
            let current_count = compare::count_rows(
                source.driver.as_ref(),
                &source.handle,
                &source.config.database_type,
                task.source_database
                    .as_deref()
                    .or(source.config.database.as_deref()),
                task.source_schema
                    .as_deref()
                    .or(source.config.schema.as_deref()),
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
    .await;

    // Dedicated sessions are task-local resources. Release them on both the
    // successful result and every count_rows error path; ordinary reused UI
    // sessions remain owned by their existing callers.
    tasks::release_task_endpoint(&state, &source).await;
    tasks::release_task_endpoint(&state, &target).await;
    result
}

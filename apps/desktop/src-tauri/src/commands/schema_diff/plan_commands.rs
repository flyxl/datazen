//! Table-level Schema Diff plan preparation (source = desired -> target).
//!
//! host-boundary F4c-3：编排下沉 `schema_diff::api::prepare_table_plan`。
//! B2 schema_diff 域刀：`prepare_schema_diff_plan_with_schemas_impl` 转发体
//! 已删——其接线（会话/作用域解析 → modules 用例）逐字迁入 `job.rs`
//! （生产调用方 `AppStateBackend::prepare_plan` 的 Table 分支），本文件只剩
//! JobRuntime 的命令入口。

use super::job;
use super::{AppState, CommandError, ColumnTypeOverride, State};

/// Prepare a DDL deploy plan (source = desired → target).
#[tauri::command]
pub async fn prepare_schema_diff_plan(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    table_names: Vec<String>,
    target_table_names: Option<Vec<String>>,
    target_only_table_names: Option<Vec<String>>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    allow_destructive: bool,
    include_indexes: Option<bool>,
    type_overrides: Option<Vec<ColumnTypeOverride>>,
) -> Result<job::SchemaDiffJobAccepted, CommandError> {
    let target_table_names = target_table_names.unwrap_or_else(|| table_names.clone());
    job::run_prepare_job(
        &state,
        crate::schema_diff::job::PrepareRequest::Table {
            source_db_session_id,
            target_db_session_id,
            table_names,
            target_table_names,
            target_only_table_names: target_only_table_names.unwrap_or_default(),
            source_schema,
            target_schema,
            allow_destructive,
            include_indexes,
            type_overrides: type_overrides.unwrap_or_default(),
        },
    )
    .await
}

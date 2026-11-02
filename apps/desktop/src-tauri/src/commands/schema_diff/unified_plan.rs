//! Unified (table + schema object) Schema Diff plan preparation.
//!
//! host-boundary F4c-3：目录/快照读取与统一计划编排下沉
//! `schema_diff::api::prepare_unified_plan`。
//! B2 schema_diff 域刀：`prepare_schema_unified_plan_impl` 转发体已删——其
//! 接线（入参校验 + 会话/作用域解析 → modules 用例）逐字迁入 `job.rs`
//! （生产调用方 `AppStateBackend::prepare_plan` 的 Unified 分支），本文件
//! 只剩 JobRuntime 的命令入口。

use super::job;
use super::{AppState, ColumnTypeOverride, CommandError, State};
use datazen_driver_api::DatabaseObject;

#[tauri::command]
pub async fn prepare_schema_unified_plan(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    table_names: Vec<String>,
    target_table_names: Option<Vec<String>>,
    target_only_table_names: Option<Vec<String>>,
    source_objects: Option<Vec<DatabaseObject>>,
    target_objects: Option<Vec<DatabaseObject>>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    allow_destructive: bool,
    include_indexes: Option<bool>,
    type_overrides: Option<Vec<ColumnTypeOverride>>,
) -> Result<super::job::SchemaDiffJobAccepted, CommandError> {
    let target_table_names = target_table_names.unwrap_or_else(|| table_names.clone());
    super::job::run_prepare_job(
        &state,
        crate::schema_diff::job::PrepareRequest::Unified {
            source_db_session_id,
            target_db_session_id,
            table_names,
            target_table_names,
            target_only_table_names: target_only_table_names.unwrap_or_default(),
            source_objects: source_objects.unwrap_or_default(),
            target_objects: target_objects.unwrap_or_default(),
            source_schema,
            target_schema,
            allow_destructive,
            include_indexes,
            type_overrides: type_overrides.unwrap_or_default(),
        },
    )
    .await
}

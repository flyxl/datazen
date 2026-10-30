//! Unified (table + schema object) Schema Diff plan preparation.
//!
//! host-boundary F4c-3：目录/快照读取与统一计划编排下沉
//! `schema_diff::api::prepare_unified_plan`，本层只做会话/作用域解析、
//! 入参校验与 IPC 错误映射。

use super::job;
use super::{
    adapter_ports, reviewed_scope, schema_diff_api, schema_diff_endpoint, AppState, CmdExt,
    ColumnTypeOverride, CommandError, SchemaDiffPlan, State,
};
use datazen::shared::scope::OrganizationScope;
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

pub(crate) async fn prepare_schema_unified_plan_impl(
    state: &AppState,
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
) -> Result<SchemaDiffPlan, CommandError> {
    let target_table_names = target_table_names.unwrap_or_else(|| table_names.clone());
    let target_only_table_names = target_only_table_names.unwrap_or_default();
    let source_objects = source_objects.unwrap_or_default();
    let target_objects = target_objects.unwrap_or_default();
    // 入参校验先行，保持本传输层的错误顺序（api 会再校验一次）。
    crate::schema_diff::plan_request::validate_plan_selection(
        &table_names,
        &target_table_names,
        &target_only_table_names,
        &source_objects,
        &target_objects,
    )
    .map_err(|error| CommandError::Validation(error.to_string()))?;

    let source_context = crate::services::migration_endpoint::runtime_session_context(
        state,
        &source_db_session_id,
        datazen::execution::application::LivePurpose::Inspection,
    )
    .await?;
    let target_context = crate::services::migration_endpoint::runtime_session_context(
        state,
        &target_db_session_id,
        datazen::execution::application::LivePurpose::Inspection,
    )
    .await?;
    let scope: OrganizationScope = reviewed_scope(state)?;
    let adapters = adapter_ports(state);
    schema_diff_api::prepare_unified_plan(schema_diff_api::PrepareUnifiedPlanRequest {
        source: &schema_diff_endpoint(source_context),
        target: &schema_diff_endpoint(target_context),
        source_session_id: &source_db_session_id,
        target_session_id: &target_db_session_id,
        table_names,
        target_table_names,
        target_only_table_names,
        source_objects,
        target_objects,
        source_schema: source_schema.as_deref(),
        target_schema: target_schema.as_deref(),
        allow_destructive,
        include_indexes,
        type_overrides,
        repository: state.schema_diff_reviewed_plans.as_ref(),
        scope: &scope,
        adapters: &adapters,
    })
    .await
    .cmd_err("prepare_schema_unified_plan")
}

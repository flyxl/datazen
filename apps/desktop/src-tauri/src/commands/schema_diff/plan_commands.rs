//! Table-level Schema Diff plan preparation (source = desired -> target).
//!
//! host-boundary F4c-3：编排下沉 `schema_diff::api::prepare_table_plan`，
//! 本层只做会话/作用域解析与 IPC 错误映射。

use super::job;
use super::{
    adapter_ports, reviewed_scope, schema_diff_api, schema_diff_endpoint, AppState, CmdExt,
    ColumnTypeOverride, CommandError, SchemaDiffPlan, State,
};
use datazen::shared::scope::OrganizationScope;

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

pub(crate) async fn prepare_schema_diff_plan_with_schemas_impl(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    table_names: Vec<String>,
    target_table_names: Vec<String>,
    target_only_table_names: Vec<String>,
    allow_destructive: bool,
    include_indexes: Option<bool>,
    type_overrides: Option<Vec<ColumnTypeOverride>>,
    source_schema_override: Option<String>,
    target_schema_override: Option<String>,
) -> Result<SchemaDiffPlan, CommandError> {
    // 入参校验先行，保持本传输层的错误顺序（api 会再校验一次，供未来
    // Web 宿主独立调用——同 F4b 的双检纪律）。
    if table_names.is_empty() && target_only_table_names.is_empty() {
        return Err(CommandError::Validation(
            "table_names and target_only_table_names must not both be empty".into(),
        ));
    }
    if table_names.len() != target_table_names.len() {
        return Err(CommandError::Validation(
            "table_names and target_table_names must have the same length".into(),
        ));
    }

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
    schema_diff_api::prepare_table_plan(schema_diff_api::PrepareTablePlanRequest {
        source: &schema_diff_endpoint(source_context),
        target: &schema_diff_endpoint(target_context),
        source_session_id: &source_db_session_id,
        target_session_id: &target_db_session_id,
        table_names,
        target_table_names,
        target_only_table_names,
        allow_destructive,
        include_indexes,
        type_overrides,
        source_schema: source_schema_override.as_deref(),
        target_schema: target_schema_override.as_deref(),
        repository: state.schema_diff_reviewed_plans.as_ref(),
        scope: &scope,
        adapters: &adapters,
    })
    .await
    .cmd_err("prepare_schema_diff_plan")
}

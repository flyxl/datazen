//! Single-table schema comparison (column-level diff plus DDL preview).
//!
//! host-boundary F4c-3：IR 对比与 DDL 预览编排下沉
//! `schema_diff::api::compare_table_schemas`；本层只做会话解析与序列化。

use super::{
    adapter_ports, comparison_preview_host, schema_diff_api, schema_diff_endpoint, AppState,
    CmdExt, CommandError, State,
};

/// Compare column-level schema differences for a single table.
pub(crate) async fn compare_table_schemas_impl(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    source_table_name: String,
    target_table_name: String,
    source_schema: Option<String>,
    target_schema: Option<String>,
) -> Result<serde_json::Value, CommandError> {
    tracing::info!(%source_db_session_id, %target_db_session_id, %source_table_name, %target_table_name, "compare_table_schemas");

    let src_context = crate::services::migration_endpoint::runtime_session_context(
        &state,
        &source_db_session_id,
        datazen::execution::application::LivePurpose::Inspection,
    )
    .await?;
    let tgt_context = crate::services::migration_endpoint::runtime_session_context(
        &state,
        &target_db_session_id,
        datazen::execution::application::LivePurpose::Inspection,
    )
    .await?;

    let comparison = schema_diff_api::compare_table_schemas(
        &schema_diff_endpoint(src_context),
        &schema_diff_endpoint(tgt_context),
        &source_table_name,
        &target_table_name,
        source_schema.as_deref(),
        target_schema.as_deref(),
        &adapter_ports(state),
        &comparison_preview_host(),
    )
    .await
    .cmd_err("compare_table_schemas")?;
    serde_json::to_value(comparison).map_err(CommandError::from)
}

#[tauri::command]
pub async fn compare_table_schemas(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    source_table_name: String,
    target_table_name: String,
    source_schema: Option<String>,
    target_schema: Option<String>,
) -> Result<serde_json::Value, CommandError> {
    compare_table_schemas_impl(
        &state,
        source_db_session_id,
        target_db_session_id,
        source_table_name,
        target_table_name,
        source_schema,
        target_schema,
    )
    .await
}

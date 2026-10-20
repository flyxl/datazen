//! Single-table schema comparison (column-level diff plus DDL preview).

use super::{
    build_create_table_ddl, diff_table_schemas, diff_table_schemas_ir, fetch_full_column_types,
    fetch_target_table_schema, normalize_dialect, resolve_table_for_dialect,
    schema_catalog_database, AppState, CmdExt, CommandError, State, TableColumnDiff,
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
        datazen_runtime::application::LivePurpose::Inspection,
    )
    .await?;
    let tgt_context = crate::services::migration_endpoint::runtime_session_context(
        &state,
        &target_db_session_id,
        datazen_runtime::application::LivePurpose::Inspection,
    )
    .await?;
    let src_config = src_context.config.clone();
    let tgt_config = tgt_context.config.clone();
    let src_driver = src_context.driver.clone();
    let src_handle = src_context.handle.clone();
    let tgt_driver = tgt_context.driver.clone();
    let tgt_handle = tgt_context.handle.clone();

    let src_table = resolve_table_for_dialect(&src_config.database_type, &source_table_name);
    let tgt_table = resolve_table_for_dialect(&tgt_config.database_type, &target_table_name);

    let src_schema = src_driver
        .get_table_schema(
            &src_handle,
            &src_table,
            schema_catalog_database(&src_config.database_type, src_config.database.as_deref()),
            source_schema.as_deref().or(src_config.schema.as_deref()),
        )
        .await
        .cmd_err("compare_table_schemas")?;
    let tgt_schema = fetch_target_table_schema(
        tgt_driver.as_ref(),
        &tgt_handle,
        &tgt_table,
        tgt_config.database.as_deref().unwrap_or_default(),
        target_schema.as_deref().or(tgt_config.schema.as_deref()),
    )
    .await
    .cmd_err("compare_table_schemas")?;

    // Source = desired: missingOnTarget → ADD, extraOnTarget → DROP.
    // `added`/`removed` kept as aliases for one release.
    let mut source_ddl: Option<String> = None;
    let mut target_ddl: Option<String> = None;
    let mut ir_diff: Option<TableColumnDiff> = None;

    if state
        .sync_adapters
        .ensure_pair(&src_config.database_type, &tgt_config.database_type)
        .is_ok()
    {
        let src_source = state.sync_adapters.get_source(&src_config.database_type);
        let tgt_source = state.sync_adapters.get_source(&tgt_config.database_type);
        let src_target = state.sync_adapters.get_target(&src_config.database_type);
        let tgt_target = state.sync_adapters.get_target(&tgt_config.database_type);

        if let (
            Some(src_adapter),
            Some(tgt_src_adapter),
            Some(src_tgt_adapter),
            Some(tgt_adapter),
        ) = (src_source, tgt_source, src_target, tgt_target)
        {
            let src_full_types = fetch_full_column_types(
                src_adapter.as_ref(),
                src_driver.as_ref(),
                &src_handle,
                &src_table,
            )
            .await
            .ok();
            let tgt_full_types = if tgt_schema.columns.is_empty() {
                None
            } else {
                fetch_full_column_types(
                    tgt_src_adapter.as_ref(),
                    tgt_driver.as_ref(),
                    &tgt_handle,
                    &tgt_table,
                )
                .await
                .ok()
            };

            let src_ir = src_adapter.table_to_ir(&src_schema, src_full_types.as_ref());
            let tgt_ir = tgt_src_adapter.table_to_ir(&tgt_schema, tgt_full_types.as_ref());
            ir_diff = Some(diff_table_schemas_ir(&source_table_name, &src_ir, &tgt_ir));
            source_ddl = Some(build_create_table_ddl(&src_ir, src_tgt_adapter.as_ref()));
            target_ddl = if tgt_ir.columns.is_empty() {
                None
            } else {
                Some(build_create_table_ddl(&tgt_ir, tgt_adapter.as_ref()))
            };
        }
    }

    let src_d = normalize_dialect(&src_config.database_type);
    let tgt_d = normalize_dialect(&tgt_config.database_type);
    let normalizer_holder = if src_d == tgt_d {
        datazen_driver_api::create_driver(&tgt_d).and_then(|d| d.type_normalizer())
    } else {
        None
    };
    let normalizer = normalizer_holder.as_deref();

    let diff = ir_diff.unwrap_or_else(|| {
        diff_table_schemas(&source_table_name, &src_schema, &tgt_schema, normalizer)
    });

    let mut result = serde_json::json!({
        "table": source_table_name,
        "missingOnTarget": diff.missing_on_target,
        "extraOnTarget": diff.extra_on_target,
        "added": diff.added,
        "removed": diff.removed,
        "changed": diff.changed,
    });
    if let Some(ddl) = source_ddl {
        result["sourceDdl"] = serde_json::Value::String(ddl);
    }
    if let Some(ddl) = target_ddl {
        result["targetDdl"] = serde_json::Value::String(ddl);
    }

    tracing::info!(%source_table_name, %target_table_name, "compare_table_schemas OK");
    Ok(result)
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

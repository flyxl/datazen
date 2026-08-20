//! Schema Diff Deploy IPC commands.

use super::error::{CmdExt, CommandError};
use super::sync::compare::{
    count_rows, diff_table_schemas_ir, fetch_full_column_types, fetch_sample_rows,
    resolve_pk_columns, row_to_json_map, rows_equal, rows_to_key_map,
};
use super::sync::types::DATA_COMPARE_MISMATCH_LIMIT;
use super::AppState;
use crate::schema_diff::deploy::{
    execute_schema_diff_deploy as run_schema_diff_deploy, plan_has_destructive, DeployOptions,
    DESTRUCTIVE_CONFIRM_TOKEN,
};
use crate::schema_diff::diff_table_schemas;
use crate::schema_diff::plan::{build_schema_diff_plan, PlanOptions};
use crate::schema_diff::types::TableColumnDiff;
use crate::schema_diff::types::{normalize_dialect, SchemaDiffDeployResult, SchemaDiffPlan};
use crate::transfer::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::transfer::ddl::build_create_table_ddl;
use std::sync::Arc;
use tauri::State;

/// Prepare a DDL deploy plan (source = desired → target).
#[tauri::command]
pub async fn prepare_schema_diff_plan(
    state: State<'_, AppState>,
    source_connection_id: String,
    target_connection_id: String,
    table_names: Vec<String>,
    allow_destructive: bool,
    include_indexes: Option<bool>,
) -> Result<SchemaDiffPlan, CommandError> {
    tracing::info!(
        %source_connection_id,
        %target_connection_id,
        tables = table_names.len(),
        allow_destructive,
        "prepare_schema_diff_plan"
    );

    if table_names.is_empty() {
        return Err(CommandError::Validation(
            "table_names must not be empty".into(),
        ));
    }

    let src_config = state
        .connection_manager
        .get_connection_config(&source_connection_id)
        .await
        .cmd_err("prepare_schema_diff_plan")?;
    let tgt_config = state
        .connection_manager
        .get_connection_config(&target_connection_id)
        .await
        .cmd_err("prepare_schema_diff_plan")?;

    let (src_driver, src_handle) = state
        .connection_manager
        .get_connection(&source_connection_id)
        .await
        .cmd_err("prepare_schema_diff_plan")?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_connection(&target_connection_id)
        .await
        .cmd_err("prepare_schema_diff_plan")?;

    let mut pairs = Vec::new();
    for table in &table_names {
        let src_schema = src_driver
            .get_table_schema(&src_handle, table)
            .await
            .cmd_err("prepare_schema_diff_plan")?;
        let tgt_schema = tgt_driver
            .get_table_schema(&tgt_handle, table)
            .await
            .cmd_err("prepare_schema_diff_plan")?;
        pairs.push((table.clone(), src_schema, tgt_schema));
    }

    let src_d = normalize_dialect(&src_config.database_type);
    let tgt_d = normalize_dialect(&tgt_config.database_type);
    let include_indexes = include_indexes.unwrap_or(true);

    let plan = if src_d != tgt_d {
        state
            .sync_adapters
            .ensure_pair(&src_config.database_type, &tgt_config.database_type)
            .map_err(CommandError::Validation)?;
        let src_adapter: Arc<dyn SyncSourceAdapter> = state
            .sync_adapters
            .get_source(&src_config.database_type)
            .ok_or_else(|| CommandError::Validation("missing source sync adapter".into()))?;
        let tgt_adapter: Arc<dyn SyncTargetAdapter> = state
            .sync_adapters
            .get_target(&tgt_config.database_type)
            .ok_or_else(|| CommandError::Validation("missing target sync adapter".into()))?;

        let mapper = |source_type: &str, col_name: &str| -> Result<String, String> {
            for (_t, src, _) in &pairs {
                if let Some(col) = src
                    .columns
                    .iter()
                    .find(|c| c.name == col_name && c.data_type == source_type)
                {
                    let ir = src_adapter.column_to_ir(col, Some(source_type));
                    return Ok(tgt_adapter.ir_type_to_native(&ir.ir_type));
                }
                if let Some(col) = src.columns.iter().find(|c| c.name == col_name) {
                    let ir = src_adapter.column_to_ir(col, Some(&col.data_type));
                    return Ok(tgt_adapter.ir_type_to_native(&ir.ir_type));
                }
            }
            Err(format!(
                "cannot map type `{source_type}` for column `{col_name}`"
            ))
        };

        build_schema_diff_plan(
            &pairs,
            &src_d,
            &tgt_d,
            PlanOptions {
                allow_destructive,
                include_indexes,
                type_mapper: Some(&mapper),
            },
        )
    } else {
        build_schema_diff_plan(
            &pairs,
            &src_d,
            &tgt_d,
            PlanOptions {
                allow_destructive,
                include_indexes,
                type_mapper: None,
            },
        )
    };

    tracing::info!(
        statements = plan.statements.len(),
        warnings = plan.warnings.len(),
        "prepare_schema_diff_plan OK"
    );
    Ok(plan)
}

/// Execute a reviewed schema diff plan on the target connection.
#[tauri::command]
pub async fn execute_schema_diff_deploy(
    state: State<'_, AppState>,
    target_connection_id: String,
    plan: SchemaDiffPlan,
    use_transaction: Option<bool>,
    confirm_destructive: Option<String>,
) -> Result<SchemaDiffDeployResult, CommandError> {
    tracing::info!(
        %target_connection_id,
        statements = plan.statements.len(),
        "execute_schema_diff_deploy"
    );

    if plan_has_destructive(&plan) {
        let token = confirm_destructive.as_deref().unwrap_or("");
        if token != DESTRUCTIVE_CONFIRM_TOKEN {
            return Err(CommandError::Validation(format!(
                "destructive plan requires confirm_destructive = \"{DESTRUCTIVE_CONFIRM_TOKEN}\""
            )));
        }
    }

    let (driver, handle) = state
        .connection_manager
        .get_connection(&target_connection_id)
        .await
        .cmd_err("execute_schema_diff_deploy")?;

    let opts = DeployOptions {
        use_transaction: use_transaction.unwrap_or(true),
        stop_on_error: true,
    };

    let result = run_schema_diff_deploy(driver.as_ref(), &handle, &plan, opts).await;
    tracing::info!(
        ?result.status,
        executed = result.executed_count,
        "execute_schema_diff_deploy OK"
    );
    Ok(result)
}

/// Compare column-level schema differences for a single table.
pub(crate) async fn compare_table_schemas_impl(
    state: &AppState,
    source_connection_id: String,
    target_connection_id: String,
    table_name: String,
) -> Result<serde_json::Value, CommandError> {
    tracing::info!(%source_connection_id, %target_connection_id, %table_name, "compare_table_schemas");

    let src_config = state
        .connection_manager
        .get_connection_config(&source_connection_id)
        .await
        .cmd_err("compare_table_schemas")?;
    let tgt_config = state
        .connection_manager
        .get_connection_config(&target_connection_id)
        .await
        .cmd_err("compare_table_schemas")?;

    let (src_driver, src_handle) = state
        .connection_manager
        .get_connection(&source_connection_id)
        .await
        .cmd_err("compare_table_schemas")?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_connection(&target_connection_id)
        .await
        .cmd_err("compare_table_schemas")?;

    let src_schema = src_driver
        .get_table_schema(&src_handle, &table_name)
        .await
        .cmd_err("compare_table_schemas")?;
    let tgt_schema = tgt_driver
        .get_table_schema(&tgt_handle, &table_name)
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
                &table_name,
            )
            .await
            .ok();
            let tgt_full_types = fetch_full_column_types(
                tgt_src_adapter.as_ref(),
                tgt_driver.as_ref(),
                &tgt_handle,
                &table_name,
            )
            .await
            .ok();

            let src_ir = src_adapter.table_to_ir(&src_schema, src_full_types.as_ref());
            let tgt_ir = tgt_src_adapter.table_to_ir(&tgt_schema, tgt_full_types.as_ref());
            ir_diff = Some(diff_table_schemas_ir(&table_name, &src_ir, &tgt_ir));
            source_ddl = Some(build_create_table_ddl(&src_ir, src_tgt_adapter.as_ref()));
            target_ddl = Some(build_create_table_ddl(&tgt_ir, tgt_adapter.as_ref()));
        }
    }

    let diff = ir_diff.unwrap_or_else(|| diff_table_schemas(&table_name, &src_schema, &tgt_schema));

    let mut result = serde_json::json!({
        "table": table_name,
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

    tracing::info!(%table_name, "compare_table_schemas OK");
    Ok(result)
}

/// Sample row-level data differences for a single table.
pub(crate) async fn compare_table_data_impl(
    state: &AppState,
    source_connection_id: String,
    target_connection_id: String,
    table_name: String,
) -> Result<serde_json::Value, CommandError> {
    tracing::info!(%source_connection_id, %target_connection_id, %table_name, "compare_table_data");

    let (src_driver, src_handle) = state
        .connection_manager
        .get_connection(&source_connection_id)
        .await
        .cmd_err("compare_table_data")?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_connection(&target_connection_id)
        .await
        .cmd_err("compare_table_data")?;

    let src_schema = src_driver
        .get_table_schema(&src_handle, &table_name)
        .await
        .cmd_err("compare_table_data")?;
    let tgt_schema = tgt_driver
        .get_table_schema(&tgt_handle, &table_name)
        .await
        .cmd_err("compare_table_data")?;

    let source_row_count = count_rows(src_driver.as_ref(), &src_handle, &table_name).await?;
    let target_row_count = count_rows(tgt_driver.as_ref(), &tgt_handle, &table_name).await?;

    let col_names: Vec<String> = src_schema.columns.iter().map(|c| c.name.clone()).collect();
    let pk_cols = resolve_pk_columns(&src_schema);

    let src_rows = fetch_sample_rows(
        src_driver.as_ref(),
        &src_handle,
        &table_name,
        &col_names,
        &pk_cols,
    )
    .await?;
    let tgt_rows = fetch_sample_rows(
        tgt_driver.as_ref(),
        &tgt_handle,
        &table_name,
        &tgt_schema
            .columns
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>(),
        &resolve_pk_columns(&tgt_schema),
    )
    .await?;

    let src_map = rows_to_key_map(&col_names, &pk_cols, &src_rows);
    let tgt_col_names: Vec<String> = tgt_schema.columns.iter().map(|c| c.name.clone()).collect();
    let tgt_map = rows_to_key_map(&tgt_col_names, &resolve_pk_columns(&tgt_schema), &tgt_rows);

    let mut mismatches = Vec::new();
    let mut truncated = false;

    for (key, src_row) in &src_map {
        match tgt_map.get(key) {
            None => {
                if mismatches.len() >= DATA_COMPARE_MISMATCH_LIMIT {
                    truncated = true;
                    break;
                }
                mismatches.push(serde_json::json!({
                    "key": key,
                    "kind": "source_only",
                    "source": row_to_json_map(&col_names, src_row),
                }));
            }
            Some(tgt_row) if !rows_equal(&col_names, src_row, &tgt_col_names, tgt_row) => {
                if mismatches.len() >= DATA_COMPARE_MISMATCH_LIMIT {
                    truncated = true;
                    break;
                }
                mismatches.push(serde_json::json!({
                    "key": key,
                    "kind": "different",
                    "source": row_to_json_map(&col_names, src_row),
                    "target": row_to_json_map(&tgt_col_names, tgt_row),
                }));
            }
            _ => {}
        }
    }

    if !truncated {
        for (key, tgt_row) in &tgt_map {
            if !src_map.contains_key(key) {
                if mismatches.len() >= DATA_COMPARE_MISMATCH_LIMIT {
                    truncated = true;
                    break;
                }
                mismatches.push(serde_json::json!({
                    "key": key,
                    "kind": "target_only",
                    "target": row_to_json_map(&tgt_col_names, tgt_row),
                }));
            }
        }
    }

    let sampled_rows = src_rows.len().max(tgt_rows.len()) as u64;

    tracing::info!(%table_name, mismatches = mismatches.len(), "compare_table_data OK");
    Ok(serde_json::json!({
        "table": table_name,
        "sourceRowCount": source_row_count,
        "targetRowCount": target_row_count,
        "sampledRows": sampled_rows,
        "mismatches": mismatches,
        "truncated": truncated,
    }))
}

#[tauri::command]
pub async fn compare_table_schemas(
    state: State<'_, AppState>,
    source_connection_id: String,
    target_connection_id: String,
    table_name: String,
) -> Result<serde_json::Value, CommandError> {
    compare_table_schemas_impl(
        &state,
        source_connection_id,
        target_connection_id,
        table_name,
    )
    .await
}

#[tauri::command]
pub async fn compare_table_data(
    state: State<'_, AppState>,
    source_connection_id: String,
    target_connection_id: String,
    table_name: String,
) -> Result<serde_json::Value, CommandError> {
    compare_table_data_impl(
        &state,
        source_connection_id,
        target_connection_id,
        table_name,
    )
    .await
}

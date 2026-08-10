//! Schema Diff Deploy IPC commands.

use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::schema_diff::deploy::{
    execute_schema_diff_deploy as run_schema_diff_deploy, plan_has_destructive, DeployOptions,
    DESTRUCTIVE_CONFIRM_TOKEN,
};
use crate::schema_diff::plan::{build_schema_diff_plan, PlanOptions};
use crate::schema_diff::types::{normalize_dialect, SchemaDiffDeployResult, SchemaDiffPlan};
use crate::sync::adapter::{SyncSourceAdapter, SyncTargetAdapter};
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

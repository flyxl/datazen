//! Execute Data Transfer (structure + data, same-family and IR).

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use super::inspect::inspect_data_transfer_impl;
use super::jobs;
use crate::data_transfer::{
    column_ir_types_by_source, create_target_tables, enforce_transfer_pairing,
    execute_transfer_data, is_same_family, source_schema_to_target_ir, DropCreateContext,
    TransferExecutionResult, TransferJob, TransferMode, ValueFormatter,
};
use crate::transfer::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use datazen_driver_api::TableType;

struct TransferAdapters {
    src_source: Arc<dyn SyncSourceAdapter>,
    tgt_target: Arc<dyn SyncTargetAdapter>,
}

async fn resolve_transfer_adapters(
    state: &AppState,
    src_db_type: &str,
    tgt_db_type: &str,
) -> Result<TransferAdapters, CommandError> {
    state
        .sync_adapters
        .ensure_pair(&src_db_type.to_string(), &tgt_db_type.to_string())
        .map_err(CommandError::Validation)?;
    let src_source = state
        .sync_adapters
        .get_source(&src_db_type.to_string())
        .ok_or_else(|| CommandError::Validation("missing source sync adapter".into()))?;
    let tgt_target = state
        .sync_adapters
        .get_target(&tgt_db_type.to_string())
        .ok_or_else(|| CommandError::Validation("missing target sync adapter".into()))?;
    Ok(TransferAdapters {
        src_source,
        tgt_target,
    })
}

pub(crate) async fn execute_data_transfer_impl(
    state: &AppState,
    job: TransferJob,
    job_id: Option<String>,
) -> Result<TransferExecutionResult, CommandError> {
    job.options.validate().map_err(CommandError::from)?;

    let src_config = state
        .connection_manager
        .get_session_config(&job.source.db_session_id)
        .await
        .cmd_err("execute_data_transfer")?;
    let tgt_config = state
        .connection_manager
        .get_session_config(&job.target.db_session_id)
        .await
        .cmd_err("execute_data_transfer")?;

    let pairing = enforce_transfer_pairing(&src_config.database_type, &tgt_config.database_type)
        .map_err(CommandError::from)?;

    if tgt_config.read_only {
        return Err(CommandError::Validation(
            "target connection is read-only; Data Transfer cannot execute".into(),
        ));
    }

    if job.write_mode.is_destructive() && !job.options.confirmed_destructive {
        return Err(CommandError::Validation(
            "destructive write mode requires confirmedDestructive".into(),
        ));
    }

    let cancelled = match job_id.as_deref() {
        Some(id) => Some(jobs::ensure_job(id).await),
        None => None,
    };

    if let Some(flag) = &cancelled {
        if flag.load(Ordering::SeqCst) {
            return Err(CommandError::Validation(
                "transfer cancelled before start".into(),
            ));
        }
    }

    let inspected = inspect_data_transfer_impl(
        state,
        job.source.db_session_id.clone(),
        job.target.db_session_id.clone(),
        Some(job.source.database.clone()),
        Some(job.target.database.clone()),
        job.mode,
        &job.tables,
    )
    .await?;

    let (src_driver, src_handle) = state
        .connection_manager
        .get_session(&job.source.db_session_id)
        .await
        .cmd_err("execute_data_transfer")?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_session(&job.target.db_session_id)
        .await
        .cmd_err("execute_data_transfer")?;

    let src_tables = src_driver
        .get_tables(&src_handle, &job.source.database)
        .await
        .cmd_err("execute_data_transfer")?;

    let mut source_schemas = HashMap::new();
    for table in src_tables
        .iter()
        .filter(|t| matches!(t.table_type, TableType::Table))
    {
        if let Ok(schema) = src_driver.get_table_schema(&src_handle, &table.name).await {
            source_schemas.insert(table.name.clone(), schema);
        }
    }

    let needs_adapters = !is_same_family(&pairing)
        || matches!(
            job.mode,
            TransferMode::Structure | TransferMode::StructureAndData
        )
        || job.write_mode == crate::data_transfer::WriteMode::DropCreateInsert;

    let adapters = if needs_adapters {
        Some(
            resolve_transfer_adapters(state, &src_config.database_type, &tgt_config.database_type)
                .await?,
        )
    } else {
        None
    };

    let mut all_tables = Vec::new();
    let mut total_rows = 0u64;
    let mut cancelled_flag = false;
    let mut partial = false;

    if matches!(
        job.mode,
        TransferMode::Structure | TransferMode::StructureAndData
    ) {
        let (src_adapter, tgt_adapter) = match &adapters {
            Some(a) => (a.src_source.as_ref(), a.tgt_target.as_ref()),
            None => {
                return Err(CommandError::Validation(
                    "IR sync adapters are required for structure operations".into(),
                ));
            }
        };

        let structure_results = create_target_tables(
            src_adapter,
            tgt_adapter,
            src_driver.as_ref(),
            &src_handle,
            tgt_driver.as_ref(),
            &tgt_handle,
            &job,
            &inspected,
            &source_schemas,
            cancelled.clone(),
        )
        .await
        .map_err(CommandError::from)?;

        for r in &structure_results {
            if !r.success {
                partial = true;
            }
        }
        all_tables.extend(structure_results);

        if partial && job.options.stop_on_error {
            if let Some(id) = job_id.as_deref() {
                jobs::remove_job(id).await;
            }
            return Ok(TransferExecutionResult {
                tables: all_tables,
                rows_inserted: 0,
                cancelled: cancelled_flag,
                partial: true,
            });
        }
    }

    if matches!(
        job.mode,
        TransferMode::Data | TransferMode::StructureAndData
    ) {
        let merged_ir_types: HashMap<String, crate::transfer::ir::IRType> =
            if let Some(a) = &adapters {
                inspected
                    .iter()
                    .filter(|t| t.enabled)
                    .filter_map(|t| {
                        let schema = source_schemas.get(&t.source_table)?;
                        let ir = source_schema_to_target_ir(
                            a.src_source.as_ref(),
                            schema,
                            None,
                            &t.target_table,
                        );
                        Some(column_ir_types_by_source(&ir))
                    })
                    .flat_map(|m| m.into_iter())
                    .collect()
            } else {
                HashMap::new()
            };

        let formatter = if !is_same_family(&pairing) {
            let a = adapters.as_ref().ok_or_else(|| {
                CommandError::Validation("cross-family execute requires IR sync adapters".into())
            })?;
            ValueFormatter::Ir {
                tgt_adapter: a.tgt_target.as_ref(),
                source_column_ir_types: &merged_ir_types,
            }
        } else {
            ValueFormatter::SameFamily {
                quote: tgt_driver.quote_char(),
            }
        };

        let drop_create = if job.write_mode == crate::data_transfer::WriteMode::DropCreateInsert {
            let a = adapters.as_ref().ok_or_else(|| {
                CommandError::Validation("drop+create requires IR sync adapters".into())
            })?;
            Some(DropCreateContext {
                src_adapter: a.src_source.as_ref(),
                tgt_adapter: a.tgt_target.as_ref(),
                src_driver: src_driver.as_ref(),
                src_handle: &src_handle,
                tgt_driver: tgt_driver.as_ref(),
                tgt_handle: &tgt_handle,
                source_schemas: &source_schemas,
            })
        } else {
            None
        };

        let data_result = execute_transfer_data(
            src_driver.as_ref(),
            &src_handle,
            tgt_driver.as_ref(),
            &tgt_handle,
            &job,
            &inspected,
            &source_schemas,
            &formatter,
            drop_create.as_ref(),
            tgt_config.read_only,
            cancelled.clone(),
        )
        .await
        .map_err(CommandError::from)?;

        total_rows = data_result.rows_inserted;
        cancelled_flag = data_result.cancelled;
        partial = partial || data_result.partial;
        all_tables.extend(data_result.tables);
    }

    if let Some(id) = job_id.as_deref() {
        jobs::remove_job(id).await;
    }

    Ok(TransferExecutionResult {
        tables: all_tables,
        rows_inserted: total_rows,
        cancelled: cancelled_flag,
        partial,
    })
}

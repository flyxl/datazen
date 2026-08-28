//! Preview Data Transfer plan (DDL + write summary).

use std::collections::HashMap;

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use super::inspect::inspect_data_transfer_impl;
use crate::data_transfer::{
    build_preview, enforce_transfer_pairing, TransferJob, TransferPreview, TransferPreviewAdapters,
};
use datazen_driver_api::TableType;

pub(crate) async fn preview_data_transfer_impl(
    state: &AppState,
    job: TransferJob,
) -> Result<TransferPreview, CommandError> {
    job.options.validate().map_err(CommandError::from)?;

    let src_config = state
        .connection_manager
        .get_session_config(&job.source.db_session_id)
        .await
        .cmd_err("preview_data_transfer")?;
    let tgt_config = state
        .connection_manager
        .get_session_config(&job.target.db_session_id)
        .await
        .cmd_err("preview_data_transfer")?;

    let pairing = enforce_transfer_pairing(&src_config.database_type, &tgt_config.database_type)
        .map_err(CommandError::from)?;

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
        .cmd_err("preview_data_transfer")?;

    let src_tables = src_driver
        .get_tables(&src_handle, &job.source.database)
        .await
        .cmd_err("preview_data_transfer")?;

    let mut source_schemas = HashMap::new();
    for table in src_tables
        .iter()
        .filter(|t| matches!(t.table_type, TableType::Table))
    {
        if let Ok(schema) = src_driver.get_table_schema(&src_handle, &table.name).await {
            source_schemas.insert(table.name.clone(), schema);
        }
    }

    let target_read_only_ok = !tgt_config.read_only;

    let adapter_handles = if state
        .sync_adapters
        .ensure_pair(&src_config.database_type, &tgt_config.database_type)
        .is_ok()
    {
        match (
            state.sync_adapters.get_source(&src_config.database_type),
            state.sync_adapters.get_target(&tgt_config.database_type),
        ) {
            (Some(src), Some(tgt)) => Some((src, tgt)),
            _ => None,
        }
    } else {
        None
    };

    let adapters = adapter_handles
        .as_ref()
        .map(|(src, tgt)| TransferPreviewAdapters {
            src_adapter: src.as_ref(),
            tgt_adapter: tgt.as_ref(),
        });

    let mut preview = build_preview(
        &job,
        &inspected,
        &pairing,
        &source_schemas,
        target_read_only_ok,
        adapters,
    )
    .map_err(CommandError::from)?;

    if tgt_config.read_only {
        preview.can_execute = false;
        preview.block_reason =
            Some("target connection is read-only; Data Transfer cannot execute".into());
    }

    Ok(preview)
}

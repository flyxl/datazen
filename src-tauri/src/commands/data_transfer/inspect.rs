//! Inspect Data Transfer table mappings (no execute).

use std::collections::HashMap;

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use super::types::{is_self_database, resolve_db_name};
use crate::commands::sync::compare::count_rows;
use crate::data_transfer::{
    enforce_transfer_pairing, inspect_tables, TableInspectResult, TableMapping, TransferMode,
};
use datazen_driver_api::TableType;

pub(crate) async fn inspect_data_transfer_impl(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    source_database: Option<String>,
    target_database: Option<String>,
    mode: TransferMode,
    mappings: &[TableMapping],
) -> Result<Vec<TableInspectResult>, CommandError> {
    let src_config = state
        .connection_manager
        .get_session_config(&source_db_session_id)
        .await
        .cmd_err("inspect_data_transfer")?;
    let tgt_config = state
        .connection_manager
        .get_session_config(&target_db_session_id)
        .await
        .cmd_err("inspect_data_transfer")?;

    enforce_transfer_pairing(&src_config.database_type, &tgt_config.database_type)
        .map_err(CommandError::from)?;

    let src_db = resolve_db_name(source_database.as_deref(), src_config.database.as_deref());
    let tgt_db = resolve_db_name(target_database.as_deref(), tgt_config.database.as_deref());

    if is_self_database(
        &source_db_session_id,
        &target_db_session_id,
        &src_db,
        &tgt_db,
        None,
        None,
    ) {
        return Err(CommandError::Validation(
            "source and target are the same database; pick different databases or connections"
                .into(),
        ));
    }

    let (src_driver, src_handle) = state
        .connection_manager
        .get_session(&source_db_session_id)
        .await
        .cmd_err("inspect_data_transfer")?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("inspect_data_transfer")?;

    super::exec::maybe_use_database(src_driver.as_ref(), &src_handle, Some(&src_db)).await?;
    super::exec::maybe_use_database(tgt_driver.as_ref(), &tgt_handle, Some(&tgt_db)).await?;

    let src_tables = src_driver
        .get_tables(&src_handle, &src_db)
        .await
        .cmd_err("inspect_data_transfer")?;
    let tgt_tables = tgt_driver
        .get_tables(&tgt_handle, &tgt_db)
        .await
        .cmd_err("inspect_data_transfer")?;

    let mut source_schemas = HashMap::new();
    for table in src_tables
        .iter()
        .filter(|t| matches!(t.table_type, TableType::Table))
    {
        if let Ok(schema) = src_driver.get_table_schema(&src_handle, &table.name).await {
            source_schemas.insert(table.name.clone(), schema);
        }
    }
    let mut target_schemas = HashMap::new();
    for table in tgt_tables
        .iter()
        .filter(|t| matches!(t.table_type, TableType::Table))
    {
        if let Ok(schema) = tgt_driver.get_table_schema(&tgt_handle, &table.name).await {
            target_schemas.insert(table.name.clone(), schema);
        }
    }

    let mut source_row_counts = HashMap::new();
    for table in src_tables
        .iter()
        .filter(|t| matches!(t.table_type, TableType::Table))
    {
        if let Ok(n) = count_rows(src_driver.as_ref(), &src_handle, &table.name).await {
            source_row_counts.insert(table.name.clone(), n);
        }
    }

    Ok(inspect_tables(
        &src_tables,
        &tgt_tables,
        mappings,
        &source_schemas,
        &target_schemas,
        mode,
        &source_row_counts,
    ))
}

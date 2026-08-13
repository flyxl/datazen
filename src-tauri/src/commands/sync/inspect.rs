//! Inspect Data Sync table mappings and hard gates (no row compare).

use std::collections::HashMap;

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use crate::data_sync::{classify_tables, require_data_sync_family, TableResult};

pub(crate) async fn inspect_data_sync_impl(
    state: &AppState,
    source_connection_id: String,
    target_connection_id: String,
) -> Result<Vec<TableResult>, CommandError> {
    let src_config = state
        .connection_manager
        .get_connection_config(&source_connection_id)
        .await
        .cmd_err("inspect_data_sync")?;
    let tgt_config = state
        .connection_manager
        .get_connection_config(&target_connection_id)
        .await
        .cmd_err("inspect_data_sync")?;

    require_data_sync_family(&src_config.database_type, &tgt_config.database_type)?;
    if src_config.id == tgt_config.id
        && src_config.database == tgt_config.database
        && src_config.schema == tgt_config.schema
    {
        return Err(CommandError::Validation(
            "self-sync of the same database is not allowed".into(),
        ));
    }

    let (src_driver, src_handle) = state
        .connection_manager
        .get_connection(&source_connection_id)
        .await
        .cmd_err("inspect_data_sync")?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_connection(&target_connection_id)
        .await
        .cmd_err("inspect_data_sync")?;

    let src_db = src_config.database.as_deref().unwrap_or("");
    let tgt_db = tgt_config.database.as_deref().unwrap_or("");
    let src_tables = src_driver
        .get_tables(&src_handle, src_db)
        .await
        .cmd_err("inspect_data_sync")?;
    let tgt_tables = tgt_driver
        .get_tables(&tgt_handle, tgt_db)
        .await
        .cmd_err("inspect_data_sync")?;

    let mut source_schemas = HashMap::new();
    for table in src_tables.iter().filter(|t| {
        matches!(t.table_type, crate::db::TableType::Table)
    }) {
        if let Ok(schema) = src_driver
            .get_table_schema(&src_handle, &table.name)
            .await
        {
            source_schemas.insert(table.name.clone(), schema);
        }
    }
    let mut target_schemas = HashMap::new();
    for table in tgt_tables.iter().filter(|t| {
        matches!(t.table_type, crate::db::TableType::Table)
    }) {
        if let Ok(schema) = tgt_driver
            .get_table_schema(&tgt_handle, &table.name)
            .await
        {
            target_schemas.insert(table.name.clone(), schema);
        }
    }

    let family = crate::data_sync::require_data_sync_family(
        &src_config.database_type,
        &tgt_config.database_type,
    )?;
    Ok(classify_tables(
        &family,
        &src_tables,
        &tgt_tables,
        &[],
        &source_schemas,
        &target_schemas,
    ))
}

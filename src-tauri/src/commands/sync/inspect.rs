//! Inspect Data Sync table mappings and hard gates (no row compare).

use std::collections::HashMap;

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use super::types::{filter_tables_by_schema, is_self_sync, resolve_db_name};
use crate::data_sync::{classify_tables, require_data_sync_family, TableMapping, TableResult};
use crate::services::metadata_schema;

pub(crate) async fn inspect_data_sync_impl(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    source_database: Option<String>,
    target_database: Option<String>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    mappings: &[TableMapping],
) -> Result<Vec<TableResult>, CommandError> {
    let src_config = state
        .connection_manager
        .get_session_config(&source_db_session_id)
        .await
        .cmd_err("inspect_data_sync")?;
    let tgt_config = state
        .connection_manager
        .get_session_config(&target_db_session_id)
        .await
        .cmd_err("inspect_data_sync")?;

    require_data_sync_family(&src_config.database_type, &tgt_config.database_type)?;

    let src_db = resolve_db_name(source_database.as_deref(), src_config.database.as_deref());
    let tgt_db = resolve_db_name(target_database.as_deref(), tgt_config.database.as_deref());

    let (src_driver, src_handle) = state
        .connection_manager
        .get_session(&source_db_session_id)
        .await
        .cmd_err("inspect_data_sync")?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("inspect_data_sync")?;

    let source_schema = metadata_schema(
        src_driver.as_ref(),
        source_schema.as_deref(),
        None,
        src_config.schema.as_deref(),
    );
    let target_schema = metadata_schema(
        tgt_driver.as_ref(),
        target_schema.as_deref(),
        None,
        tgt_config.schema.as_deref(),
    );

    if is_self_sync(
        &source_db_session_id,
        &target_db_session_id,
        &src_db,
        &tgt_db,
        source_schema.as_deref(),
        target_schema.as_deref(),
    ) {
        return Err(CommandError::Validation(
            "self-sync of the same database is not allowed".into(),
        ));
    }

    let src_tables = filter_tables_by_schema(
        src_driver
            .get_tables(&src_handle, &src_db, source_schema.as_deref())
            .await
            .cmd_err("inspect_data_sync")?,
        source_schema.as_deref(),
    );
    let tgt_tables = filter_tables_by_schema(
        tgt_driver
            .get_tables(&tgt_handle, &tgt_db, target_schema.as_deref())
            .await
            .cmd_err("inspect_data_sync")?,
        target_schema.as_deref(),
    );

    let mut source_schemas = HashMap::new();
    let mut source_schema_errors = HashMap::new();
    for table in src_tables
        .iter()
        .filter(|t| matches!(t.table_type, crate::db::TableType::Table))
    {
        let table_schema = metadata_schema(
            src_driver.as_ref(),
            source_schema.as_deref(),
            table.schema.as_deref(),
            None,
        );
        match src_driver
            .get_table_schema(&src_handle, &table.name, &src_db, table_schema.as_deref())
            .await
        {
            Ok(schema) => {
                source_schemas.insert(table.name.clone(), schema);
            }
            Err(error) if src_config.database_type == "sqlserver" => {
                source_schema_errors.insert(table.name.clone(), error.to_string());
            }
            Err(_) => {}
        }
    }
    let mut target_schemas = HashMap::new();
    let mut target_schema_errors = HashMap::new();
    for table in tgt_tables
        .iter()
        .filter(|t| matches!(t.table_type, crate::db::TableType::Table))
    {
        let table_schema = metadata_schema(
            tgt_driver.as_ref(),
            target_schema.as_deref(),
            table.schema.as_deref(),
            None,
        );
        match tgt_driver
            .get_table_schema(&tgt_handle, &table.name, &tgt_db, table_schema.as_deref())
            .await
        {
            Ok(schema) => {
                target_schemas.insert(table.name.clone(), schema);
            }
            Err(error) if tgt_config.database_type == "sqlserver" => {
                target_schema_errors.insert(table.name.clone(), error.to_string());
            }
            Err(_) => {}
        }
    }

    let family = crate::data_sync::require_data_sync_family(
        &src_config.database_type,
        &tgt_config.database_type,
    )?;
    let mut results = classify_tables(
        &family,
        &src_tables,
        &tgt_tables,
        mappings,
        &source_schemas,
        &target_schemas,
    );
    for result in &mut results {
        if result.status == crate::data_sync::TableMappingStatus::Incompatible {
            let source_error = source_schema_errors.get(&result.source_table);
            let target_error = target_schema_errors.get(&result.target_table);
            match (source_error, target_error) {
                (Some(source), Some(target)) => {
                    result.incompatible_reason = Some(format!(
                        "SQL Server schema preflight failed; source: {source}; target: {target}"
                    ));
                }
                (Some(error), None) => {
                    result.incompatible_reason = Some(format!(
                        "SQL Server source schema preflight failed: {error}"
                    ));
                }
                (None, Some(error)) => {
                    result.incompatible_reason = Some(format!(
                        "SQL Server target schema preflight failed: {error}"
                    ));
                }
                (None, None) => {}
            }
        }
        if result.status != crate::data_sync::TableMappingStatus::Matched {
            continue;
        }
        if let Some(schema) = source_schemas.get(&result.source_table) {
            result.columns = schema
                .columns
                .iter()
                .map(|column| column.name.clone())
                .collect();
            result.column_types = schema
                .columns
                .iter()
                .map(|column| column.data_type.clone())
                .collect();
            result.primary_keys = schema.effective_primary_keys();
        }
        result.source_filter = mappings
            .iter()
            .find(|mapping| mapping.source_table == result.source_table)
            .and_then(|mapping| mapping.source_filter.clone());
    }
    Ok(results)
}

//! Inspect Data Sync table mappings and hard gates (no row compare).
//!
//! B2 sync 域刀（commands-layer-review §3.5）：`inspect_data_sync_impl` 转发
//! 名拆除，本文件是域内核心 [`run_inspect`]——命令入口、prepare 预检与
//! revalidate 共用。分类之后的收尾投影（SQL Server 预检原因 + Matched 行
//! 列投影/主键/结构化过滤）已下沉 `data_sync::api::finalize_inspect_results`；
//! 留在宿主的是会话解析（`services::migration_endpoint`）与逐表 schema
//! 读取（`metadata_schema` 梯子在 modules 的 schema 树，migration 树不反向
//! 依赖，见域刀报告）。

use std::collections::HashMap;

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use crate::data_sync::api::{filter_tables_by_schema, is_self_sync, resolve_db_name};
use crate::data_sync::{classify_tables, require_data_sync_family, TableMapping, TableResult};
use datazen::services::schema_scope::metadata_schema;

/// Inspect one pair of sessions: pairing gate → self-sync gate → per-table
/// schema reads → classification → match-metadata enrichment.
pub(crate) async fn run_inspect(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    source_database: Option<String>,
    target_database: Option<String>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    mappings: &[TableMapping],
) -> Result<Vec<TableResult>, CommandError> {
    let src_context = crate::services::migration_endpoint::runtime_session_context(
        state,
        &source_db_session_id,
        datazen::execution::application::LivePurpose::Inspection,
    )
    .await?;
    let tgt_context = crate::services::migration_endpoint::runtime_session_context(
        state,
        &target_db_session_id,
        datazen::execution::application::LivePurpose::Inspection,
    )
    .await?;
    let src_config = src_context.config.clone();
    let tgt_config = tgt_context.config.clone();

    require_data_sync_family(&src_config.database_type, &tgt_config.database_type)?;

    let src_db = resolve_db_name(source_database.as_deref(), src_config.database.as_deref());
    let tgt_db = resolve_db_name(target_database.as_deref(), tgt_config.database.as_deref());

    let src_driver = src_context.driver.clone();
    let src_handle = src_context.handle.clone();
    let tgt_driver = tgt_context.driver.clone();
    let tgt_handle = tgt_context.handle.clone();

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
        .filter(|t| matches!(t.table_type, datazen_driver_api::TableType::Table))
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
        .filter(|t| matches!(t.table_type, datazen_driver_api::TableType::Table))
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
    let results = classify_tables(
        &family,
        &src_tables,
        &tgt_tables,
        mappings,
        &source_schemas,
        &target_schemas,
    );
    Ok(crate::data_sync::api::finalize_inspect_results(
        results,
        mappings,
        &source_schemas,
        &source_schema_errors,
        &target_schema_errors,
    ))
}

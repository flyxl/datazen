//! Preview Data Transfer plan (DDL + write summary).

use std::collections::HashMap;

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use super::inspect::inspect_data_transfer_impl;
use crate::data_transfer::{
    build_preview, enforce_transfer_pairing, TransferJob, TransferPreview, TransferPreviewAdapters,
};
use datazen_driver_api::{TableSchema, TableType};

async fn count_scoped_source_rows(
    driver: &dyn crate::db::DatabaseDriver,
    handle: &crate::db::ConnectionHandle,
    endpoint: &crate::data_transfer::model::Endpoint,
    table: &str,
    scope: &crate::data_transfer::recordset::SourceScope,
    limit: Option<u64>,
) -> Result<u64, CommandError> {
    let relation = datazen_driver_api::sql_identifiers::qualify_relation_sql(
        &driver.driver_type(),
        Some(&endpoint.database),
        endpoint.normalized_schema(),
        table,
        driver.quote_char(),
    );
    let mut sql = format!("SELECT COUNT(*) FROM {relation}");
    if let Some(where_sql) = &scope.where_sql {
        sql.push(' ');
        sql.push_str(where_sql);
    }
    let result = driver
        .query_with_params(handle, &sql, &scope.count_params)
        .await
        .cmd_err("preview_data_transfer")?;
    let value = result
        .rows
        .first()
        .and_then(|row| row.first())
        .and_then(Option::as_ref)
        .ok_or_else(|| CommandError::Validation("source count query returned no count".into()))?;
    let count = match value {
        crate::db::Value::Integer(value) => u64::try_from(*value)
            .map_err(|_| CommandError::Validation("source count was negative".into())),
        crate::db::Value::String(value) => value.parse::<u64>().map_err(|_| {
            CommandError::Validation("source count was not an unsigned integer".into())
        }),
        _ => {
            return Err(CommandError::Validation(
                "source count query returned an unsupported value".into(),
            ));
        }
    }?;
    Ok(limit.map_or(count, |limit| count.min(limit)))
}

/// Preview the source scope and SQL statements for a native-dialog-selected
/// file target. The file path remains in the server registry and the plan only
/// stores its opaque token.
async fn preview_sql_file_target(
    state: &AppState,
    mut job: TransferJob,
) -> Result<TransferPreview, CommandError> {
    let destination = job
        .sql_file_target
        .as_mut()
        .ok_or_else(|| CommandError::Validation("SQL file target is missing".into()))?;
    destination
        .normalize_qualifiers()
        .map_err(CommandError::from)?;
    let file_token = destination.file_token.clone();
    let destination_path =
        crate::data_transfer::sql_file::resolve_path(&file_token).map_err(CommandError::from)?;
    crate::data_transfer::sql_file::validate_output_path(
        &destination_path,
        destination.normalized_compression(),
    )
    .map_err(CommandError::from)?;
    crate::data_transfer::metadata::metadata_relation_ref(&job.source, "")?;

    let src_config = state
        .connection_manager
        .get_session_config(&job.source.db_session_id)
        .await
        .cmd_err("preview_data_transfer")?;
    let (src_driver, src_handle) = state
        .connection_manager
        .get_session(&job.source.db_session_id)
        .await
        .cmd_err("preview_data_transfer")?;
    job.source.schema = crate::services::metadata_schema(
        src_driver.as_ref(),
        job.source.normalized_schema(),
        None,
        src_config.schema.as_deref(),
    );
    let destination = job
        .sql_file_target
        .as_ref()
        .ok_or_else(|| CommandError::Validation("SQL file target is missing".into()))?;
    let target_driver =
        crate::data_transfer::sql_file::resolve_target_driver(src_driver.clone(), destination)
            .map_err(CommandError::from)?;
    crate::data_transfer::sql_file::validate_target_scope_for_driver(
        target_driver.as_ref(),
        destination,
    )
    .map_err(CommandError::from)?;
    let explicit_target_dialect = destination.normalized_database_type().is_some();
    let adapters = if explicit_target_dialect {
        let target_type = destination
            .normalized_database_type()
            .ok_or_else(|| CommandError::Validation("SQL file target dialect is empty".into()))?;
        state
            .sync_adapters
            .ensure_pair(&src_config.database_type, &target_type.to_string())
            .map_err(CommandError::Validation)?;
        let src_adapter = state
            .sync_adapters
            .get_source(&src_config.database_type)
            .ok_or_else(|| CommandError::Validation("missing source sync adapter".into()))?;
        let tgt_adapter = state
            .sync_adapters
            .get_target(&target_type.to_string())
            .ok_or_else(|| CommandError::Validation("missing target sync adapter".into()))?;
        Some((src_adapter, tgt_adapter))
    } else {
        None
    };
    // The source-dialect SQL-file path still needs the source adapter for
    // indexes and foreign keys. Keep this separate from the explicit
    // cross-dialect adapter pair so the legacy direct renderer remains the
    // fallback when a build has no registered source adapter.
    let structure_adapters = if let Some(pair) = adapters.as_ref() {
        Some(pair.clone())
    } else if state
        .sync_adapters
        .ensure_type(&src_config.database_type)
        .is_ok()
    {
        match (
            state.sync_adapters.get_source(&src_config.database_type),
            state.sync_adapters.get_target(&src_config.database_type),
        ) {
            (Some(source), Some(target)) => Some((source, target)),
            _ => None,
        }
    } else {
        None
    };
    crate::data_transfer::sql_file::validate_target_dialect_job(&job)
        .map_err(CommandError::from)?;
    let source_tables = src_driver
        .get_tables(
            &src_handle,
            &job.source.database,
            job.source.normalized_schema(),
        )
        .await
        .cmd_err("preview_data_transfer")?;
    let source_tables: Vec<_> = source_tables
        .into_iter()
        .filter(|table| {
            crate::data_transfer::metadata::table_in_endpoint_schema(&job.source, table)
        })
        .collect();
    let mut source_schemas = HashMap::new();
    for table in source_tables
        .iter()
        .filter(|t| matches!(t.table_type, TableType::Table))
    {
        let schema = crate::data_transfer::metadata::load_table_schema(
            src_driver.as_ref(),
            &src_handle,
            &job.source,
            &table.name,
        )
        .await
        .map_err(|error| {
            CommandError::Validation(format!(
                "failed to inspect source table '{}': {error}",
                table.name
            ))
        })?;
        source_schemas.insert(table.name.clone(), schema);
    }
    if let Some((src_adapter, _)) = &structure_adapters {
        crate::data_transfer::structure::enrich_source_types(
            src_adapter.as_ref(),
            src_driver.as_ref(),
            &src_handle,
            &job.source,
            &mut source_schemas,
        )
        .await
        .map_err(CommandError::from)?;
    }
    let mappings = if job.tables.is_empty() {
        source_tables
            .iter()
            .filter(|table| matches!(table.table_type, TableType::Table))
            .map(|table| {
                let mut mapping = crate::data_transfer::TableMapping::auto(&table.name);
                mapping.create_new = true;
                mapping
            })
            .collect()
    } else {
        job.tables
            .iter()
            .cloned()
            .map(|mut mapping| {
                mapping.create_new = true;
                mapping
            })
            .collect()
    };
    job.tables = mappings;
    for mapping in job.tables.iter().filter(|mapping| mapping.enabled) {
        let schema = source_schemas.get(&mapping.source_table).ok_or_else(|| {
            CommandError::Validation(format!(
                "cannot validate source scope for '{}': source schema is unavailable",
                mapping.source_table
            ))
        })?;
        crate::data_transfer::recordset::build_source_scope(
            schema,
            mapping.source_filter.as_ref(),
            mapping.recordset.as_ref(),
            src_driver.quote_char(),
            &src_driver.driver_type(),
            |index, data_type| {
                src_driver
                    .parameter_placeholder(index, data_type)
                    .map_err(|error| {
                        crate::data_transfer::TransferError::unsupported(error.to_string())
                    })
            },
            |column| {
                schema
                    .columns
                    .iter()
                    .find(|candidate| candidate.name == column)
                    .map(|candidate| candidate.data_type.clone())
            },
        )
        .map_err(|error| CommandError::Validation(error.to_string()))?;
    }
    let empty_targets = Vec::new();
    let mut inspected = crate::data_transfer::inspect_tables(
        &source_tables,
        &empty_targets,
        &job.tables,
        &source_schemas,
        &HashMap::new(),
        job.mode,
        &HashMap::new(),
    );
    if let Some((source_adapter, _)) = structure_adapters.as_ref() {
        crate::data_transfer::structure::validate_transfer_source_columns(
            &job,
            &inspected,
            &source_schemas,
            source_adapter.as_ref(),
        )
        .map_err(CommandError::from)?;
    }
    if let Some((source_adapter, target_adapter)) = structure_adapters.as_ref() {
        crate::data_transfer::structure::enrich_create_new_target_types(
            &mut inspected,
            &source_schemas,
            source_adapter.as_ref(),
            target_adapter.as_ref(),
        );
        crate::data_transfer::structure::validate_transfer_column_types(
            &job,
            &inspected,
            &source_schemas,
            source_adapter.as_ref(),
            target_adapter.as_ref(),
        )
        .map_err(CommandError::from)?;
    }
    if matches!(
        job.mode,
        crate::data_transfer::TransferMode::Structure
            | crate::data_transfer::TransferMode::StructureAndData
    ) {
        if let Some((source_adapter, _)) = structure_adapters.as_ref() {
            crate::data_transfer::structure::validate_source_structure_metadata(
                source_adapter.as_ref(),
                src_driver.as_ref(),
                &src_handle,
                &job.source,
                &source_schemas,
                &inspected,
            )
            .await
            .map_err(CommandError::from)?;
        }
    }
    if let Some((src_adapter, _)) = &adapters {
        crate::data_transfer::sql_file::validate_target_ir(
            src_adapter.as_ref(),
            src_driver.as_ref(),
            target_driver.as_ref(),
            &source_schemas,
            &inspected,
        )
        .map_err(CommandError::from)?;
    }
    let mut preview = TransferPreview {
        plan_id: String::new(),
        pairing_path: "sqlFile".into(),
        mode: job.mode,
        write_mode: job.write_mode,
        ddl: Vec::new(),
        write_plans: Vec::new(),
        warnings: vec![format!(
            "SQL file uses the '{}' database dialect and is published atomically after all selected tables succeed",
            target_driver.driver_type()
        )],
        can_execute: true,
        block_reason: None,
    };
    if matches!(
        job.mode,
        crate::data_transfer::TransferMode::Structure
            | crate::data_transfer::TransferMode::StructureAndData
    ) {
        preview.ddl = crate::data_transfer::sql_file::build_structure_plan(
            structure_adapters
                .as_ref()
                .map(|(source, _)| source.as_ref()),
            structure_adapters
                .as_ref()
                .map(|(_, target)| target.as_ref()),
            target_driver.as_ref(),
            &job,
            &inspected,
            &source_schemas,
        )
        .map_err(CommandError::from)?;
    }
    for table in inspected.iter().filter(|table| table.enabled) {
        let Some(schema) = source_schemas.get(&table.source_table) else {
            preview.can_execute = false;
            preview.block_reason = Some(format!(
                "source schema is unavailable for '{}'",
                table.source_table
            ));
            continue;
        };
        if table.column_mappings.iter().all(|mapping| mapping.skip) {
            preview.can_execute = false;
            preview.block_reason = Some(format!(
                "table '{}' has no active column mappings",
                table.source_table
            ));
            continue;
        }
        if matches!(
            job.mode,
            crate::data_transfer::TransferMode::Data
                | crate::data_transfer::TransferMode::StructureAndData
        ) {
            let mapping = job
                .tables
                .iter()
                .find(|mapping| mapping.source_table == table.source_table);
            let scope = crate::data_transfer::recordset::build_source_scope(
                schema,
                mapping.and_then(|mapping| mapping.source_filter.as_ref()),
                mapping.and_then(|mapping| mapping.recordset.as_ref()),
                src_driver.quote_char(),
                &src_driver.driver_type(),
                |index, data_type| {
                    src_driver
                        .parameter_placeholder(index, data_type)
                        .map_err(|error| {
                            crate::data_transfer::TransferError::unsupported(error.to_string())
                        })
                },
                |column| {
                    schema
                        .columns
                        .iter()
                        .find(|candidate| candidate.name == column)
                        .map(|candidate| candidate.data_type.clone())
                },
            )
            .map_err(CommandError::from)?;
            let estimated_rows = if mapping.is_some_and(|mapping| mapping.recordset.is_some()) {
                Some(
                    count_scoped_source_rows(
                        src_driver.as_ref(),
                        &src_handle,
                        &job.source,
                        &table.source_table,
                        &scope,
                        mapping
                            .and_then(|mapping| mapping.recordset.as_ref().and_then(|r| r.limit)),
                    )
                    .await?,
                )
            } else {
                table.source_row_count
            };
            preview
                .write_plans
                .push(crate::data_transfer::model::WritePlanItem {
                    source_table: table.source_table.clone(),
                    target_table: table.target_table.clone(),
                    write_mode: job.write_mode,
                    mapped_columns: table.column_mappings.clone(),
                    estimated_rows,
                    preamble: Vec::new(),
                    source_filter_preview: scope.where_sql,
                    recordset_preview: scope.recordset_sql,
                });
        }
    }
    let target_schemas: HashMap<String, TableSchema> = HashMap::new();
    let plan_id = super::plans::issue_plan(
        job,
        &preview,
        src_driver.as_ref(),
        target_driver.as_ref(),
        &source_schemas,
        &target_schemas,
        false,
    )
    .map_err(CommandError::from)?;
    preview.plan_id = plan_id;
    Ok(preview)
}

pub(crate) async fn preview_data_transfer_impl(
    state: &AppState,
    mut job: TransferJob,
) -> Result<TransferPreview, CommandError> {
    job.options.validate().map_err(CommandError::from)?;
    job.validate_destination().map_err(CommandError::from)?;
    if job.sql_file_target.is_some() {
        return preview_sql_file_target(state, job).await;
    }
    let target = job
        .target
        .as_mut()
        .ok_or_else(|| CommandError::Validation("database target is missing".into()))?;

    let src_config = state
        .connection_manager
        .get_session_config(&job.source.db_session_id)
        .await
        .cmd_err("preview_data_transfer")?;
    let tgt_config = state
        .connection_manager
        .get_session_config(&target.db_session_id)
        .await
        .cmd_err("preview_data_transfer")?;

    let (src_driver, src_handle) = state
        .connection_manager
        .get_session(&job.source.db_session_id)
        .await
        .cmd_err("preview_data_transfer")?;
    let (target_driver, target_handle) = state
        .connection_manager
        .get_session(&target.db_session_id)
        .await
        .cmd_err("preview_data_transfer")?;
    job.source.schema = crate::services::metadata_schema(
        src_driver.as_ref(),
        job.source.normalized_schema(),
        None,
        src_config.schema.as_deref(),
    );
    target.schema = crate::services::metadata_schema(
        target_driver.as_ref(),
        target.normalized_schema(),
        None,
        tgt_config.schema.as_deref(),
    );
    let target = target.clone();
    crate::data_transfer::metadata::metadata_relation_ref(&job.source, "")?;
    crate::data_transfer::metadata::metadata_relation_ref(&target, "")?;

    let pairing = enforce_transfer_pairing(&src_config.database_type, &tgt_config.database_type)
        .map_err(CommandError::from)?;

    let inspected = inspect_data_transfer_impl(
        state,
        job.source.db_session_id.clone(),
        target.db_session_id.clone(),
        Some(job.source.database.clone()),
        Some(target.database.clone()),
        job.source.normalized_schema(),
        target.normalized_schema(),
        job.mode,
        &job.tables,
    )
    .await?;

    let src_tables = src_driver
        .get_tables(
            &src_handle,
            &job.source.database,
            job.source.normalized_schema(),
        )
        .await
        .cmd_err("preview_data_transfer")?;

    let src_tables: Vec<_> = src_tables
        .into_iter()
        .filter(|table| {
            crate::data_transfer::metadata::table_in_endpoint_schema(&job.source, table)
        })
        .collect();

    let mut source_schemas = HashMap::new();
    for table in src_tables
        .iter()
        .filter(|t| matches!(t.table_type, TableType::Table))
    {
        let schema = crate::data_transfer::metadata::load_table_schema(
            src_driver.as_ref(),
            &src_handle,
            &job.source,
            &table.name,
        )
        .await
        .map_err(|error| {
            CommandError::Validation(format!(
                "failed to inspect source table '{}': {error}",
                table.name
            ))
        })?;
        source_schemas.insert(table.name.clone(), schema);
    }

    // Capture target schemas for the immutable plan. A missing target table
    // is represented as `None` in the fingerprint and is expected only for a
    // CREATE NEW mapping; a table appearing before execution then invalidates
    // the plan instead of silently changing its operation.
    let mut target_schemas: HashMap<String, TableSchema> = HashMap::new();
    for table in inspected.iter().filter(|table| {
        table.enabled
            && !matches!(
                table.status,
                crate::data_transfer::model::TableMappingStatus::CreateNew
            )
            && !table.target_table.trim().is_empty()
    }) {
        let schema = crate::data_transfer::metadata::load_table_schema(
            target_driver.as_ref(),
            &target_handle,
            &target,
            &table.target_table,
        )
        .await
        .map_err(|error| {
            CommandError::Validation(format!(
                "failed to inspect target table '{}': {error}",
                table.target_table
            ))
        })?;
        target_schemas.insert(table.target_table.clone(), schema);
    }

    // Preserve the actual target namespace for qualified FK references. A
    // bare table name alone is ambiguous when the connection can see multiple
    // schemas containing the same relation name.
    let target_relations = if job.mode == crate::data_transfer::TransferMode::Data {
        target_driver
            .get_tables(&target_handle, &target.database, target.normalized_schema())
            .await
            .cmd_err("preview_data_transfer")?
            .into_iter()
            .filter(|table| {
                crate::data_transfer::metadata::table_in_endpoint_schema(&target, table)
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let target_read_only_ok = !tgt_config.read_only;

    // Keep the raw source snapshot for immutable-plan validation. The
    // precision enrichment below is required by DDL/value conversion, but it
    // is not part of the live schema identity revalidated before execution.
    let source_schemas_for_plan = source_schemas.clone();

    // Validate the complete source scope against the preview snapshot and
    // verify the source driver can bind filters, bounds and limits before
    // issuing a plan. The same builder is used by execution below.
    for mapping in job.tables.iter().filter(|mapping| mapping.enabled) {
        let schema = source_schemas_for_plan
            .get(&mapping.source_table)
            .ok_or_else(|| {
                CommandError::Validation(format!(
                    "cannot validate source scope for '{}': source schema is unavailable",
                    mapping.source_table
                ))
            })?;
        crate::data_transfer::recordset::build_source_scope(
            schema,
            mapping.source_filter.as_ref(),
            mapping.recordset.as_ref(),
            src_driver.quote_char(),
            &src_driver.driver_type(),
            |index, data_type| {
                src_driver
                    .parameter_placeholder(index, data_type)
                    .map_err(|error| {
                        crate::data_transfer::TransferError::unsupported(error.to_string())
                    })
            },
            |column| {
                schema
                    .columns
                    .iter()
                    .find(|candidate| candidate.name == column)
                    .map(|candidate| candidate.data_type.clone())
            },
        )
        .map_err(|error| {
            CommandError::Validation(format!(
                "source driver cannot execute the selected source scope: {error}"
            ))
        })?;
    }

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

    if let Some((source, _)) = &adapter_handles {
        crate::data_transfer::structure::enrich_source_types(
            source.as_ref(),
            src_driver.as_ref(),
            &src_handle,
            &job.source,
            &mut source_schemas,
        )
        .await
        .map_err(CommandError::from)?;
        if matches!(
            job.mode,
            crate::data_transfer::TransferMode::Structure
                | crate::data_transfer::TransferMode::StructureAndData
        ) || job.write_mode == crate::data_transfer::WriteMode::DropCreateInsert
        {
            crate::data_transfer::structure::validate_source_structure_metadata(
                source.as_ref(),
                src_driver.as_ref(),
                &src_handle,
                &job.source,
                &source_schemas,
                &inspected,
            )
            .await
            .map_err(CommandError::from)?;
        }
    }

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

    if matches!(
        job.mode,
        crate::data_transfer::TransferMode::Structure
            | crate::data_transfer::TransferMode::StructureAndData
    ) {
        if let Some((source_adapter, target_adapter)) = adapter_handles.as_ref() {
            preview.ddl = crate::data_transfer::build_database_structure_plan(
                source_adapter.as_ref(),
                target_adapter.as_ref(),
                &job,
                &inspected,
                &source_schemas,
            )
            .map_err(CommandError::from)?;
        }
    }

    if job.mode == crate::data_transfer::TransferMode::StructureAndData
        && job.write_mode == crate::data_transfer::WriteMode::DropCreateInsert
    {
        crate::data_transfer::structure::validate_drop_create_target_dependencies(
            target_driver.as_ref(),
            &target_handle,
            &target,
            &preview.ddl,
        )
        .await
        .map_err(CommandError::from)?;
    }

    // Return the same typed placeholder shape that execution will use. The
    // preview is review evidence, so an anonymous `?` would hide a PostgreSQL
    // cast such as `$1::integer` and make the reviewed SQL differ from the
    // actual source predicate.
    for write_plan in &mut preview.write_plans {
        let Some(mapping) = job
            .tables
            .iter()
            .find(|mapping| mapping.source_table == write_plan.source_table)
        else {
            continue;
        };
        let Some(schema) = source_schemas_for_plan.get(&write_plan.source_table) else {
            continue;
        };
        let scope = crate::data_transfer::recordset::build_source_scope(
            schema,
            mapping.source_filter.as_ref(),
            mapping.recordset.as_ref(),
            src_driver.quote_char(),
            &src_driver.driver_type(),
            |index, data_type| {
                src_driver
                    .parameter_placeholder(index, data_type)
                    .map_err(|error| {
                        crate::data_transfer::TransferError::unsupported(error.to_string())
                    })
            },
            |column| {
                schema
                    .columns
                    .iter()
                    .find(|candidate| candidate.name == column)
                    .map(|candidate| candidate.data_type.clone())
            },
        )
        .map_err(|error| {
            CommandError::Validation(format!("cannot render typed source scope preview: {error}"))
        })?;
        write_plan.source_filter_preview = scope.where_sql.clone();
        if let Some(recordset) = mapping.recordset.as_ref() {
            if recordset.tuple_range.is_some() {
                write_plan.recordset_preview = Some(
                    crate::data_transfer::recordset::preview_summary(
                        schema,
                        recordset,
                        src_driver.quote_char(),
                    )
                    .map_err(CommandError::from)?,
                );
            } else {
                write_plan.recordset_preview = scope.recordset_sql.clone();
            }
            write_plan.estimated_rows = Some(
                count_scoped_source_rows(
                    src_driver.as_ref(),
                    &src_handle,
                    &job.source,
                    &write_plan.source_table,
                    &scope,
                    mapping
                        .recordset
                        .as_ref()
                        .and_then(|recordset| recordset.limit),
                )
                .await?,
            );
        } else {
            write_plan.recordset_preview = scope.recordset_sql;
        }
    }

    if matches!(
        job.mode,
        crate::data_transfer::TransferMode::Data
            | crate::data_transfer::TransferMode::StructureAndData
    ) {
        if let Err(error) = target_driver.parameter_placeholder(1, None) {
            preview.can_execute = false;
            preview.block_reason = Some(error.to_string());
        }
    }

    if tgt_config.read_only {
        preview.can_execute = false;
        preview.block_reason =
            Some("target connection is read-only; Data Transfer cannot execute".into());
    }

    let target_table_dependencies = if job.mode == crate::data_transfer::TransferMode::Data {
        let dependencies = crate::data_transfer::capture_target_fk_dependencies(
            &preview.write_plans,
            &target_schemas,
            &target_relations,
            &target,
        );
        crate::data_transfer::reorder_preview_write_plans(&mut preview, &dependencies);
        dependencies
    } else {
        Vec::new()
    };

    let plan_id = super::plans::issue_plan_with_target_dependencies(
        job,
        &preview,
        src_driver.as_ref(),
        target_driver.as_ref(),
        &source_schemas_for_plan,
        &target_schemas,
        tgt_config.read_only,
        target_table_dependencies,
    )
    .map_err(CommandError::from)?;
    preview.plan_id = plan_id;

    Ok(preview)
}

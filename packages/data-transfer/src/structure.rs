//! Structure phase: CREATE (IR) and DROP helpers for Data Transfer.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use datazen_driver_api::TableSchema;

use crate::transfer::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::transfer::ir::{IRColumn, IRDefault, IRTable, IRType};
use datazen_driver_api::{ConnectionHandle, DatabaseDriver};

use super::error::TransferError;
use super::model::{
    DdlPreviewItem, DdlPreviewKind, TableExecutionOutcome, TableExecutionResult,
    TableInspectResult, TableMappingStatus, TransferJob, TransferMode,
};

pub fn build_drop_table_sql(table: &str, tgt_adapter: &dyn SyncTargetAdapter) -> String {
    format!("DROP TABLE IF EXISTS {}", tgt_adapter.quote_ident(table))
}

pub fn target_relation_ref(
    job: &TransferJob,
    table: &str,
    adapter: &dyn SyncTargetAdapter,
) -> String {
    let target = job.target.as_ref();
    adapter.qualify_relation(
        target
            .map(|target| target.database.as_str())
            .unwrap_or_default(),
        target.and_then(|target| target.normalized_schema()),
        table,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseStructurePhase {
    /// Create every selected table and secondary index before row transfer.
    Prepare,
    /// Install foreign keys after row transfer so child-first mapping order
    /// cannot reject rows whose parent table is selected later.
    ForeignKeys,
    /// Structure-only transfers have no data phase, so emit all planned DDL.
    All,
}

/// Execute the immutable database structure statements captured at preview.
/// The caller validates the whole plan and selected-table dependencies before
/// entering this function, so a rejected mapping cannot partially write.
pub async fn execute_database_structure_plan(
    tgt_driver: &dyn DatabaseDriver,
    tgt_handle: &ConnectionHandle,
    plan: &[DdlPreviewItem],
    selected_tables: &HashSet<String>,
    phase: DatabaseStructurePhase,
    cancelled: Option<Arc<AtomicBool>>,
    write_started: Option<&AtomicBool>,
) -> Vec<TableExecutionResult> {
    let mut results = Vec::new();
    let planned: Vec<_> = plan
        .iter()
        .filter(|item| {
            selected_tables.contains(&item.source_table)
                && match phase {
                    DatabaseStructurePhase::Prepare => item.kind != DdlPreviewKind::ForeignKey,
                    DatabaseStructurePhase::ForeignKeys => item.kind == DdlPreviewKind::ForeignKey,
                    DatabaseStructurePhase::All => true,
                }
        })
        .collect();
    for (index, item) in planned.iter().enumerate() {
        if cancelled
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::SeqCst))
        {
            results.push(TableExecutionResult::database(
                &item.source_table,
                &item.target_table,
                Some(0),
                TableExecutionOutcome::NotStarted,
                Some("transfer cancelled during structure; completed DDL remains applied".into()),
            ));
            append_unattempted_structure_results(&mut results, &planned[index + 1..]);
            break;
        }
        if let Some(write_started) = write_started {
            write_started.store(true, Ordering::SeqCst);
        }
        match tgt_driver.execute(tgt_handle, &item.ddl).await {
            Ok(_) => results.push(TableExecutionResult::database(
                &item.source_table,
                &item.target_table,
                Some(0),
                TableExecutionOutcome::Committed,
                None,
            )),
            Err(error) => {
                let operation = match item.kind {
                    DdlPreviewKind::DropTable => "DROP TABLE",
                    DdlPreviewKind::Table => "CREATE TABLE",
                    DdlPreviewKind::Index => "CREATE INDEX",
                    DdlPreviewKind::ForeignKey => "ADD FOREIGN KEY",
                };
                results.push(TableExecutionResult::database(
                    &item.source_table,
                    &item.target_table,
                    None,
                    TableExecutionOutcome::Unknown,
                    Some(format!("{operation} failed; outcome UNKNOWN: {error}")),
                ));
                // A DDL acknowledgement can be lost after applying the
                // statement. Never send a later planned write in that case.
                append_unattempted_structure_results(&mut results, &planned[index + 1..]);
                break;
            }
        }
    }
    results
}

fn append_unattempted_structure_results(
    results: &mut Vec<TableExecutionResult>,
    items: &[&DdlPreviewItem],
) {
    for item in items {
        results.push(TableExecutionResult::database(
            &item.source_table,
            &item.target_table,
            Some(0),
            TableExecutionOutcome::NotStarted,
            Some("not started because an earlier structure statement stopped the phase".into()),
        ));
    }
}

/// Read precision-bearing metadata for the same renderer used by both commands.
pub async fn enrich_source_types(
    adapter: &dyn SyncSourceAdapter,
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    endpoint: &super::model::Endpoint,
    schemas: &mut HashMap<String, TableSchema>,
) -> Result<(), TransferError> {
    for (table, schema) in schemas {
        let relation = if driver.sync_family() == "mysql"
            && endpoint.normalized_schema().is_none()
            && !endpoint.database.trim().is_empty()
        {
            // MySQL's selected catalog lives in `Endpoint.database`, while
            // its connection's current database can differ from a transfer
            // scope. Pass the explicit catalog to the driver-owned query.
            format!("{}.{}", endpoint.database, table)
        } else {
            super::metadata::metadata_relation_ref(endpoint, table)?
        };
        let full = crate::transfer::full_types::fetch_full_column_types(
            adapter, driver, handle, &relation,
        )
        .await
        .map_err(TransferError::validation)?;
        for column in &mut schema.columns {
            if let Some(native) = full.get(&column.name) {
                column.data_type = native.clone();
            }
        }
    }
    Ok(())
}

/// Reject source-generated columns whose expression is absent from the
/// transfer structure IR. Driver adapters provide catalog SQL for their
/// generated-column metadata; absence of rows means this specific gap is not
/// present for the inspected relation.
pub async fn validate_source_structure_metadata(
    adapter: &dyn SyncSourceAdapter,
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    endpoint: &super::model::Endpoint,
    schemas: &HashMap<String, TableSchema>,
    inspected: &[TableInspectResult],
) -> Result<(), TransferError> {
    for table in inspected
        .iter()
        .filter(|table| table.enabled)
        .map(|table| table.source_table.as_str())
    {
        if !schemas.contains_key(table) {
            continue;
        }
        // Validate the source relation syntax while keeping catalog, schema,
        // and table as separate values for driver-owned catalog SQL.
        super::metadata::metadata_relation_ref(endpoint, table)?;
        let Some(sql) = adapter.unsupported_transfer_structure_query(
            &endpoint.database,
            endpoint.normalized_schema(),
            table,
        ) else {
            continue;
        };
        let result = driver
            .query(handle, &sql)
            .await
            .map_err(|error| TransferError::validation(error.to_string()))?;
        if let Some(object) = result.rows.iter().find_map(|row| match row.first() {
            Some(Some(datazen_driver_api::Value::String(object))) => Some(object.as_str()),
            _ => None,
        }) {
            return Err(TransferError::unsupported(format!(
                "source object '{object}' on '{table}' is not represented by the transfer structure plan"
            )));
        }
    }
    Ok(())
}

/// Validate the selected source-to-target columns before preview acceptance or
/// execution enters its first DDL/data write. Drivers opt into this
/// Data-Transfer-only seam; Data Sync and Schema Diff never call it.
pub fn validate_transfer_column_types(
    job: &TransferJob,
    inspected: &[TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
    source_adapter: &dyn SyncSourceAdapter,
    target_adapter: &dyn SyncTargetAdapter,
) -> Result<(), TransferError> {
    validate_transfer_source_columns(job, inspected, source_schemas, source_adapter)?;
    let structure_mode = matches!(
        job.mode,
        TransferMode::Structure | TransferMode::StructureAndData
    );
    for table in inspected.iter().filter(|table| table.enabled) {
        if !matches!(
            table.status,
            TableMappingStatus::Matched | TableMappingStatus::CreateNew
        ) {
            continue;
        }
        let creates_target = (structure_mode && table.create_new)
            || job.write_mode == super::model::WriteMode::DropCreateInsert;
        let transfers_data = matches!(
            job.mode,
            TransferMode::Data | TransferMode::StructureAndData
        );
        if !creates_target && !transfers_data {
            continue;
        }

        let schema = source_schemas.get(&table.source_table).ok_or_else(|| {
            TransferError::validation(format!(
                "source schema is unavailable for '{}' during transfer type validation",
                table.source_table
            ))
        })?;
        let active: Vec<_> = if table.column_mappings.is_empty() {
            schema
                .columns
                .iter()
                .map(|column| (column.name.as_str(), column.name.as_str(), None))
                .collect()
        } else {
            table
                .column_mappings
                .iter()
                .filter(|mapping| !mapping.skip)
                .map(|mapping| {
                    (
                        mapping.source_column.as_str(),
                        mapping.target_column.as_str(),
                        mapping.target_native_type.as_deref(),
                    )
                })
                .collect()
        };

        for (source_name, target_name, configured_native) in active {
            let source_column = schema
                .columns
                .iter()
                .find(|column| column.name == source_name)
                .ok_or_else(|| {
                    TransferError::validation(format!(
                        "source column '{}.{}' is unavailable during transfer type validation",
                        table.source_table, source_name
                    ))
                })?;
            let source_ir = source_adapter.column_to_ir(source_column, None);
            let inspected_target = if creates_target {
                None
            } else {
                table.target_column_types.get(target_name)
            };
            let target_native = if creates_target {
                configured_native
                    .filter(|native| !native.trim().is_empty())
                    .map(str::to_string)
                    .or_else(|| Some(inferred_create_native_type(&source_ir, target_adapter)))
            } else {
                inspected_target
                    .map(|metadata| metadata.native_type.clone())
                    .filter(|native| !native.trim().is_empty())
            };
            target_adapter
                .validate_transfer_column_type(
                    source_column,
                    &source_ir,
                    source_adapter.transfer_source_text_limit_bytes(source_column),
                    source_adapter.transfer_source_requires_collation_preservation(source_column)
                        && !job.options.use_target_default_collation,
                    source_adapter.transfer_source_type_is_native_only(source_column, &source_ir),
                    target_native.as_deref(),
                    creates_target,
                )
                .map_err(|reason| {
                    TransferError::unsupported(format!(
                        "cannot map source column '{}.{}' to '{}.{}': {reason}",
                        table.source_table, source_name, table.target_table, target_name
                    ))
                })?;
            if let Some(target_native) = target_native.as_deref() {
                target_adapter
                    .validate_transfer_target_character_metadata(
                        &source_ir,
                        target_native,
                        inspected_target.and_then(|metadata| metadata.character_set.as_deref()),
                        inspected_target.and_then(|metadata| metadata.collation.as_deref()),
                        creates_target,
                    )
                    .map_err(|reason| {
                        TransferError::unsupported(format!(
                            "cannot map source column '{}.{}' to '{}.{}': {reason}",
                            table.source_table, source_name, table.target_table, target_name
                        ))
                    })?;
            }
        }
    }
    Ok(())
}

/// Source-only half of transfer type preflight, also used by SQL-file exports
/// whose target dialect may intentionally use the source adapter's renderer.
/// This checks only mapped source columns and never adds structure-only checks to
/// Data + Insert.
pub fn validate_transfer_source_columns(
    job: &TransferJob,
    inspected: &[TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
    source_adapter: &dyn SyncSourceAdapter,
) -> Result<(), TransferError> {
    let needs_structure = matches!(
        job.mode,
        TransferMode::Structure | TransferMode::StructureAndData
    ) || job.write_mode == super::model::WriteMode::DropCreateInsert;
    let needs_data = matches!(
        job.mode,
        TransferMode::Data | TransferMode::StructureAndData
    );
    if !needs_structure && !needs_data {
        return Ok(());
    }
    for table in inspected.iter().filter(|table| table.enabled) {
        if !matches!(
            table.status,
            TableMappingStatus::Matched | TableMappingStatus::CreateNew
        ) {
            continue;
        }
        let creates_table = (matches!(
            job.mode,
            TransferMode::Structure | TransferMode::StructureAndData
        ) && table.create_new)
            || job.write_mode == super::model::WriteMode::DropCreateInsert;
        if !needs_data && !creates_table {
            continue;
        }
        let schema = source_schemas.get(&table.source_table).ok_or_else(|| {
            TransferError::validation(format!(
                "source schema is unavailable for '{}' during transfer source validation",
                table.source_table
            ))
        })?;
        let active_names: Vec<&str> = if table.column_mappings.is_empty() {
            schema
                .columns
                .iter()
                .map(|column| column.name.as_str())
                .collect()
        } else {
            table
                .column_mappings
                .iter()
                .filter(|mapping| !mapping.skip)
                .map(|mapping| mapping.source_column.as_str())
                .collect()
        };
        for source_name in active_names {
            let Some(source_column) = schema
                .columns
                .iter()
                .find(|column| column.name == source_name)
            else {
                return Err(TransferError::validation(format!(
                    "source column '{}.{}' is unavailable during transfer type validation",
                    table.source_table, source_name
                )));
            };
            source_adapter
                .validate_transfer_source_column(source_column)
                .map_err(|reason| {
                    TransferError::unsupported(format!(
                        "cannot transfer source column '{}.{}': {reason}",
                        table.source_table, source_name
                    ))
                })?;
        }
    }
    Ok(())
}

/// A DROP is unsafe while an unselected target relation has an incoming FK.
/// Inspect the live target catalog before claiming/writing the immutable plan;
/// this also catches dependencies added after preview.
pub async fn validate_drop_create_target_dependencies(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    endpoint: &super::model::Endpoint,
    plan: &[DdlPreviewItem],
) -> Result<(), TransferError> {
    let selected: HashSet<String> = plan
        .iter()
        .filter(|item| item.kind == DdlPreviewKind::DropTable)
        .map(|item| item.target_table.clone())
        .collect();
    if selected.is_empty() {
        return Ok(());
    }
    let tables = driver
        .get_tables(handle, &endpoint.database, None)
        .await
        .map_err(|error| TransferError::validation(error.to_string()))?;
    for owner in tables
        .iter()
        .filter(|table| matches!(&table.table_type, datazen_driver_api::TableType::Table))
    {
        let bare_owner = owner.name.rsplit('.').next().unwrap_or(&owner.name);
        let owner_schema = owner
            .schema
            .as_deref()
            .or_else(|| endpoint.normalized_schema());
        let owner_is_selected = selected.iter().any(|target_table| {
            target_table == bare_owner
                && endpoint
                    .normalized_schema()
                    .map_or(true, |target_schema| owner_schema == Some(target_schema))
        });
        if owner_is_selected {
            continue;
        }
        let table_schema = owner
            .schema
            .as_deref()
            .or_else(|| endpoint.normalized_schema());
        let schema = driver
            .get_table_schema(handle, bare_owner, &endpoint.database, table_schema)
            .await
            .map_err(|error| {
                TransferError::validation(format!(
                    "cannot verify Drop + Create dependencies for target table '{}': {error}",
                    owner.name
                ))
            })?;
        for foreign_key in &schema.foreign_keys {
            let referenced = foreign_key.referenced_table.trim();
            let parts: Vec<_> = referenced.split('.').collect();
            let referenced_table = parts.last().copied().unwrap_or(referenced);
            let referenced_schema = parts.get(parts.len().saturating_sub(2)).copied();
            let hits_selected = selected.iter().any(|target_table| {
                target_table == referenced_table
                    && (referenced_schema.is_none()
                        || endpoint.normalized_schema().is_none()
                        || referenced_schema == endpoint.normalized_schema())
            });
            if hits_selected {
                return Err(TransferError::validation(format!(
                    "Drop + Create cannot replace target table '{}' while unselected table '{}' has foreign key '{}'; remove or redirect that dependency first",
                    referenced, owner.name, foreign_key.name
                )));
            }
        }
    }
    Ok(())
}

pub fn source_schema_to_target_ir(
    src_adapter: &dyn SyncSourceAdapter,
    schema: &TableSchema,
    full_types: Option<&HashMap<String, String>>,
    target_table: &str,
) -> IRTable {
    let mut ir = src_adapter.table_to_ir(schema, full_types);
    ir.name = target_table.to_string();
    ir
}

pub fn column_ir_types_by_source(ir: &IRTable) -> HashMap<String, IRType> {
    ir.columns
        .iter()
        .map(|c| (c.name.clone(), c.ir_type.clone()))
        .collect()
}

pub fn table_mapping_for<'a>(
    job: &'a TransferJob,
    source_table: &str,
) -> Option<&'a super::model::TableMapping> {
    job.tables.iter().find(|m| m.source_table == source_table)
}

/// Fill `target_native_type` on create-new column mappings from IR → target native DDL types.
pub fn enrich_create_new_target_types(
    results: &mut [TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
    src_adapter: &dyn SyncSourceAdapter,
    tgt_adapter: &dyn SyncTargetAdapter,
) {
    for result in results.iter_mut() {
        if !result.create_new || result.source_table.is_empty() {
            continue;
        }
        let Some(schema) = source_schemas.get(&result.source_table) else {
            continue;
        };
        let ir = src_adapter.table_to_ir(schema, None);
        for col_map in &mut result.column_mappings {
            if col_map
                .target_native_type
                .as_ref()
                .is_some_and(|s| !s.trim().is_empty())
            {
                continue;
            }
            let Some(ir_col) = ir.columns.iter().find(|c| c.name == col_map.source_column) else {
                continue;
            };
            col_map.target_native_type = Some(inferred_create_native_type(ir_col, tgt_adapter));
        }
    }
}

/// The preview suggestion is also the baseline for deciding whether an
/// identity type was actually customized. Keeping this in one helper avoids
/// treating the populated suggestion field as a user override by itself.
fn inferred_create_native_type(column: &IRColumn, tgt_adapter: &dyn SyncTargetAdapter) -> String {
    let ddl_ir_type = if column.default_expr.is_some()
        && !tgt_adapter.transfer_allows_column_default(&column.ir_type)
    {
        tgt_adapter
            .transfer_default_capable_type_for(&column.ir_type)
            .unwrap_or_else(|| column.ir_type.clone())
    } else {
        column.ir_type.clone()
    };
    tgt_adapter.transfer_ir_type_to_native(&ddl_ir_type)
}

fn validate_identity_type_overrides(
    ir: &IRTable,
    mapping: &super::model::TableMapping,
    tgt_adapter: &dyn SyncTargetAdapter,
) -> Result<(), TransferError> {
    for column in ir.columns.iter().filter(|column| column.is_auto_increment) {
        let Some(binding) = mapping
            .column_mappings
            .iter()
            .find(|binding| binding.source_column == column.name && !binding.skip)
        else {
            continue;
        };
        let Some(native) = binding
            .target_native_type
            .as_deref()
            .map(str::trim)
            .filter(|native| !native.is_empty())
        else {
            continue;
        };
        let inferred = inferred_create_native_type(column, tgt_adapter);
        if !native.eq_ignore_ascii_case(inferred.trim()) {
            return Err(TransferError::unsupported(format!(
                "identity/auto-increment column '{}.{}' has custom target type '{}'; inferred type '{}' is required to preserve identity semantics",
                mapping.source_table, column.name, native, inferred
            )));
        }
    }
    Ok(())
}

/// Build the same mapped column projection used by CREATE so dependent-object
/// preflight can validate target capabilities against the actual target types.
pub fn mapped_target_table_ir(
    src_adapter: &dyn SyncSourceAdapter,
    tgt_adapter: &dyn SyncTargetAdapter,
    schema: &TableSchema,
    table: &TableInspectResult,
    job: &TransferJob,
) -> Result<IRTable, TransferError> {
    let mut ir = source_schema_to_target_ir(src_adapter, schema, None, &table.target_table);
    if let Some(mapping) = table_mapping_for(job, &table.source_table) {
        validate_identity_type_overrides(&ir, mapping, tgt_adapter)?;
        apply_column_type_overrides(&mut ir, mapping, tgt_adapter)?;
    }
    Ok(ir)
}

/// Apply the exact active projection to CREATE, including key renames. An empty
/// mapping means automatic identity mapping; an explicit all-skipped mapping fails.
pub fn apply_column_type_overrides(
    ir: &mut IRTable,
    mapping: &super::model::TableMapping,
    tgt_adapter: &dyn SyncTargetAdapter,
) -> Result<(), TransferError> {
    if mapping.column_mappings.is_empty() {
        return Ok(());
    }
    let original = ir.columns.clone();
    let mut projected = Vec::new();
    let mut names = std::collections::HashSet::new();
    let mut renamed_keys = HashMap::new();
    for binding in mapping.column_mappings.iter().filter(|c| !c.skip) {
        if binding.target_column.trim().is_empty() || !names.insert(binding.target_column.clone()) {
            return Err(TransferError::validation(
                "target columns must be nonempty and unique",
            ));
        }
        let mut column = original
            .iter()
            .find(|c| c.name == binding.source_column)
            .cloned()
            .ok_or_else(|| {
                TransferError::validation(format!(
                    "source column '{}' not found",
                    binding.source_column
                ))
            })?;
        if let Some(native) = binding
            .target_native_type
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if !native.eq_ignore_ascii_case(
                tgt_adapter
                    .transfer_ir_type_to_native(&column.ir_type)
                    .trim(),
            ) {
                column.ir_type = IRType::Other(native.to_string());
            }
        }
        renamed_keys.insert(column.name.clone(), binding.target_column.clone());
        column.name = binding.target_column.clone();
        projected.push(column);
    }
    if projected.is_empty() {
        return Err(TransferError::validation(
            "CREATE requires at least one active column",
        ));
    }
    // A subset of a composite primary key is not a primary key.
    let complete_key = ir
        .primary_keys
        .iter()
        .all(|key| renamed_keys.contains_key(key));
    ir.primary_keys = if complete_key {
        ir.primary_keys
            .iter()
            .filter_map(|key| renamed_keys.get(key).cloned())
            .collect()
    } else {
        Vec::new()
    };
    for column in &mut projected {
        column.is_primary_key = ir.primary_keys.contains(&column.name);
    }
    ir.columns = projected;
    Ok(())
}

/// The sole CREATE renderer used by preview, create-new, and drop/recreate.
pub fn mapped_create_ddl(
    src_adapter: &dyn SyncSourceAdapter,
    tgt_adapter: &dyn SyncTargetAdapter,
    schema: &TableSchema,
    table: &TableInspectResult,
    job: &TransferJob,
) -> Result<String, TransferError> {
    let mapping = table_mapping_for(job, &table.source_table);
    if !schema.check_constraints.is_empty() {
        return Err(TransferError::unsupported(format!(
            "table '{}' has CHECK constraints that the target adapter cannot preserve",
            table.source_table
        )));
    }
    for foreign_key in &schema.foreign_keys {
        if foreign_key.deferrability != datazen_driver_api::ForeignKeyDeferrability::NotDeferrable {
            return Err(TransferError::unsupported(format!(
                "foreign key '{}' on '{}' has unknown or deferrable timing that cannot be preserved",
                foreign_key.name, table.source_table
            )));
        }
    }
    let use_target_default_collation = job.options.use_target_default_collation;
    let mut source_table_options = schema.table_options.clone();
    if use_target_default_collation {
        source_table_options.collation = None;
        source_table_options.charset = None;
    }
    let table_options = tgt_adapter
        .render_source_table_options(&source_table_options)
        .map_err(|error| {
            TransferError::unsupported(format!(
                "cannot preserve table options on '{}': {error}",
                table.source_table
            ))
        })?;
    if let Some(ddl) = mapping
        .and_then(|m| m.ddl_override.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if table_options.is_some() {
            return Err(TransferError::unsupported(format!(
                "DDL override for '{}' cannot prove that the captured table options are preserved",
                table.source_table
            )));
        }
        if schema.columns.iter().any(|column| column.is_auto_increment) {
            return Err(TransferError::unsupported(format!(
                "DDL override for '{}' cannot prove that its identity/auto-increment columns are preserved",
                table.source_table
            )));
        }
        return Ok(ddl.to_string());
    }
    let mut ir = mapped_target_table_ir(src_adapter, tgt_adapter, schema, table, job)?;
    ir.table_options = table_options;
    for column in &ir.columns {
        if let Some(default) = &column.default_expr {
            if matches!(default, IRDefault::RawExpression(_)) {
                return Err(TransferError::unsupported(format!(
                    "default expression on '{}.{}' is not portable through the target renderer",
                    table.source_table, column.name
                )));
            }
            if tgt_adapter.format_default(default).is_none() {
                return Err(TransferError::unsupported(format!(
                    "default on '{}.{}' cannot be rendered by the target adapter and would be lost",
                    table.source_table, column.name
                )));
            }
            if !crate::transfer::ddl::transfer_column_default_is_supported(
                &column.ir_type,
                tgt_adapter,
            ) {
                return Err(TransferError::unsupported(format!(
                    "default on '{}.{}' cannot be represented by the target type",
                    table.source_table, column.name
                )));
            }
            if let IRDefault::Literal(value) = default {
                let value = value.trim();
                let portable = value.parse::<i64>().is_ok()
                    || value.parse::<f64>().is_ok()
                    || matches!(
                        value.to_ascii_lowercase().as_str(),
                        "true" | "false" | "null"
                    )
                    || (value.starts_with('\'') && value.ends_with('\''));
                if !portable {
                    return Err(TransferError::unsupported(format!(
                        "default on '{}.{}' is not a portable literal",
                        table.source_table, column.name
                    )));
                }
            }
        }
    }
    for column in ir.columns.iter().filter(|column| column.is_auto_increment) {
        if tgt_adapter.auto_increment_keyword().is_none() {
            return Err(TransferError::unsupported(format!(
                "identity/auto-increment column '{}' on '{}' cannot be rendered by the target adapter",
                column.name, table.source_table
            )));
        }
        if !matches!(
            column.ir_type,
            IRType::Int8 | IRType::Int16 | IRType::Int32 | IRType::Int64
        ) {
            return Err(TransferError::unsupported(format!(
                "identity/auto-increment column '{}' on '{}' requires an integer target type",
                column.name, table.source_table
            )));
        }
    }
    if job.mode == TransferMode::StructureAndData
        && !tgt_adapter.supports_explicit_identity_values()
        && ir.columns.iter().any(|column| column.is_auto_increment)
    {
        return Err(TransferError::unsupported(format!(
            "target cannot insert explicit values into identity columns for '{}'; choose Structure-only or remove the identity mapping",
            table.source_table
        )));
    }
    Ok(crate::transfer::ddl::build_transfer_create_table_ddl_ref(
        &ir,
        tgt_adapter,
        &target_relation_ref(job, &table.target_table, tgt_adapter),
    ))
}

/// CREATE tables marked `CreateNew` when mode includes structure.
pub async fn create_target_tables(
    src_adapter: &dyn SyncSourceAdapter,
    tgt_adapter: &dyn SyncTargetAdapter,
    _src_driver: &dyn DatabaseDriver,
    _src_handle: &ConnectionHandle,
    tgt_driver: &dyn DatabaseDriver,
    tgt_handle: &ConnectionHandle,
    job: &TransferJob,
    inspected: &[TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
    cancelled: Option<Arc<AtomicBool>>,
) -> Result<Vec<TableExecutionResult>, TransferError> {
    create_target_tables_with_write_observer(
        src_adapter,
        tgt_adapter,
        _src_driver,
        _src_handle,
        tgt_driver,
        tgt_handle,
        job,
        inspected,
        source_schemas,
        cancelled,
        None,
    )
    .await
}

pub async fn create_target_tables_with_write_observer(
    src_adapter: &dyn SyncSourceAdapter,
    tgt_adapter: &dyn SyncTargetAdapter,
    _src_driver: &dyn DatabaseDriver,
    _src_handle: &ConnectionHandle,
    tgt_driver: &dyn DatabaseDriver,
    tgt_handle: &ConnectionHandle,
    job: &TransferJob,
    inspected: &[TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
    cancelled: Option<Arc<AtomicBool>>,
    write_started: Option<&AtomicBool>,
) -> Result<Vec<TableExecutionResult>, TransferError> {
    if !matches!(
        job.mode,
        TransferMode::Structure | TransferMode::StructureAndData
    ) {
        return Ok(Vec::new());
    }

    if job.write_mode == super::model::WriteMode::DropCreateInsert
        && job.mode == TransferMode::StructureAndData
    {
        return Ok(Vec::new());
    }
    validate_transfer_column_types(job, inspected, source_schemas, src_adapter, tgt_adapter)?;
    let mut results = Vec::new();

    let create_tables: Vec<_> = inspected
        .iter()
        .filter(|t| t.enabled && t.status == TableMappingStatus::CreateNew)
        .collect();
    for (table_index, table) in create_tables.iter().enumerate() {
        if let Some(flag) = &cancelled {
            if flag.load(Ordering::SeqCst) {
                results.push(TableExecutionResult::database(
                    &table.source_table,
                    &table.target_table,
                    Some(0),
                    TableExecutionOutcome::NotStarted,
                    Some(
                        "transfer cancelled during structure; completed DDL remains applied".into(),
                    ),
                ));
                break;
            }
        }

        let Some(schema) = source_schemas.get(&table.source_table) else {
            results.push(TableExecutionResult::database(
                &table.source_table,
                &table.target_table,
                Some(0),
                TableExecutionOutcome::NotStarted,
                Some("source schema not loaded".into()),
            ));
            if job.options.stop_on_error {
                break;
            }
            continue;
        };

        let ddl = match mapped_create_ddl(src_adapter, tgt_adapter, schema, table, job) {
            Ok(ddl) => ddl,
            Err(error) => {
                results.push(TableExecutionResult::database(
                    &table.source_table,
                    &table.target_table,
                    Some(0),
                    TableExecutionOutcome::NotStarted,
                    Some(error.to_string()),
                ));
                if job.options.stop_on_error {
                    break;
                }
                continue;
            }
        };

        if let Some(write_started) = write_started {
            write_started.store(true, Ordering::SeqCst);
        }
        match tgt_driver.execute(tgt_handle, &ddl).await {
            Ok(_) => results.push(TableExecutionResult::database(
                &table.source_table,
                &table.target_table,
                Some(0),
                TableExecutionOutcome::Committed,
                None,
            )),
            Err(e) => {
                results.push(TableExecutionResult::database(
                    &table.source_table,
                    &table.target_table,
                    None,
                    TableExecutionOutcome::Unknown,
                    Some(format!("CREATE failed; outcome UNKNOWN: {e}")),
                ));
                for not_started in create_tables.iter().skip(table_index + 1) {
                    results.push(TableExecutionResult::database(
                        &not_started.source_table,
                        &not_started.target_table,
                        Some(0),
                        TableExecutionOutcome::NotStarted,
                        Some("not started because an earlier table has an unknown outcome".into()),
                    ));
                }
                break;
            }
        }
    }

    Ok(results)
}

#[derive(Debug)]
pub enum DropCreateFailure {
    NotStarted(TransferError),
    Unknown(TransferError),
}

/// DROP + CREATE (IR) for a single table (DropCreateInsert preamble).
pub async fn drop_and_recreate_table(
    src_adapter: &dyn SyncSourceAdapter,
    tgt_adapter: &dyn SyncTargetAdapter,
    _src_driver: &dyn DatabaseDriver,
    _src_handle: &ConnectionHandle,
    tgt_driver: &dyn DatabaseDriver,
    tgt_handle: &ConnectionHandle,
    table: &TableInspectResult,
    job: &TransferJob,
    source_schemas: &HashMap<String, TableSchema>,
    write_started: Option<&AtomicBool>,
) -> Result<(), DropCreateFailure> {
    let schema = source_schemas.get(&table.source_table).ok_or_else(|| {
        DropCreateFailure::NotStarted(TransferError::validation("source schema not loaded"))
    })?;
    validate_transfer_column_types(
        job,
        std::slice::from_ref(table),
        source_schemas,
        src_adapter,
        tgt_adapter,
    )
    .map_err(DropCreateFailure::NotStarted)?;
    // Validate/render everything before the destructive first statement.
    let ddl = mapped_create_ddl(src_adapter, tgt_adapter, schema, table, job)
        .map_err(DropCreateFailure::NotStarted)?;
    let drop_sql = format!(
        "DROP TABLE IF EXISTS {}",
        target_relation_ref(job, &table.target_table, tgt_adapter)
    );
    if let Some(write_started) = write_started {
        write_started.store(true, Ordering::SeqCst);
    }
    tgt_driver
        .execute(tgt_handle, &drop_sql)
        .await
        .map_err(|e| {
            DropCreateFailure::Unknown(TransferError::validation(format!(
                "DROP failed; outcome UNKNOWN: {e}"
            )))
        })?;
    tgt_driver.execute(tgt_handle, &ddl).await.map_err(|e| {
        DropCreateFailure::Unknown(TransferError::validation(format!(
            "CREATE failed after DROP; outcome UNKNOWN: {e}"
        )))
    })?;
    Ok(())
}

pub fn table_eligible_for_data(table: &TableInspectResult, job: &TransferJob) -> bool {
    if !table.enabled {
        return false;
    }
    match table.status {
        TableMappingStatus::Matched => true,
        TableMappingStatus::CreateNew => matches!(job.mode, TransferMode::StructureAndData),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::WriteMode;
    use super::*;
    use crate::transfer::ir::{IRColumn, IRDefault, IRTable, IRType};
    use datazen_driver_api::Value;

    struct DummyTarget;

    impl SyncTargetAdapter for DummyTarget {
        fn ir_type_to_native(&self, ir: &IRType) -> String {
            match ir {
                IRType::Int32 => "INT".into(),
                _ => "TEXT".into(),
            }
        }
        fn format_default(&self, d: &IRDefault) -> Option<String> {
            match d {
                IRDefault::Literal(s) => Some(s.clone()),
                _ => None,
            }
        }
        fn format_literal(&self, _v: &Option<Value>, _ir: &IRType) -> String {
            "NULL".into()
        }
    }

    #[test]
    fn drop_sql_uses_adapter_quoting() {
        let sql = build_drop_table_sql("users", &DummyTarget);
        assert_eq!(sql, r#"DROP TABLE IF EXISTS "users""#);
    }

    #[test]
    fn source_ir_uses_target_table_name() {
        let schema = TableSchema {
            table_name: "src_name".into(),
            columns: vec![datazen_driver_api::ColumnSchema {
                name: "id".into(),
                data_type: "int".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: false,
            }],
            primary_keys: vec!["id".into()],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: Default::default(),
        };

        struct SrcAdapter;
        impl SyncSourceAdapter for SrcAdapter {
            fn column_to_ir(
                &self,
                column: &datazen_driver_api::ColumnSchema,
                _native_full_type: Option<&str>,
            ) -> IRColumn {
                IRColumn {
                    name: column.name.clone(),
                    ir_type: IRType::Int32,
                    nullable: column.nullable,
                    default_expr: None,
                    is_primary_key: column.is_primary_key,
                    is_auto_increment: false,
                    comment: None,
                }
            }
        }

        let ir = source_schema_to_target_ir(&SrcAdapter, &schema, None, "tgt_name");
        assert_eq!(ir.name, "tgt_name");
        assert_eq!(ir.columns[0].name, "id");
    }

    #[test]
    fn enrich_create_new_fills_target_native_types() {
        let schema = TableSchema {
            table_name: "reviews".into(),
            columns: vec![
                datazen_driver_api::ColumnSchema {
                    name: "id".into(),
                    data_type: "bigint".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: true,
                    is_auto_increment: false,
                },
                datazen_driver_api::ColumnSchema {
                    name: "rating".into(),
                    data_type: "smallint".into(),
                    nullable: true,
                    default_value: None,
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
            ],
            primary_keys: vec!["id".into()],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: Default::default(),
        };
        let mut schemas = HashMap::new();
        schemas.insert("reviews".into(), schema);

        struct SrcAdapter;
        impl SyncSourceAdapter for SrcAdapter {
            fn column_to_ir(
                &self,
                column: &datazen_driver_api::ColumnSchema,
                _native_full_type: Option<&str>,
            ) -> IRColumn {
                let ir_type = match column.data_type.as_str() {
                    "bigint" => IRType::Int64,
                    "smallint" => IRType::Int16,
                    _ => IRType::Text,
                };
                IRColumn {
                    name: column.name.clone(),
                    ir_type,
                    nullable: column.nullable,
                    default_expr: None,
                    is_primary_key: column.is_primary_key,
                    is_auto_increment: false,
                    comment: None,
                }
            }
        }

        let mut results = vec![TableInspectResult {
            source_table: "reviews".into(),
            target_table: "reviews".into(),
            status: TableMappingStatus::CreateNew,
            create_new: true,
            enabled: true,
            column_mappings: vec![
                super::super::model::ColumnMapping {
                    source_column: "id".into(),
                    target_column: "id".into(),
                    skip: false,
                    target_native_type: None,
                },
                super::super::model::ColumnMapping {
                    source_column: "rating".into(),
                    target_column: "rating".into(),
                    skip: false,
                    target_native_type: None,
                },
            ],
            source_columns: vec!["id".into(), "rating".into()],
            source_primary_keys: vec!["id".into()],
            target_columns: vec![],
            source_column_types: HashMap::new(),
            target_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: None,
            recordset: None,
        }];

        struct TgtAdapter;
        impl SyncTargetAdapter for TgtAdapter {
            fn ir_type_to_native(&self, ir: &IRType) -> String {
                match ir {
                    IRType::Int64 => "BIGINT".into(),
                    IRType::Int16 => "SMALLINT".into(),
                    _ => "TEXT".into(),
                }
            }
            fn format_default(&self, d: &IRDefault) -> Option<String> {
                match d {
                    IRDefault::Literal(s) => Some(s.clone()),
                    _ => None,
                }
            }
            fn format_literal(&self, _v: &Option<Value>, _ir: &IRType) -> String {
                "NULL".into()
            }
        }

        enrich_create_new_target_types(&mut results, &schemas, &SrcAdapter, &TgtAdapter);
        assert_eq!(
            results[0].column_mappings[0].target_native_type.as_deref(),
            Some("BIGINT")
        );
        assert_eq!(
            results[0].column_mappings[1].target_native_type.as_deref(),
            Some("SMALLINT")
        );
    }

    #[test]
    fn apply_column_type_overrides_sets_ir_other() {
        let mut ir = IRTable {
            name: "t".into(),
            columns: vec![IRColumn {
                name: "id".into(),
                ir_type: IRType::Int32,
                nullable: false,
                default_expr: None,
                is_primary_key: true,
                is_auto_increment: false,
                comment: None,
            }],
            primary_keys: vec!["id".into()],
            table_options: None,
        };
        let mapping = super::super::model::TableMapping {
            source_table: "t".into(),
            target_table: "t".into(),
            create_new: true,
            enabled: true,
            column_mappings: vec![super::super::model::ColumnMapping {
                source_column: "id".into(),
                target_column: "id".into(),
                skip: false,
                target_native_type: Some("BIGINT".into()),
            }],
            ddl_override: None,
            source_filter: None,
            recordset: None,
        };
        apply_column_type_overrides(&mut ir, &mapping, &DummyTarget).unwrap();
        assert_eq!(ir.columns[0].ir_type, IRType::Other("BIGINT".into()));
    }

    #[test]
    fn apply_column_type_overrides_keeps_ir_when_native_matches_adapter() {
        let mut ir = IRTable {
            name: "t".into(),
            columns: vec![IRColumn {
                name: "created_at".into(),
                ir_type: IRType::Timestamp {
                    with_timezone: false,
                },
                nullable: false,
                default_expr: Some(IRDefault::CurrentTimestamp),
                is_primary_key: false,
                is_auto_increment: false,
                comment: None,
            }],
            primary_keys: vec![],
            table_options: None,
        };
        let mapping = super::super::model::TableMapping {
            source_table: "t".into(),
            target_table: "t".into(),
            create_new: true,
            enabled: true,
            column_mappings: vec![super::super::model::ColumnMapping {
                source_column: "created_at".into(),
                target_column: "created_at".into(),
                skip: false,
                target_native_type: Some("DATETIME".into()),
            }],
            ddl_override: None,
            source_filter: None,
            recordset: None,
        };

        struct TgtAdapter;
        impl SyncTargetAdapter for TgtAdapter {
            fn ir_type_to_native(&self, ir: &IRType) -> String {
                match ir {
                    IRType::Timestamp { .. } => "DATETIME".into(),
                    _ => "TEXT".into(),
                }
            }
            fn format_default(&self, d: &IRDefault) -> Option<String> {
                match d {
                    IRDefault::CurrentTimestamp => Some("CURRENT_TIMESTAMP".into()),
                    _ => None,
                }
            }
            fn allows_column_default(&self, ir: &IRType) -> bool {
                matches!(ir, IRType::Timestamp { .. })
            }
            fn format_literal(&self, _v: &Option<Value>, _ir: &IRType) -> String {
                "NULL".into()
            }
        }

        apply_column_type_overrides(&mut ir, &mapping, &TgtAdapter).unwrap();
        assert!(matches!(
            ir.columns[0].ir_type,
            IRType::Timestamp {
                with_timezone: false
            }
        ));
        assert!(ir.columns[0].default_expr.is_some());
    }

    #[test]
    fn create_new_eligible_for_data_in_structure_and_data() {
        let table = TableInspectResult {
            source_table: "a".into(),
            target_table: "b".into(),
            status: TableMappingStatus::CreateNew,
            create_new: true,
            enabled: true,
            column_mappings: vec![],
            source_columns: vec![],
            source_primary_keys: vec![],
            target_columns: vec![],
            source_column_types: HashMap::new(),
            target_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: None,
            recordset: None,
        };
        let job = TransferJob {
            source: super::super::model::Endpoint {
                db_session_id: "s".into(),
                database: "db".into(),
                schema: None,
            },
            target: Some(super::super::model::Endpoint {
                db_session_id: "t".into(),
                database: "db".into(),
                schema: None,
            }),
            sql_file_target: None,
            mode: TransferMode::StructureAndData,
            write_mode: WriteMode::Insert,
            tables: vec![],
            options: super::super::model::TransferOptions::default(),
        };
        assert!(table_eligible_for_data(&table, &job));
    }

    #[tokio::test]
    async fn unknown_create_outcome_stops_following_structure_tables() {
        use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
        use std::sync::atomic::AtomicBool;

        struct SourceAdapter;
        impl SyncSourceAdapter for SourceAdapter {
            fn column_to_ir(
                &self,
                column: &datazen_driver_api::ColumnSchema,
                _native_full_type: Option<&str>,
            ) -> IRColumn {
                IRColumn {
                    name: column.name.clone(),
                    ir_type: IRType::Int32,
                    nullable: column.nullable,
                    default_expr: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                    comment: None,
                }
            }
        }

        let schema = TableSchema {
            table_name: "a".into(),
            columns: vec![datazen_driver_api::ColumnSchema {
                name: "id".into(),
                data_type: "int".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            }],
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: Default::default(),
        };
        let inspected: Vec<_> = ["a", "b"]
            .into_iter()
            .map(|name| TableInspectResult {
                source_table: name.into(),
                target_table: name.into(),
                status: TableMappingStatus::CreateNew,
                create_new: true,
                enabled: true,
                column_mappings: vec![super::super::model::ColumnMapping {
                    source_column: "id".into(),
                    target_column: "id".into(),
                    skip: false,
                    target_native_type: None,
                }],
                source_primary_keys: vec![],
                source_columns: vec!["id".into()],
                target_columns: vec![],
                source_column_types: HashMap::new(),
                target_column_types: HashMap::new(),
                incompatible_reason: None,
                source_row_count: None,
                recordset: None,
            })
            .collect();
        let mut job = TransferJob {
            source: super::super::model::Endpoint {
                db_session_id: "source".into(),
                database: "db".into(),
                schema: None,
            },
            target: Some(super::super::model::Endpoint {
                db_session_id: "target".into(),
                database: "db".into(),
                schema: None,
            }),
            sql_file_target: None,
            mode: TransferMode::StructureAndData,
            write_mode: WriteMode::Insert,
            tables: vec![
                super::super::model::TableMapping::auto("a"),
                super::super::model::TableMapping::auto("b"),
            ],
            options: super::super::model::TransferOptions::default(),
        };
        job.options.stop_on_error = false;
        let source_schemas =
            HashMap::from([("a".into(), schema.clone()), ("b".into(), schema.clone())]);
        let source_driver = MockDriver::new("postgres", MockDriverOptions::default());
        let target_driver = MockDriver::new(
            "mysql",
            MockDriverOptions {
                execute_error: Some("injected CREATE acknowledgement loss".into()),
                ..Default::default()
            },
        );
        let source_handle = ConnectionHandle {
            id: "source".into(),
            pool_id: "source".into(),
        };
        let target_handle = ConnectionHandle {
            id: "target".into(),
            pool_id: "target".into(),
        };
        let write_started = AtomicBool::new(false);

        let results = create_target_tables_with_write_observer(
            &SourceAdapter,
            &DummyTarget,
            source_driver.as_ref(),
            &source_handle,
            target_driver.as_ref(),
            &target_handle,
            &job,
            &inspected,
            &source_schemas,
            None,
            Some(&write_started),
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].outcome, Some(TableExecutionOutcome::Unknown));
        assert_eq!(results[0].rows_inserted, None);
        assert_eq!(results[1].outcome, Some(TableExecutionOutcome::NotStarted));
        assert_eq!(target_driver.execute_calls(), 1);
        assert!(write_started.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn structure_preflight_failures_are_not_started_and_success_can_continue() {
        use crate::transfer::ir::IRColumn;
        use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};

        struct SourceAdapter {
            reject_columns: bool,
        }
        impl SyncSourceAdapter for SourceAdapter {
            fn validate_transfer_source_column(
                &self,
                _column: &datazen_driver_api::ColumnSchema,
            ) -> Result<(), String> {
                if self.reject_columns {
                    Err("source array columns are not decodable".into())
                } else {
                    Ok(())
                }
            }

            fn column_to_ir(
                &self,
                column: &datazen_driver_api::ColumnSchema,
                _native_full_type: Option<&str>,
            ) -> IRColumn {
                IRColumn {
                    name: column.name.clone(),
                    ir_type: IRType::Int32,
                    nullable: column.nullable,
                    default_expr: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                    comment: None,
                }
            }
        }

        let schema = TableSchema {
            table_name: "fixture".into(),
            columns: vec![datazen_driver_api::ColumnSchema {
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            }],
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: Default::default(),
        };
        let inspected: Vec<_> = ["a", "b", "c"]
            .into_iter()
            .map(|name| TableInspectResult {
                source_table: name.into(),
                target_table: name.into(),
                status: TableMappingStatus::CreateNew,
                create_new: true,
                enabled: true,
                column_mappings: vec![super::super::model::ColumnMapping {
                    source_column: "id".into(),
                    target_column: "id".into(),
                    skip: false,
                    target_native_type: None,
                }],
                source_primary_keys: vec![],
                source_columns: vec!["id".into()],
                target_columns: vec![],
                source_column_types: HashMap::new(),
                target_column_types: HashMap::new(),
                incompatible_reason: None,
                source_row_count: None,
                recordset: None,
            })
            .collect();
        let mut mappings: Vec<_> = ["a", "b", "c"]
            .into_iter()
            .map(super::super::model::TableMapping::auto)
            .collect();
        mappings[1].column_mappings = vec![super::super::model::ColumnMapping {
            source_column: "missing".into(),
            target_column: "id".into(),
            skip: false,
            target_native_type: None,
        }];
        let mut job = TransferJob {
            source: super::super::model::Endpoint {
                db_session_id: "source".into(),
                database: "db".into(),
                schema: None,
            },
            target: Some(super::super::model::Endpoint {
                db_session_id: "target".into(),
                database: "db".into(),
                schema: None,
            }),
            sql_file_target: None,
            mode: TransferMode::Structure,
            write_mode: WriteMode::Insert,
            tables: mappings,
            options: super::super::model::TransferOptions::default(),
        };
        job.options.stop_on_error = false;
        let source_schemas = HashMap::from([
            ("a".into(), schema.clone()),
            ("b".into(), schema.clone()),
            ("c".into(), schema),
        ]);
        let source_driver = MockDriver::new("postgres", MockDriverOptions::default());
        let target_driver = MockDriver::new("mysql", MockDriverOptions::default());
        let source_handle = ConnectionHandle {
            id: "source".into(),
            pool_id: "source".into(),
        };
        let target_handle = ConnectionHandle {
            id: "target".into(),
            pool_id: "target".into(),
        };
        let write_started = AtomicBool::new(false);
        let preflight_error = create_target_tables_with_write_observer(
            &SourceAdapter {
                reject_columns: true,
            },
            &DummyTarget,
            source_driver.as_ref(),
            &source_handle,
            target_driver.as_ref(),
            &target_handle,
            &job,
            &inspected,
            &source_schemas,
            None,
            Some(&write_started),
        )
        .await
        .unwrap_err();
        assert!(preflight_error.to_string().contains("not decodable"));
        assert_eq!(target_driver.execute_calls(), 0);
        assert!(!write_started.load(Ordering::SeqCst));

        let valid_inspected = vec![inspected[2].clone()];
        let mut valid_job = job.clone();
        valid_job.tables = vec![super::super::model::TableMapping::auto("c")];
        let results = create_target_tables_with_write_observer(
            &SourceAdapter {
                reject_columns: false,
            },
            &DummyTarget,
            source_driver.as_ref(),
            &source_handle,
            target_driver.as_ref(),
            &target_handle,
            &valid_job,
            &valid_inspected,
            &source_schemas,
            None,
            Some(&write_started),
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].outcome, Some(TableExecutionOutcome::Committed));
        assert_eq!(target_driver.execute_calls(), 1);
        assert!(write_started.load(Ordering::SeqCst));

        let cancelled_target = MockDriver::new("mysql", MockDriverOptions::default());
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let cancelled_write_started = AtomicBool::new(false);
        let cancelled_results = create_target_tables_with_write_observer(
            &SourceAdapter {
                reject_columns: false,
            },
            &DummyTarget,
            source_driver.as_ref(),
            &source_handle,
            cancelled_target.as_ref(),
            &target_handle,
            &valid_job,
            &valid_inspected,
            &source_schemas,
            Some(cancelled),
            Some(&cancelled_write_started),
        )
        .await
        .unwrap();
        assert_eq!(cancelled_results.len(), 1);
        assert_eq!(
            cancelled_results[0].outcome,
            Some(TableExecutionOutcome::NotStarted)
        );
        assert_eq!(cancelled_target.execute_calls(), 0);
        assert!(!cancelled_write_started.load(Ordering::SeqCst));

        valid_job.mode = TransferMode::StructureAndData;
        valid_job.write_mode = WriteMode::DropCreateInsert;
        let skipped_structure = create_target_tables_with_write_observer(
            &SourceAdapter {
                reject_columns: false,
            },
            &DummyTarget,
            source_driver.as_ref(),
            &source_handle,
            cancelled_target.as_ref(),
            &target_handle,
            &valid_job,
            &valid_inspected,
            &source_schemas,
            None,
            None,
        )
        .await
        .unwrap();
        assert!(skipped_structure.is_empty());

        let mut data_only = valid_job.clone();
        data_only.mode = TransferMode::Data;
        data_only.write_mode = WriteMode::Insert;
        let no_structure = create_target_tables(
            &SourceAdapter {
                reject_columns: false,
            },
            &DummyTarget,
            source_driver.as_ref(),
            &source_handle,
            target_driver.as_ref(),
            &target_handle,
            &data_only,
            &valid_inspected,
            &source_schemas,
            None,
        )
        .await
        .unwrap();
        assert!(no_structure.is_empty());
    }

    #[test]
    fn mapped_create_projects_renames_and_does_not_invent_partial_primary_key() {
        let mut ir = IRTable {
            name: "copy".into(),
            columns: ["id", "tenant", "payload"]
                .iter()
                .map(|name| IRColumn {
                    name: (*name).into(),
                    ir_type: IRType::Int32,
                    nullable: false,
                    default_expr: None,
                    is_primary_key: *name != "payload",
                    is_auto_increment: false,
                    comment: None,
                })
                .collect(),
            primary_keys: vec!["id".into(), "tenant".into()],
            table_options: None,
        };
        let mut mapping = super::super::model::TableMapping::auto("source");
        mapping.column_mappings = vec![
            super::super::model::ColumnMapping {
                source_column: "payload".into(),
                target_column: "renamed".into(),
                skip: false,
                target_native_type: None,
            },
            super::super::model::ColumnMapping {
                source_column: "id".into(),
                target_column: "new_id".into(),
                skip: false,
                target_native_type: None,
            },
            super::super::model::ColumnMapping {
                source_column: "tenant".into(),
                target_column: "tenant".into(),
                skip: true,
                target_native_type: None,
            },
        ];
        apply_column_type_overrides(&mut ir, &mapping, &DummyTarget).unwrap();
        assert_eq!(
            ir.columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["renamed", "new_id"]
        );
        assert!(ir.primary_keys.is_empty());
        let sql = crate::transfer::ddl::build_create_table_ddl_ref(
            &ir,
            &DummyTarget,
            "\"dest\".\"copy\"",
        );
        assert!(sql.starts_with("CREATE TABLE \"dest\".\"copy\""));
        assert!(!sql.contains("tenant"));
        assert!(!sql.contains("PRIMARY KEY"));
        mapping.column_mappings[1].target_column = "renamed".into();
        assert!(apply_column_type_overrides(&mut ir, &mapping, &DummyTarget).is_err());
    }
}

//! SQL generation for one reviewed batch.
//!
//! The handler hands over the rows it is about to write and receives the
//! statements for the live target dialect. Nothing here re-reads schemas or
//! re-derives a projection: those were cached by `mapped_table_schemas` during
//! the compare Job, and §5.1 forbids executing anything that was not in the
//! reviewed selection.

use std::collections::HashMap;

use datazen_driver_api::{IRType, Value};

use crate::data_sync::job::artifact::RelationIdentity;
use crate::data_sync::{
    generate_table_sql_with_qualified_table_and_policy, ChangeOperation, DataSyncError, RowChange,
    SqlStatement, SyncOptions, TableChangeSet, TableMappingStatus, TableResult,
};

use super::super::apply::{
    identity_insert_target_for_projection, resolve_projection_types, sqlserver_write_preflight,
    validate_projected_identity_values,
};
use super::{invalid, HostDataSync, RelationMeta};

pub(super) fn generate(
    host: &HostDataSync,
    relation: &RelationIdentity,
    source_table: &str,
    target_table: &str,
    changes: Vec<RowChange>,
    options: &SyncOptions,
) -> Result<Vec<SqlStatement>, DataSyncError> {
    options.validate()?;
    if relation.table != target_table {
        return Err(invalid(format!(
            "batch relation {} does not match target table {target_table}",
            relation.table
        )));
    }
    // The handler only ever sends reviewed, selected rows. Anything else would
    // silently shrink the batch below the committed range, so it is rejected
    // rather than dropped.
    if let Some(change) = changes
        .iter()
        .find(|change| !change.selected || change.operation == ChangeOperation::Unchanged)
    {
        return Err(invalid(format!(
            "batch for {} carries an unreviewed {} row; compare again",
            target_table,
            match change.operation {
                ChangeOperation::Insert => "insert",
                ChangeOperation::Update => "update",
                ChangeOperation::Delete => "delete",
                ChangeOperation::Unchanged => "unchanged",
            }
        )));
    }
    if changes.is_empty() {
        return Ok(Vec::new());
    }
    let meta = host.relation_meta(false, target_table)?;
    let slot = host.target_slot()?;
    let family = slot.session.family.clone();
    let preview_source = host
        .sync_source_adapter(&meta.db_type)
        .map_err(|error| invalid(error.to_string()))?;
    let preview_target = host
        .sync_target_adapter(&meta.db_type)
        .map_err(|error| invalid(error.to_string()))?;
    let change_set = TableChangeSet {
        source_table: source_table.to_string(),
        target_table: target_table.to_string(),
        changes,
    };
    let projection = projection_for(&meta);
    if family == "sqlserver" {
        sqlserver_write_preflight(
            &meta.schema_obj,
            &change_set,
            options.conflict_policy,
            preview_target.supports_explicit_identity_values(),
        )
        .map_err(invalid)?;
        if preview_target.supports_explicit_identity_values() {
            validate_projected_identity_values(&meta.schema_obj, &projection.columns, &change_set)
                .map_err(invalid)?;
        }
    }
    let column_types = resolve_projection_types(&projection, &meta.schema_obj, &family)
        .map_err(|error| invalid(error.to_string()))?;
    let preview_types = meta
        .schema_obj
        .columns
        .iter()
        .map(|column| {
            let ir = preview_source.column_to_ir(column, Some(&column.data_type));
            (column.name.clone(), ir.ir_type)
        })
        .collect::<HashMap<String, IRType>>();
    let preview_literal = |column: &str, value: &Option<Value>, _data_type: Option<&str>| {
        let ir_type = preview_types.get(column).ok_or_else(|| {
            DataSyncError::validation(format!(
                "target preview metadata for column '{column}' is missing; compare again"
            ))
        })?;
        Ok(preview_target.format_literal(value, ir_type))
    };
    let qualified_table =
        preview_target.qualify_relation(&meta.database, meta.schema.as_deref(), target_table);
    let statements = generate_table_sql_with_qualified_table_and_policy(
        &change_set,
        &qualified_table,
        &projection.primary_keys,
        &projection.columns,
        &column_types,
        |name| preview_target.quote_ident(name),
        |index, data_type| {
            slot.session
                .driver
                .parameter_placeholder(index, data_type)
                .map_err(|error| DataSyncError::validation(error.to_string()))
        },
        options.conflict_policy,
        preview_literal,
    )?;
    let has_insert = change_set
        .changes
        .iter()
        .any(|change| change.operation == ChangeOperation::Insert);
    let identity_insert_target = if has_insert {
        identity_insert_target_for_projection(
            &family,
            &meta.schema_obj,
            &projection.columns,
            &meta.database,
            meta.schema.as_deref(),
            target_table,
            preview_target.supports_explicit_identity_values(),
            slot.session
                .driver
                .explicit_identity_insert_requires_session_toggle(),
        )
    } else {
        None
    };
    Ok(statements
        .into_iter()
        .map(|mut statement| {
            if statement.operation == ChangeOperation::Insert {
                statement.identity_insert = identity_insert_target.clone();
            }
            statement
        })
        .collect())
}

/// The canonical projection cached at compare time. `resolve_projection_types`
/// re-validates it against the live target schema, which is what turns a stale
/// plan into a "compare again" rejection instead of a wrong write.
fn projection_for(meta: &RelationMeta) -> TableResult {
    TableResult {
        source_table: meta.table.clone(),
        target_table: meta.table.clone(),
        status: TableMappingStatus::Matched,
        incompatible_reason: None,
        columns: meta.columns.clone(),
        column_types: meta
            .columns
            .iter()
            .map(|column| meta.column_types.get(column).cloned().unwrap_or_default())
            .collect(),
        primary_keys: meta.pk_columns.clone(),
        unchanged_count: 0,
        rows: Vec::new(),
        warnings: Vec::new(),
        source_filter: None,
    }
}

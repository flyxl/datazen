//! Compare selected tables, generate ChangeSet SQL, and apply.

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use super::comparison_store::StreamingComparisonStoreWriter;
use super::inspect::inspect_data_sync_impl;
use super::keyset_source::DriverKeysetSource;
use super::plans;
use crate::data_sync::{
    compare_table_pages_to_sink, generate_table_sql_with_qualified_table_and_policy,
    ChangeOperation, ChangeSet, ComparisonResult, ConflictPolicy, DataSyncError, SyncOptions,
    SyncSourceFilter, TableChangeSet, TableMapping, TableMappingStatus, TableResult,
};
use crate::services::metadata_schema;
use std::collections::HashMap;

pub(crate) async fn compare_data_sync_impl(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    tables: Vec<String>,
    job_id: Option<String>,
    source_database: Option<String>,
    target_database: Option<String>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    options: SyncOptions,
    mappings: &[TableMapping],
    source_filters: &HashMap<String, SyncSourceFilter>,
) -> Result<plans::SyncComparisonPreview, CommandError> {
    let source_config = state
        .connection_manager
        .get_session_config(&source_db_session_id)
        .await
        .cmd_err("compare_data_sync")?;
    let target_config = state
        .connection_manager
        .get_session_config(&target_db_session_id)
        .await
        .cmd_err("compare_data_sync")?;
    crate::data_sync::require_data_sync_family(
        &source_config.database_type,
        &target_config.database_type,
    )?;
    let (src_driver, src_handle) = state
        .connection_manager
        .get_session(&source_db_session_id)
        .await
        .cmd_err("compare_data_sync")?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("compare_data_sync")?;

    let source_snapshot = src_driver
        .begin_read_snapshot(&src_handle)
        .await
        .cmd_err("compare_data_sync")?;
    let target_snapshot = match tgt_driver.begin_read_snapshot(&tgt_handle).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            if let Err(cleanup_error) = src_driver.rollback(source_snapshot).await {
                tracing::warn!(
                    error = %cleanup_error,
                    "failed to roll back source Data Sync read snapshot after target setup failed"
                );
            }
            return Err(error.into());
        }
    };

    let result = compare_data_sync_impl_inner(
        state,
        source_db_session_id,
        target_db_session_id,
        tables,
        job_id,
        source_database,
        target_database,
        source_schema,
        target_schema,
        options,
        mappings,
        source_filters,
    )
    .await;

    let source_cleanup = src_driver.rollback(source_snapshot).await;
    let target_cleanup = tgt_driver.rollback(target_snapshot).await;
    if let Err(error) = &source_cleanup {
        tracing::warn!(
            error = %error,
            "failed to roll back source Data Sync read snapshot"
        );
    }
    if let Err(error) = &target_cleanup {
        tracing::warn!(
            error = %error,
            "failed to roll back target Data Sync read snapshot"
        );
    }

    match result {
        Err(error) => Err(error),
        Ok(preview) => {
            source_cleanup.map_err(CommandError::from)?;
            target_cleanup.map_err(CommandError::from)?;
            Ok(preview)
        }
    }
}

async fn compare_data_sync_impl_inner(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    tables: Vec<String>,
    job_id: Option<String>,
    source_database: Option<String>,
    target_database: Option<String>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    options: SyncOptions,
    mappings: &[TableMapping],
    source_filters: &HashMap<String, SyncSourceFilter>,
) -> Result<plans::SyncComparisonPreview, CommandError> {
    let cancelled = match job_id.as_deref() {
        Some(id) => Some(super::jobs::ensure_job(id).await),
        None => None,
    };
    let src_config = state
        .connection_manager
        .get_session_config(&source_db_session_id)
        .await
        .cmd_err("compare_data_sync")?;
    let tgt_config = state
        .connection_manager
        .get_session_config(&target_db_session_id)
        .await
        .cmd_err("compare_data_sync")?;
    let source_database_name =
        super::types::resolve_db_name(source_database.as_deref(), src_config.database.as_deref());
    let target_database_name =
        super::types::resolve_db_name(target_database.as_deref(), tgt_config.database.as_deref());
    let family = crate::data_sync::require_data_sync_family(
        &src_config.database_type,
        &tgt_config.database_type,
    )?;
    let (src_driver, src_handle) = state
        .connection_manager
        .get_session(&source_db_session_id)
        .await
        .cmd_err("compare_data_sync")?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("compare_data_sync")?;
    let source_quote = src_driver.quote_char();
    let target_quote = tgt_driver.quote_char();
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
    let inspected = inspect_data_sync_impl(
        state,
        source_db_session_id.clone(),
        target_db_session_id.clone(),
        Some(source_database_name.clone()),
        Some(target_database_name.clone()),
        source_schema.clone(),
        target_schema.clone(),
        mappings,
    )
    .await?;
    let wanted: std::collections::HashSet<String> = tables.into_iter().collect();
    state
        .sync_adapters
        .ensure_pair(&src_config.database_type, &tgt_config.database_type)
        .map_err(CommandError::Validation)?;
    let src_key_adapter = state
        .sync_adapters
        .get_source(&src_config.database_type)
        .ok_or_else(|| {
            CommandError::Validation("source driver has no Data Sync key contract".into())
        })?;
    let tgt_key_adapter = state
        .sync_adapters
        .get_source(&tgt_config.database_type)
        .ok_or_else(|| {
            CommandError::Validation("target driver has no Data Sync key contract".into())
        })?;

    options.validate().map_err(CommandError::from)?;
    let mut comparison_writer =
        StreamingComparisonStoreWriter::new().map_err(CommandError::Validation)?;
    for mapping in inspected {
        if mapping.status != TableMappingStatus::Matched
            || (!wanted.is_empty() && !wanted.contains(&mapping.source_table))
        {
            comparison_writer
                .add_table(mapping)
                .map_err(CommandError::Validation)?;
            continue;
        }
        if cancelled
            .as_ref()
            .is_some_and(|c| c.load(std::sync::atomic::Ordering::SeqCst))
        {
            return Err(CommandError::from(
                crate::data_sync::DataSyncError::cancelled("compare cancelled"),
            ));
        }
        let schema = src_driver
            .get_table_schema(
                &src_handle,
                &mapping.source_table,
                &source_database_name,
                source_schema.as_deref(),
            )
            .await
            .cmd_err("compare_data_sync")?;
        let target_table_schema = tgt_driver
            .get_table_schema(
                &tgt_handle,
                &mapping.target_table,
                &target_database_name,
                target_schema.as_deref(),
            )
            .await
            .cmd_err("compare_data_sync")?;
        let pk_columns = schema.effective_primary_keys();
        let sync_filter = source_filters.get(&mapping.source_table).cloned();
        if let Some(filter) = sync_filter.as_ref() {
            super::filter_validation::validate_filter_schemas(
                filter,
                &schema,
                &target_table_schema,
                &mapping.source_table,
                &mapping.target_table,
            )?;
        }
        let column_names: Vec<String> = schema.columns.iter().map(|c| c.name.clone()).collect();
        let source_column_types: HashMap<String, String> = schema
            .columns
            .iter()
            .map(|column| (column.name.clone(), column.data_type.clone()))
            .collect();
        let target_column_types: HashMap<String, String> = target_table_schema
            .columns
            .iter()
            .map(|column| (column.name.clone(), column.data_type.clone()))
            .collect();
        let source_recordset_limit = sync_filter
            .as_ref()
            .map(|filter| filter.recordset_limit(&schema))
            .transpose()
            .map_err(|error| CommandError::Validation(error.to_string()))?
            .flatten();
        let target_recordset_limit = sync_filter
            .as_ref()
            .map(|filter| filter.recordset_limit(&target_table_schema))
            .transpose()
            .map_err(|error| CommandError::Validation(error.to_string()))?
            .flatten();
        let (src_contracts, tgt_contracts) = super::filter_validation::resolve_key_contracts(
            &pk_columns,
            src_key_adapter.as_ref(),
            tgt_key_adapter.as_ref(),
            &schema,
            &target_table_schema,
            &mapping.source_table,
            &mapping.target_table,
        )?;
        if let Some(filter) = sync_filter.as_ref() {
            super::filter_validation::validate_filter_endpoints(
                filter,
                &pk_columns,
                src_driver.as_ref(),
                tgt_driver.as_ref(),
                src_key_adapter.as_ref(),
                tgt_key_adapter.as_ref(),
                &schema,
                &target_table_schema,
                &src_contracts,
                &tgt_contracts,
                &mapping.source_table,
            )?;
        }
        let pk_indexes: Vec<usize> = pk_columns
            .iter()
            .filter_map(|pk| column_names.iter().position(|c| c == pk))
            .collect();
        let mut src_source = DriverKeysetSource::new(
            src_driver.clone(),
            src_handle.clone(),
            mapping.source_table.clone(),
            Some(source_database_name.clone()),
            source_schema.clone(),
            column_names.clone(),
            pk_columns.clone(),
            source_quote,
            &family,
            src_key_adapter.clone(),
            src_contracts,
            sync_filter.clone(),
            source_column_types,
            source_recordset_limit,
        )?;
        let mut tgt_source = DriverKeysetSource::new(
            tgt_driver.clone(),
            tgt_handle.clone(),
            mapping.target_table.clone(),
            Some(target_database_name.clone()),
            target_schema.clone(),
            column_names.clone(),
            pk_columns.clone(),
            target_quote,
            &family,
            tgt_key_adapter.clone(),
            tgt_contracts,
            sync_filter.clone(),
            target_column_types,
            target_recordset_limit,
        )?;
        let mut table_metadata = TableResult::matched(
            mapping.source_table.clone(),
            mapping.target_table.clone(),
            Vec::new(),
        );
        table_metadata.columns = column_names.clone();
        table_metadata.primary_keys = pk_columns.clone();
        table_metadata.column_types = schema.columns.iter().map(|c| c.data_type.clone()).collect();
        table_metadata.source_filter = sync_filter.clone();
        comparison_writer
            .begin_table(table_metadata)
            .map_err(CommandError::Validation)?;
        let table_result = compare_table_pages_to_sink(
            &mapping.source_table,
            &mapping.target_table,
            &pk_indexes,
            &column_names,
            &options,
            &mut src_source,
            &mut tgt_source,
            cancelled.clone(),
            &mut comparison_writer,
        )
        .await
        .map_err(CommandError::from)?;
        comparison_writer
            .finish_table(table_result.unchanged_count)
            .map_err(CommandError::Validation)?;
    }
    let comparison = comparison_writer
        .finish()
        .map_err(CommandError::Validation)?;
    let source_schema_name = source_schema.clone();
    let target_schema_name = target_schema.clone();
    let mut source_entries = Vec::new();
    let mut target_entries = Vec::new();
    for table in comparison
        .summaries()
        .map_err(CommandError::Validation)?
        .into_iter()
        .filter(|table| table.table.status == TableMappingStatus::Matched)
    {
        let source_schema_snapshot = src_driver
            .get_table_schema(
                &src_handle,
                &table.table.source_table,
                &source_database_name,
                source_schema.as_deref(),
            )
            .await
            .cmd_err("compare_data_sync")?;
        let target_schema_snapshot = tgt_driver
            .get_table_schema(
                &tgt_handle,
                &table.table.target_table,
                &target_database_name,
                target_schema.as_deref(),
            )
            .await
            .cmd_err("compare_data_sync")?;
        source_entries.push((
            table.table.source_table.clone(),
            Some(source_schema_snapshot),
            table.table.source_filter.clone(),
        ));
        target_entries.push((
            table.table.target_table.clone(),
            Some(target_schema_snapshot),
            table.table.source_filter.clone(),
        ));
    }
    let source_schema_fingerprint = plans::fingerprint_relations_with_filters(
        &source_database_name,
        source_schema_name.as_deref(),
        source_entries,
    )
    .map_err(CommandError::Validation)?;
    let target_schema_fingerprint = plans::fingerprint_relations_with_filters(
        &target_database_name,
        target_schema_name.as_deref(),
        target_entries,
    )
    .map_err(CommandError::Validation)?;
    plans::issue_plan_with_store(
        source_db_session_id,
        target_db_session_id,
        source_database_name,
        target_database_name,
        source_schema_name,
        target_schema_name,
        src_driver.as_ref(),
        tgt_driver.as_ref(),
        source_schema_fingerprint,
        target_schema_fingerprint,
        comparison,
        options,
        tgt_config.read_only,
    )
    .map_err(CommandError::Validation)
}

fn resolve_projection_types(
    projection: &TableResult,
    schema: &datazen_driver_api::TableSchema,
    family: &str,
) -> Result<Vec<String>, CommandError> {
    let columns = &projection.columns;
    let unique: std::collections::HashSet<_> = columns.iter().collect();
    if columns.is_empty()
        || unique.len() != columns.len()
        || columns.len() != schema.columns.len()
        || projection.column_types.len() != columns.len()
        || projection.primary_keys != schema.effective_primary_keys()
    {
        return Err(CommandError::Validation(
            "comparison projection is stale or missing; compare again".into(),
        ));
    }
    columns
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let column = schema
                .columns
                .iter()
                .find(|c| &c.name == name)
                .ok_or_else(|| {
                    CommandError::Validation(format!("target column {name} changed; compare again"))
                })?;
            if !crate::data_sync::types_eq::types_equivalent(
                family,
                &projection.column_types[index],
                &column.data_type,
            ) {
                return Err(CommandError::Validation(format!(
                    "target type for {name} changed; compare again"
                )));
            }
            Ok(column.data_type.clone())
        })
        .collect()
}

fn sqlserver_write_preflight(
    schema: &datazen_driver_api::TableSchema,
    changes: &TableChangeSet,
    conflict_policy: ConflictPolicy,
    target_supports_explicit_identity: bool,
) -> Result<(), String> {
    let has_insert = changes
        .changes
        .iter()
        .any(|change| change.operation == ChangeOperation::Insert);
    if has_insert && !target_supports_explicit_identity {
        if let Some(column) = schema
            .columns
            .iter()
            .find(|column| column.is_auto_increment)
        {
            return Err(format!(
                "SQL Server Data Sync cannot insert selected rows into identity column '{}' because scoped IDENTITY_INSERT support is unavailable",
                column.name
            ));
        }
    }

    let needs_optimistic_condition = conflict_policy != ConflictPolicy::Force
        && changes.changes.iter().any(|change| {
            matches!(
                change.operation,
                ChangeOperation::Update | ChangeOperation::Delete
            )
        });
    if needs_optimistic_condition {
        let primary_keys = schema.effective_primary_keys();
        for column in &schema.columns {
            if primary_keys.iter().any(|key| key == &column.name) {
                continue;
            }
            let base_type = column
                .data_type
                .trim()
                .split(['(', ' ', ','])
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if matches!(
                base_type.as_str(),
                "char" | "nchar" | "varchar" | "nvarchar" | "text" | "ntext"
            ) {
                return Err(format!(
                    "SQL Server optimistic write conditions cannot verify column '{}' exactly because collation and trailing-space comparison semantics are unavailable; choose an explicit conflict policy that does not compare the prior row or use a table without this column type",
                    column.name
                ));
            }
            if matches!(
                base_type.as_str(),
                "xml" | "image" | "geography" | "geometry" | "hierarchyid" | "sql_variant"
            ) {
                return Err(format!(
                    "SQL Server optimistic write conditions do not support equality for column '{}' of type '{}'; refusing to generate UPDATE/DELETE SQL",
                    column.name, column.data_type
                ));
            }
        }
    }
    Ok(())
}

pub(crate) async fn generate_data_sync_sql_impl(
    state: &AppState,
    target_db_session_id: String,
    tables: Vec<TableResult>,
    options: SyncOptions,
    target_database: Option<String>,
    target_schema: Option<String>,
) -> Result<Vec<crate::data_sync::SqlStatement>, CommandError> {
    options.validate().map_err(CommandError::from)?;
    let comparison = ComparisonResult::new(tables);
    let set = ChangeSet::from_comparison("ui-preview", &comparison, &options);
    set.validate_executable().map_err(CommandError::from)?;

    let tgt_config = state
        .connection_manager
        .get_session_config(&target_db_session_id)
        .await
        .cmd_err("generate_data_sync_sql")?;
    let family = crate::data_sync::require_data_sync_family(
        &tgt_config.database_type,
        &tgt_config.database_type,
    )?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("generate_data_sync_sql")?;
    let target_database_name =
        super::types::resolve_db_name(target_database.as_deref(), tgt_config.database.as_deref());
    let target_schema = metadata_schema(
        tgt_driver.as_ref(),
        target_schema.as_deref(),
        None,
        tgt_config.schema.as_deref(),
    );
    state
        .sync_adapters
        .ensure_type(&tgt_config.database_type)
        .map_err(CommandError::Validation)?;
    let preview_source = state
        .sync_adapters
        .get_source(&tgt_config.database_type)
        .ok_or_else(|| {
            CommandError::Validation("target driver has no SQL preview type adapter".into())
        })?;
    let preview_target = state
        .sync_adapters
        .get_target(&tgt_config.database_type)
        .ok_or_else(|| {
            CommandError::Validation("target driver has no SQL preview literal renderer".into())
        })?;

    let mut statements = Vec::new();
    for table in &set.tables {
        let schema = tgt_driver
            .get_table_schema(
                &tgt_handle,
                &table.target_table,
                &target_database_name,
                target_schema.as_deref(),
            )
            .await
            .cmd_err("generate_data_sync_sql")?;
        if family == "sqlserver" {
            sqlserver_write_preflight(
                &schema,
                table,
                options.conflict_policy,
                preview_target.supports_explicit_identity_values(),
            )
            .map_err(CommandError::Validation)?;
        }
        let projection = comparison
            .tables
            .iter()
            .find(|t| t.source_table == table.source_table && t.target_table == table.target_table)
            .ok_or_else(|| {
                CommandError::Validation("comparison projection missing; compare again".into())
            })?;
        let column_names = &projection.columns;
        let column_types = resolve_projection_types(projection, &schema, &family)?;
        let pk = &projection.primary_keys;
        let preview_types = schema
            .columns
            .iter()
            .map(|column| {
                let ir = preview_source.column_to_ir(column, Some(&column.data_type));
                (column.name.clone(), ir.ir_type)
            })
            .collect::<std::collections::HashMap<_, _>>();
        let preview_literal = |column: &str,
                               value: &Option<datazen_driver_api::Value>,
                               _data_type: Option<&str>|
         -> Result<String, DataSyncError> {
            let ir_type = preview_types.get(column).ok_or_else(|| {
                DataSyncError::validation(format!(
                    "target preview metadata for column '{column}' is missing; compare again"
                ))
            })?;
            Ok(preview_target.format_literal(value, ir_type))
        };
        let qualified_table = preview_target.qualify_relation(
            &target_database_name,
            target_schema.as_deref(),
            &table.target_table,
        );
        let stmts = generate_table_sql_with_qualified_table_and_policy(
            table,
            &qualified_table,
            pk,
            column_names,
            &column_types,
            |name| preview_target.quote_ident(name),
            |index, data_type| {
                tgt_driver
                    .parameter_placeholder(index, data_type)
                    .map_err(|error| DataSyncError::validation(error.to_string()))
            },
            options.conflict_policy,
            preview_literal,
        )
        .map_err(CommandError::from)?;
        statements.extend(stmts);
    }
    Ok(statements)
}

pub(crate) async fn apply_data_sync_impl(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    tables: Vec<String>,
    job_id: Option<String>,
    source_database: Option<String>,
    target_database: Option<String>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    options: SyncOptions,
) -> Result<crate::data_sync::ExecutionResult, CommandError> {
    let _ = (
        state,
        source_db_session_id,
        target_db_session_id,
        tables,
        job_id,
        source_database,
        target_database,
        source_schema,
        target_schema,
        options,
    );
    Err(CommandError::Validation("legacy apply cannot preserve reviewed row selection; compare, generate selected SQL, then execute that plan".into()))
}

/// Re-run inspect gates for selected tables; returns stale table names when structure/PK drifted.
pub(crate) async fn revalidate_data_sync_impl(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    tables: Vec<String>,
    source_database: Option<String>,
    target_database: Option<String>,
    source_schema: Option<String>,
    target_schema: Option<String>,
) -> Result<serde_json::Value, CommandError> {
    let inspected = inspect_data_sync_impl(
        state,
        source_db_session_id,
        target_db_session_id,
        source_database,
        target_database,
        source_schema,
        target_schema,
        &[],
    )
    .await?;
    let wanted: std::collections::HashSet<String> = tables.into_iter().collect();
    let mut stale = Vec::new();
    for row in inspected {
        if !wanted.is_empty()
            && !wanted.contains(&row.source_table)
            && !wanted.contains(&row.target_table)
        {
            continue;
        }
        if row.status != TableMappingStatus::Matched {
            stale.push(serde_json::json!({
                "sourceTable": row.source_table,
                "targetTable": row.target_table,
                "status": row.status,
                "reason": row.incompatible_reason,
            }));
        }
    }
    Ok(serde_json::json!({
        "ok": stale.is_empty(),
        "staleTables": stale,
    }))
}

#[cfg(test)]
mod tests {
    use super::sqlserver_write_preflight;
    use crate::commands::sync::types::{resolve_options, SyncOptionsInput};
    use crate::data_sync::{ChangeOperation, ConflictPolicy, RowChange, TableChangeSet};
    use datazen_driver_api::{ColumnSchema, TableSchema, Value};

    #[test]
    fn sqlserver_write_preflight_rejects_identity_inserts_without_session_support() {
        let schema = test_sqlserver_schema(true);
        let changes = TableChangeSet {
            source_table: "source".into(),
            target_table: "target".into(),
            changes: vec![RowChange {
                operation: ChangeOperation::Insert,
                key: vec![Value::Integer(1)],
                source_row: Some(vec![
                    Some(Value::Integer(1)),
                    Some(Value::String("x".into())),
                ]),
                target_row: None,
                changed_columns: Vec::new(),
                selected: true,
            }],
        };
        let error = sqlserver_write_preflight(&schema, &changes, ConflictPolicy::Abort, false)
            .expect_err("IDENTITY_INSERT must not be silently skipped");
        assert!(error.contains("IDENTITY_INSERT"), "{error}");
    }

    #[test]
    fn sqlserver_write_preflight_rejects_unverifiable_string_conditions() {
        let schema = test_sqlserver_schema(false);
        let changes = TableChangeSet {
            source_table: "source".into(),
            target_table: "target".into(),
            changes: vec![RowChange {
                operation: ChangeOperation::Update,
                key: vec![Value::Integer(1)],
                source_row: Some(vec![
                    Some(Value::Integer(1)),
                    Some(Value::String("new".into())),
                ]),
                target_row: Some(vec![
                    Some(Value::Integer(1)),
                    Some(Value::String("old".into())),
                ]),
                changed_columns: vec!["label".into()],
                selected: true,
            }],
        };
        let error = sqlserver_write_preflight(&schema, &changes, ConflictPolicy::Abort, false)
            .expect_err("case/trailing-space changes require exact SQL equality");
        assert!(error.contains("collation and trailing-space"), "{error}");
        assert!(
            sqlserver_write_preflight(&schema, &changes, ConflictPolicy::Force, false,).is_ok()
        );
    }

    fn test_sqlserver_schema(identity: bool) -> TableSchema {
        TableSchema {
            table_name: "target".into(),
            columns: vec![
                ColumnSchema {
                    name: "id".into(),
                    data_type: "int".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: true,
                    is_auto_increment: identity,
                },
                ColumnSchema {
                    name: "label".into(),
                    data_type: "nvarchar(64)".into(),
                    nullable: true,
                    default_value: None,
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
            ],
            primary_keys: vec!["id".into()],
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            check_constraints: Vec::new(),
            table_options: Default::default(),
        }
    }

    #[test]
    fn sync_options_input_overrides_defaults() {
        let input = SyncOptionsInput {
            insert: Some(false),
            update: Some(true),
            delete: Some(true),
            matching_strategy: None,
            batch_size: Some(50),
            large_value_mode: None,
            conflict_policy: None,
        };
        let opts = resolve_options(Some(input));
        assert!(!opts.insert);
        assert!(opts.update);
        assert!(opts.delete);
        assert_eq!(opts.batch_size, 50);
    }
    #[test]
    fn reordered_target_columns_keep_canonical_values_and_reject_drift() {
        use crate::data_sync::{
            generate_table_sql, ChangeSet, ComparisonResult, RowChange, SyncOptions, TableResult,
        };
        use datazen_driver_api::{ColumnSchema, TableSchema, Value};
        let opts = SyncOptions::default();
        let mut result = TableResult::matched(
            "source",
            "target",
            vec![RowChange::update(
                vec![Value::Integer(1)],
                vec![
                    Some(Value::Integer(1)),
                    Some(Value::Integer(10)),
                    Some(Value::Integer(20)),
                ],
                vec![
                    Some(Value::Integer(1)),
                    Some(Value::Integer(9)),
                    Some(Value::Integer(20)),
                ],
                vec!["a".into()],
                &opts,
            )],
        );
        result.columns = vec!["id".into(), "a".into(), "b".into()];
        result.column_types = vec!["INT".into(); 3];
        result.primary_keys = vec!["id".into()];
        let mut schema = TableSchema {
            table_name: "target".into(),
            columns: ["id", "b", "a"]
                .iter()
                .map(|name| ColumnSchema {
                    name: (*name).into(),
                    data_type: "INT".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: *name == "id",
                    is_auto_increment: false,
                })
                .collect(),
            primary_keys: vec!["id".into()],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: Default::default(),
        };
        let types = super::resolve_projection_types(&result, &schema, "mysql").unwrap();
        let set =
            ChangeSet::from_comparison("t", &ComparisonResult::new(vec![result.clone()]), &opts);
        let statements = generate_table_sql(
            &set.tables[0],
            None,
            &result.primary_keys,
            &result.columns,
            &types,
            |name| format!("`{name}`"),
            |_, _| "?".into(),
        )
        .unwrap();
        assert!(matches!(statements[0].parameters[0], Value::Integer(10)));
        assert_eq!(
            statements[0].sql,
            "UPDATE `target` SET `a` = ? WHERE `id` = ? AND (`a` = ? OR (`a` IS NULL AND ? IS NULL)) AND (`b` = ? OR (`b` IS NULL AND ? IS NULL))"
        );
        schema.primary_keys = vec!["b".into()];
        assert!(super::resolve_projection_types(&result, &schema, "mysql").is_err());
        schema.primary_keys = vec!["id".into()];
        schema.columns[2].data_type = "TEXT".into();
        assert!(super::resolve_projection_types(&result, &schema, "mysql").is_err());
        result.columns.clear();
        assert!(super::resolve_projection_types(&result, &schema, "mysql").is_err());
    }
}

//! Compare selected tables, generate ChangeSet SQL, and apply.

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use super::exec::execute_data_sync_impl;
use super::inspect::inspect_data_sync_impl;
use super::keyset_source::DriverKeysetSource;
pub(crate) use super::types::resolve_options;
use super::types::SyncOptionsInput;
use crate::data_sync::{
    compare_table_pages, generate_table_sql, mysql_placeholder, postgres_placeholder,
    quote_ident_sql, ChangeSet, ComparisonResult, SyncOptions, TableMapping, TableMappingStatus,
    TableResult,
};

fn ident_quote(family: &str) -> char {
    if family == "mysql" {
        '`'
    } else {
        '"'
    }
}

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
) -> Result<Vec<TableResult>, CommandError> {
    let cancelled = match job_id.as_deref() {
        Some(id) => Some(super::jobs::ensure_job(id).await),
        None => None,
    };
    let inspected = inspect_data_sync_impl(
        state,
        source_db_session_id.clone(),
        target_db_session_id.clone(),
        source_database.clone(),
        target_database.clone(),
        source_schema.clone(),
        target_schema.clone(),
        mappings,
    )
    .await?;
    let wanted: std::collections::HashSet<String> = tables.into_iter().collect();
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
    let family = crate::data_sync::require_data_sync_family(
        &src_config.database_type,
        &tgt_config.database_type,
    )?;
    let quote = ident_quote(&family);
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

    let src_db = source_database
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or(src_config.database.as_deref());
    let tgt_db = target_database
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or(tgt_config.database.as_deref());
    super::compare::maybe_use_database(src_driver.as_ref(), &src_handle, src_db).await?;
    super::compare::maybe_use_database(tgt_driver.as_ref(), &tgt_handle, tgt_db).await?;

    let mut out = Vec::new();
    for mapping in inspected {
        if mapping.status != TableMappingStatus::Matched
            || (!wanted.is_empty() && !wanted.contains(&mapping.source_table))
        {
            out.push(mapping);
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
            .get_table_schema(&src_handle, &mapping.source_table)
            .await
            .cmd_err("compare_data_sync")?;
        let column_names: Vec<String> = schema.columns.iter().map(|c| c.name.clone()).collect();
        let pk_columns = schema.primary_keys.clone();
        let pk_indexes: Vec<usize> = pk_columns
            .iter()
            .filter_map(|pk| column_names.iter().position(|c| c == pk))
            .collect();
        let mut src_source = DriverKeysetSource::new(
            src_driver.clone(),
            src_handle.clone(),
            mapping.source_table.clone(),
            source_schema.clone(),
            column_names.clone(),
            pk_columns.clone(),
            quote,
            &family,
        );
        let mut tgt_source = DriverKeysetSource::new(
            tgt_driver.clone(),
            tgt_handle.clone(),
            mapping.target_table.clone(),
            target_schema.clone(),
            column_names.clone(),
            pk_columns,
            quote,
            &family,
        );
        let table_result = compare_table_pages(
            &mapping.source_table,
            &mapping.target_table,
            &pk_indexes,
            &column_names,
            &options,
            &mut src_source,
            &mut tgt_source,
            cancelled.clone(),
        )
        .await
        .map_err(CommandError::from)?;
        out.push(table_result);
    }
    Ok(out)
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
    let quote = ident_quote(&family);
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("generate_data_sync_sql")?;

    let tgt_db = target_database
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or(tgt_config.database.as_deref());
    super::compare::maybe_use_database(tgt_driver.as_ref(), &tgt_handle, tgt_db).await?;

    let mut statements = Vec::new();
    for table in &set.tables {
        let schema = tgt_driver
            .get_table_schema(&tgt_handle, &table.target_table)
            .await
            .cmd_err("generate_data_sync_sql")?;
        let column_names: Vec<String> = schema.columns.iter().map(|c| c.name.clone()).collect();
        let pk = schema.primary_keys.clone();
        let stmts = if family == "mysql" {
            generate_table_sql(
                table,
                None,
                &pk,
                &column_names,
                |n| quote_ident_sql(n, quote),
                mysql_placeholder,
            )
        } else {
            generate_table_sql(
                table,
                target_schema.as_deref(),
                &pk,
                &column_names,
                |n| quote_ident_sql(n, quote),
                postgres_placeholder,
            )
        }
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
    let compared = compare_data_sync_impl(
        state,
        source_db_session_id.clone(),
        target_db_session_id.clone(),
        tables,
        job_id.clone(),
        source_database.clone(),
        target_database.clone(),
        source_schema.clone(),
        target_schema.clone(),
        options.clone(),
        &[],
    )
    .await?;
    let statements = generate_data_sync_sql_impl(
        state,
        target_db_session_id.clone(),
        compared,
        options,
        target_database.clone(),
        target_schema,
    )
    .await?;
    execute_data_sync_impl(
        state,
        target_db_session_id,
        statements,
        job_id,
        target_database,
    )
    .await
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
    use super::{ident_quote, resolve_options, SyncOptionsInput};

    #[test]
    fn mysql_uses_backticks_postgres_uses_double_quotes() {
        assert_eq!(ident_quote("mysql"), '`');
        assert_eq!(ident_quote("postgresql"), '"');
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
        };
        let opts = resolve_options(Some(input));
        assert!(!opts.insert);
        assert!(opts.update);
        assert!(opts.delete);
        assert_eq!(opts.batch_size, 50);
    }
}

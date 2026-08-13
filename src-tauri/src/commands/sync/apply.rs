//! Compare selected tables and apply the resulting ChangeSet.

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use super::exec::execute_data_sync_impl;
use super::inspect::inspect_data_sync_impl;
use crate::data_sync::{
    compare_sorted_rows, generate_table_sql, mysql_placeholder, postgres_placeholder,
    quote_ident_sql, ChangeSet, ComparisonResult, SyncOptions, TableMappingStatus, TableResult,
};
fn ident_quote(family: &str) -> char {
    if family == "mysql" {
        '`'
    } else {
        '"'
    }
}

fn select_sql(table: &str, columns: &[String], quote: char) -> String {
    let cols = columns
        .iter()
        .map(|c| quote_ident_sql(c, quote))
        .collect::<Vec<_>>()
        .join(", ");
    format!("SELECT {cols} FROM {}", quote_ident_sql(table, quote))
}

pub(crate) async fn compare_data_sync_impl(
    state: &AppState,
    source_connection_id: String,
    target_connection_id: String,
    tables: Vec<String>,
    job_id: Option<String>,
) -> Result<Vec<TableResult>, CommandError> {
    let cancelled = match job_id.as_deref() {
        Some(id) => Some(super::jobs::ensure_job(id).await),
        None => None,
    };
    let mappings = inspect_data_sync_impl(
        state,
        source_connection_id.clone(),
        target_connection_id.clone(),
    )
    .await?;
    let wanted: std::collections::HashSet<String> = tables.into_iter().collect();
    let src_config = state
        .connection_manager
        .get_connection_config(&source_connection_id)
        .await
        .cmd_err("compare_data_sync")?;
    let tgt_config = state
        .connection_manager
        .get_connection_config(&target_connection_id)
        .await
        .cmd_err("compare_data_sync")?;
    let family = crate::data_sync::require_data_sync_family(
        &src_config.database_type,
        &tgt_config.database_type,
    )?;
    let quote = ident_quote(&family);
    let (src_driver, src_handle) = state
        .connection_manager
        .get_connection(&source_connection_id)
        .await
        .cmd_err("compare_data_sync")?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_connection(&target_connection_id)
        .await
        .cmd_err("compare_data_sync")?;

    let options = SyncOptions::default();
    let mut out = Vec::new();
    for mapping in mappings {
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
        let pk_indexes: Vec<usize> = schema
            .primary_keys
            .iter()
            .filter_map(|pk| column_names.iter().position(|c| c == pk))
            .collect();
        let sql = select_sql(&mapping.source_table, &column_names, quote);
        let src_rows = src_driver
            .query(&src_handle, &sql)
            .await
            .cmd_err("compare_data_sync")?
            .rows;
        let tgt_sql = select_sql(&mapping.target_table, &column_names, quote);
        let tgt_rows = tgt_driver
            .query(&tgt_handle, &tgt_sql)
            .await
            .cmd_err("compare_data_sync")?
            .rows;
        let rows = compare_sorted_rows(&src_rows, &tgt_rows, &pk_indexes, &column_names, &options)
            .map_err(CommandError::from)?;
        out.push(TableResult::matched(
            mapping.source_table,
            mapping.target_table,
            rows,
        ));
    }
    Ok(out)
}

pub(crate) async fn apply_data_sync_impl(
    state: &AppState,
    source_connection_id: String,
    target_connection_id: String,
    tables: Vec<String>,
    job_id: Option<String>,
) -> Result<crate::data_sync::ExecutionResult, CommandError> {
    let compared = compare_data_sync_impl(
        state,
        source_connection_id.clone(),
        target_connection_id.clone(),
        tables,
        job_id.clone(),
    )
    .await?;
    let options = SyncOptions::default();
    let comparison = ComparisonResult::new(compared);
    let set = ChangeSet::from_comparison("ui-apply", &comparison, &options);
    let tgt_config = state
        .connection_manager
        .get_connection_config(&target_connection_id)
        .await
        .cmd_err("apply_data_sync")?;
    let family = crate::data_sync::require_data_sync_family(
        &tgt_config.database_type,
        &tgt_config.database_type,
    )?;
    let quote = ident_quote(&family);
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_connection(&target_connection_id)
        .await
        .cmd_err("apply_data_sync")?;
    let mut statements = Vec::new();
    for table in &set.tables {
        let schema = tgt_driver
            .get_table_schema(&tgt_handle, &table.target_table)
            .await
            .cmd_err("apply_data_sync")?;
        let column_names: Vec<String> = schema.columns.iter().map(|c| c.name.clone()).collect();
        let pk = schema.primary_keys.clone();
        let stmts = if family == "mysql" {
            generate_table_sql(
                table,
                &pk,
                &column_names,
                |n| quote_ident_sql(n, quote),
                mysql_placeholder,
            )
        } else {
            generate_table_sql(
                table,
                &pk,
                &column_names,
                |n| quote_ident_sql(n, quote),
                postgres_placeholder,
            )
        }
        .map_err(CommandError::from)?;
        statements.extend(stmts);
    }
    execute_data_sync_impl(state, target_connection_id, statements, job_id).await
}

#[cfg(test)]
mod tests {
    use super::ident_quote;

    #[test]
    fn mysql_uses_backticks_postgres_uses_double_quotes() {
        assert_eq!(ident_quote("mysql"), '`');
        assert_eq!(ident_quote("postgresql"), '"');
    }
}

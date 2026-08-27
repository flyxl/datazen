//! Batch INSERT execute path (same-family and IR).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use datazen_driver_api::TableSchema;

use crate::data_sync::sql::{format_literal, quote_ident_sql};
use crate::db::{ConnectionHandle, DatabaseDriver, Value};
use crate::transfer::adapter::SyncTargetAdapter;
use crate::transfer::ir::IRType;

use super::error::TransferError;
use super::model::{
    ColumnMapping, TableExecutionResult, TableInspectResult, TransferExecutionResult, TransferJob,
    TransferMode, WriteMode,
};
use super::structure::{drop_and_recreate_table, table_eligible_for_data};

pub struct DropCreateContext<'a> {
    pub src_adapter: &'a dyn crate::transfer::adapter::SyncSourceAdapter,
    pub tgt_adapter: &'a dyn SyncTargetAdapter,
    pub src_driver: &'a dyn DatabaseDriver,
    pub src_handle: &'a ConnectionHandle,
    pub tgt_driver: &'a dyn DatabaseDriver,
    pub tgt_handle: &'a ConnectionHandle,
    pub source_schemas: &'a HashMap<String, TableSchema>,
}

pub enum ValueFormatter<'a> {
    SameFamily {
        quote: char,
    },
    Ir {
        tgt_adapter: &'a dyn SyncTargetAdapter,
        source_column_ir_types: &'a HashMap<String, IRType>,
    },
}

pub fn is_self_table_overwrite(
    source_conn: &str,
    source_db: &str,
    target_conn: &str,
    target_db: &str,
    source_table: &str,
    target_table: &str,
) -> bool {
    source_conn == target_conn && source_db == target_db && source_table == target_table
}

pub fn active_column_mappings(mappings: &[ColumnMapping]) -> Vec<&ColumnMapping> {
    mappings.iter().filter(|m| !m.skip).collect()
}

#[allow(dead_code)]
pub fn build_insert_sql(
    table: &str,
    columns: &[&ColumnMapping],
    values: &[Option<Value>],
    quote: char,
) -> String {
    let q = |name: &str| quote_ident_sql(name, quote);
    let col_list: Vec<String> = columns.iter().map(|c| q(&c.target_column)).collect();
    let val_list: Vec<String> = values.iter().map(format_literal).collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({})",
        q(table),
        col_list.join(", "),
        val_list.join(", ")
    )
}

pub fn build_batch_insert_sql(
    table: &str,
    columns: &[&ColumnMapping],
    rows: &[Vec<Option<Value>>],
    quote: char,
) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let q = |name: &str| quote_ident_sql(name, quote);
    let col_list: Vec<String> = columns.iter().map(|c| q(&c.target_column)).collect();
    let value_groups: Vec<String> = rows
        .iter()
        .map(|row| {
            let vals: Vec<String> = row.iter().map(format_literal).collect();
            format!("({})", vals.join(", "))
        })
        .collect();
    format!(
        "INSERT INTO {} ({}) VALUES {}",
        q(table),
        col_list.join(", "),
        value_groups.join(", ")
    )
}

pub fn build_batch_insert_sql_ir(
    table: &str,
    columns: &[&ColumnMapping],
    rows: &[Vec<Option<Value>>],
    tgt_adapter: &dyn SyncTargetAdapter,
    source_column_ir_types: &HashMap<String, IRType>,
) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let q = |name: &str| tgt_adapter.quote_ident(name);
    let col_list: Vec<String> = columns.iter().map(|c| q(&c.target_column)).collect();
    let value_groups: Vec<String> = rows
        .iter()
        .map(|row| {
            let vals: Vec<String> = columns
                .iter()
                .zip(row.iter())
                .map(|(col, value)| {
                    let ir_type = source_column_ir_types
                        .get(col.source_column.as_str())
                        .cloned()
                        .unwrap_or(IRType::Text);
                    let transformed = tgt_adapter.transform_value(value, &ir_type);
                    tgt_adapter.format_literal(&transformed, &ir_type)
                })
                .collect();
            format!("({})", vals.join(", "))
        })
        .collect();
    format!(
        "INSERT INTO {} ({}) VALUES {}",
        q(table),
        col_list.join(", "),
        value_groups.join(", ")
    )
}

pub fn build_truncate_sql(table: &str, quote: char) -> String {
    format!("TRUNCATE TABLE {}", quote_ident_sql(table, quote))
}

pub fn map_row_values(
    source_row: &[Option<Value>],
    source_schema: &TableSchema,
    columns: &[&ColumnMapping],
) -> Result<Vec<Option<Value>>, TransferError> {
    let index: HashMap<&str, usize> = source_schema
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| (c.name.as_str(), i))
        .collect();
    let mut out = Vec::with_capacity(columns.len());
    for col in columns {
        let idx = index
            .get(col.source_column.as_str())
            .copied()
            .ok_or_else(|| {
                TransferError::validation(format!(
                    "source column '{}' not found",
                    col.source_column
                ))
            })?;
        out.push(source_row.get(idx).cloned().unwrap_or(None));
    }
    Ok(out)
}

fn build_insert_for_rows(
    table: &str,
    columns: &[&ColumnMapping],
    rows: &[Vec<Option<Value>>],
    formatter: &ValueFormatter<'_>,
) -> String {
    match formatter {
        ValueFormatter::SameFamily { quote } => {
            build_batch_insert_sql(table, columns, rows, *quote)
        }
        ValueFormatter::Ir {
            tgt_adapter,
            source_column_ir_types,
        } => build_batch_insert_sql_ir(table, columns, rows, *tgt_adapter, source_column_ir_types),
    }
}

pub async fn execute_transfer_data(
    src_driver: &dyn DatabaseDriver,
    src_handle: &ConnectionHandle,
    tgt_driver: &dyn DatabaseDriver,
    tgt_handle: &ConnectionHandle,
    job: &TransferJob,
    inspected: &[TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
    formatter: &ValueFormatter<'_>,
    drop_create: Option<&DropCreateContext<'_>>,
    target_read_only: bool,
    cancelled: Option<Arc<AtomicBool>>,
) -> Result<TransferExecutionResult, TransferError> {
    if target_read_only {
        return Err(TransferError::validation(
            "target connection is read-only; Data Transfer cannot execute",
        ));
    }

    if !matches!(
        job.mode,
        TransferMode::Data | TransferMode::StructureAndData
    ) {
        return Ok(TransferExecutionResult {
            tables: Vec::new(),
            rows_inserted: 0,
            cancelled: false,
            partial: false,
        });
    }

    if job.write_mode.is_destructive() && !job.options.confirmed_destructive {
        return Err(TransferError::validation(
            "destructive write mode requires confirmedDestructive",
        ));
    }

    job.options.validate()?;

    let quote = tgt_driver.quote_char();
    let batch = job.options.batch_size as usize;
    let mut tables_out = Vec::new();
    let mut total_rows = 0u64;
    let mut partial = false;

    for table in inspected.iter().filter(|t| table_eligible_for_data(t, job)) {
        if let Some(flag) = &cancelled {
            if flag.load(Ordering::SeqCst) {
                return Ok(TransferExecutionResult {
                    tables: tables_out,
                    rows_inserted: total_rows,
                    cancelled: true,
                    partial: true,
                });
            }
        }

        if is_self_table_overwrite(
            &job.source.db_session_id,
            &job.source.database,
            &job.target.db_session_id,
            &job.target.database,
            &table.source_table,
            &table.target_table,
        ) {
            return Err(TransferError::validation(format!(
                "self-overwrite of table '{}' is not allowed",
                table.source_table
            )));
        }

        let columns = active_column_mappings(&table.column_mappings);
        if columns.is_empty() {
            tables_out.push(TableExecutionResult {
                source_table: table.source_table.clone(),
                target_table: table.target_table.clone(),
                rows_inserted: 0,
                success: false,
                error: Some("no column mappings".into()),
            });
            if job.options.stop_on_error {
                partial = true;
                break;
            }
            continue;
        }

        let Some(src_schema) = source_schemas.get(&table.source_table) else {
            tables_out.push(TableExecutionResult {
                source_table: table.source_table.clone(),
                target_table: table.target_table.clone(),
                rows_inserted: 0,
                success: false,
                error: Some("source schema not loaded".into()),
            });
            if job.options.stop_on_error {
                partial = true;
                break;
            }
            continue;
        };

        if job.write_mode == WriteMode::DropCreateInsert {
            let Some(ctx) = drop_create else {
                return Err(TransferError::validation(
                    "drop+create requires IR adapters",
                ));
            };
            if let Err(e) = drop_and_recreate_table(
                ctx.src_adapter,
                ctx.tgt_adapter,
                ctx.src_driver,
                ctx.src_handle,
                ctx.tgt_driver,
                ctx.tgt_handle,
                &table.source_table,
                &table.target_table,
                ctx.source_schemas,
            )
            .await
            {
                tables_out.push(TableExecutionResult {
                    source_table: table.source_table.clone(),
                    target_table: table.target_table.clone(),
                    rows_inserted: 0,
                    success: false,
                    error: Some(e.to_string()),
                });
                if job.options.stop_on_error {
                    partial = true;
                    break;
                }
                continue;
            }
        } else if job.write_mode == WriteMode::TruncateInsert {
            let truncate_sql = build_truncate_sql(&table.target_table, quote);
            if let Err(e) = tgt_driver
                .execute(tgt_handle, &truncate_sql)
                .await
                .map_err(|e| TransferError::validation(e.to_string()))
            {
                tables_out.push(TableExecutionResult {
                    source_table: table.source_table.clone(),
                    target_table: table.target_table.clone(),
                    rows_inserted: 0,
                    success: false,
                    error: Some(format!("truncate failed: {e}")),
                });
                if job.options.stop_on_error {
                    partial = true;
                    break;
                }
                continue;
            }
        }

        let select_cols: Vec<String> = columns
            .iter()
            .map(|c| quote_ident_sql(&c.source_column, quote))
            .collect();
        let base_sql = format!(
            "SELECT {} FROM {}",
            select_cols.join(", "),
            quote_ident_sql(&table.source_table, quote)
        );

        let mut offset = 0usize;
        let mut table_rows = 0u64;
        let mut table_error: Option<String> = None;

        loop {
            if let Some(flag) = &cancelled {
                if flag.load(Ordering::SeqCst) {
                    return Ok(TransferExecutionResult {
                        tables: tables_out,
                        rows_inserted: total_rows,
                        cancelled: true,
                        partial: true,
                    });
                }
            }

            let sql = if src_driver.supports_offset() {
                format!("{base_sql} LIMIT {batch} OFFSET {offset}")
            } else if offset == 0 {
                base_sql.clone()
            } else {
                break;
            };

            let result = src_driver
                .query(src_handle, &sql)
                .await
                .map_err(|e| TransferError::validation(e.to_string()))?;

            if result.rows.is_empty() {
                break;
            }

            let mapped_rows: Result<Vec<Vec<Option<Value>>>, TransferError> = result
                .rows
                .iter()
                .map(|row| map_row_values(row, src_schema, &columns))
                .collect();

            match mapped_rows {
                Ok(rows) => {
                    let insert_sql =
                        build_insert_for_rows(&table.target_table, &columns, &rows, formatter);
                    if insert_sql.is_empty() {
                        break;
                    }
                    if let Err(e) = tgt_driver
                        .execute(tgt_handle, &insert_sql)
                        .await
                        .map_err(|e| TransferError::validation(e.to_string()))
                    {
                        table_error = Some(e.to_string());
                        if job.options.stop_on_error {
                            partial = true;
                        }
                        break;
                    }
                    let n = rows.len() as u64;
                    table_rows += n;
                    total_rows += n;
                }
                Err(e) => {
                    table_error = Some(e.to_string());
                    if job.options.stop_on_error {
                        partial = true;
                    }
                    break;
                }
            }

            if !src_driver.supports_offset() {
                break;
            }
            if result.rows.len() < batch {
                break;
            }
            offset += batch;
        }

        tables_out.push(TableExecutionResult {
            source_table: table.source_table.clone(),
            target_table: table.target_table.clone(),
            rows_inserted: table_rows,
            success: table_error.is_none(),
            error: table_error,
        });

        if partial {
            break;
        }
    }

    Ok(TransferExecutionResult {
        tables: tables_out,
        rows_inserted: total_rows,
        cancelled: false,
        partial,
    })
}

/// Same-family convenience wrapper (kept for tests and backward compatibility).
#[allow(dead_code)]
pub async fn execute_same_family_data(
    src_driver: &dyn DatabaseDriver,
    src_handle: &ConnectionHandle,
    tgt_driver: &dyn DatabaseDriver,
    tgt_handle: &ConnectionHandle,
    job: &TransferJob,
    inspected: &[TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
    target_read_only: bool,
    cancelled: Option<Arc<AtomicBool>>,
) -> Result<TransferExecutionResult, TransferError> {
    let quote = tgt_driver.quote_char();
    let formatter = ValueFormatter::SameFamily { quote };
    execute_transfer_data(
        src_driver,
        src_handle,
        tgt_driver,
        tgt_handle,
        job,
        inspected,
        source_schemas,
        &formatter,
        None,
        target_read_only,
        cancelled,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Value;

    #[test]
    fn self_overwrite_detected() {
        assert!(is_self_table_overwrite(
            "c1", "db", "c1", "db", "users", "users"
        ));
        assert!(!is_self_table_overwrite(
            "c1", "db", "c1", "db", "users", "clients"
        ));
    }

    #[test]
    fn batch_insert_sql() {
        let cols = vec![ColumnMapping {
            source_column: "id".into(),
            target_column: "id".into(),
            skip: false,
        }];
        let refs: Vec<&ColumnMapping> = cols.iter().collect();
        let sql = build_batch_insert_sql(
            "users",
            &refs,
            &[vec![Some(Value::Integer(1))], vec![Some(Value::Integer(2))]],
            '"',
        );
        assert!(sql.contains("INSERT INTO"));
        assert!(sql.contains("VALUES"));
    }

    #[test]
    fn truncate_sql_quotes_table() {
        let sql = build_truncate_sql("users", '"');
        assert_eq!(sql, r#"TRUNCATE TABLE "users""#);
    }

    #[test]
    fn ir_batch_insert_uses_adapter_literals() {
        struct LitTarget;
        impl SyncTargetAdapter for LitTarget {
            fn ir_type_to_native(&self, _ir: &IRType) -> String {
                "INT".into()
            }
            fn format_default(&self, _d: &crate::transfer::ir::IRDefault) -> Option<String> {
                None
            }
            fn format_literal(&self, value: &Option<Value>, _ir: &IRType) -> String {
                match value {
                    Some(Value::Integer(n)) => n.to_string(),
                    _ => "NULL".into(),
                }
            }
        }

        let cols = vec![ColumnMapping {
            source_column: "id".into(),
            target_column: "id".into(),
            skip: false,
        }];
        let refs: Vec<&ColumnMapping> = cols.iter().collect();
        let mut ir_types = HashMap::new();
        ir_types.insert("id".into(), IRType::Int32);
        let sql = build_batch_insert_sql_ir(
            "users",
            &refs,
            &[vec![Some(Value::Integer(42))]],
            &LitTarget,
            &ir_types,
        );
        assert!(sql.contains("INSERT INTO \"users\""));
        assert!(sql.contains("(42)"));
    }
}

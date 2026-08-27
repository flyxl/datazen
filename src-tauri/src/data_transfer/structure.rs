//! Structure phase: CREATE (IR) and DROP helpers for Data Transfer.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use datazen_driver_api::TableSchema;

use crate::db::{ConnectionHandle, DatabaseDriver};
use crate::transfer::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::transfer::ir::{IRTable, IRType};

use super::error::TransferError;
use super::model::{
    TableExecutionResult, TableInspectResult, TableMappingStatus, TransferJob, TransferMode,
};
use super::preview::build_create_ddl;

pub fn build_drop_table_sql(table: &str, tgt_adapter: &dyn SyncTargetAdapter) -> String {
    format!("DROP TABLE IF EXISTS {}", tgt_adapter.quote_ident(table))
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

pub async fn fetch_full_column_types(
    adapter: &dyn SyncSourceAdapter,
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    table: &str,
) -> Result<HashMap<String, String>, TransferError> {
    let Some(sql) = adapter.full_column_types_query(table) else {
        return Ok(HashMap::new());
    };
    let result = driver
        .query(handle, &sql)
        .await
        .map_err(|e| TransferError::validation(e.to_string()))?;
    let mut map = HashMap::new();
    for row in &result.rows {
        if let (
            Some(Some(crate::db::Value::String(name))),
            Some(Some(crate::db::Value::String(ft))),
        ) = (row.get(0), row.get(1))
        {
            map.insert(name.clone(), ft.clone());
        }
    }
    Ok(map)
}

pub fn column_ir_types_by_source(ir: &IRTable) -> HashMap<String, IRType> {
    ir.columns
        .iter()
        .map(|c| (c.name.clone(), c.ir_type.clone()))
        .collect()
}

/// CREATE tables marked `CreateNew` when mode includes structure.
pub async fn create_target_tables(
    src_adapter: &dyn SyncSourceAdapter,
    tgt_adapter: &dyn SyncTargetAdapter,
    src_driver: &dyn DatabaseDriver,
    src_handle: &ConnectionHandle,
    tgt_driver: &dyn DatabaseDriver,
    tgt_handle: &ConnectionHandle,
    job: &TransferJob,
    inspected: &[TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
    cancelled: Option<Arc<AtomicBool>>,
) -> Result<Vec<TableExecutionResult>, TransferError> {
    if !matches!(
        job.mode,
        TransferMode::Structure | TransferMode::StructureAndData
    ) {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    for table in inspected
        .iter()
        .filter(|t| t.enabled && t.status == TableMappingStatus::CreateNew)
    {
        if let Some(flag) = &cancelled {
            if flag.load(Ordering::SeqCst) {
                return Err(TransferError::cancelled(
                    "transfer cancelled during structure",
                ));
            }
        }

        let Some(schema) = source_schemas.get(&table.source_table) else {
            results.push(TableExecutionResult {
                source_table: table.source_table.clone(),
                target_table: table.target_table.clone(),
                rows_inserted: 0,
                success: false,
                error: Some("source schema not loaded".into()),
            });
            if job.options.stop_on_error {
                break;
            }
            continue;
        };

        let full_types =
            fetch_full_column_types(src_adapter, src_driver, src_handle, &table.source_table)
                .await
                .unwrap_or_default();
        let ir =
            source_schema_to_target_ir(src_adapter, schema, Some(&full_types), &table.target_table);
        let ddl = build_create_ddl(&ir, tgt_adapter);

        match tgt_driver.execute(tgt_handle, &ddl).await {
            Ok(_) => results.push(TableExecutionResult {
                source_table: table.source_table.clone(),
                target_table: table.target_table.clone(),
                rows_inserted: 0,
                success: true,
                error: None,
            }),
            Err(e) => {
                results.push(TableExecutionResult {
                    source_table: table.source_table.clone(),
                    target_table: table.target_table.clone(),
                    rows_inserted: 0,
                    success: false,
                    error: Some(format!("CREATE failed: {e}")),
                });
                if job.options.stop_on_error {
                    break;
                }
            }
        }
    }

    Ok(results)
}

/// DROP + CREATE (IR) for a single table (DropCreateInsert preamble).
pub async fn drop_and_recreate_table(
    src_adapter: &dyn SyncSourceAdapter,
    tgt_adapter: &dyn SyncTargetAdapter,
    src_driver: &dyn DatabaseDriver,
    src_handle: &ConnectionHandle,
    tgt_driver: &dyn DatabaseDriver,
    tgt_handle: &ConnectionHandle,
    source_table: &str,
    target_table: &str,
    source_schemas: &HashMap<String, TableSchema>,
) -> Result<(), TransferError> {
    let drop_sql = build_drop_table_sql(target_table, tgt_adapter);
    tgt_driver
        .execute(tgt_handle, &drop_sql)
        .await
        .map_err(|e| TransferError::validation(format!("DROP failed: {e}")))?;

    let Some(schema) = source_schemas.get(source_table) else {
        return Err(TransferError::validation(format!(
            "source schema not loaded for '{source_table}'"
        )));
    };

    let full_types = fetch_full_column_types(src_adapter, src_driver, src_handle, source_table)
        .await
        .unwrap_or_default();
    let ir = source_schema_to_target_ir(src_adapter, schema, Some(&full_types), target_table);
    let ddl = build_create_ddl(&ir, tgt_adapter);
    tgt_driver
        .execute(tgt_handle, &ddl)
        .await
        .map_err(|e| TransferError::validation(format!("CREATE failed: {e}")))?;
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
    use crate::db::Value;
    use crate::transfer::ir::{IRColumn, IRDefault};

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
    fn create_new_eligible_for_data_in_structure_and_data() {
        let table = TableInspectResult {
            source_table: "a".into(),
            target_table: "b".into(),
            status: TableMappingStatus::CreateNew,
            create_new: true,
            enabled: true,
            column_mappings: vec![],
            source_columns: vec![],
            target_columns: vec![],
            incompatible_reason: None,
            source_row_count: None,
        };
        let job = TransferJob {
            source: super::super::model::Endpoint {
                db_session_id: "s".into(),
                database: "db".into(),
                schema: None,
            },
            target: super::super::model::Endpoint {
                db_session_id: "t".into(),
                database: "db".into(),
                schema: None,
            },
            mode: TransferMode::StructureAndData,
            write_mode: WriteMode::Insert,
            tables: vec![],
            options: super::super::model::TransferOptions::default(),
        };
        assert!(table_eligible_for_data(&table, &job));
    }
}

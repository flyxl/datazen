//! Preview plan: DDL (IR) + write plan summary.

use std::collections::HashMap;

use datazen_driver_api::TableSchema;

use crate::transfer::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::transfer::ddl::build_create_table_ddl;
use crate::transfer::pairing::SyncPairing;

use super::error::TransferError;
use super::model::{
    DdlPreviewItem, TableInspectResult, TableMappingStatus, TransferJob, TransferMode,
    TransferPreview, WriteMode, WritePlanItem,
};
use super::structure::{
    apply_column_type_overrides, build_drop_table_sql, source_schema_to_target_ir,
    table_mapping_for,
};

/// Optional IR adapters for real DDL generation and execute eligibility.
pub struct TransferPreviewAdapters<'a> {
    pub src_adapter: &'a dyn SyncSourceAdapter,
    pub tgt_adapter: &'a dyn SyncTargetAdapter,
}

pub fn build_preview(
    job: &TransferJob,
    inspected: &[TableInspectResult],
    pairing: &SyncPairing,
    source_schemas: &HashMap<String, TableSchema>,
    target_read_only_ok: bool,
    adapters: Option<TransferPreviewAdapters<'_>>,
) -> Result<TransferPreview, TransferError> {
    job.options.validate()?;

    let mut warnings = Vec::new();
    let mut ddl = Vec::new();
    let mut write_plans = Vec::new();
    let mut block_reason: Option<String> = None;

    let needs_data = matches!(
        job.mode,
        TransferMode::Data | TransferMode::StructureAndData
    );
    let needs_structure = matches!(
        job.mode,
        TransferMode::Structure | TransferMode::StructureAndData
    );
    let adapters_available = adapters.is_some();
    let ir_pairing = matches!(pairing, SyncPairing::Ir);

    if job.write_mode.is_destructive() && !job.options.confirmed_destructive {
        block_reason = Some(
            "destructive write mode requires explicit confirmation (confirmedDestructive)".into(),
        );
    }

    if (needs_structure || job.write_mode == WriteMode::DropCreateInsert)
        && (ir_pairing || inspected.iter().any(|t| t.enabled && t.create_new))
        && !adapters_available
    {
        block_reason.get_or_insert_with(|| {
            "IR sync adapters are required for structure or drop+create operations".into()
        });
    }

    for table in inspected.iter().filter(|t| t.enabled) {
        if table.status == TableMappingStatus::Incompatible {
            block_reason.get_or_insert_with(|| {
                format!(
                    "table {} is incompatible: {}",
                    table.source_table,
                    table.incompatible_reason.clone().unwrap_or_default()
                )
            });
            continue;
        }

        let table_mapping = table_mapping_for(job, &table.source_table);

        let create_ddl_for_table = |source_table: &str, target_table: &str| -> Option<String> {
            let schema = source_schemas.get(source_table)?;
            if let Some(adapters) = &adapters {
                // Full types are resolved at execute time; preview uses schema types only.
                let mut ir =
                    source_schema_to_target_ir(adapters.src_adapter, schema, None, target_table);
                if let Some(mapping) = table_mapping {
                    apply_column_type_overrides(&mut ir, mapping);
                }
                Some(build_create_table_ddl(&ir, adapters.tgt_adapter))
            } else {
                None
            }
        };

        if needs_structure && table.create_new {
            if let Some(override_ddl) = table_mapping
                .and_then(|m| m.ddl_override.as_deref())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                ddl.push(DdlPreviewItem {
                    source_table: table.source_table.clone(),
                    target_table: table.target_table.clone(),
                    ddl: override_ddl.to_string(),
                });
            } else if let Some(ddl_sql) =
                create_ddl_for_table(&table.source_table, &table.target_table)
            {
                ddl.push(DdlPreviewItem {
                    source_table: table.source_table.clone(),
                    target_table: table.target_table.clone(),
                    ddl: ddl_sql,
                });
            } else if let Some(schema) = source_schemas.get(&table.source_table) {
                warnings.push(format!(
                    "CREATE TABLE for '{}' requires IR adapters at execute time",
                    table.target_table
                ));
                ddl.push(DdlPreviewItem {
                    source_table: table.source_table.clone(),
                    target_table: table.target_table.clone(),
                    ddl: format!(
                        "-- CREATE TABLE {} (from source schema; IR DDL at execute)\n-- columns: {}",
                        table.target_table,
                        schema
                            .columns
                            .iter()
                            .map(|c| format!("{} {}", c.name, c.data_type))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                });
            }
        }

        if needs_data {
            if table.create_new && !needs_structure {
                block_reason.get_or_insert_with(|| {
                    format!(
                        "data transfer to new table '{}' requires structure step first",
                        table.target_table
                    )
                });
                continue;
            }

            if !matches!(
                table.status,
                TableMappingStatus::Matched | TableMappingStatus::CreateNew
            ) {
                continue;
            }

            let active_cols: Vec<_> = table
                .column_mappings
                .iter()
                .filter(|c| !c.skip)
                .cloned()
                .collect();
            if active_cols.is_empty() && table.status == TableMappingStatus::Matched {
                block_reason.get_or_insert_with(|| {
                    format!("no column mappings for table '{}'", table.source_table)
                });
                continue;
            }

            let mut preamble = Vec::new();
            match job.write_mode {
                WriteMode::Insert => {}
                WriteMode::TruncateInsert => {
                    if let Some(adapters) = &adapters {
                        preamble.push(format!(
                            "TRUNCATE TABLE {}",
                            adapters.tgt_adapter.quote_ident(&table.target_table)
                        ));
                    } else {
                        preamble.push(format!("TRUNCATE TABLE {}", table.target_table));
                    }
                }
                WriteMode::DropCreateInsert => {
                    if let Some(adapters) = &adapters {
                        preamble.push(build_drop_table_sql(
                            &table.target_table,
                            adapters.tgt_adapter,
                        ));
                        if let Some(create_sql) =
                            create_ddl_for_table(&table.source_table, &table.target_table)
                        {
                            preamble.push(create_sql);
                        }
                    } else {
                        preamble.push(format!("DROP TABLE IF EXISTS {}", table.target_table));
                        preamble.push(format!("CREATE TABLE {} (...)", table.target_table));
                    }
                }
            }

            write_plans.push(WritePlanItem {
                source_table: table.source_table.clone(),
                target_table: table.target_table.clone(),
                write_mode: job.write_mode,
                mapped_columns: active_cols,
                estimated_rows: table.source_row_count,
                preamble,
            });
        }
    }

    if needs_data && ir_pairing && !adapters_available {
        block_reason.get_or_insert_with(|| {
            "cross-family data execute requires IR sync adapters for both endpoints".into()
        });
    }

    let can_execute = block_reason.is_none()
        && target_read_only_ok
        && (needs_data || needs_structure)
        && inspected
            .iter()
            .any(|t| t.enabled && t.status != TableMappingStatus::Incompatible);

    Ok(TransferPreview {
        pairing_path: pairing.path_label().into(),
        mode: job.mode,
        write_mode: job.write_mode,
        ddl,
        write_plans,
        warnings,
        can_execute,
        block_reason,
    })
}

/// Build CREATE TABLE DDL when IR adapters are available.
pub fn build_create_ddl(
    ir_table: &crate::transfer::ir::IRTable,
    tgt_adapter: &dyn SyncTargetAdapter,
) -> String {
    build_create_table_ddl(ir_table, tgt_adapter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_transfer::model::{Endpoint, TableMapping, TransferOptions};
    use crate::db::Value;
    use crate::transfer::ir::{IRColumn, IRDefault, IRType};

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

    struct DummySource;

    impl SyncSourceAdapter for DummySource {
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

    fn sample_job(mode: TransferMode, write_mode: WriteMode) -> TransferJob {
        TransferJob {
            source: Endpoint {
                db_session_id: "s".into(),
                database: "src".into(),
                schema: None,
            },
            target: Endpoint {
                db_session_id: "t".into(),
                database: "tgt".into(),
                schema: None,
            },
            mode,
            write_mode,
            tables: vec![TableMapping::auto("users")],
            options: TransferOptions::default(),
        }
    }

    #[test]
    fn preview_blocks_destructive_without_confirm() {
        let job = sample_job(TransferMode::Data, WriteMode::TruncateInsert);
        let inspected = vec![TableInspectResult {
            source_table: "users".into(),
            target_table: "users".into(),
            status: TableMappingStatus::Matched,
            create_new: false,
            enabled: true,
            column_mappings: vec![super::super::model::ColumnMapping {
                source_column: "id".into(),
                target_column: "id".into(),
                skip: false,
                target_native_type: None,
            }],
            source_columns: vec!["id".into()],
            target_columns: vec!["id".into()],
            source_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: Some(10),
        }];
        let preview = build_preview(
            &job,
            &inspected,
            &SyncPairing::Direct {
                family: "postgresql".into(),
            },
            &HashMap::new(),
            true,
            None,
        )
        .unwrap();
        assert!(!preview.can_execute);
        assert!(preview.block_reason.unwrap().contains("destructive"));
    }

    #[test]
    fn preview_allows_ir_when_adapters_available() {
        let job = sample_job(TransferMode::Data, WriteMode::Insert);
        let inspected = vec![TableInspectResult {
            source_table: "users".into(),
            target_table: "users".into(),
            status: TableMappingStatus::Matched,
            create_new: false,
            enabled: true,
            column_mappings: vec![super::super::model::ColumnMapping {
                source_column: "id".into(),
                target_column: "id".into(),
                skip: false,
                target_native_type: None,
            }],
            source_columns: vec!["id".into()],
            target_columns: vec!["id".into()],
            source_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: Some(10),
        }];
        let preview = build_preview(
            &job,
            &inspected,
            &SyncPairing::Ir,
            &HashMap::new(),
            true,
            Some(TransferPreviewAdapters {
                src_adapter: &DummySource,
                tgt_adapter: &DummyTarget,
            }),
        )
        .unwrap();
        assert!(preview.can_execute, "{:?}", preview.block_reason);
        assert!(preview.block_reason.is_none());
    }

    #[test]
    fn preview_emits_real_create_ddl_with_adapters() {
        let job = sample_job(TransferMode::Structure, WriteMode::Insert);
        let schema = TableSchema {
            table_name: "users".into(),
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
        let mut schemas = HashMap::new();
        schemas.insert("users".into(), schema);

        let inspected = vec![TableInspectResult {
            source_table: "users".into(),
            target_table: "users_copy".into(),
            status: TableMappingStatus::CreateNew,
            create_new: true,
            enabled: true,
            column_mappings: vec![],
            source_columns: vec!["id".into()],
            target_columns: vec![],
            source_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: None,
        }];

        let preview = build_preview(
            &job,
            &inspected,
            &SyncPairing::Ir,
            &schemas,
            true,
            Some(TransferPreviewAdapters {
                src_adapter: &DummySource,
                tgt_adapter: &DummyTarget,
            }),
        )
        .unwrap();

        assert_eq!(preview.ddl.len(), 1);
        assert!(preview.ddl[0].ddl.contains("CREATE TABLE"));
        assert!(preview.ddl[0].ddl.contains("\"users_copy\""));
        assert!(!preview.ddl[0].ddl.contains("-- CREATE TABLE"));
    }
}

//! Preview plan: DDL (IR) + write plan summary.

use std::collections::HashMap;

use datazen_driver_api::TableSchema;

use crate::transfer::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::transfer::pairing::SyncPairing;

use super::error::TransferError;
use super::model::{
    DdlPreviewItem, TableInspectResult, TableMappingStatus, TransferJob, TransferMode,
    TransferPreview, WriteMode, WritePlanItem,
};
use super::structure::{mapped_create_ddl, table_mapping_for, target_relation_ref};

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

    let mut warnings = vec!["Data writes use a transaction per table. Completed DDL remains applied on cancellation or data failure; commit/rollback failures have unknown outcomes.".into()];
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

    if let Some(adapters) = &adapters {
        super::structure::validate_transfer_column_types(
            job,
            inspected,
            source_schemas,
            adapters.src_adapter,
            adapters.tgt_adapter,
        )?;
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
        if job.options.use_target_default_collation
            && (needs_structure || job.write_mode == WriteMode::DropCreateInsert)
        {
            warnings.push(format!(
                "Table '{}' will use the target database's default character set and collation. Text ordering, case/accent comparisons, and unique-index behavior may differ from the source.",
                table.target_table
            ));
        }

        // Resolve once, using the same renderer as both execution paths, but
        // only for operations that actually create structure. In particular,
        // Data + Insert into an existing table must not be blocked by source
        // DDL properties (collation, comments, generated columns, etc.) that
        // are irrelevant to the data-only write. DropCreateInsert is itself a
        // structure operation even when selected with Data mode.
        let create_sql = if needs_structure || job.write_mode == WriteMode::DropCreateInsert {
            match (&adapters, source_schemas.get(&table.source_table)) {
                (Some(adapters), Some(schema)) => Some(mapped_create_ddl(
                    adapters.src_adapter,
                    adapters.tgt_adapter,
                    schema,
                    table,
                    job,
                )?),
                _ => None,
            }
        } else {
            None
        };
        let create_ddl_for_table = |_source_table: &str, _target_table: &str| create_sql.clone();

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
                    kind: super::model::DdlPreviewKind::Table,
                    depends_on: Vec::new(),
                });
            } else if let Some(ddl_sql) =
                create_ddl_for_table(&table.source_table, &table.target_table)
            {
                ddl.push(DdlPreviewItem {
                    source_table: table.source_table.clone(),
                    target_table: table.target_table.clone(),
                    ddl: ddl_sql,
                    kind: super::model::DdlPreviewKind::Table,
                    depends_on: Vec::new(),
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
                    kind: super::model::DdlPreviewKind::Table,
                    depends_on: Vec::new(),
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
                            target_relation_ref(job, &table.target_table, adapters.tgt_adapter)
                        ));
                    } else {
                        preamble.push(format!("TRUNCATE TABLE {}", table.target_table));
                    }
                }
                WriteMode::DropCreateInsert => {
                    if let Some(adapters) = &adapters {
                        preamble.push(format!(
                            "DROP TABLE IF EXISTS {}",
                            target_relation_ref(job, &table.target_table, adapters.tgt_adapter)
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
                estimated_rows: if table_mapping.is_some_and(|mapping| {
                    mapping.source_filter.is_some() || mapping.recordset.is_some()
                }) {
                    // Inspection counts are intentionally unfiltered. Do not
                    // present them as an exact estimate for a scoped copy.
                    None
                } else {
                    table.source_row_count
                },
                preamble,
                source_filter_preview: table_mapping
                    .and_then(|mapping| mapping.source_filter.as_ref())
                    .and_then(|filter| filter.preview_where('"').ok().flatten()),
                recordset_preview: table_mapping
                    .and_then(|mapping| mapping.recordset.as_ref())
                    .and_then(|recordset| {
                        source_schemas.get(&table.source_table).and_then(|schema| {
                            super::recordset::preview_summary(schema, recordset, '"').ok()
                        })
                    }),
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
        plan_id: String::new(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Endpoint, TableMapping, TransferOptions};
    use crate::transfer::ir::{IRColumn, IRDefault, IRType};
    use datazen_driver_api::Value;

    struct DummyTarget;

    impl SyncTargetAdapter for DummyTarget {
        fn ir_type_to_native(&self, ir: &IRType) -> String {
            match ir {
                IRType::Int32 => "INT".into(),
                IRType::Other(native) => native.clone(),
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
            target: Some(Endpoint {
                db_session_id: "t".into(),
                database: "tgt".into(),
                schema: None,
            }),
            sql_file_target: None,
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
            source_primary_keys: vec!["id".into()],
            target_columns: vec!["id".into()],
            source_column_types: HashMap::new(),
            target_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: Some(10),
            recordset: None,
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
        let source_schemas = HashMap::from([(
            "users".into(),
            TableSchema {
                table_name: "users".into(),
                columns: vec![datazen_driver_api::ColumnSchema {
                    name: "id".into(),
                    data_type: "integer".into(),
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
            },
        )]);
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
            source_primary_keys: vec!["id".into()],
            target_columns: vec!["id".into()],
            source_column_types: HashMap::new(),
            target_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: Some(10),
            recordset: None,
        }];
        let preview = build_preview(
            &job,
            &inspected,
            &SyncPairing::Ir,
            &source_schemas,
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
    fn data_only_preview_skips_unportable_source_table_options() {
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
            source_primary_keys: vec!["id".into()],
            target_columns: vec!["id".into()],
            source_column_types: HashMap::new(),
            target_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: Some(1),
            recordset: None,
        }];
        let mut schemas = HashMap::new();
        schemas.insert(
            "users".into(),
            TableSchema {
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
                check_constraints: vec![],
                table_options: datazen_driver_api::TableOptions {
                    charset: Some("utf8mb4".into()),
                    collation: Some("utf8mb4_0900_ai_ci".into()),
                    comment: Some("source-only comment".into()),
                    ..Default::default()
                },
            },
        );

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
        .expect("data-only preview does not render source DDL");

        assert!(preview.can_execute, "{:?}", preview.block_reason);
        assert!(preview.block_reason.is_none());
        assert!(preview.ddl.is_empty());
        assert_eq!(preview.write_plans.len(), 1);

        let mut drop_create_job = job;
        drop_create_job.write_mode = WriteMode::DropCreateInsert;
        drop_create_job.options.confirmed_destructive = true;
        let error = build_preview(
            &drop_create_job,
            &inspected,
            &SyncPairing::Ir,
            &schemas,
            true,
            Some(TransferPreviewAdapters {
                src_adapter: &DummySource,
                tgt_adapter: &DummyTarget,
            }),
        )
        .expect_err("DropCreateInsert still validates source DDL options");
        assert!(error.to_string().contains("source table collation"));
    }

    #[test]
    fn preview_marks_scoped_rows_and_exposes_recordset_summary() {
        let mut job = sample_job(TransferMode::Data, WriteMode::Insert);
        job.tables[0].recordset = Some(super::super::model::TransferRecordset {
            order_by: None,
            start: None,
            end: None,
            tuple_range: None,
            limit: Some(25),
        });
        let schema = TableSchema {
            table_name: "users".into(),
            columns: vec![datazen_driver_api::ColumnSchema {
                name: "id".into(),
                data_type: "INTEGER".into(),
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
            source_primary_keys: vec!["id".into()],
            target_columns: vec!["id".into()],
            source_column_types: HashMap::new(),
            target_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: Some(100),
            recordset: job.tables[0].recordset.clone(),
        }];
        let mut schemas = HashMap::new();
        schemas.insert("users".into(), schema);
        let preview = build_preview(
            &job,
            &inspected,
            &SyncPairing::Direct {
                family: "postgresql".into(),
            },
            &schemas,
            true,
            None,
        )
        .unwrap();
        assert_eq!(preview.write_plans[0].estimated_rows, None);
        assert_eq!(
            preview.write_plans[0].recordset_preview.as_deref(),
            Some(r#"ORDER BY "id" ASC LIMIT 25"#)
        );
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
            check_constraints: vec![],
            table_options: Default::default(),
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
            source_primary_keys: vec!["id".into()],
            target_columns: vec![],
            source_column_types: HashMap::new(),
            target_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: None,
            recordset: None,
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

    #[test]
    fn preview_create_ddl_honors_target_native_type_override() {
        let mut job = sample_job(TransferMode::Structure, WriteMode::Insert);
        job.tables = vec![TableMapping {
            source_table: "users".into(),
            target_table: "users_copy".into(),
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
        }];
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
            check_constraints: vec![],
            table_options: Default::default(),
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
            source_primary_keys: vec!["id".into()],
            target_columns: vec![],
            source_column_types: HashMap::new(),
            target_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: None,
            recordset: None,
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
        assert!(preview.ddl[0].ddl.contains("BIGINT"));
        assert!(!preview.ddl[0].ddl.contains(" INT"));
    }

    #[test]
    fn preview_timestamp_keeps_datetime_when_target_native_type_matches() {
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
            fn default_capable_type_for(&self, ir: &IRType) -> Option<IRType> {
                match ir {
                    IRType::Other(_) => Some(IRType::Varchar {
                        length: Some(16383),
                    }),
                    _ => None,
                }
            }
            fn format_literal(&self, _v: &Option<Value>, _ir: &IRType) -> String {
                "NULL".into()
            }
        }

        struct TsSource;
        impl SyncSourceAdapter for TsSource {
            fn column_to_ir(
                &self,
                column: &datazen_driver_api::ColumnSchema,
                _native_full_type: Option<&str>,
            ) -> IRColumn {
                IRColumn {
                    name: column.name.clone(),
                    ir_type: IRType::Timestamp {
                        with_timezone: false,
                    },
                    nullable: false,
                    default_expr: Some(IRDefault::CurrentTimestamp),
                    is_primary_key: false,
                    is_auto_increment: false,
                    comment: None,
                }
            }
        }

        let mut job = sample_job(TransferMode::Structure, WriteMode::Insert);
        job.tables = vec![TableMapping {
            source_table: "reviews".into(),
            target_table: "reviews".into(),
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
        }];
        let schema = TableSchema {
            table_name: "reviews".into(),
            columns: vec![datazen_driver_api::ColumnSchema {
                name: "created_at".into(),
                data_type: "timestamp".into(),
                nullable: false,
                default_value: Some("now()".into()),
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
        let mut schemas = HashMap::new();
        schemas.insert("reviews".into(), schema);

        let inspected = vec![TableInspectResult {
            source_table: "reviews".into(),
            target_table: "reviews".into(),
            status: TableMappingStatus::CreateNew,
            create_new: true,
            enabled: true,
            column_mappings: vec![],
            source_columns: vec!["created_at".into()],
            source_primary_keys: vec![],
            target_columns: vec![],
            source_column_types: HashMap::new(),
            target_column_types: HashMap::new(),
            incompatible_reason: None,
            source_row_count: None,
            recordset: None,
        }];

        let preview = build_preview(
            &job,
            &inspected,
            &SyncPairing::Ir,
            &schemas,
            true,
            Some(TransferPreviewAdapters {
                src_adapter: &TsSource,
                tgt_adapter: &TgtAdapter,
            }),
        )
        .unwrap();

        assert_eq!(preview.ddl.len(), 1);
        assert!(
            preview.ddl[0].ddl.contains("DATETIME"),
            "ddl={}",
            preview.ddl[0].ddl
        );
        assert!(
            !preview.ddl[0].ddl.contains("VARCHAR"),
            "ddl={}",
            preview.ddl[0].ddl
        );
        assert!(
            preview.ddl[0].ddl.contains("DEFAULT CURRENT_TIMESTAMP"),
            "ddl={}",
            preview.ddl[0].ddl
        );
    }
}

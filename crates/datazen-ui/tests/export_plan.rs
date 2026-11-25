//! T043-S1: `platform/exportPlan.ts` minimal mirror — extension and
//! DDL rules, source validation, file planning, zip decision and default
//! names. Error wording is asserted verbatim against the TS messages.

use std::collections::HashSet;

use chrono::{TimeZone, Utc};
use serde_json::json;

use datazen_ui::platform::export_plan::{
    build_file_plans, ddl_for_structure, default_export_name, format_extension, is_sql_data_format,
    prepare_export, resolve_ddl, unique_filename, validate_table_source, ExportDataFormat,
    ExportFilePlan, ExportMode, ExportOutputMode, ExportPlanError, ExportTableInput,
    ExportTableSource, ProvidedColumn, TablesExportRequest,
};

fn table(name: &str) -> ExportTableInput {
    ExportTableInput {
        table_name: name.to_owned(),
        ..Default::default()
    }
}

fn table_with_ddl(name: &str, ddl: &str) -> ExportTableInput {
    ExportTableInput {
        table_name: name.to_owned(),
        ddl: Some(ddl.to_owned()),
        ..Default::default()
    }
}

fn request(
    tables: Vec<ExportTableInput>,
    mode: ExportMode,
    format: ExportDataFormat,
    output: ExportOutputMode,
) -> TablesExportRequest {
    TablesExportRequest {
        tables,
        mode,
        data_format: format,
        output_mode: output,
        ..Default::default()
    }
}

fn pinned_now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 19, 10, 23, 14)
        .single()
        .expect("fixed test time")
}

#[test]
fn export_data_format_wire_names_match_typescript() {
    assert_eq!(ExportDataFormat::Csv.as_str(), "csv");
    assert_eq!(ExportDataFormat::Tsv.as_str(), "tsv");
    assert_eq!(ExportDataFormat::Json.as_str(), "json");
    assert_eq!(ExportDataFormat::Markdown.as_str(), "markdown");
    assert_eq!(ExportDataFormat::SqlInsert.as_str(), "sql_insert");
    assert_eq!(ExportDataFormat::SqlUpdate.as_str(), "sql_update");
    assert_eq!(ExportDataFormat::Xlsx.as_str(), "xlsx");
    assert_eq!(ExportDataFormat::Parquet.as_str(), "parquet");
    assert_eq!(ExportDataFormat::Parquet.to_string(), "parquet");
}

#[test]
fn format_extension_matches_data_format() {
    assert_eq!(format_extension(ExportDataFormat::Csv), "csv");
    assert_eq!(format_extension(ExportDataFormat::Tsv), "tsv");
    assert_eq!(format_extension(ExportDataFormat::Json), "json");
    assert_eq!(format_extension(ExportDataFormat::Markdown), "md");
    assert_eq!(format_extension(ExportDataFormat::SqlInsert), "sql");
    assert_eq!(format_extension(ExportDataFormat::SqlUpdate), "sql");
    assert_eq!(format_extension(ExportDataFormat::Xlsx), "xlsx");
    assert_eq!(format_extension(ExportDataFormat::Parquet), "parquet");
}

#[test]
fn is_sql_data_format_only_for_sql_formats() {
    assert!(is_sql_data_format(ExportDataFormat::SqlInsert));
    assert!(is_sql_data_format(ExportDataFormat::SqlUpdate));
    assert!(!is_sql_data_format(ExportDataFormat::Csv));
    assert!(!is_sql_data_format(ExportDataFormat::Parquet));
}

#[test]
fn resolve_ddl_prefers_non_blank_own_ddl() {
    assert_eq!(
        resolve_ddl(&table_with_ddl("t", "CREATE TABLE t (id INT)")),
        "CREATE TABLE t (id INT)"
    );
    // Blank own DDL is not DDL: the placeholder applies.
    assert_eq!(
        resolve_ddl(&table_with_ddl("t", "   ")),
        "-- DDL unavailable for t"
    );
    assert_eq!(resolve_ddl(&table("t")), "-- DDL unavailable for t");
}

#[test]
fn ddl_for_structure_table_source_falls_back_to_placeholder() {
    assert_eq!(
        ddl_for_structure(&table("t")).expect("table source plans"),
        "-- DDL unavailable for t"
    );
    assert_eq!(
        ddl_for_structure(&table_with_ddl("t", "CREATE TABLE t (id INT)")).expect("own ddl plans"),
        "CREATE TABLE t (id INT)"
    );
}

#[test]
fn ddl_for_structure_non_table_source_needs_explicit_ddl() {
    let sql = ExportTableInput {
        table_name: "q".to_owned(),
        source: ExportTableSource::Sql {
            sql: "SELECT * FROM t".to_owned(),
        },
        ..Default::default()
    };
    let err = ddl_for_structure(&sql).expect_err("sql source without ddl must fail");
    assert_eq!(
        err.to_string(),
        "structure export for non-table source 'q' needs an explicit ddl: \
         SQL and provided-data sources have no table DDL to include"
    );
    let provided = ExportTableInput {
        table_name: "p".to_owned(),
        source: ExportTableSource::Provided {
            columns: vec![ProvidedColumn {
                name: "a".to_owned(),
            }],
            rows: vec![vec![json!(1)]],
        },
        ..Default::default()
    };
    assert_eq!(
        ddl_for_structure(&provided).expect_err("provided source without ddl must fail"),
        ExportPlanError::StructureNeedsExplicitDdl {
            table: "p".to_owned()
        }
    );
    // A blank explicit ddl is not explicit ddl.
    let blank = ExportTableInput {
        table_name: "q".to_owned(),
        ddl: Some("  ".to_owned()),
        source: ExportTableSource::Sql {
            sql: "SELECT * FROM t".to_owned(),
        },
        ..Default::default()
    };
    assert!(ddl_for_structure(&blank).is_err());
    // A real explicit ddl keeps a filtered export working as a structure export.
    let with_ddl = ExportTableInput {
        table_name: "q".to_owned(),
        ddl: Some("CREATE TABLE q (id INT)".to_owned()),
        source: ExportTableSource::Sql {
            sql: "SELECT * FROM t".to_owned(),
        },
        ..Default::default()
    };
    assert_eq!(
        ddl_for_structure(&with_ddl).expect("explicit ddl plans"),
        "CREATE TABLE q (id INT)"
    );
}

#[test]
fn validate_table_source_accepts_table_and_well_formed_sources() {
    assert!(validate_table_source(&table("t")).is_ok());
    let sql = ExportTableInput {
        table_name: "q".to_owned(),
        source: ExportTableSource::Sql {
            sql: "SELECT 1".to_owned(),
        },
        ..Default::default()
    };
    assert!(validate_table_source(&sql).is_ok());
    let provided = ExportTableInput {
        table_name: "p".to_owned(),
        source: ExportTableSource::Provided {
            columns: vec![ProvidedColumn {
                name: "a".to_owned(),
            }],
            rows: vec![vec![json!("x")]],
        },
        ..Default::default()
    };
    assert!(validate_table_source(&provided).is_ok());
}

#[test]
fn validate_table_source_rejects_blank_sql() {
    let sql = ExportTableInput {
        table_name: "q".to_owned(),
        source: ExportTableSource::Sql {
            sql: "   ".to_owned(),
        },
        ..Default::default()
    };
    assert_eq!(
        validate_table_source(&sql).expect_err("blank sql must fail"),
        ExportPlanError::EmptySqlSource {
            table: "q".to_owned()
        }
    );
    assert_eq!(
        ExportPlanError::EmptySqlSource {
            table: "q".to_owned()
        }
        .to_string(),
        "SQL data source 'q' must not be empty"
    );
}

#[test]
fn validate_table_source_rejects_malformed_provided() {
    let no_columns = ExportTableInput {
        table_name: "p".to_owned(),
        source: ExportTableSource::Provided {
            columns: vec![],
            rows: vec![],
        },
        ..Default::default()
    };
    assert_eq!(
        validate_table_source(&no_columns).expect_err("no columns must fail"),
        ExportPlanError::ProvidedNoColumns {
            table: "p".to_owned()
        }
    );
    assert_eq!(
        ExportPlanError::ProvidedNoColumns {
            table: "p".to_owned()
        }
        .to_string(),
        "provided data source 'p' has no columns (empty schema)"
    );

    let duplicate = ExportTableInput {
        table_name: "p".to_owned(),
        source: ExportTableSource::Provided {
            columns: vec![
                ProvidedColumn {
                    name: "a".to_owned(),
                },
                ProvidedColumn {
                    name: "a".to_owned(),
                },
            ],
            rows: vec![],
        },
        ..Default::default()
    };
    assert_eq!(
        validate_table_source(&duplicate).expect_err("duplicate column must fail"),
        ExportPlanError::ProvidedDuplicateColumn {
            table: "p".to_owned(),
            column: "a".to_owned()
        }
    );
    assert_eq!(
        ExportPlanError::ProvidedDuplicateColumn {
            table: "p".to_owned(),
            column: "a".to_owned()
        }
        .to_string(),
        "provided data source 'p' has a duplicate column name 'a'"
    );

    let row_length = ExportTableInput {
        table_name: "p".to_owned(),
        source: ExportTableSource::Provided {
            columns: vec![
                ProvidedColumn {
                    name: "a".to_owned(),
                },
                ProvidedColumn {
                    name: "b".to_owned(),
                },
            ],
            rows: vec![vec![json!(1)], vec![json!(1), json!(2)]],
        },
        ..Default::default()
    };
    assert_eq!(
        validate_table_source(&row_length).expect_err("short row must fail"),
        ExportPlanError::ProvidedRowLength {
            table: "p".to_owned(),
            row: 0,
            values: 1,
            columns: 2,
        }
    );
    assert_eq!(
        ExportPlanError::ProvidedRowLength {
            table: "p".to_owned(),
            row: 0,
            values: 1,
            columns: 2,
        }
        .to_string(),
        "provided data source 'p' row 0 has 1 values but the schema has 2 columns"
    );
}

#[test]
fn unique_filename_deduplicates_sql_names() {
    let mut seen = HashSet::new();
    assert_eq!(unique_filename(&mut seen, "t.sql"), "t.sql");
    assert_eq!(unique_filename(&mut seen, "t.sql"), "t.1.sql");
    assert_eq!(unique_filename(&mut seen, "t.sql"), "t.2.sql");
    assert_eq!(unique_filename(&mut seen, "other.sql"), "other.sql");
    // A fresh name is used verbatim, even with a repeated `.sql` suffix;
    // only a collision strips the suffixes (TS `while` loop) before numbering.
    assert_eq!(unique_filename(&mut seen, "t.sql.sql"), "t.sql.sql");
    // The collision strips both suffixes ("t") and numbers past the
    // already-taken `t.1.sql` / `t.2.sql`.
    assert_eq!(unique_filename(&mut seen, "t.sql.sql"), "t.3.sql");
}

#[test]
fn build_file_plans_structure_only_emits_one_ddl_plan_per_table() {
    let plans = build_file_plans(&request(
        vec![table_with_ddl("t", "CREATE TABLE t (id INT)"), table("u")],
        ExportMode::StructureOnly,
        ExportDataFormat::Xlsx,
        ExportOutputMode::Single,
    ))
    .expect("structure plans");
    assert_eq!(
        plans,
        vec![
            ExportFilePlan::Ddl {
                filename: "t.sql".to_owned(),
                text: "CREATE TABLE t (id INT)".to_owned(),
            },
            ExportFilePlan::Ddl {
                filename: "u.sql".to_owned(),
                text: "-- DDL unavailable for u".to_owned(),
            },
        ]
    );
}

#[test]
fn build_file_plans_structure_only_deduplicates_table_names() {
    let plans = build_file_plans(&request(
        vec![table("t"), table("t")],
        ExportMode::StructureOnly,
        ExportDataFormat::Csv,
        ExportOutputMode::Single,
    ))
    .expect("structure plans");
    assert_eq!(plans[0].filename(), "t.sql");
    assert_eq!(plans[1].filename(), "t.1.sql");
}

#[test]
fn build_file_plans_data_only_emits_one_data_plan_per_table() {
    let input = table("t");
    let plans = build_file_plans(&request(
        vec![input.clone()],
        ExportMode::DataOnly,
        ExportDataFormat::Json,
        ExportOutputMode::Single,
    ))
    .expect("data plans");
    assert_eq!(
        plans,
        vec![ExportFilePlan::Data {
            filename: "t.json".to_owned(),
            table: input,
            format: ExportDataFormat::Json,
            ddl_prefix: None,
        }]
    );
}

#[test]
fn build_file_plans_data_and_structure_splits_ddl_and_data_for_csv() {
    let input = table("t");
    let plans = build_file_plans(&request(
        vec![input.clone()],
        ExportMode::DataAndStructure,
        ExportDataFormat::Csv,
        ExportOutputMode::Single,
    ))
    .expect("full plans");
    assert_eq!(
        plans,
        vec![
            ExportFilePlan::Ddl {
                filename: "t.sql".to_owned(),
                text: "-- DDL unavailable for t".to_owned(),
            },
            ExportFilePlan::Data {
                filename: "t.csv".to_owned(),
                table: input,
                format: ExportDataFormat::Csv,
                ddl_prefix: None,
            },
        ]
    );
}

#[test]
fn build_file_plans_data_and_structure_prefixes_ddl_for_sql_format() {
    let input = table("t");
    let plans = build_file_plans(&request(
        vec![input.clone()],
        ExportMode::DataAndStructure,
        ExportDataFormat::SqlInsert,
        ExportOutputMode::Single,
    ))
    .expect("sql plans");
    assert_eq!(
        plans,
        vec![ExportFilePlan::Data {
            filename: "t.sql".to_owned(),
            table: input,
            format: ExportDataFormat::SqlInsert,
            ddl_prefix: Some("-- DDL unavailable for t".to_owned()),
        }]
    );
}

#[test]
fn prepare_export_refuses_empty_tables() {
    let err = prepare_export(&request(
        vec![],
        ExportMode::DataOnly,
        ExportDataFormat::Csv,
        ExportOutputMode::Single,
    ))
    .expect_err("no tables must fail planning");
    assert_eq!(err, ExportPlanError::NoTables);
    assert_eq!(err.to_string(), "no tables selected");
}

#[test]
fn prepare_export_single_file_never_needs_zip() {
    let prepared = prepare_export(&request(
        vec![table("t")],
        ExportMode::DataOnly,
        ExportDataFormat::Csv,
        ExportOutputMode::Single,
    ))
    .expect("single plan");
    assert_eq!(prepared.plans.len(), 1);
    assert!(!prepared.use_zip);
}

#[test]
fn prepare_export_zip_output_always_needs_zip() {
    let prepared = prepare_export(&request(
        vec![table("t")],
        ExportMode::DataOnly,
        ExportDataFormat::Csv,
        ExportOutputMode::Zip,
    ))
    .expect("zip plan");
    assert!(prepared.use_zip);
}

#[test]
fn prepare_export_multi_file_non_sql_needs_zip() {
    let prepared = prepare_export(&request(
        vec![table("t"), table("u")],
        ExportMode::DataOnly,
        ExportDataFormat::Csv,
        ExportOutputMode::Single,
    ))
    .expect("multi plan");
    assert_eq!(prepared.plans.len(), 2);
    assert!(prepared.use_zip);
}

#[test]
fn prepare_export_multi_file_all_sql_merges_without_zip() {
    let structure = prepare_export(&request(
        vec![table("t"), table("u")],
        ExportMode::StructureOnly,
        ExportDataFormat::Csv,
        ExportOutputMode::Single,
    ))
    .expect("structure plans");
    assert_eq!(structure.plans.len(), 2);
    assert!(!structure.use_zip);

    let sql_data = prepare_export(&request(
        vec![table("t"), table("u")],
        ExportMode::DataAndStructure,
        ExportDataFormat::SqlUpdate,
        ExportOutputMode::Single,
    ))
    .expect("sql plans");
    assert_eq!(sql_data.plans.len(), 2);
    assert!(!sql_data.use_zip);
}

#[test]
fn prepare_export_multi_file_mixed_sql_and_data_needs_zip() {
    // data_and_structure in CSV: DDL (.sql) + data (.csv) per table.
    let prepared = prepare_export(&request(
        vec![table("t")],
        ExportMode::DataAndStructure,
        ExportDataFormat::Csv,
        ExportOutputMode::Single,
    ))
    .expect("full plans");
    assert_eq!(prepared.plans.len(), 2);
    assert!(prepared.use_zip);
}

#[test]
fn default_export_name_pins_prefix_timestamp_and_extension() {
    let now = pinned_now();
    assert_eq!(
        default_export_name(
            &request(
                vec![],
                ExportMode::DataOnly,
                ExportDataFormat::Csv,
                ExportOutputMode::Single
            ),
            false,
            now,
        ),
        "export_data_2026-11-19-10-23-14.csv"
    );
    assert_eq!(
        default_export_name(
            &request(
                vec![],
                ExportMode::StructureOnly,
                ExportDataFormat::Xlsx,
                ExportOutputMode::Single
            ),
            false,
            now,
        ),
        "export_structure_2026-11-19-10-23-14.sql"
    );
    assert_eq!(
        default_export_name(
            &request(
                vec![],
                ExportMode::DataAndStructure,
                ExportDataFormat::Parquet,
                ExportOutputMode::Single
            ),
            false,
            now,
        ),
        "export_full_2026-11-19-10-23-14.parquet"
    );
    assert_eq!(
        default_export_name(
            &request(
                vec![],
                ExportMode::DataOnly,
                ExportDataFormat::Csv,
                ExportOutputMode::Zip
            ),
            true,
            now,
        ),
        "export_data_2026-11-19-10-23-14.zip"
    );
}

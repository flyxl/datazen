//! T043-S1 independent audit: `platform/export_plan.rs` (the minimal
//! `exportPlan.ts` mirror).
//!
//! Written by the tester, not the coder. Expectations re-derive from
//! `packages/frontend/src/platform/exportPlan.ts`. Focus: the two
//! deliberately omitted functions (`isBinaryFormat`, `planFilename`) are
//! genuinely unused by S1, `unique_filename` on names without a `.sql`
//! suffix, `default_export_extension` precedence (zip beats
//! structure-only), empty-string (not just blank) DDL, and the
//! validate-before-plan ordering in `build_file_plans`.

use std::collections::HashSet;

use chrono::{TimeZone, Utc};
use serde_json::json;

use crate::platform::export_plan::{
    build_file_plans, ddl_for_structure, default_export_extension, default_export_name,
    format_extension, is_sql_data_format, prepare_export, resolve_ddl, unique_filename,
    ExportDataFormat, ExportFilePlan, ExportMode, ExportOutputMode,
    ExportPlanError, ExportTableInput, ExportTableSource, ProvidedColumn, TablesExportRequest,
};

fn table(name: &str) -> ExportTableInput {
    ExportTableInput {
        table_name: name.to_owned(),
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

// ── is_sql_data_format: remaining formats ───────────────────────────────────

#[test]
fn audit_is_sql_data_format_false_for_the_other_six_formats() {
    for format in [
        ExportDataFormat::Tsv,
        ExportDataFormat::Json,
        ExportDataFormat::Markdown,
        ExportDataFormat::Xlsx,
        ExportDataFormat::Parquet,
        ExportDataFormat::Csv,
    ] {
        assert!(!is_sql_data_format(format), "{format} is not a sql format");
    }
}

// ── unique_filename: names without a `.sql` suffix ──────────────────────────

#[test]
fn audit_unique_filename_without_sql_suffix_still_numbers() {
    // TS: the stem loop strips nothing, the candidate is `stem.n.sql`.
    let mut seen = HashSet::new();
    assert_eq!(unique_filename(&mut seen, "t"), "t");
    assert_eq!(unique_filename(&mut seen, "t"), "t.1.sql");
    assert_eq!(unique_filename(&mut seen, "t"), "t.2.sql");
}

#[test]
fn audit_unique_filename_suffix_strip_only_applies_on_collision() {
    // A fresh `a.sql.sql` is verbatim; on collision the stem is `a`
    // (both suffixes stripped) and numbering skips taken names.
    let mut seen = HashSet::new();
    assert_eq!(unique_filename(&mut seen, "a.sql"), "a.sql");
    assert_eq!(unique_filename(&mut seen, "a.sql.sql"), "a.sql.sql");
    assert_eq!(unique_filename(&mut seen, "a.sql.sql"), "a.1.sql");
    assert_eq!(unique_filename(&mut seen, "a.sql"), "a.2.sql");
}

// ── resolve_ddl / ddl_for_structure: empty-string DDL ───────────────────────

#[test]
fn audit_resolve_ddl_empty_string_is_not_ddl() {
    let empty = ExportTableInput {
        table_name: "t".to_owned(),
        ddl: Some(String::new()),
        ..Default::default()
    };
    assert_eq!(resolve_ddl(&empty), "-- DDL unavailable for t");
}

#[test]
fn audit_ddl_for_structure_table_source_with_blank_ddl_uses_placeholder() {
    let blank = ExportTableInput {
        table_name: "t".to_owned(),
        ddl: Some(" \t\n".to_owned()),
        ..Default::default()
    };
    assert_eq!(
        ddl_for_structure(&blank).expect("table source plans"),
        "-- DDL unavailable for t"
    );
}

// ── default_export_extension: precedence ────────────────────────────────────

#[test]
fn audit_default_export_extension_zip_beats_structure_only() {
    // TS checks `useZip` first: a structure-only export in a zip
    // container is `.zip`, not `.sql`.
    let req = request(
        vec![],
        ExportMode::StructureOnly,
        ExportDataFormat::Csv,
        ExportOutputMode::Zip,
    );
    assert_eq!(default_export_extension(&req, true), "zip");
    assert_eq!(
        default_export_name(&req, true, pinned_now()),
        "export_structure_2026-11-19-10-23-14.zip"
    );
}

#[test]
fn audit_default_export_name_full_prefix_uses_format_extension() {
    assert_eq!(
        default_export_name(
            &request(
                vec![],
                ExportMode::DataAndStructure,
                ExportDataFormat::Tsv,
                ExportOutputMode::Single
            ),
            false,
            pinned_now(),
        ),
        "export_full_2026-11-19-10-23-14.tsv"
    );
    assert_eq!(
        default_export_name(
            &request(
                vec![],
                ExportMode::DataOnly,
                ExportDataFormat::Markdown,
                ExportOutputMode::Single
            ),
            false,
            pinned_now(),
        ),
        "export_data_2026-11-19-10-23-14.md"
    );
}

// ── build_file_plans: validate-before-plan ordering ─────────────────────────

#[test]
fn audit_build_file_plans_validates_all_tables_before_planning_any() {
    // A malformed second table fails the whole call; no partial plan
    // list is returned (TS `validateTableSource` runs in a first loop).
    let bad = ExportTableInput {
        table_name: "p".to_owned(),
        source: ExportTableSource::Provided {
            columns: vec![],
            rows: vec![],
        },
        ..Default::default()
    };
    let err = build_file_plans(&request(
        vec![table("t"), bad],
        ExportMode::DataOnly,
        ExportDataFormat::Csv,
        ExportOutputMode::Single,
    ))
    .expect_err("a bad table must fail the whole plan");
    assert_eq!(
        err,
        ExportPlanError::ProvidedNoColumns {
            table: "p".to_owned(),
        }
    );
}

#[test]
fn audit_build_file_plans_data_and_structure_sql_update_with_provided_and_ddl() {
    // Provided + explicit DDL in a sql format: one data plan carrying
    // the DDL prefix (no separate DDL file).
    let input = ExportTableInput {
        table_name: "p".to_owned(),
        ddl: Some("CREATE TABLE p (id INT)".to_owned()),
        source: ExportTableSource::Provided {
            columns: vec![ProvidedColumn {
                name: "a".to_owned(),
            }],
            rows: vec![vec![json!(1)]],
        },
        ..Default::default()
    };
    let plans = build_file_plans(&request(
        vec![input.clone()],
        ExportMode::DataAndStructure,
        ExportDataFormat::SqlUpdate,
        ExportOutputMode::Single,
    ))
    .expect("provided sql_update with ddl plans");
    assert_eq!(
        plans,
        vec![ExportFilePlan::Data {
            filename: "p.sql".to_owned(),
            table: input,
            format: ExportDataFormat::SqlUpdate,
            ddl_prefix: Some("CREATE TABLE p (id INT)".to_owned()),
        }]
    );
}

#[test]
fn audit_build_file_plans_data_and_structure_sql_format_provided_without_ddl_fails() {
    // Same refusal as the CSV branch, but through the sql-format arm's
    // `ddl_for_structure(table)?` — the error-propagation region of
    // that `?` is otherwise uncovered (the Ok path alone executes the
    // line but never takes the early return).
    let input = ExportTableInput {
        table_name: "p".to_owned(),
        source: ExportTableSource::Provided {
            columns: vec![ProvidedColumn {
                name: "a".to_owned(),
            }],
            rows: vec![vec![json!(1)]],
        },
        ..Default::default()
    };
    let err = build_file_plans(&request(
        vec![input],
        ExportMode::DataAndStructure,
        ExportDataFormat::SqlInsert,
        ExportOutputMode::Single,
    ))
    .expect_err("provided without ddl must fail in sql-format data_and_structure");
    assert_eq!(
        err,
        ExportPlanError::StructureNeedsExplicitDdl {
            table: "p".to_owned(),
        }
    );
}

#[test]
fn audit_build_file_plans_data_and_structure_provided_without_ddl_fails() {
    let input = ExportTableInput {
        table_name: "p".to_owned(),
        source: ExportTableSource::Provided {
            columns: vec![ProvidedColumn {
                name: "a".to_owned(),
            }],
            rows: vec![vec![json!(1)]],
        },
        ..Default::default()
    };
    // Non-sql format: the DDL plan is built first and refuses.
    let err = build_file_plans(&request(
        vec![input],
        ExportMode::DataAndStructure,
        ExportDataFormat::Csv,
        ExportOutputMode::Single,
    ))
    .expect_err("provided without ddl must fail in data_and_structure");
    assert_eq!(
        err,
        ExportPlanError::StructureNeedsExplicitDdl {
            table: "p".to_owned(),
        }
    );
}

// ── prepare_export: zip decision corners ────────────────────────────────────

#[test]
fn audit_prepare_export_single_sql_data_plan_never_needs_zip() {
    let prepared = prepare_export(&request(
        vec![table("t")],
        ExportMode::DataAndStructure,
        ExportDataFormat::SqlInsert,
        ExportOutputMode::Single,
    ))
    .expect("single sql plan");
    assert_eq!(prepared.plans.len(), 1);
    assert!(!prepared.use_zip);
}

#[test]
fn audit_prepare_export_data_only_two_tables_same_name_needs_zip() {
    // Same name twice: filenames are `t.csv` twice (data plans are
    // verbatim, not deduplicated) and two non-sql files need a zip.
    let prepared = prepare_export(&request(
        vec![table("t"), table("t")],
        ExportMode::DataOnly,
        ExportDataFormat::Csv,
        ExportOutputMode::Single,
    ))
    .expect("two data plans");
    assert_eq!(prepared.plans.len(), 2);
    assert_eq!(prepared.plans[0].filename(), "t.csv");
    assert_eq!(prepared.plans[1].filename(), "t.csv");
    assert!(prepared.use_zip);
}

#[test]
fn audit_format_extension_matches_ts_for_every_format() {
    // Full sweep (coder covered each too, but this pins the table in
    // one place against `DataFormat::extension`).
    for (format, ext) in [
        (ExportDataFormat::Csv, "csv"),
        (ExportDataFormat::Tsv, "tsv"),
        (ExportDataFormat::Json, "json"),
        (ExportDataFormat::Markdown, "md"),
        (ExportDataFormat::SqlInsert, "sql"),
        (ExportDataFormat::SqlUpdate, "sql"),
        (ExportDataFormat::Xlsx, "xlsx"),
        (ExportDataFormat::Parquet, "parquet"),
    ] {
        assert_eq!(format_extension(format), ext);
    }
}

#[test]
fn audit_validate_table_source_error_display_matches_ts_verbatim() {
    assert_eq!(
        ExportPlanError::StructureNeedsExplicitDdl {
            table: "q".to_owned(),
        }
        .to_string(),
        "structure export for non-table source 'q' needs an explicit ddl: \
         SQL and provided-data sources have no table DDL to include"
    );
}

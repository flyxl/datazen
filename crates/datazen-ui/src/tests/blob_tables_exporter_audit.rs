//! T043-S1 independent audit: `platform/blob_tables_exporter.rs`.
//!
//! Written by the tester, not the coder. Every test below re-derives its
//! expectation from the React original
//! (`packages/frontend/src/platform/blobTablesExporter.ts`) or from the
//! Rust mirror's own documented contract — none of them repeats a coder
//! assertion verbatim. Focus: gate-order combinations the coder did not
//! exercise, DDL resolution in `data_and_structure`, the S1/S2 boundary
//! (provided + parquet plans fine in S1; the refusal is S2's
//! `emitDataPlan`), error propagation from the planner through
//! `plan_export`, and the documented float-rendering divergence in
//! `cell_to_text` (JS `String(1)` is `"1"`, serde_json renders `1.0`).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use serde_json::{json, Value};

use crate::platform::blob_tables_exporter::{
    base64_to_bytes, create_blob_tables_exporter, csv_cell, download_blob, escape_csv_field,
    BlobTablesExportError, BlobTablesExporter, BlobTablesExporterOptions, DdlLoader,
    DeliveredContent, TablePage, TablePageReader, TablePageRequestContext,
};
use crate::platform::export_plan::{
    ExportDataFormat, ExportFilePlan, ExportMode, ExportOutputMode, ExportPlanError,
    ExportTableInput, ExportTableSource, ProvidedColumn, TablesExportRequest,
};

fn pinned_now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 19, 10, 23, 14)
        .single()
        .expect("fixed test time")
}

struct NoopReader;

impl TablePageReader for NoopReader {
    fn read_page(
        &self,
        table_name: &str,
        cursor: Option<&Value>,
        context: &TablePageRequestContext,
    ) -> Pin<Box<dyn Future<Output = Result<TablePage, BlobTablesExportError>> + Send + '_>> {
        let _ = (table_name, cursor, context);
        Box::pin(async move {
            Ok(TablePage {
                columns: vec![],
                rows: vec![],
                next_cursor: None,
            })
        })
    }
}

/// A DDL loader answering one fixed value for every table.
struct FixedLoader(Option<String>);

impl DdlLoader for FixedLoader {
    fn load_ddl(
        &self,
        table_name: &str,
    ) -> Pin<Box<dyn Future<Output = Option<String>> + Send + '_>> {
        let _ = table_name;
        let out = self.0.clone();
        Box::pin(async move { out })
    }
}

fn exporter(load_ddl: Option<Arc<dyn DdlLoader>>, pin_now: bool) -> BlobTablesExporter {
    let mut options = BlobTablesExporterOptions {
        load_ddl,
        ..Default::default()
    };
    if pin_now {
        options.now = Some(Arc::new(pinned_now));
    }
    create_blob_tables_exporter(Arc::new(NoopReader), options)
}

fn table(name: &str) -> ExportTableInput {
    ExportTableInput {
        table_name: name.to_owned(),
        ..Default::default()
    }
}

fn provided_table(name: &str) -> ExportTableInput {
    ExportTableInput {
        table_name: name.to_owned(),
        source: ExportTableSource::Provided {
            columns: vec![ProvidedColumn {
                name: "a".to_owned(),
            }],
            rows: vec![vec![json!(1)]],
        },
        ..Default::default()
    }
}

fn sql_table(name: &str) -> ExportTableInput {
    ExportTableInput {
        table_name: name.to_owned(),
        source: ExportTableSource::Sql {
            sql: "SELECT * FROM t WHERE id > 1".to_owned(),
        },
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
        db_session_id: "sess-1".to_owned(),
        database: Some("db".to_owned()),
        schema: Some("public".to_owned()),
        database_type: Some("postgres".to_owned()),
        mode,
        data_format: format,
        output_mode: output,
        tables,
        ..Default::default()
    }
}

fn run(
    exporter: &BlobTablesExporter,
    request: TablesExportRequest,
) -> Result<crate::platform::blob_tables_exporter::PlannedExport, BlobTablesExportError> {
    futures::executor::block_on(exporter.plan_export(request))
}

// ── csv_cell: coder-uncovered corners ───────────────────────────────────────

#[test]
fn audit_csv_cell_negative_number_is_guarded_like_react() {
    // React `applyCsvFormulaGuard` quotes `-5` deliberately (see the
    // exportFormat.ts comment); the guard does not special-case numbers.
    assert_eq!(csv_cell(&json!(-5)), "\"-5\"");
    assert_eq!(csv_cell(&json!(-5.5)), "\"-5.5\"");
}

#[test]
fn audit_csv_cell_tab_does_not_trigger_quoting() {
    // RFC 4180 quoting fires on delimiter / quote / line break only;
    // a tab is inert text.
    assert_eq!(csv_cell(&json!("a\tb")), "a\tb");
    assert_eq!(csv_cell(&json!("a b")), "a b");
}

#[test]
fn audit_csv_cell_float_rendering_diverges_from_js_and_is_pinned() {
    // Divergence, pinned: JS `String(1)` is `"1"`, serde_json renders
    // the float `1.0` as `"1.0"`. Exponents, on the other hand, match
    // JS exactly (`String(1e21)` is `"1e+21"` in JS, and serde_json
    // renders `"1e+21"` too) — the module docs over-claim here, see the
    // audit report; only integral floats diverge.
    assert_eq!(csv_cell(&json!(1.0)), "1.0");
    assert_eq!(csv_cell(&json!(1.5)), "1.5");
    assert_eq!(csv_cell(&json!(1e21)), "1e+21");
    assert_eq!(escape_csv_field(&json!(2.0)), "2.0");
}

#[test]
fn audit_csv_cell_nested_json_is_quoted_once() {
    // An object serialises to JSON containing quotes, so the field is
    // quoted and the inner quotes doubled — exactly once.
    assert_eq!(
        csv_cell(&json!({"a": "b,c"})),
        "\"{\"\"a\"\":\"\"b,c\"\"}\""
    );
}

// ── base64_to_bytes: coder-uncovered corners ────────────────────────────────

#[test]
fn audit_base64_rejects_unpadded_and_bad_length_input_like_atob() {
    // `atob` throws when the stripped length is not a multiple of 4.
    assert!(base64_to_bytes("aGVsbG8").is_err());
    assert!(base64_to_bytes("aGVsbG8=").is_ok());
}

#[test]
fn audit_base64_strips_spaces_and_form_feeds_like_atob() {
    assert_eq!(
        base64_to_bytes(" aGVs\rbG8=\x0c").expect("ascii whitespace is stripped"),
        b"hello"
    );
}

#[test]
fn audit_base64_rejects_non_alphabet_bytes() {
    assert_eq!(
        base64_to_bytes("aGVsbG8*").expect_err("'*' is not base64"),
        BlobTablesExportError::InvalidBase64
    );
    // URL-safe alphabet is not standard base64: `-` / `_` must fail.
    assert!(base64_to_bytes("ab-_ab-_").is_err());
}

// ── download_blob ──────────────────────────────────────────────────────────

#[test]
fn audit_download_blob_pins_charset_for_text_plain_too() {
    let download = download_blob(
        DeliveredContent::Text("x".to_owned()),
        "f.txt",
        "text/plain",
    );
    assert_eq!(download.mime_type, "text/plain;charset=utf-8");
    assert_eq!(download.file_name, "f.txt");
    assert_eq!(download.contents, DeliveredContent::Text("x".to_owned()));
}

// ── gate order: combinations the coder did not exercise ────────────────────

#[test]
fn audit_gate_empty_beats_zip_and_sql_and_format_together() {
    let err = run(
        &exporter(None, false),
        request(
            vec![],
            ExportMode::DataOnly,
            ExportDataFormat::Parquet,
            ExportOutputMode::Zip,
        ),
    )
    .expect_err("empty tables must win over every other gate");
    assert_eq!(err, BlobTablesExportError::NoTablesSelected);
}

#[test]
fn audit_gate_structure_only_still_refuses_zip_output() {
    // The format gate is exempt for `structure_only`, but the zip gate
    // is not: React checks `outputMode === 'zip'` unconditionally.
    let err = run(
        &exporter(None, false),
        request(
            vec![table("t")],
            ExportMode::StructureOnly,
            ExportDataFormat::Xlsx,
            ExportOutputMode::Zip,
        ),
    )
    .expect_err("zip must fail even in structure_only");
    assert_eq!(err, BlobTablesExportError::ZipOutputUnsupported);
}

#[test]
fn audit_gate_sql_refused_in_data_and_structure_too() {
    // Only `structure_only` exempts the sql gate; `data_and_structure`
    // still plans a data file, so a filtered export is refused.
    let err = run(
        &exporter(None, false),
        request(
            vec![sql_table("q")],
            ExportMode::DataAndStructure,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect_err("sql source must fail in data_and_structure");
    assert_eq!(err, BlobTablesExportError::FilteredExportUnsupported);
}

#[test]
fn audit_gate_unsupported_format_names_parquet() {
    let err = run(
        &exporter(None, false),
        request(
            vec![table("t")],
            ExportMode::DataOnly,
            ExportDataFormat::Parquet,
            ExportOutputMode::Single,
        ),
    )
    .expect_err("parquet must fail for a table source");
    assert_eq!(
        err.to_string(),
        "'parquet' format is not supported by the web exporter"
    );
}

#[test]
fn audit_gate_sql_exempt_in_structure_only_with_explicit_ddl_plans() {
    // The sql gate lets `structure_only` through and planning succeeds
    // when the table carries explicit DDL.
    let mut input = sql_table("q");
    input.ddl = Some("CREATE TABLE q (id INT)".to_owned());
    let planned = run(
        &exporter(None, true),
        request(
            vec![input],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("structure_only sql with explicit ddl plans");
    assert_eq!(
        planned.plans,
        vec![ExportFilePlan::Ddl {
            filename: "q.sql".to_owned(),
            text: "CREATE TABLE q (id INT)".to_owned(),
        }]
    );
}

// ── planner errors propagate through plan_export ────────────────────────────

#[test]
fn audit_plan_error_duplicate_provided_columns_propagate() {
    let bad = ExportTableInput {
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
    let err = run(
        &exporter(None, false),
        request(
            vec![bad],
            ExportMode::DataOnly,
            ExportDataFormat::Json,
            ExportOutputMode::Single,
        ),
    )
    .expect_err("duplicate provided columns must fail in planning");
    assert_eq!(
        err,
        BlobTablesExportError::Plan(ExportPlanError::ProvidedDuplicateColumn {
            table: "p".to_owned(),
            column: "a".to_owned(),
        })
    );
}

#[test]
fn audit_plan_error_blank_sql_source_propagates_in_structure_only() {
    // The sql gate is exempt in `structure_only`, so planning runs and
    // `validate_table_source` rejects the blank statement first.
    let blank_sql = ExportTableInput {
        table_name: "q".to_owned(),
        ddl: Some("CREATE TABLE q (id INT)".to_owned()),
        source: ExportTableSource::Sql {
            sql: "   ".to_owned(),
        },
        ..Default::default()
    };
    let err = run(
        &exporter(None, false),
        request(
            vec![blank_sql],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect_err("blank sql must fail validation");
    assert_eq!(
        err,
        BlobTablesExportError::Plan(ExportPlanError::EmptySqlSource {
            table: "q".to_owned(),
        })
    );
}

#[test]
fn audit_plan_error_provided_row_length_propagates() {
    let bad = ExportTableInput {
        table_name: "p".to_owned(),
        source: ExportTableSource::Provided {
            columns: vec![ProvidedColumn {
                name: "a".to_owned(),
            }],
            rows: vec![vec![json!(1), json!(2)]],
        },
        ..Default::default()
    };
    let err = run(
        &exporter(None, false),
        request(
            vec![bad],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect_err("short provided row must fail in planning");
    assert_eq!(
        err,
        BlobTablesExportError::Plan(ExportPlanError::ProvidedRowLength {
            table: "p".to_owned(),
            row: 0,
            values: 2,
            columns: 1,
        })
    );
}

// ── DDL resolution in data_and_structure ────────────────────────────────────

#[test]
fn audit_ddl_loader_fills_prefix_for_sql_format_in_data_and_structure() {
    // `data_and_structure` is not `data_only`, so the loader runs; the
    // loaded DDL rides as the data plan's prefix (single `.sql` file).
    // The table must be `provided`: a table source with a non-CSV format
    // is refused by the CSV-only gate (in React too), so the loader path
    // for a sql format is only reachable with all-provided tables.
    let planned = run(
        &exporter(
            Some(Arc::new(FixedLoader(Some(
                "CREATE TABLE p (id INT)".to_owned(),
            )))),
            true,
        ),
        request(
            vec![provided_table("p")],
            ExportMode::DataAndStructure,
            ExportDataFormat::SqlInsert,
            ExportOutputMode::Single,
        ),
    )
    .expect("loaded ddl prefixes the sql data plan");
    assert_eq!(
        planned.plans,
        vec![ExportFilePlan::Data {
            filename: "p.sql".to_owned(),
            table: ExportTableInput {
                table_name: "p".to_owned(),
                ddl: Some("CREATE TABLE p (id INT)".to_owned()),
                source: ExportTableSource::Provided {
                    columns: vec![ProvidedColumn {
                        name: "a".to_owned(),
                    }],
                    rows: vec![vec![json!(1)]],
                },
                ..Default::default()
            },
            format: ExportDataFormat::SqlInsert,
            ddl_prefix: Some("CREATE TABLE p (id INT)".to_owned()),
        }]
    );
    assert_eq!(planned.file_name, "export_full_2026-11-19-10-23-14.sql");
}

#[test]
fn audit_ddl_own_blank_plus_loader_none_falls_through_to_placeholder() {
    let mut input = table("t");
    input.ddl = Some(String::new());
    let planned = run(
        &exporter(Some(Arc::new(FixedLoader(None))), false),
        request(
            vec![input],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("empty own ddl plus no loader plans the placeholder");
    assert_eq!(
        planned.plans,
        vec![ExportFilePlan::Ddl {
            filename: "t.sql".to_owned(),
            text: "-- DDL unavailable for t".to_owned(),
        }]
    );
    // The plan text is the placeholder (blank own DDL counts as no
    // DDL), but the effective request keeps the blank `ddl` verbatim —
    // React also returns the table unchanged when the loader answers
    // nothing, so `Some("")` is the faithful value, not `None`.
    assert_eq!(planned.request.tables[0].ddl.as_deref(), Some(""));
}

#[test]
fn audit_ddl_same_name_tables_deduplicate_through_exporter() {
    let planned = run(
        &exporter(None, false),
        request(
            vec![table("t"), table("t")],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("same-name tables plan");
    assert_eq!(planned.plans[0].filename(), "t.sql");
    assert_eq!(planned.plans[1].filename(), "t.1.sql");
}

// ── S1/S2 boundary: provided + parquet plans in S1 ─────────────────────────

#[test]
fn audit_provided_parquet_plans_in_s1_refusal_is_s2_emit() {
    // All-provided exempts the format gate and planning a `.parquet`
    // data plan needs no zip, so S1 returns a plan; React refuses
    // parquet for provided data only inside `emitDataPlan` (S2).
    let planned = run(
        &exporter(None, true),
        request(
            vec![provided_table("p")],
            ExportMode::DataOnly,
            ExportDataFormat::Parquet,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided parquet plans in S1");
    assert_eq!(planned.plans.len(), 1);
    assert_eq!(planned.plans[0].filename(), "p.parquet");
    assert_eq!(planned.file_name, "export_data_2026-11-19-10-23-14.parquet");
}

#[test]
fn audit_provided_xlsx_plans_in_s1() {
    let planned = run(
        &exporter(None, false),
        request(
            vec![provided_table("p")],
            ExportMode::DataOnly,
            ExportDataFormat::Xlsx,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided xlsx plans in S1");
    assert_eq!(planned.plans[0].filename(), "p.xlsx");
}

// ── factory clock ───────────────────────────────────────────────────────────

#[test]
fn audit_factory_without_pinned_now_still_names_by_mode_and_format() {
    // The default clock is `Utc::now`; only the shape is assertable.
    let planned = run(
        &exporter(None, false),
        request(
            vec![table("t")],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("plans with the real clock");
    assert!(planned.file_name.starts_with("export_data_"));
    assert!(planned.file_name.ends_with(".csv"));
}

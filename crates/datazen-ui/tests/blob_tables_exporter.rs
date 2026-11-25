//! T043-S1: `platform/blobTablesExporter.ts` slice 1 — CSV cell shaping,
//! base64 decode, Blob download request, factory shell, capability gates
//! (in React order), DDL resolution and the gate+plan seam for S2.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use serde_json::{json, Value};

use datazen_ui::platform::blob_tables_exporter::{
    apply_csv_formula_guard, base64_to_bytes, create_blob_tables_exporter, csv_cell, download_blob,
    escape_csv_field, BlobDownload, BlobTablesExportError, BlobTablesExporter,
    BlobTablesExporterOptions, DdlLoader, DeliveredContent, TablePage, TablePageReader,
    TablePageRequestContext,
};
use datazen_ui::platform::export_plan::{
    ExportDataFormat, ExportFilePlan, ExportMode, ExportOutputMode, ExportPlanError,
    ExportTableInput, ExportTableSource, ProvidedColumn, TablesExportRequest,
};

fn pinned_now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 19, 10, 23, 14)
        .single()
        .expect("fixed test time")
}

/// A reader that is never called by S1 (the page loop is S2's); it only
/// proves the factory stores the reader.
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

/// A DDL loader answering one fixed value (or `None`) for every table.
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

/// A DDL loader that counts its calls.
struct CountingLoader {
    calls: Arc<AtomicUsize>,
    ddl: Option<String>,
}

impl DdlLoader for CountingLoader {
    fn load_ddl(
        &self,
        table_name: &str,
    ) -> Pin<Box<dyn Future<Output = Option<String>> + Send + '_>> {
        let _ = table_name;
        self.calls.fetch_add(1, Ordering::SeqCst);
        let out = self.ddl.clone();
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
) -> Result<datazen_ui::platform::blob_tables_exporter::PlannedExport, BlobTablesExportError> {
    futures::executor::block_on(exporter.plan_export(request))
}

// ── csv_cell / escape_csv_field / apply_csv_formula_guard ────────────────

#[test]
fn csv_cell_plain_values_pass_through() {
    assert_eq!(csv_cell(&json!("hello")), "hello");
    assert_eq!(csv_cell(&json!(42)), "42");
    assert_eq!(csv_cell(&json!(true)), "true");
    assert_eq!(csv_cell(&Value::Null), "");
}

#[test]
fn csv_cell_quotes_fields_with_delimiter_quote_or_line_break() {
    assert_eq!(csv_cell(&json!("a,b")), "\"a,b\"");
    assert_eq!(csv_cell(&json!("say \"hi\"")), "\"say \"\"hi\"\"\"");
    assert_eq!(csv_cell(&json!("a\nb")), "\"a\nb\"");
    assert_eq!(csv_cell(&json!("a\rb")), "\"a\rb\"");
}

#[test]
fn csv_cell_renders_objects_and_arrays_as_json() {
    assert_eq!(csv_cell(&json!({ "a": 1 })), "\"{\"\"a\"\":1}\"");
    assert_eq!(csv_cell(&json!([1, 2])), "\"[1,2]\"");
}

#[test]
fn csv_cell_guards_every_formula_prefix() {
    assert_eq!(csv_cell(&json!("=1+1")), "\"=1+1\"");
    assert_eq!(csv_cell(&json!("+1")), "\"+1\"");
    assert_eq!(csv_cell(&json!("-x")), "\"-x\"");
    assert_eq!(csv_cell(&json!("@x")), "\"@x\"");
    // A value that does not start with a prefix is untouched.
    assert_eq!(csv_cell(&json!("x=1")), "x=1");
    assert_eq!(csv_cell(&json!("")), "");
}

#[test]
fn csv_cell_does_not_double_guard_an_already_quoted_field() {
    // `=a,b` is quoted by escaping (it contains a comma), so the guard
    // sees the leading `"` and leaves it alone.
    assert_eq!(csv_cell(&json!("=a,b")), "\"=a,b\"");
}

#[test]
fn escape_csv_field_is_the_kernel_face_without_the_guard() {
    // The guard lives in the CSV export layer only: the bare escaping
    // kernel does NOT quote a formula prefix.
    assert_eq!(escape_csv_field(&json!("=1+1")), "=1+1");
    assert_eq!(escape_csv_field(&Value::Null), "");
    assert_eq!(escape_csv_field(&json!({ "a": 1 })), "\"{\"\"a\"\":1}\"");
}

#[test]
fn apply_csv_formula_guard_wraps_unquoted_prefixes_only() {
    assert_eq!(apply_csv_formula_guard("=x"), "\"=x\"");
    assert_eq!(apply_csv_formula_guard("+x"), "\"+x\"");
    assert_eq!(apply_csv_formula_guard("-x"), "\"-x\"");
    assert_eq!(apply_csv_formula_guard("@x"), "\"@x\"");
    assert_eq!(apply_csv_formula_guard("\"=x\""), "\"=x\"");
    assert_eq!(apply_csv_formula_guard("x"), "x");
    assert_eq!(apply_csv_formula_guard(""), "");
}

// ── base64_to_bytes ───────────────────────────────────────────────────────

#[test]
fn base64_to_bytes_decodes_standard_base64() {
    assert_eq!(base64_to_bytes("aGVsbG8=").expect("valid base64"), b"hello");
    assert_eq!(base64_to_bytes("").expect("empty input"), b"");
}

#[test]
fn base64_to_bytes_strips_ascii_whitespace_like_atob() {
    assert_eq!(
        base64_to_bytes("aGVs\nbG8=\t").expect("whitespace is stripped"),
        b"hello"
    );
}

#[test]
fn base64_to_bytes_rejects_invalid_input() {
    let err = base64_to_bytes("!!!").expect_err("invalid base64 must fail");
    assert_eq!(err, BlobTablesExportError::InvalidBase64);
    assert_eq!(err.to_string(), "invalid base64 payload");
}

// ── download_blob ─────────────────────────────────────────────────────────

#[test]
fn download_blob_pins_charset_utf8_on_the_mime_type() {
    let download = download_blob(
        DeliveredContent::Text("a,b\n".to_owned()),
        "export.csv",
        "text/csv",
    );
    assert_eq!(
        download,
        BlobDownload {
            contents: DeliveredContent::Text("a,b\n".to_owned()),
            file_name: "export.csv".to_owned(),
            mime_type: "text/csv;charset=utf-8".to_owned(),
        }
    );
    // A binary MIME type gets the same pinning.
    let binary = download_blob(
        DeliveredContent::Bytes(vec![0x50, 0x4b]),
        "export.xlsx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    );
    assert_eq!(
        binary.mime_type,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet;charset=utf-8"
    );
}

// ── capability gates (React order) ──────────────────────────────────────────

#[test]
fn gate_empty_tables_first_even_when_format_would_also_fail() {
    let err = run(
        &exporter(None, false),
        request(
            vec![],
            ExportMode::DataOnly,
            ExportDataFormat::Tsv,
            ExportOutputMode::Single,
        ),
    )
    .expect_err("empty tables must fail");
    assert_eq!(err, BlobTablesExportError::NoTablesSelected);
    assert_eq!(err.to_string(), "no tables selected");
}

#[test]
fn gate_non_csv_format_refused_for_table_sources() {
    let err = run(
        &exporter(None, false),
        request(
            vec![table("t")],
            ExportMode::DataOnly,
            ExportDataFormat::Tsv,
            ExportOutputMode::Single,
        ),
    )
    .expect_err("tsv must fail for a table source");
    assert_eq!(
        err,
        BlobTablesExportError::UnsupportedFormat {
            format: ExportDataFormat::Tsv,
        }
    );
    assert_eq!(
        err.to_string(),
        "'tsv' format is not supported by the web exporter"
    );
}

#[test]
fn gate_format_checked_before_zip() {
    let err = run(
        &exporter(None, false),
        request(
            vec![table("t")],
            ExportMode::DataOnly,
            ExportDataFormat::Json,
            ExportOutputMode::Zip,
        ),
    )
    .expect_err("format gate precedes the zip gate");
    assert_eq!(
        err,
        BlobTablesExportError::UnsupportedFormat {
            format: ExportDataFormat::Json,
        }
    );
}

#[test]
fn gate_structure_only_exempts_the_format_gate() {
    let planned = run(
        &exporter(None, true),
        request(
            vec![table("t")],
            ExportMode::StructureOnly,
            ExportDataFormat::Xlsx,
            ExportOutputMode::Single,
        ),
    )
    .expect("structure_only ignores the data format");
    assert_eq!(planned.plans.len(), 1);
    assert!(matches!(planned.plans[0], ExportFilePlan::Ddl { .. }));
    assert_eq!(
        planned.file_name,
        "export_structure_2026-11-19-10-23-14.sql"
    );
}

#[test]
fn gate_all_provided_exempts_the_format_gate() {
    let planned = run(
        &exporter(None, true),
        request(
            vec![provided_table("p")],
            ExportMode::DataOnly,
            ExportDataFormat::Json,
            ExportOutputMode::Single,
        ),
    )
    .expect("all-provided json plans");
    assert_eq!(planned.plans.len(), 1);
    assert_eq!(planned.plans[0].filename(), "p.json");
    assert_eq!(planned.file_name, "export_data_2026-11-19-10-23-14.json");
}

#[test]
fn gate_mixed_provided_and_table_stays_csv_only() {
    let err = run(
        &exporter(None, false),
        request(
            vec![provided_table("p"), table("t")],
            ExportMode::DataOnly,
            ExportDataFormat::Json,
            ExportOutputMode::Single,
        ),
    )
    .expect_err("a mixed request stays csv-only");
    assert_eq!(
        err,
        BlobTablesExportError::UnsupportedFormat {
            format: ExportDataFormat::Json,
        }
    );
}

#[test]
fn gate_zip_output_refused() {
    let err = run(
        &exporter(None, false),
        request(
            vec![table("t")],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Zip,
        ),
    )
    .expect_err("zip must fail");
    assert_eq!(err, BlobTablesExportError::ZipOutputUnsupported);
    assert_eq!(
        err.to_string(),
        "zip output is not supported by the web exporter"
    );
}

#[test]
fn gate_zip_checked_before_sql_sources() {
    let err = run(
        &exporter(None, false),
        request(
            vec![sql_table("q")],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Zip,
        ),
    )
    .expect_err("zip gate precedes the sql gate");
    assert_eq!(err, BlobTablesExportError::ZipOutputUnsupported);
}

#[test]
fn gate_sql_source_refused_in_data_modes() {
    let err = run(
        &exporter(None, false),
        request(
            vec![sql_table("q")],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect_err("sql source must fail");
    assert_eq!(err, BlobTablesExportError::FilteredExportUnsupported);
    assert_eq!(
        err.to_string(),
        "filtered export is not supported by the web exporter"
    );
}

#[test]
fn gate_sql_source_exempt_in_structure_only_but_planning_refuses_without_ddl() {
    // The gate lets it through; the planner refuses: a sql source has no
    // table DDL to include without an explicit `ddl`.
    let err = run(
        &exporter(None, false),
        request(
            vec![sql_table("q")],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect_err("structure mode for a sql source without ddl must fail in planning");
    assert_eq!(
        err,
        BlobTablesExportError::Plan(ExportPlanError::StructureNeedsExplicitDdl {
            table: "q".to_owned()
        })
    );
    assert_eq!(
        err.to_string(),
        "structure export for non-table source 'q' needs an explicit ddl: \
         SQL and provided-data sources have no table DDL to include"
    );
}

#[test]
fn gate_multi_table_csv_data_only_needs_zip() {
    let err = run(
        &exporter(None, false),
        request(
            vec![table("t"), table("u")],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect_err("multi-table csv data export needs a zip");
    assert_eq!(err, BlobTablesExportError::MultiFileNeedsZip);
    assert_eq!(
        err.to_string(),
        "this export produces multiple files and needs a zip archive, \
         which is not supported by the web exporter; export one table at a time \
         in data-only mode, or use the desktop app"
    );
}

#[test]
fn gate_multi_table_structure_only_merges_without_zip() {
    let planned = run(
        &exporter(None, true),
        request(
            vec![table("t"), table("u")],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("multi-table structure plans merge");
    assert_eq!(planned.plans.len(), 2);
    assert_eq!(
        planned.file_name,
        "export_structure_2026-11-19-10-23-14.sql"
    );
}

// ── DDL resolution ──────────────────────────────────────────────────────────

#[test]
fn ddl_request_own_wins_over_loader() {
    let mut input = table("t");
    input.ddl = Some("CREATE TABLE t (id INT)".to_owned());
    let planned = run(
        &exporter(
            Some(Arc::new(FixedLoader(Some("LOADED DDL".to_owned())))),
            false,
        ),
        request(
            vec![input],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("own ddl plans");
    assert_eq!(
        planned.plans,
        vec![ExportFilePlan::Ddl {
            filename: "t.sql".to_owned(),
            text: "CREATE TABLE t (id INT)".to_owned(),
        }]
    );
    assert_eq!(
        planned.request.tables[0].ddl.as_deref(),
        Some("CREATE TABLE t (id INT)")
    );
}

#[test]
fn ddl_loader_fills_blank_own_ddl() {
    let mut input = table("t");
    input.ddl = Some("   ".to_owned());
    let planned = run(
        &exporter(
            Some(Arc::new(FixedLoader(Some("LOADED DDL".to_owned())))),
            false,
        ),
        request(
            vec![input],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("loaded ddl plans");
    assert_eq!(
        planned.plans,
        vec![ExportFilePlan::Ddl {
            filename: "t.sql".to_owned(),
            text: "LOADED DDL".to_owned(),
        }]
    );
    assert_eq!(planned.request.tables[0].ddl.as_deref(), Some("LOADED DDL"));
}

#[test]
fn ddl_blank_loaded_ddl_falls_through_to_placeholder() {
    let planned = run(
        &exporter(Some(Arc::new(FixedLoader(Some("  ".to_owned())))), false),
        request(
            vec![table("t")],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("blank loaded ddl falls through");
    assert_eq!(
        planned.plans,
        vec![ExportFilePlan::Ddl {
            filename: "t.sql".to_owned(),
            text: "-- DDL unavailable for t".to_owned(),
        }]
    );
    assert_eq!(planned.request.tables[0].ddl, None);
}

#[test]
fn ddl_loader_returning_none_falls_through_to_placeholder() {
    let planned = run(
        &exporter(Some(Arc::new(FixedLoader(None))), false),
        request(
            vec![table("t")],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("a loader answering none falls through");
    assert_eq!(
        planned.plans,
        vec![ExportFilePlan::Ddl {
            filename: "t.sql".to_owned(),
            text: "-- DDL unavailable for t".to_owned(),
        }]
    );
    assert_eq!(planned.request.tables[0].ddl, None);
}

#[test]
fn ddl_no_loader_no_ddl_leaves_placeholder() {
    let planned = run(
        &exporter(None, false),
        request(
            vec![table("t")],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("placeholder plans");
    assert_eq!(
        planned.plans,
        vec![ExportFilePlan::Ddl {
            filename: "t.sql".to_owned(),
            text: "-- DDL unavailable for t".to_owned(),
        }]
    );
}

#[test]
fn ddl_loader_not_called_in_data_only_mode() {
    let calls = Arc::new(AtomicUsize::new(0));
    let loader = Arc::new(CountingLoader {
        calls: Arc::clone(&calls),
        ddl: Some("SHOULD NOT APPEAR".to_owned()),
    });
    let planned = run(
        &exporter(Some(loader), false),
        request(
            vec![table("t")],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("data-only plans without ddl");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(matches!(planned.plans[0], ExportFilePlan::Data { .. }));
}

#[test]
fn ddl_loader_called_once_per_structure_table() {
    let calls = Arc::new(AtomicUsize::new(0));
    let loader = Arc::new(CountingLoader {
        calls: Arc::clone(&calls),
        ddl: Some("LOADED".to_owned()),
    });
    let planned = run(
        &exporter(Some(loader), false),
        request(
            vec![table("t"), table("u")],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("loaded ddl plans");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        planned.plans,
        vec![
            ExportFilePlan::Ddl {
                filename: "t.sql".to_owned(),
                text: "LOADED".to_owned(),
            },
            ExportFilePlan::Ddl {
                filename: "u.sql".to_owned(),
                text: "LOADED".to_owned(),
            },
        ]
    );
}

// ── request context + file name ─────────────────────────────────────────────

#[test]
fn planned_export_carries_request_context_from_the_request() {
    let planned = run(
        &exporter(None, false),
        request(
            vec![table("t")],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("plans");
    assert_eq!(
        planned.request_context,
        TablePageRequestContext {
            db_session_id: "sess-1".to_owned(),
            database: Some("db".to_owned()),
            schema: Some("public".to_owned()),
            driver_type: Some("postgres".to_owned()),
        }
    );
}

#[test]
fn planned_export_request_context_nulls_when_request_has_none() {
    let req = TablesExportRequest {
        db_session_id: "sess-2".to_owned(),
        mode: ExportMode::DataOnly,
        data_format: ExportDataFormat::Csv,
        output_mode: ExportOutputMode::Single,
        tables: vec![table("t")],
        ..Default::default()
    };
    let planned = run(&exporter(None, false), req).expect("plans");
    assert_eq!(
        planned.request_context,
        TablePageRequestContext {
            db_session_id: "sess-2".to_owned(),
            database: None,
            schema: None,
            driver_type: None,
        }
    );
}

#[test]
fn planned_export_file_name_uses_pinned_now() {
    let planned = run(
        &exporter(None, true),
        request(
            vec![table("t")],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("plans");
    assert_eq!(planned.file_name, "export_data_2026-11-19-10-23-14.csv");
}

// ── error display ───────────────────────────────────────────────────────────

#[test]
fn error_display_covers_rune_side_variants() {
    assert_eq!(
        BlobTablesExportError::ReadFailed("boom".to_owned()).to_string(),
        "boom"
    );
    assert_eq!(
        BlobTablesExportError::NoDownloadSink.to_string(),
        "no Blob download sink is wired for the web exporter; \
         the host must inject a deliver sink"
    );
    assert_eq!(
        BlobTablesExportError::Plan(ExportPlanError::NoTables).to_string(),
        "no tables selected"
    );
}

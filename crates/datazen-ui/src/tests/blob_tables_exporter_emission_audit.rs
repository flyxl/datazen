//! T043-S2 independent audit: `platform/blobTablesExporter.ts` slice 2 —
//! data emission (the `provided` branch through the formatter mirror, the
//! `table` branch through the page loop) and merged delivery.
//!
//! Every expectation below is re-derived from the React originals
//! (`packages/frontend/src/platform/blobTablesExporter.ts` and
//! `packages/frontend/src/lib/exportFormat.ts`) plus `gridClipboard.ts`
//! for the shared escaping kernels — not from the coder's test file and
//! not from the coder's implementation notes. Where the Rust mirror
//! knowingly diverges from JS, the test pins the Rust behaviour and the
//! comment says so (integral floats, nested-object key order, the
//! `isPrimaryKey` middle step, ragged provided rows).

use std::collections::VecDeque;
use std::io::Read;
use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};
use serde_json::{json, Value};

use crate::platform::blob_tables_exporter::{
    base64_to_bytes, create_blob_tables_exporter, BlobDownload, BlobTablesExportError,
    BlobTablesExporter, BlobTablesExporterOptions, DeliverFn, DeliveredContent, EmittedDataPlan,
    ExportTablesOutcome, PlannedExport, TablePage, TablePageReader, TablePageRequestContext,
};
use crate::platform::export_format::{format_rows_as_text, format_rows_as_xlsx};
use crate::platform::export_plan::{
    ExportDataFormat, ExportFilePlan, ExportMode, ExportOutputMode, ExportTableInput,
    ExportTableSource, ProvidedColumn, TablesExportRequest,
};

fn pinned_now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 19, 10, 23, 14)
        .single()
        .expect("fixed test time")
}

/// A reader that answers a scripted page sequence and records every
/// call (table, cursor, schema) for the page-loop assertions.
struct QueueReader {
    pages: Mutex<VecDeque<TablePage>>,
    calls: Mutex<Vec<(String, Option<Value>, Option<String>)>>,
    fail: Mutex<Option<String>>,
}

impl QueueReader {
    fn new(pages: Vec<TablePage>) -> Self {
        QueueReader {
            pages: Mutex::new(pages.into()),
            calls: Mutex::new(Vec::new()),
            fail: Mutex::new(None),
        }
    }

    fn failing(message: &str) -> Self {
        let reader = QueueReader::new(vec![]);
        *reader.fail.lock().expect("reader lock") = Some(message.to_owned());
        reader
    }

    fn calls(&self) -> Vec<(String, Option<Value>, Option<String>)> {
        self.calls.lock().expect("reader lock").clone()
    }
}

impl TablePageReader for QueueReader {
    fn read_page(
        &self,
        table_name: &str,
        cursor: Option<&Value>,
        context: &TablePageRequestContext,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<TablePage, BlobTablesExportError>> + Send + '_>,
    > {
        self.calls.lock().expect("reader lock").push((
            table_name.to_owned(),
            cursor.cloned(),
            context.schema.clone(),
        ));
        if let Some(message) = self.fail.lock().expect("reader lock").clone() {
            return Box::pin(async move { Err(BlobTablesExportError::ReadFailed(message)) });
        }
        let page = self
            .pages
            .lock()
            .expect("reader lock")
            .pop_front()
            .unwrap_or(TablePage {
                columns: vec![],
                rows: vec![],
                next_cursor: None,
            });
        Box::pin(async move { Ok(page) })
    }
}

/// A deliver sink that records every download instead of downloading.
struct CaptureSink {
    downloads: Mutex<Vec<BlobDownload>>,
}

impl CaptureSink {
    fn sink(capture: &Arc<CaptureSink>) -> DeliverFn {
        let capture = Arc::clone(capture);
        Arc::new(move |download: &BlobDownload| {
            capture
                .downloads
                .lock()
                .expect("deliver lock")
                .push(download.clone());
        })
    }

    fn downloads(&self) -> Vec<BlobDownload> {
        self.downloads.lock().expect("deliver lock").clone()
    }
}

fn exporter(reader: Arc<dyn TablePageReader>, deliver: Option<DeliverFn>) -> BlobTablesExporter {
    let options = BlobTablesExporterOptions {
        deliver,
        now: Some(Arc::new(pinned_now)),
        ..Default::default()
    };
    create_blob_tables_exporter(reader, options)
}

fn table_input(name: &str) -> ExportTableInput {
    ExportTableInput {
        table_name: name.to_owned(),
        ..Default::default()
    }
}

fn provided_table(name: &str, columns: &[&str], rows: Vec<Vec<Value>>) -> ExportTableInput {
    ExportTableInput {
        table_name: name.to_owned(),
        source: ExportTableSource::Provided {
            columns: columns
                .iter()
                .map(|name| ProvidedColumn {
                    name: (*name).to_owned(),
                })
                .collect(),
            rows,
        },
        ..Default::default()
    }
}

fn request(
    tables: Vec<ExportTableInput>,
    mode: ExportMode,
    format: ExportDataFormat,
) -> TablesExportRequest {
    TablesExportRequest {
        db_session_id: "sess-1".to_owned(),
        database: Some("db".to_owned()),
        schema: Some("public".to_owned()),
        database_type: Some("postgres".to_owned()),
        mode,
        data_format: format,
        output_mode: ExportOutputMode::Single,
        tables,
        ..Default::default()
    }
}

fn plan(
    exporter: &BlobTablesExporter,
    request: TablesExportRequest,
) -> Result<PlannedExport, BlobTablesExportError> {
    futures::executor::block_on(exporter.plan_export(request))
}

fn emit(
    exporter: &BlobTablesExporter,
    planned: &PlannedExport,
    index: usize,
) -> Result<EmittedDataPlan, BlobTablesExportError> {
    futures::executor::block_on(exporter.emit_data_plan(planned, &planned.plans[index]))
}

fn export(
    exporter: &BlobTablesExporter,
    request: TablesExportRequest,
) -> Result<ExportTablesOutcome, BlobTablesExportError> {
    futures::executor::block_on(exporter.export_tables(request))
}

fn page(columns: &[&str], rows: Vec<Vec<Value>>, next_cursor: Option<Value>) -> TablePage {
    TablePage {
        columns: columns.iter().map(|name| (*name).to_owned()).collect(),
        rows,
        next_cursor,
    }
}

fn text_of(emitted: &EmittedDataPlan) -> &str {
    match &emitted.content {
        DeliveredContent::Text(text) => text,
        DeliveredContent::Bytes(_) => panic!("expected text content, got bytes"),
    }
}

fn bytes_of(emitted: &EmittedDataPlan) -> &[u8] {
    match &emitted.content {
        DeliveredContent::Bytes(bytes) => bytes,
        DeliveredContent::Text(_) => panic!("expected bytes content, got text"),
    }
}

/// Read one file out of the emitted XLSX zip container.
fn xlsx_entry(bytes: &[u8], name: &str) -> String {
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("emitted xlsx is a zip");
    let mut file = archive.by_name(name).expect("xlsx entry exists");
    let mut text = String::new();
    file.read_to_string(&mut text).expect("xlsx entry is text");
    text
}

/// A hand-built planned export, for merge paths the planner refuses
/// (a binary plan inside a multi-plan merge cannot come out of
/// `plan_export`, which rejects the zip it would need).
fn hand_planned(request: TablesExportRequest, plans: Vec<ExportFilePlan>) -> PlannedExport {
    PlannedExport {
        request_context: TablePageRequestContext {
            db_session_id: "sess-1".to_owned(),
            database: Some("db".to_owned()),
            schema: Some("public".to_owned()),
            driver_type: Some("postgres".to_owned()),
        },
        file_name: "export.sql".to_owned(),
        request,
        plans,
    }
}

// ── provided branch: CSV ────────────────────────────────────────────────────

#[test]
fn audit_provided_csv_header_rows_and_trailing_newlines() {
    // React: header first, one `\n`-terminated line per row; the
    // formula guard quotes a `=`-leading field, a comma forces RFC 4180
    // quoting, null is empty.
    let exp = exporter(Arc::new(QueueReader::new(vec![])), None);
    let planned = plan(
        &exp,
        request(
            vec![provided_table(
                "t",
                &["id", "note"],
                vec![
                    vec![json!(1), json!("a,b")],
                    vec![json!(2), json!("=2+2")],
                    vec![json!(3), Value::Null],
                ],
            )],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
        ),
    )
    .expect("plan");
    let emitted = emit(&exp, &planned, 0).expect("emit");
    assert_eq!(emitted.mime_type, "text/csv");
    assert_eq!(
        text_of(&emitted),
        "id,note\n1,\"a,b\"\n2,\"=2+2\"\n3,\n",
        "every line, header included, ends with \\n"
    );
}

#[test]
fn audit_provided_csv_integral_float_renders_1_point_0() {
    // Pinned divergence: JS `String(1)` renders `1`; serde_json (ryu)
    // renders an integral float as `1.0`. The doc says so; the test
    // pins the Rust side of it.
    let exp = exporter(Arc::new(QueueReader::new(vec![])), None);
    let planned = plan(
        &exp,
        request(
            vec![provided_table("t", &["v"], vec![vec![json!(1.0)]])],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
        ),
    )
    .expect("plan");
    assert_eq!(text_of(&emit(&exp, &planned, 0).expect("emit")), "v\n1.0\n");
}

#[test]
fn audit_provided_csv_exponent_1e21_matches_js() {
    // S1 tester finding (a): `1e21` renders as `"1e+21"` on both sides —
    // the exponent is NOT part of the divergence, only `1.0` is.
    let exp = exporter(Arc::new(QueueReader::new(vec![])), None);
    let planned = plan(
        &exp,
        request(
            vec![provided_table("t", &["v"], vec![vec![json!(1e21)]])],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
        ),
    )
    .expect("plan");
    assert_eq!(
        text_of(&emit(&exp, &planned, 0).expect("emit")),
        "v\n1e+21\n"
    );
}

#[test]
fn audit_provided_csv_ignores_a_ddl_prefix() {
    // React's CSV branch never looks at `plan.ddlPrefix` (only SQL data
    // plans ever carry one, per `build_file_plans`); a hand-built plan
    // carrying one must still emit bare CSV.
    let exp = exporter(Arc::new(QueueReader::new(vec![])), None);
    let table = provided_table("t", &["id"], vec![vec![json!(1)]]);
    let req = request(
        vec![table.clone()],
        ExportMode::DataOnly,
        ExportDataFormat::Csv,
    );
    let planned = hand_planned(
        req,
        vec![ExportFilePlan::Data {
            filename: "t.csv".to_owned(),
            table,
            format: ExportDataFormat::Csv,
            ddl_prefix: Some("CREATE TABLE t (id INTEGER);".to_owned()),
        }],
    );
    assert_eq!(text_of(&emit(&exp, &planned, 0).expect("emit")), "id\n1\n");
}

#[test]
fn audit_provided_csv_ragged_row_follows_the_positional_contract() {
    // React's CSV branch maps over the row itself (`row.map`), so a
    // short row would come out short and a long row long. The Rust
    // mirror iterates over the columns instead (missing cell reads as
    // null, extras ignored) — the same `cellAt` contract
    // `formatRowsAsText` documents. This is unreachable through
    // `export_tables`: the planner rejects a ragged provided row with
    // `ProvidedRowLength` before emission, so the delta only shows for
    // hand-built plans. Pinned here as the Rust behaviour, non-blocking.
    let exp = exporter(Arc::new(QueueReader::new(vec![])), None);
    let table = provided_table(
        "t",
        &["a", "b"],
        vec![vec![json!(1)], vec![json!(1), json!(2), json!(3)]],
    );
    let req = request(
        vec![table.clone()],
        ExportMode::DataOnly,
        ExportDataFormat::Csv,
    );
    let planned = hand_planned(
        req,
        vec![ExportFilePlan::Data {
            filename: "t.csv".to_owned(),
            table,
            format: ExportDataFormat::Csv,
            ddl_prefix: None,
        }],
    );
    assert_eq!(
        text_of(&emit(&exp, &planned, 0).expect("emit")),
        "a,b\n1,\n1,2\n"
    );
}

// ── provided branch: XLSX ───────────────────────────────────────────────────

#[test]
fn audit_provided_xlsx_workbook_is_named_sheet1() {
    // React: `book_append_sheet(wb, ws, 'Sheet1')` — the sheet is
    // always `Sheet1`, whatever the table is called.
    let exp = exporter(Arc::new(QueueReader::new(vec![])), None);
    let planned = plan(
        &exp,
        request(
            vec![provided_table(
                "some_other_name",
                &["id"],
                vec![vec![json!(1)]],
            )],
            ExportMode::DataOnly,
            ExportDataFormat::Xlsx,
        ),
    )
    .expect("plan");
    let emitted = emit(&exp, &planned, 0).expect("emit");
    assert_eq!(
        emitted.mime_type,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    );
    let workbook = xlsx_entry(bytes_of(&emitted), "xl/workbook.xml");
    assert!(
        workbook.contains("name=\"Sheet1\""),
        "workbook names its sheet Sheet1: {workbook}"
    );
}

#[test]
fn audit_provided_xlsx_cells_match_the_rows_structurally() {
    // XLSX bytes differ from SheetJS by design, so parity is
    // structural: the workbook is a valid zip whose sheet holds the
    // same cells — strings in the shared table, the number as a numeric
    // cell, the boolean as a `b` cell, null as no cell at all.
    let exp = exporter(Arc::new(QueueReader::new(vec![])), None);
    let planned = plan(
        &exp,
        request(
            vec![provided_table(
                "t",
                &["id", "name", "ok"],
                vec![
                    vec![json!(42), json!("Alice"), json!(true)],
                    vec![Value::Null, json!("Bob"), json!(false)],
                ],
            )],
            ExportMode::DataOnly,
            ExportDataFormat::Xlsx,
        ),
    )
    .expect("plan");
    let emitted = emit(&exp, &planned, 0).expect("emit");
    let bytes = bytes_of(&emitted);
    assert_eq!(&bytes[..4], b"PK\x03\x04", "an xlsx is a zip");
    let strings = xlsx_entry(bytes, "xl/sharedStrings.xml");
    for expected in ["id", "name", "ok", "Alice", "Bob"] {
        assert!(strings.contains(expected), "shared strings hold {expected}");
    }
    let sheet = xlsx_entry(bytes, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("<v>42</v>"), "number cell: {sheet}");
    assert!(sheet.contains("t=\"b\""), "boolean cell: {sheet}");
    // A null cell writes nothing: no cell element references row 2
    // column A beyond the header/number rows' own cells.
    assert!(!sheet.contains("<c r=\"A3\""), "null stays blank: {sheet}");
}

#[test]
fn audit_provided_xlsx_base64_round_trips_through_base64_to_bytes() {
    // React hands `formatRowsAsXlsx`'s base64 to `base64ToBytes` before
    // the Blob; the mirror must hand its own base64 to the same decode
    // and get the workbook bytes back.
    let formatted = format_rows_as_xlsx(
        &[ProvidedColumn {
            name: "id".to_owned(),
        }],
        &[vec![json!(7)]],
    )
    .expect("xlsx");
    let bytes = base64_to_bytes(&formatted.data_base64).expect("decode");
    assert_eq!(&bytes[..4], b"PK\x03\x04");
    assert!(xlsx_entry(&bytes, "xl/worksheets/sheet1.xml").contains("<v>7</v>"));
}

#[test]
fn audit_provided_xlsx_with_ddl_prefix_is_refused() {
    // React: a DDL prefix cannot ride a workbook — reaching here with
    // one is a planner bug, not a user error.
    let exp = exporter(Arc::new(QueueReader::new(vec![])), None);
    let table = provided_table("t", &["id"], vec![vec![json!(1)]]);
    let req = request(
        vec![table.clone()],
        ExportMode::DataOnly,
        ExportDataFormat::Xlsx,
    );
    let planned = hand_planned(
        req,
        vec![ExportFilePlan::Data {
            filename: "t.xlsx".to_owned(),
            table,
            format: ExportDataFormat::Xlsx,
            ddl_prefix: Some("CREATE TABLE t (id INTEGER);".to_owned()),
        }],
    );
    let err = emit(&exp, &planned, 0).expect_err("ddl on xlsx must fail");
    assert_eq!(err, BlobTablesExportError::DdlPrefixOnXlsx);
    assert_eq!(err.to_string(), "DDL cannot be prefixed to an XLSX export");
}

#[test]
fn audit_provided_xlsx_empty_rows_still_has_the_header() {
    let exp = exporter(Arc::new(QueueReader::new(vec![])), None);
    let planned = plan(
        &exp,
        request(
            vec![provided_table("t", &["id", "name"], vec![])],
            ExportMode::DataOnly,
            ExportDataFormat::Xlsx,
        ),
    )
    .expect("plan");
    let emitted = emit(&exp, &planned, 0).expect("emit");
    let strings = xlsx_entry(bytes_of(&emitted), "xl/sharedStrings.xml");
    assert!(strings.contains("id") && strings.contains("name"));
}

// ── provided branch: Parquet refusal ───────────────────────────────────────

#[test]
fn audit_provided_parquet_is_refused_loudly() {
    let exp = exporter(Arc::new(QueueReader::new(vec![])), None);
    let planned = plan(
        &exp,
        request(
            vec![provided_table("t", &["id"], vec![vec![json!(1)]])],
            ExportMode::DataOnly,
            ExportDataFormat::Parquet,
        ),
    )
    .expect("plan");
    let err = emit(&exp, &planned, 0).expect_err("parquet must fail");
    assert_eq!(err, BlobTablesExportError::ProvidedParquetUnsupported);
    assert_eq!(
        err.to_string(),
        "provided-data export cannot format 'parquet'"
    );
}

// ── provided branch: other text formats ────────────────────────────────────

fn cols(names: &[&str]) -> Vec<ProvidedColumn> {
    names
        .iter()
        .map(|name| ProvidedColumn {
            name: (*name).to_owned(),
        })
        .collect()
}

#[test]
fn audit_tsv_tabs_and_line_breaks_become_spaces() {
    // `escapeTsvField`: null is empty, objects as JSON, then tabs and
    // line breaks become spaces (carriage returns are dropped).
    let out = format_rows_as_text(
        "t",
        &cols(&["a", "b"]),
        &[vec![json!("x\ty\nz\rw"), json!({"k": 1})]],
        ExportDataFormat::Tsv,
        None,
        None,
    )
    .expect("tsv");
    assert_eq!(out.mime_type, "text/tab-separated-values");
    assert_eq!(out.content, "a\tb\nx y zw\t{\"k\":1}");
}

#[test]
fn audit_json_top_level_keys_follow_column_order() {
    // `rowsAsRecords` builds `{ column: value }` in column order; the
    // top level is built by hand here for exactly that reason.
    let out = format_rows_as_text(
        "t",
        &cols(&["b", "a"]),
        &[vec![json!(1), json!(2)]],
        ExportDataFormat::Json,
        None,
        None,
    )
    .expect("json");
    assert_eq!(out.mime_type, "application/json");
    let b = out.content.find("\"b\"").expect("b key");
    let a = out.content.find("\"a\"").expect("a key");
    assert!(b < a, "column order, not sorted: {}", out.content);
    assert_eq!(out.content, "[\n  {\n    \"b\": 1,\n    \"a\": 2\n  }\n]");
}

#[test]
fn audit_json_nested_object_keys_are_sorted_pinned_divergence() {
    // Pinned divergence: a nested object renders through serde_json,
    // whose map order is sorted — JS `JSON.stringify` keeps insertion
    // order (`{"z": 1, "a": 2}`). The wire `Value` has already lost
    // the original key order, so no mirror can recover it.
    let out = format_rows_as_text(
        "t",
        &cols(&["data"]),
        &[vec![json!({"z": 1, "a": 2})]],
        ExportDataFormat::Json,
        None,
        None,
    )
    .expect("json");
    let a = out.content.find("\"a\"").expect("a key");
    let z = out.content.find("\"z\"").expect("z key");
    assert!(a < z, "nested keys sort: {}", out.content);
}

#[test]
fn audit_json_empty_rows_is_an_empty_array() {
    let out = format_rows_as_text("t", &cols(&["a"]), &[], ExportDataFormat::Json, None, None)
        .expect("json");
    assert_eq!(out.content, "[]");
}

#[test]
fn audit_markdown_escapes_pipes_and_line_breaks() {
    // The one serializer's Markdown branch (DB-10): `|` → `\|`,
    // line breaks → `<br>`, header plus a `---` separator row.
    let out = format_rows_as_text(
        "t",
        &cols(&["a|b", "c"]),
        &[vec![json!("x\ny"), json!(1)]],
        ExportDataFormat::Markdown,
        None,
        None,
    )
    .expect("markdown");
    assert_eq!(out.mime_type, "text/markdown");
    assert_eq!(out.content, "| a\\|b | c |\n| --- | --- |\n| x<br>y | 1 |");
}

#[test]
fn audit_sql_insert_batches_every_500_rows() {
    // `SQL_INSERT_BATCH_SIZE` is 500 (TablePlus-style): 501 rows make
    // two INSERT statements inside one BEGIN/COMMIT transaction.
    let rows: Vec<Vec<Value>> = (0..501).map(|i| vec![json!(i)]).collect();
    let out = format_rows_as_text(
        "t",
        &cols(&["id"]),
        &rows,
        ExportDataFormat::SqlInsert,
        Some("postgres"),
        None,
    )
    .expect("insert");
    assert_eq!(out.mime_type, "text/sql");
    assert_eq!(out.content.matches("INSERT INTO").count(), 2);
    assert!(out
        .content
        .starts_with("BEGIN;\nINSERT INTO \"t\" (\"id\") VALUES\n  (0),"));
    assert!(out.content.ends_with(";\nCOMMIT;"));
}

#[test]
fn audit_sql_insert_empty_rows_is_begin_then_commit() {
    let out = format_rows_as_text(
        "t",
        &cols(&["id"]),
        &[],
        ExportDataFormat::SqlInsert,
        None,
        None,
    )
    .expect("insert");
    assert_eq!(out.content, "BEGIN;\nCOMMIT;");
}

#[test]
fn audit_sql_insert_mysql_backslash_escapes() {
    // MySQL-family escaper, three passes: `\` → `\\`, `"` → `\"`,
    // `'` → `''`; identifiers quote with backticks.
    let out = format_rows_as_text(
        "t",
        &cols(&["c"]),
        &[vec![json!("a\\b\"c'd")]],
        ExportDataFormat::SqlInsert,
        Some("mysql"),
        None,
    )
    .expect("insert");
    assert_eq!(
        out.content,
        "BEGIN;\nINSERT INTO `t` (`c`) VALUES\n  ('a\\\\b\\\"c''d');\nCOMMIT;"
    );
}

#[test]
fn audit_sql_insert_sqlserver_brackets_and_transaction() {
    let out = format_rows_as_text(
        "t",
        &cols(&["id"]),
        &[vec![json!(1)]],
        ExportDataFormat::SqlInsert,
        Some("sqlserver"),
        None,
    )
    .expect("insert");
    assert_eq!(
        out.content,
        "BEGIN TRANSACTION;\nINSERT INTO [t] ([id]) VALUES\n  (1);\nCOMMIT;"
    );
}

#[test]
fn audit_sql_update_primary_key_wins_over_first_column() {
    // The caller's `primaryKey`, when it is among the exported
    // columns, is the key — not the first column.
    let out = format_rows_as_text(
        "t",
        &cols(&["id", "email", "name"]),
        &[vec![json!(1), json!("a@x"), json!("n")]],
        ExportDataFormat::SqlUpdate,
        Some("postgres"),
        Some("email"),
    )
    .expect("update");
    assert_eq!(
        out.content,
        "BEGIN;\nUPDATE \"t\" SET \"id\" = 1, \"name\" = 'n' WHERE \"email\" = 'a@x';\nCOMMIT;"
    );
}

#[test]
fn audit_sql_update_without_primary_key_uses_first_column() {
    // Pinned divergence: React also checks a column's `isPrimaryKey`
    // between the caller's key and the first column; `ProvidedColumn`
    // is name-only, so that middle step is unavailable here and the
    // mirror falls back straight to the first column.
    let out = format_rows_as_text(
        "t",
        &cols(&["id", "name"]),
        &[vec![json!(1), json!("n")]],
        ExportDataFormat::SqlUpdate,
        Some("postgres"),
        None,
    )
    .expect("update");
    assert_eq!(
        out.content,
        "BEGIN;\nUPDATE \"t\" SET \"name\" = 'n' WHERE \"id\" = 1;\nCOMMIT;"
    );
}

#[test]
fn audit_sql_update_single_column_table_has_nothing_to_set() {
    let err = format_rows_as_text(
        "t",
        &cols(&["id"]),
        &[vec![json!(1)]],
        ExportDataFormat::SqlUpdate,
        None,
        None,
    )
    .expect_err("single-column update must fail");
    assert_eq!(
        err.to_string(),
        "sql_update export needs at least one non-key column to SET"
    );
}

#[test]
fn audit_sql_data_plan_ddl_prefix_joins_with_blank_line() {
    // React: `{ddlPrefix}\n\n{content}` — a DDL prefix on a SQL data
    // plan rides in the same file, separated by a blank line.
    let mut table = provided_table("t", &["id"], vec![vec![json!(1)]]);
    table.ddl = Some("CREATE TABLE t (id INTEGER);".to_owned());
    let exp = exporter(Arc::new(QueueReader::new(vec![])), None);
    let planned = plan(
        &exp,
        request(
            vec![table],
            ExportMode::DataAndStructure,
            ExportDataFormat::SqlInsert,
        ),
    )
    .expect("plan");
    let emitted = emit(&exp, &planned, 0).expect("emit");
    assert_eq!(
        text_of(&emitted),
        "CREATE TABLE t (id INTEGER);\n\nBEGIN;\nINSERT INTO \"t\" (\"id\") VALUES\n  (1);\nCOMMIT;"
    );
}

#[test]
fn audit_format_text_csv_joins_without_trailing_newline() {
    // `formatRowsAsText` joins lines with no trailing newline — unlike
    // the exporter's own `\n`-terminated `csv_cell` emission, which is
    // why the exporter never calls this branch for CSV.
    let out = format_rows_as_text(
        "t",
        &cols(&["a", "b"]),
        &[vec![json!(1), json!(2)]],
        ExportDataFormat::Csv,
        None,
        None,
    )
    .expect("csv");
    assert_eq!(out.content, "a,b\n1,2");
}

#[test]
fn audit_format_text_refuses_binary_formats() {
    for format in [ExportDataFormat::Xlsx, ExportDataFormat::Parquet] {
        let err = format_rows_as_text("t", &cols(&["a"]), &[], format, None, None)
            .expect_err("binary format as text must fail");
        assert_eq!(
            err.to_string(),
            format!("formatRowsAsText cannot format '{format}' as text")
        );
    }
}

#[test]
fn audit_format_text_and_xlsx_require_columns() {
    let err = format_rows_as_text("t", &[], &[], ExportDataFormat::Csv, None, None)
        .expect_err("no columns must fail");
    assert_eq!(err.to_string(), "export needs at least one column");
    let err = format_rows_as_xlsx(&[], &[]).expect_err("no columns must fail");
    assert_eq!(err.to_string(), "export needs at least one column");
}

// ── table branch: the page loop ─────────────────────────────────────────────

#[test]
fn audit_table_branch_reads_pages_until_the_cursor_is_none() {
    // React: page until the reader says the table is exhausted; the
    // first page contributes the header, every page its rows.
    let reader = Arc::new(QueueReader::new(vec![
        page(
            &["id", "v"],
            vec![vec![json!(1), json!("a")]],
            Some(json!("p1")),
        ),
        page(
            &["id", "v"],
            vec![vec![json!(2), json!("b")]],
            Some(json!("p2")),
        ),
        page(&["id", "v"], vec![vec![json!(3), json!("c")]], None),
    ]));
    let exp = exporter(reader.clone(), None);
    let planned = plan(
        &exp,
        request(
            vec![table_input("t")],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
        ),
    )
    .expect("plan");
    let emitted = emit(&exp, &planned, 0).expect("emit");
    assert_eq!(emitted.mime_type, "text/csv");
    assert_eq!(text_of(&emitted), "id,v\n1,a\n2,b\n3,c\n");
    assert_eq!(
        reader.calls(),
        vec![
            ("t".to_owned(), None, Some("public".to_owned())),
            ("t".to_owned(), Some(json!("p1")), Some("public".to_owned())),
            ("t".to_owned(), Some(json!("p2")), Some("public".to_owned())),
        ],
        "first call passes no cursor; later calls pass the previous page's"
    );
}

#[test]
fn audit_table_branch_null_cursor_counts_as_exhausted() {
    // S1 tester finding (d): a reader answering `Some(Value::Null)`
    // means "no more pages", exactly as React's `cursor === null`
    // break — the loop must NOT call the reader again after it.
    let reader = Arc::new(QueueReader::new(vec![page(
        &["id"],
        vec![vec![json!(1)]],
        Some(Value::Null),
    )]));
    let exp = exporter(reader.clone(), None);
    let planned = plan(
        &exp,
        request(
            vec![table_input("t")],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
        ),
    )
    .expect("plan");
    assert_eq!(text_of(&emit(&exp, &planned, 0).expect("emit")), "id\n1\n");
    assert_eq!(reader.calls().len(), 1, "no call after the null cursor");
}

#[test]
fn audit_table_branch_schema_override_applies_per_call() {
    // React: every page call overrides the request context's schema
    // with the table's own (`schema: table.schema ?? requestContext.schema`).
    let reader = Arc::new(QueueReader::new(vec![
        page(&["id"], vec![vec![json!(1)]], None),
        page(&["id"], vec![vec![json!(2)]], None),
    ]));
    let exp = exporter(reader.clone(), None);
    let mut first = table_input("one");
    first.schema = Some("tenant".to_owned());
    let second = table_input("two");
    // Two tables in one request would plan two `.csv` files, which
    // needs a zip and is refused before emission — so each table is
    // planned (and emitted) on its own; the reader still sees both
    // calls in order.
    for table in [first, second] {
        let planned = plan(
            &exp,
            request(vec![table], ExportMode::DataOnly, ExportDataFormat::Csv),
        )
        .expect("plan");
        emit(&exp, &planned, 0).expect("emit");
    }
    assert_eq!(
        reader.calls(),
        vec![
            ("one".to_owned(), None, Some("tenant".to_owned())),
            ("two".to_owned(), None, Some("public".to_owned())),
        ]
    );
}

#[test]
fn audit_table_branch_header_comes_from_the_first_page() {
    // React writes the header from the first page's columns, not from
    // the table's SELECT list.
    let reader = Arc::new(QueueReader::new(vec![page(
        &["a", "b"],
        vec![vec![json!(1), json!(2)]],
        None,
    )]));
    let exp = exporter(reader, None);
    let mut table = table_input("t");
    table.columns = vec!["x".to_owned()];
    let planned = plan(
        &exp,
        request(vec![table], ExportMode::DataOnly, ExportDataFormat::Csv),
    )
    .expect("plan");
    assert_eq!(
        text_of(&emit(&exp, &planned, 0).expect("emit")),
        "a,b\n1,2\n"
    );
}

#[test]
fn audit_table_branch_ddl_prefix_goes_first() {
    let reader = Arc::new(QueueReader::new(vec![page(
        &["id"],
        vec![vec![json!(1)]],
        None,
    )]));
    let exp = exporter(reader, None);
    // Data-and-structure in CSV plans a DDL file plus a data file,
    // which needs a zip and is refused before emission — so the data
    // plan is hand-built with its DDL prefix, the way the SQL-format
    // planner would carry it.
    let table = table_input("t");
    let req = request(
        vec![table.clone()],
        ExportMode::DataOnly,
        ExportDataFormat::Csv,
    );
    let planned = hand_planned(
        req,
        vec![ExportFilePlan::Data {
            filename: "t.csv".to_owned(),
            table,
            format: ExportDataFormat::Csv,
            ddl_prefix: Some("CREATE TABLE t (id INTEGER);".to_owned()),
        }],
    );
    let emitted = emit(&exp, &planned, 0).expect("emit");
    assert_eq!(text_of(&emitted), "CREATE TABLE t (id INTEGER);\n\nid\n1\n");
}

#[test]
fn audit_table_branch_empty_first_page_is_just_the_header() {
    let reader = Arc::new(QueueReader::new(vec![page(&["id", "v"], vec![], None)]));
    let exp = exporter(reader, None);
    let planned = plan(
        &exp,
        request(
            vec![table_input("t")],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
        ),
    )
    .expect("plan");
    assert_eq!(text_of(&emit(&exp, &planned, 0).expect("emit")), "id,v\n");
}

#[test]
fn audit_table_branch_reader_failure_propagates() {
    let exp = exporter(Arc::new(QueueReader::failing("db is down")), None);
    let planned = plan(
        &exp,
        request(
            vec![table_input("t")],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
        ),
    )
    .expect("plan");
    let err = emit(&exp, &planned, 0).expect_err("reader failure must fail");
    assert_eq!(
        err,
        BlobTablesExportError::ReadFailed("db is down".to_owned())
    );
    assert_eq!(err.to_string(), "db is down");
}

#[test]
fn audit_emit_data_plan_refuses_a_ddl_plan() {
    let exp = exporter(Arc::new(QueueReader::new(vec![])), None);
    let mut table = table_input("t");
    table.ddl = Some("CREATE TABLE t (id INTEGER);".to_owned());
    let planned = plan(
        &exp,
        request(
            vec![table],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
        ),
    )
    .expect("plan");
    let err = emit(&exp, &planned, 0).expect_err("ddl plan must fail");
    assert_eq!(err, BlobTablesExportError::NotDataPlan);
}

// ── merged delivery ────────────────────────────────────────────────────────

#[test]
fn audit_single_data_plan_delivers_with_its_own_mime() {
    // React: a single data plan delivers as-is with its own MIME type.
    let capture = Arc::new(CaptureSink {
        downloads: Mutex::new(vec![]),
    });
    let exp = exporter(
        Arc::new(QueueReader::new(vec![])),
        Some(CaptureSink::sink(&capture)),
    );
    let outcome = export(
        &exp,
        request(
            vec![provided_table("t", &["id"], vec![vec![json!(1)]])],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
        ),
    )
    .expect("export");
    assert_eq!(outcome, ExportTablesOutcome::Saved(1));
    let downloads = capture.downloads();
    assert_eq!(downloads.len(), 1);
    assert_eq!(downloads[0].mime_type, "text/csv;charset=utf-8");
    assert_eq!(
        downloads[0].contents,
        DeliveredContent::Text("id\n1\n".to_owned())
    );
}

#[test]
fn audit_single_xlsx_plan_delivers_bytes_with_xlsx_mime() {
    let capture = Arc::new(CaptureSink {
        downloads: Mutex::new(vec![]),
    });
    let exp = exporter(
        Arc::new(QueueReader::new(vec![])),
        Some(CaptureSink::sink(&capture)),
    );
    let outcome = export(
        &exp,
        request(
            vec![provided_table("t", &["id"], vec![vec![json!(1)]])],
            ExportMode::DataOnly,
            ExportDataFormat::Xlsx,
        ),
    )
    .expect("export");
    assert_eq!(outcome, ExportTablesOutcome::Saved(1));
    let downloads = capture.downloads();
    assert_eq!(downloads.len(), 1);
    assert_eq!(
        downloads[0].mime_type,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet;charset=utf-8"
    );
    match &downloads[0].contents {
        DeliveredContent::Bytes(bytes) => assert_eq!(&bytes[..4], b"PK\x03\x04"),
        other => panic!("expected bytes, got {other:?}"),
    }
}

#[test]
fn audit_single_ddl_plan_merges_without_separator_and_text_csv_mime() {
    // S1 tester finding (c): React's merge path calls `deliver(chunks,
    // fileName)` with no mime, so `downloadBlob`'s default `text/csv`
    // applies; the Rust merge passes `"text/csv"` explicitly. A single
    // DDL plan takes the merge path (it is not a data plan), with no
    // separator and no trailing blank line.
    let capture = Arc::new(CaptureSink {
        downloads: Mutex::new(vec![]),
    });
    let exp = exporter(
        Arc::new(QueueReader::new(vec![])),
        Some(CaptureSink::sink(&capture)),
    );
    let mut table = table_input("t");
    table.ddl = Some("CREATE TABLE t (id INTEGER);".to_owned());
    let outcome = export(
        &exp,
        request(
            vec![table],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
        ),
    )
    .expect("export");
    assert_eq!(outcome, ExportTablesOutcome::Saved(1));
    let downloads = capture.downloads();
    assert_eq!(downloads.len(), 1);
    assert_eq!(downloads[0].mime_type, "text/csv;charset=utf-8");
    assert_eq!(
        downloads[0].contents,
        DeliveredContent::Text("CREATE TABLE t (id INTEGER);".to_owned())
    );
}

#[test]
fn audit_multi_ddl_merge_uses_separators_and_blank_lines() {
    // React: several plans are concatenated with
    // `-- ===== {filename} =====` separators, `\n\n` after each plan.
    let capture = Arc::new(CaptureSink {
        downloads: Mutex::new(vec![]),
    });
    let exp = exporter(
        Arc::new(QueueReader::new(vec![])),
        Some(CaptureSink::sink(&capture)),
    );
    let mut one = table_input("one");
    one.ddl = Some("CREATE TABLE one (id INTEGER);".to_owned());
    let mut two = table_input("two");
    two.ddl = Some("CREATE TABLE two (id INTEGER);".to_owned());
    let outcome = export(
        &exp,
        request(
            vec![one, two],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
        ),
    )
    .expect("export");
    assert_eq!(outcome, ExportTablesOutcome::Saved(2));
    let downloads = capture.downloads();
    assert_eq!(downloads.len(), 1);
    assert_eq!(downloads[0].mime_type, "text/csv;charset=utf-8");
    assert_eq!(
        downloads[0].contents,
        DeliveredContent::Text(
            "-- ===== one.sql =====\n\nCREATE TABLE one (id INTEGER);\n\n\
             -- ===== two.sql =====\n\nCREATE TABLE two (id INTEGER);\n\n"
                .to_owned()
        )
    );
}

#[test]
fn audit_multi_sql_data_plans_merge_with_ddl_prefixes() {
    // Two tables, data-and-structure in SQL format: each table plans
    // one data file with its DDL as prefix (all `.sql`, so no zip).
    // The merge wraps each with its separator and a trailing blank line.
    let capture = Arc::new(CaptureSink {
        downloads: Mutex::new(vec![]),
    });
    let exp = exporter(
        Arc::new(QueueReader::new(vec![])),
        Some(CaptureSink::sink(&capture)),
    );
    let mut one = provided_table("one", &["id"], vec![vec![json!(1)]]);
    one.ddl = Some("CREATE TABLE one (id INTEGER);".to_owned());
    let mut two = provided_table("two", &["id"], vec![vec![json!(2)]]);
    two.ddl = Some("CREATE TABLE two (id INTEGER);".to_owned());
    let outcome = export(
        &exp,
        request(
            vec![one, two],
            ExportMode::DataAndStructure,
            ExportDataFormat::SqlInsert,
        ),
    )
    .expect("export");
    assert_eq!(outcome, ExportTablesOutcome::Saved(2));
    let downloads = capture.downloads();
    assert_eq!(downloads.len(), 1);
    assert_eq!(
        downloads[0].contents,
        DeliveredContent::Text(
            "-- ===== one.sql =====\n\nCREATE TABLE one (id INTEGER);\n\n\
             BEGIN;\nINSERT INTO \"one\" (\"id\") VALUES\n  (1);\nCOMMIT;\n\n\
             -- ===== two.sql =====\n\nCREATE TABLE two (id INTEGER);\n\n\
             BEGIN;\nINSERT INTO \"two\" (\"id\") VALUES\n  (2);\nCOMMIT;\n\n"
                .to_owned()
        )
    );
}

#[test]
fn audit_binary_plan_in_merge_is_refused_and_delivers_nothing() {
    // React: a binary plan in the merge throws — it cannot ride a
    // single text download. Only reachable with hand-built plans: the
    // planner refuses the zip a multi-file non-`.sql` plan would need.
    let capture = Arc::new(CaptureSink {
        downloads: Mutex::new(vec![]),
    });
    let exp = exporter(
        Arc::new(QueueReader::new(vec![])),
        Some(CaptureSink::sink(&capture)),
    );
    let table = provided_table("t", &["id"], vec![vec![json!(1)]]);
    let req = request(
        vec![table.clone()],
        ExportMode::DataOnly,
        ExportDataFormat::Xlsx,
    );
    let planned = hand_planned(
        req,
        vec![
            ExportFilePlan::Ddl {
                filename: "t.sql".to_owned(),
                text: "CREATE TABLE t (id INTEGER);".to_owned(),
            },
            ExportFilePlan::Data {
                filename: "t.xlsx".to_owned(),
                table,
                format: ExportDataFormat::Xlsx,
                ddl_prefix: None,
            },
        ],
    );
    let err = futures::executor::block_on(exp.emit_planned(&planned))
        .expect_err("binary merge must fail");
    assert_eq!(err, BlobTablesExportError::BinaryCannotMerge);
    assert_eq!(
        err.to_string(),
        "this export produces a binary file that cannot be merged into a single download"
    );
    assert!(
        capture.downloads().is_empty(),
        "nothing delivered on failure"
    );
}

#[test]
fn audit_no_deliver_sink_fails_loudly() {
    // There is no Rune default: React's default *is* the real Blob
    // download, which cannot run here, so an exporter built without a
    // sink fails instead of silently dropping the file.
    let exp = exporter(Arc::new(QueueReader::new(vec![])), None);
    let err = export(
        &exp,
        request(
            vec![provided_table("t", &["id"], vec![vec![json!(1)]])],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
        ),
    )
    .expect_err("no sink must fail");
    assert_eq!(err, BlobTablesExportError::NoDownloadSink);
}

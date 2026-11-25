//! T043-S2: `platform/blobTablesExporter.ts` slice 2 — data emission
//! (the `provided` branch through the formatter mirror, the `table`
//! branch through the page loop) and merged delivery (single-plan
//! direct, multi-plan `.sql` merge, binary-in-merge refusal, `Saved`).
//!
//! Every expectation below is re-derived from the React original
//! (`packages/frontend/src/platform/blobTablesExporter.ts` and
//! `packages/frontend/src/lib/exportFormat.ts`), not from the coder's
//! implementation notes.

use std::collections::VecDeque;
use std::io::Read;
use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};
use serde_json::{json, Value};

use datazen_ui::platform::blob_tables_exporter::{
    create_blob_tables_exporter, BlobDownload, BlobTablesExportError, BlobTablesExporter,
    BlobTablesExporterOptions, DeliverFn, DeliveredContent, EmittedDataPlan,
    ExportTablesOutcome, PlannedExport, TablePage, TablePageReader, TablePageRequestContext,
};
use datazen_ui::platform::export_format::{
    format_rows_as_text, format_rows_as_xlsx, ExportFormatError,
};
use datazen_ui::platform::export_plan::{
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
struct ScriptedReader {
    pages: Mutex<VecDeque<TablePage>>,
    calls: Mutex<Vec<(String, Option<Value>, Option<String>)>>,
    fail: Mutex<Option<String>>,
}

impl ScriptedReader {
    fn new(pages: Vec<TablePage>) -> Self {
        ScriptedReader {
            pages: Mutex::new(pages.into()),
            calls: Mutex::new(Vec::new()),
            fail: Mutex::new(None),
        }
    }

    fn failing(message: &str) -> Self {
        let reader = ScriptedReader::new(vec![]);
        *reader.fail.lock().expect("reader lock") = Some(message.to_owned());
        reader
    }

    fn calls(&self) -> Vec<(String, Option<Value>, Option<String>)> {
        self.calls.lock().expect("reader lock").clone()
    }
}

impl TablePageReader for ScriptedReader {
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
        let page = self.pages.lock().expect("reader lock").pop_front().unwrap_or(TablePage {
            columns: vec![],
            rows: vec![],
            next_cursor: None,
        });
        Box::pin(async move { Ok(page) })
    }
}

/// A deliver sink that records every download instead of downloading.
struct CaptureDeliver {
    downloads: Mutex<Vec<BlobDownload>>,
}

impl CaptureDeliver {
    fn sink(capture: &Arc<CaptureDeliver>) -> DeliverFn {
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

fn exporter(
    reader: Arc<dyn TablePageReader>,
    deliver: Option<DeliverFn>,
    pin_now: bool,
) -> BlobTablesExporter {
    let mut options = BlobTablesExporterOptions {
        deliver,
        ..Default::default()
    };
    if pin_now {
        options.now = Some(Arc::new(pinned_now));
    }
    create_blob_tables_exporter(reader, options)
}

fn table(name: &str) -> ExportTableInput {
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

// ── provided branch: CSV ────────────────────────────────────────────────────

#[test]
fn provided_csv_emits_header_plus_trailing_newline_lines() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let planned = plan(
        &exp,
        request(
            vec![provided_table(
                "p",
                &["id", "note"],
                vec![
                    vec![json!(1), json!("hello")],
                    vec![json!(2), json!("a,b")],
                    vec![json!(3), json!("=1+1")],
                    vec![json!(4), Value::Null],
                ],
            )],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided csv plans");
    let emitted = emit(&exp, &planned, 0).expect("provided csv emits");
    assert_eq!(emitted.mime_type, "text/csv");
    assert_eq!(
        text_of(&emitted),
        "id,note\n1,hello\n2,\"a,b\"\n3,\"=1+1\"\n4,\n"
    );
}

#[test]
fn provided_csv_with_no_rows_is_just_the_header() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let planned = plan(
        &exp,
        request(
            vec![provided_table("p", &["id", "note"], vec![])],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided csv plans");
    let emitted = emit(&exp, &planned, 0).expect("provided csv emits");
    assert_eq!(text_of(&emitted), "id,note\n");
}

#[test]
fn provided_csv_renders_objects_as_json_and_floats_like_js_except_integral() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let planned = plan(
        &exp,
        request(
            vec![provided_table(
                "p",
                &["v"],
                vec![vec![json!({"a": 1})], vec![json!(1.5)], vec![json!(1e21)]],
            )],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided csv plans");
    let emitted = emit(&exp, &planned, 0).expect("provided csv emits");
    // The object serialises to JSON (quoted, inner quotes doubled);
    // 1e21 renders "1e+21" exactly like JS; 1.5 is unremarkable.
    assert_eq!(text_of(&emitted), "v\n\"{\"\"a\"\":1}\"\n1.5\n1e+21\n");
}

// ── provided branch: other text formats ────────────────────────────────────

#[test]
fn provided_tsv_replaces_tabs_and_line_breaks_with_spaces() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let planned = plan(
        &exp,
        request(
            vec![provided_table(
                "p",
                &["a", "b"],
                vec![
                    vec![json!(1), json!("x\ty")],
                    vec![Value::Null, json!("p\nq")],
                    vec![json!({"k": 1}), json!("r\rs")],
                ],
            )],
            ExportMode::DataOnly,
            ExportDataFormat::Tsv,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided tsv plans");
    let emitted = emit(&exp, &planned, 0).expect("provided tsv emits");
    assert_eq!(emitted.mime_type, "text/tab-separated-values");
    // `\r` is removed outright (React replaces it with `''`), while
    // `\t` and `\n` become spaces.
    assert_eq!(text_of(&emitted), "a\tb\n1\tx y\n\tp q\n{\"k\":1}\trs");
}

#[test]
fn provided_json_keeps_column_order_and_pretty_prints() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let planned = plan(
        &exp,
        request(
            vec![provided_table(
                "p",
                &["b", "a"],
                vec![vec![json!(1), json!("x")], vec![json!(2), json!("y")]],
            )],
            ExportMode::DataOnly,
            ExportDataFormat::Json,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided json plans");
    let emitted = emit(&exp, &planned, 0).expect("provided json emits");
    assert_eq!(emitted.mime_type, "application/json");
    // Keys follow the column order ("b" before "a") — a serde_json::Map
    // would sort them, so the records are built by hand.
    assert_eq!(
        text_of(&emitted),
        "[\n  {\n    \"b\": 1,\n    \"a\": \"x\"\n  },\n  {\n    \"b\": 2,\n    \"a\": \"y\"\n  }\n]"
    );
}

#[test]
fn provided_json_covers_null_bool_and_empty_containers() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let planned = plan(
        &exp,
        request(
            vec![provided_table(
                "p",
                &["a"],
                vec![
                    vec![Value::Null],
                    vec![json!(true)],
                    vec![json!(false)],
                    vec![json!([])],
                    vec![json!({})],
                    vec![json!({"k1": 1, "k2": 2})],
                ],
            )],
            ExportMode::DataOnly,
            ExportDataFormat::Json,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided json plans");
    let emitted = emit(&exp, &planned, 0).expect("provided json emits");
    assert_eq!(
        text_of(&emitted),
        "[\n  {\n    \"a\": null\n  },\n  {\n    \"a\": true\n  },\n  {\n    \"a\": false\n  },\n  {\n    \"a\": []\n  },\n  {\n    \"a\": {}\n  },\n  {\n    \"a\": {\n      \"k1\": 1,\n      \"k2\": 2\n    }\n  }\n]"
    );
}

#[test]
fn provided_json_empty_rows_is_an_empty_array() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let planned = plan(
        &exp,
        request(
            vec![provided_table("p", &["a"], vec![])],
            ExportMode::DataOnly,
            ExportDataFormat::Json,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided json plans");
    let emitted = emit(&exp, &planned, 0).expect("provided json emits");
    assert_eq!(text_of(&emitted), "[]");
}

#[test]
fn provided_json_nests_arrays_and_objects() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let planned = plan(
        &exp,
        request(
            vec![provided_table(
                "p",
                &["v"],
                vec![vec![json!({"a": [1, {"b": "x"}]})]],
            )],
            ExportMode::DataOnly,
            ExportDataFormat::Json,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided json plans");
    let emitted = emit(&exp, &planned, 0).expect("provided json emits");
    assert_eq!(
        text_of(&emitted),
        "[\n  {\n    \"v\": {\n      \"a\": [\n        1,\n        {\n          \"b\": \"x\"\n        }\n      ]\n    }\n  }\n]"
    );
}

#[test]
fn format_rows_as_text_ignores_extra_cells_in_an_over_long_row() {
    // The planner rejects an over-long row before emission, so the
    // "extras are ignored" rule is only reachable through the formatter
    // directly.
    let columns = vec![ProvidedColumn {
        name: "v".to_owned(),
    }];
    let result = format_rows_as_text(
        "t",
        &columns,
        &[vec![json!(1), json!("EXTRA")]],
        ExportDataFormat::Json,
        None,
        None,
    )
    .expect("over-long rows format");
    assert_eq!(result.content, "[\n  {\n    \"v\": 1\n  }\n]");
}

#[test]
fn provided_markdown_escapes_pipes_and_line_breaks() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let planned = plan(
        &exp,
        request(
            vec![provided_table(
                "p",
                &["a", "x|y"],
                vec![vec![json!(1), json!("p\nq")], vec![json!("r\\s"), json!("t")]],
            )],
            ExportMode::DataOnly,
            ExportDataFormat::Markdown,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided markdown plans");
    let emitted = emit(&exp, &planned, 0).expect("provided markdown emits");
    assert_eq!(emitted.mime_type, "text/markdown");
    assert_eq!(
        text_of(&emitted),
        "| a | x\\|y |\n| --- | --- |\n| 1 | p<br>q |\n| r\\\\s | t |"
    );
}

// ── provided branch: SQL formats ───────────────────────────────────────────

#[test]
fn provided_sql_insert_quotes_identifiers_and_doubles_quotes() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let planned = plan(
        &exp,
        request(
            vec![provided_table(
                "t",
                &["id", "name"],
                vec![
                    vec![json!(1), json!("o'clock")],
                    vec![json!(2), Value::Null],
                    vec![json!(3), json!(true)],
                    vec![json!(4), json!(false)],
                ],
            )],
            ExportMode::DataOnly,
            ExportDataFormat::SqlInsert,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided sql_insert plans");
    let emitted = emit(&exp, &planned, 0).expect("provided sql_insert emits");
    assert_eq!(emitted.mime_type, "text/sql");
    assert_eq!(
        text_of(&emitted),
        "BEGIN;\nINSERT INTO \"t\" (\"id\", \"name\") VALUES\n  (1, 'o''clock'),\n  (2, NULL),\n  (3, TRUE),\n  (4, FALSE);\nCOMMIT;"
    );
}

#[test]
fn provided_sql_insert_empty_rows_is_begin_then_commit() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let planned = plan(
        &exp,
        request(
            vec![provided_table("t", &["id"], vec![])],
            ExportMode::DataOnly,
            ExportDataFormat::SqlInsert,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided sql_insert plans");
    let emitted = emit(&exp, &planned, 0).expect("provided sql_insert emits");
    assert_eq!(text_of(&emitted), "BEGIN;\nCOMMIT;");
}

#[test]
fn provided_sql_insert_batches_every_500_rows() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let rows: Vec<Vec<Value>> = (1..=501).map(|i| vec![json!(i)]).collect();
    let planned = plan(
        &exp,
        request(
            vec![provided_table("t", &["id"], rows)],
            ExportMode::DataOnly,
            ExportDataFormat::SqlInsert,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided sql_insert plans");
    let emitted = emit(&exp, &planned, 0).expect("provided sql_insert emits");
    let content = text_of(&emitted);
    assert_eq!(content.matches("INSERT INTO \"t\" (\"id\") VALUES").count(), 2);
    assert!(content.starts_with("BEGIN;\nINSERT INTO"));
    assert!(content.ends_with("  (501);\nCOMMIT;"));
    // Row 500 closes the first batch with `;`, row 499 is mid-batch.
    assert_eq!(content.matches("  (500);").count(), 1);
    assert_eq!(content.matches("  (499),").count(), 1);
}

#[test]
fn provided_sql_insert_mysql_quotes_with_backticks_and_backslashes() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let mut req = request(
        vec![provided_table(
            "t",
            &["id", "name"],
            vec![
                vec![json!(1), json!("a\\b\"c'd")],
                vec![Value::Null, json!(false)],
                vec![json!(2), json!(true)],
            ],
        )],
        ExportMode::DataOnly,
        ExportDataFormat::SqlInsert,
        ExportOutputMode::Single,
    );
    req.database_type = Some("mysql".to_owned());
    let planned = plan(&exp, req).expect("provided sql_insert plans");
    let emitted = emit(&exp, &planned, 0).expect("provided sql_insert emits");
    assert_eq!(
        text_of(&emitted),
        "BEGIN;\nINSERT INTO `t` (`id`, `name`) VALUES\n  (1, 'a\\\\b\\\"c''d'),\n  (NULL, FALSE),\n  (2, TRUE);\nCOMMIT;"
    );
}

#[test]
fn provided_sql_insert_sqlserver_brackets_and_begins_a_transaction() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let mut req = request(
        vec![provided_table(
            "t]",
            &["a]b"],
            vec![vec![json!(1)]],
        )],
        ExportMode::DataOnly,
        ExportDataFormat::SqlInsert,
        ExportOutputMode::Single,
    );
    req.database_type = Some("sqlserver".to_owned());
    let planned = plan(&exp, req).expect("provided sql_insert plans");
    let emitted = emit(&exp, &planned, 0).expect("provided sql_insert emits");
    assert_eq!(
        text_of(&emitted),
        "BEGIN TRANSACTION;\nINSERT INTO [t]]] ([a]]b]) VALUES\n  (1);\nCOMMIT;"
    );
}

#[test]
fn provided_sql_insert_no_quote_dialect_leaves_identifiers_bare() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let mut req = request(
        vec![provided_table("t", &["id"], vec![vec![json!(1)]])],
        ExportMode::DataOnly,
        ExportDataFormat::SqlInsert,
        ExportOutputMode::Single,
    );
    req.database_type = Some("mongodb".to_owned());
    let planned = plan(&exp, req).expect("provided sql_insert plans");
    let emitted = emit(&exp, &planned, 0).expect("provided sql_insert emits");
    assert_eq!(
        text_of(&emitted),
        "BEGIN;\nINSERT INTO t (id) VALUES\n  (1);\nCOMMIT;"
    );
}

#[test]
fn provided_sql_update_sets_non_key_columns_where_the_key_matches() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let mut input = provided_table(
        "t",
        &["id", "name"],
        vec![vec![json!(1), json!("x")], vec![json!(2), json!({"k": 1})]],
    );
    input.primary_key = Some("id".to_owned());
    let planned = plan(
        &exp,
        request(
            vec![input],
            ExportMode::DataOnly,
            ExportDataFormat::SqlUpdate,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided sql_update plans");
    let emitted = emit(&exp, &planned, 0).expect("provided sql_update emits");
    assert_eq!(emitted.mime_type, "text/sql");
    assert_eq!(
        text_of(&emitted),
        "BEGIN;\nUPDATE \"t\" SET \"name\" = 'x' WHERE \"id\" = 1;\nUPDATE \"t\" SET \"name\" = '{\"k\":1}' WHERE \"id\" = 2;\nCOMMIT;"
    );
}

#[test]
fn provided_sql_update_falls_back_to_the_first_column_as_key() {
    // No primaryKey, and ProvidedColumn carries no isPrimaryKey — the
    // first column is the key.
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let planned = plan(
        &exp,
        request(
            vec![provided_table(
                "t",
                &["id", "name"],
                vec![vec![json!(1), json!("x")]],
            )],
            ExportMode::DataOnly,
            ExportDataFormat::SqlUpdate,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided sql_update plans");
    let emitted = emit(&exp, &planned, 0).expect("provided sql_update emits");
    assert_eq!(
        text_of(&emitted),
        "BEGIN;\nUPDATE \"t\" SET \"name\" = 'x' WHERE \"id\" = 1;\nCOMMIT;"
    );
}

#[test]
fn provided_sql_update_unknown_primary_key_falls_back_to_the_first_column() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let mut input = provided_table("t", &["id", "name"], vec![vec![json!(1), json!("x")]]);
    input.primary_key = Some("missing".to_owned());
    let planned = plan(
        &exp,
        request(
            vec![input],
            ExportMode::DataOnly,
            ExportDataFormat::SqlUpdate,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided sql_update plans");
    let emitted = emit(&exp, &planned, 0).expect("provided sql_update emits");
    assert_eq!(
        text_of(&emitted),
        "BEGIN;\nUPDATE \"t\" SET \"name\" = 'x' WHERE \"id\" = 1;\nCOMMIT;"
    );
}

#[test]
fn provided_sql_update_single_column_table_has_nothing_to_set() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let planned = plan(
        &exp,
        request(
            vec![provided_table("t", &["id"], vec![vec![json!(1)]])],
            ExportMode::DataOnly,
            ExportDataFormat::SqlUpdate,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided sql_update plans");
    let err = emit(&exp, &planned, 0).expect_err("single-column sql_update must fail");
    assert_eq!(
        err,
        BlobTablesExportError::Format(ExportFormatError::SqlUpdateNeedsSetColumn)
    );
    assert_eq!(
        err.to_string(),
        "sql_update export needs at least one non-key column to SET"
    );
}

#[test]
fn provided_sql_data_plan_with_ddl_prefix_prefixes_with_blank_line() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let mut input = provided_table("p", &["a"], vec![vec![json!(1)]]);
    input.ddl = Some("CREATE TABLE p (a INT)".to_owned());
    let planned = plan(
        &exp,
        request(
            vec![input],
            ExportMode::DataAndStructure,
            ExportDataFormat::SqlInsert,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided sql_insert with ddl plans");
    assert!(matches!(
        planned.plans[0],
        ExportFilePlan::Data {
            ddl_prefix: Some(_),
            ..
        }
    ));
    let emitted = emit(&exp, &planned, 0).expect("provided sql_insert emits");
    assert_eq!(
        text_of(&emitted),
        "CREATE TABLE p (a INT)\n\nBEGIN;\nINSERT INTO \"p\" (\"a\") VALUES\n  (1);\nCOMMIT;"
    );
}

// ── provided branch: parquet refusal + xlsx ────────────────────────────────

#[test]
fn provided_parquet_is_refused_loudly() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let planned = plan(
        &exp,
        request(
            vec![provided_table("p", &["a"], vec![vec![json!(1)]])],
            ExportMode::DataOnly,
            ExportDataFormat::Parquet,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided parquet plans in S1");
    let err = emit(&exp, &planned, 0).expect_err("provided parquet must fail in S2");
    assert_eq!(err, BlobTablesExportError::ProvidedParquetUnsupported);
    assert_eq!(err.to_string(), "provided-data export cannot format 'parquet'");
}

#[test]
fn provided_xlsx_with_ddl_prefix_is_refused() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    // data_and_structure + xlsx splits DDL and data, so the data plan
    // carries no prefix — build the prefixed plan by hand instead.
    let planned = PlannedExport {
        request: request(
            vec![provided_table("p", &["a"], vec![vec![json!(1)]])],
            ExportMode::DataOnly,
            ExportDataFormat::Xlsx,
            ExportOutputMode::Single,
        ),
        plans: vec![ExportFilePlan::Data {
            filename: "p.xlsx".to_owned(),
            table: provided_table("p", &["a"], vec![vec![json!(1)]]),
            format: ExportDataFormat::Xlsx,
            ddl_prefix: Some("CREATE TABLE p (a INT)".to_owned()),
        }],
        file_name: "export_data_2026-11-19-10-23-14.xlsx".to_owned(),
        request_context: TablePageRequestContext {
            db_session_id: "sess-1".to_owned(),
            database: Some("db".to_owned()),
            schema: Some("public".to_owned()),
            driver_type: Some("postgres".to_owned()),
        },
    };
    let err = futures::executor::block_on(exp.emit_data_plan(&planned, &planned.plans[0]))
        .expect_err("xlsx with a ddl prefix must fail");
    assert_eq!(err, BlobTablesExportError::DdlPrefixOnXlsx);
    assert_eq!(err.to_string(), "DDL cannot be prefixed to an XLSX export");
}

#[test]
fn provided_xlsx_emits_a_workbook_with_the_same_cells() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let planned = plan(
        &exp,
        request(
            vec![provided_table(
                "p",
                &["id", "name", "flag", "note"],
                vec![
                    vec![json!(1), json!("x"), json!(true), Value::Null],
                    vec![json!(2.5), json!("y"), json!(false), json!({"k": 1})],
                ],
            )],
            ExportMode::DataOnly,
            ExportDataFormat::Xlsx,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided xlsx plans");
    let emitted = emit(&exp, &planned, 0).expect("provided xlsx emits");
    assert_eq!(
        emitted.mime_type,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    );
    let bytes = bytes_of(&emitted);
    // A zip container (PK magic), read back structurally the way React's
    // exportFormat test does: same sheet name, same header and cell text.
    assert!(bytes.starts_with(b"PK\x03\x04"));
    let workbook = xlsx_entry(bytes, "xl/workbook.xml");
    assert!(workbook.contains("Sheet1"));
    let shared = xlsx_entry(bytes, "xl/sharedStrings.xml");
    for text in ["id", "name", "flag", "note", "x", "y", "{\"k\":1}"] {
        assert!(shared.contains(text), "shared strings lack {text:?}");
    }
    let sheet = xlsx_entry(bytes, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("<v>1</v>"));
    assert!(sheet.contains("<v>2.5</v>"));
}

#[test]
fn provided_xlsx_overlong_cell_fails_with_the_actionable_limit() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let long = "x".repeat(32_768);
    let planned = plan(
        &exp,
        request(
            vec![provided_table("p", &["a"], vec![vec![json!(long)]])],
            ExportMode::DataOnly,
            ExportDataFormat::Xlsx,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided xlsx plans");
    let err = emit(&exp, &planned, 0).expect_err("overlong xlsx cell must fail");
    assert_eq!(
        err,
        BlobTablesExportError::Format(ExportFormatError::XlsxFailed(
            "a cell exceeds Excel's 32767-character limit; shorten the value or export as CSV"
                .to_owned()
        ))
    );
}

#[test]
fn provided_xlsx_too_many_columns_fails_with_the_worksheet_limits() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let names: Vec<String> = (0..16_385).map(|i| format!("c{i}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let planned = plan(
        &exp,
        request(
            vec![provided_table("p", &refs, vec![])],
            ExportMode::DataOnly,
            ExportDataFormat::Xlsx,
            ExportOutputMode::Single,
        ),
    )
    .expect("provided xlsx plans");
    let err = emit(&exp, &planned, 0).expect_err("too many xlsx columns must fail");
    assert_eq!(
        err,
        BlobTablesExportError::Format(ExportFormatError::XlsxFailed(
            "export exceeds Excel's worksheet limits (1048576 rows / 16384 columns)".to_owned()
        ))
    );
}

// ── formatter mirror, called directly ───────────────────────────────────────

#[test]
fn format_rows_as_text_csv_branch_joins_without_trailing_newline() {
    let columns = vec![ProvidedColumn {
        name: "a".to_owned(),
    }];
    let result = format_rows_as_text(
        "t",
        &columns,
        &[vec![json!(1)]],
        ExportDataFormat::Csv,
        None,
        None,
    )
    .expect("csv formats as text");
    assert_eq!(result.content, "a\n1");
    assert_eq!(result.extension, "csv");
    assert_eq!(result.mime_type, "text/csv");
}

#[test]
fn format_rows_as_text_refuses_binary_formats() {
    let columns = vec![ProvidedColumn {
        name: "a".to_owned(),
    }];
    for format in [ExportDataFormat::Xlsx, ExportDataFormat::Parquet] {
        let err = format_rows_as_text("t", &columns, &[], format, None, None)
            .expect_err("binary formats cannot format as text");
        assert_eq!(
            err,
            ExportFormatError::CannotFormatAsText { format }
        );
        assert_eq!(
            err.to_string(),
            format!("formatRowsAsText cannot format '{format}' as text")
        );
    }
}

#[test]
fn format_rows_as_text_and_xlsx_refuse_empty_columns() {
    let err = format_rows_as_text("t", &[], &[], ExportDataFormat::Json, None, None)
        .expect_err("no columns must fail");
    assert_eq!(err, ExportFormatError::NoColumns);
    assert_eq!(err.to_string(), "export needs at least one column");
    let err = format_rows_as_xlsx(&[], &[]).expect_err("no columns must fail");
    assert_eq!(err, ExportFormatError::NoColumns);
}

// ── table branch: the page loop ─────────────────────────────────────────────

#[test]
fn table_branch_pages_until_the_cursor_is_none() {
    let reader = Arc::new(ScriptedReader::new(vec![
        page(
            &["id", "name"],
            vec![vec![json!(1), json!("a")]],
            Some(json!("c1")),
        ),
        page(
            &["ignored", "header"],
            vec![vec![json!(2), json!("b")], vec![json!(3), json!("c,d")]],
            None,
        ),
    ]));
    let exp = exporter(reader.clone(), None, false);
    let planned = plan(
        &exp,
        request(
            vec![table("t")],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("table csv plans");
    let emitted = emit(&exp, &planned, 0).expect("table csv emits");
    assert_eq!(emitted.mime_type, "text/csv");
    // Only the first page contributes the header; every page contributes
    // its rows.
    assert_eq!(text_of(&emitted), "id,name\n1,a\n2,b\n3,\"c,d\"\n");
    assert_eq!(
        reader.calls(),
        vec![
            ("t".to_owned(), None, Some("public".to_owned())),
            ("t".to_owned(), Some(json!("c1")), Some("public".to_owned())),
        ]
    );
}

#[test]
fn table_branch_null_cursor_counts_as_exhausted() {
    // (d): React breaks on `null` as well as `undefined`; a reader that
    // answers `Some(Value::Null)` must stop the loop after one page.
    let reader = Arc::new(ScriptedReader::new(vec![page(
        &["id"],
        vec![vec![json!(1)]],
        Some(Value::Null),
    )]));
    let exp = exporter(reader.clone(), None, false);
    let planned = plan(
        &exp,
        request(
            vec![table("t")],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("table csv plans");
    let emitted = emit(&exp, &planned, 0).expect("table csv emits");
    assert_eq!(text_of(&emitted), "id\n1\n");
    assert_eq!(reader.calls().len(), 1);
}

#[test]
fn table_branch_overrides_the_request_schema_with_the_table_schema() {
    let reader = Arc::new(ScriptedReader::new(vec![page(
        &["id"],
        vec![vec![json!(1)]],
        None,
    )]));
    let exp = exporter(reader.clone(), None, false);
    let mut input = table("t");
    input.schema = Some("tenant".to_owned());
    let planned = plan(
        &exp,
        request(
            vec![input],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("table csv plans");
    emit(&exp, &planned, 0).expect("table csv emits");
    assert_eq!(
        reader.calls(),
        vec![("t".to_owned(), None, Some("tenant".to_owned()))]
    );
}

#[test]
fn table_branch_with_ddl_prefix_prefixes_with_blank_line() {
    let reader = Arc::new(ScriptedReader::new(vec![page(
        &["id"],
        vec![vec![json!(1)]],
        None,
    )]));
    let exp = exporter(reader.clone(), None, false);
    // A table source with a non-CSV format is refused by the CSV-only
    // gate, so the table branch never actually carries a prefix through
    // `plan_export` — build the prefixed plan by hand, as the React
    // `emitDataPlan` contract still covers it.
    let prefixed = PlannedExport {
        request: request(
            vec![table("t")],
            ExportMode::DataAndStructure,
            ExportDataFormat::SqlInsert,
            ExportOutputMode::Single,
        ),
        plans: vec![ExportFilePlan::Data {
            filename: "t.sql".to_owned(),
            table: table("t"),
            format: ExportDataFormat::SqlInsert,
            ddl_prefix: Some("CREATE TABLE t (id INT)".to_owned()),
        }],
        file_name: "export_full_2026-11-19-10-23-14.sql".to_owned(),
        request_context: TablePageRequestContext {
            db_session_id: "sess-1".to_owned(),
            database: Some("db".to_owned()),
            schema: Some("public".to_owned()),
            driver_type: Some("postgres".to_owned()),
        },
    };
    let emitted = futures::executor::block_on(exp.emit_data_plan(&prefixed, &prefixed.plans[0]))
        .expect("table branch emits with a prefix");
    assert_eq!(
        text_of(&emitted),
        "CREATE TABLE t (id INT)\n\nid\n1\n"
    );
}

#[test]
fn table_branch_reader_failure_propagates() {
    let exp = exporter(Arc::new(ScriptedReader::failing("boom")), None, false);
    let planned = plan(
        &exp,
        request(
            vec![table("t")],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("table csv plans");
    let err = emit(&exp, &planned, 0).expect_err("reader failure must fail");
    assert_eq!(err, BlobTablesExportError::ReadFailed("boom".to_owned()));
    assert_eq!(err.to_string(), "boom");
}

#[test]
fn table_branch_empty_first_page_is_just_the_header() {
    let reader = Arc::new(ScriptedReader::new(vec![page(&["id"], vec![], None)]));
    let exp = exporter(reader.clone(), None, false);
    let planned = plan(
        &exp,
        request(
            vec![table("t")],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("table csv plans");
    let emitted = emit(&exp, &planned, 0).expect("table csv emits");
    assert_eq!(text_of(&emitted), "id\n");
}

#[test]
fn emit_data_plan_refuses_a_ddl_plan() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let planned = plan(
        &exp,
        request(
            vec![table("t")],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("structure plans");
    let err = emit(&exp, &planned, 0).expect_err("a ddl plan cannot be emitted as data");
    assert_eq!(err, BlobTablesExportError::NotDataPlan);
    assert_eq!(
        err.to_string(),
        "emit_data_plan called with a DDL plan; only data plans can be emitted"
    );
}

// ── merged delivery ─────────────────────────────────────────────────────────

#[test]
fn export_tables_single_provided_csv_delivers_with_its_own_mime() {
    let capture = Arc::new(CaptureDeliver {
        downloads: Mutex::new(Vec::new()),
    });
    let exp = exporter(
        Arc::new(ScriptedReader::new(vec![])),
        Some(CaptureDeliver::sink(&capture)),
        true,
    );
    let outcome = export(
        &exp,
        request(
            vec![provided_table("p", &["a"], vec![vec![json!(1)]])],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("export delivers");
    assert_eq!(outcome, ExportTablesOutcome::Saved(1));
    assert_eq!(
        capture.downloads(),
        vec![BlobDownload {
            contents: DeliveredContent::Text("a\n1\n".to_owned()),
            file_name: "export_data_2026-11-19-10-23-14.csv".to_owned(),
            mime_type: "text/csv;charset=utf-8".to_owned(),
        }]
    );
}

#[test]
fn export_tables_single_provided_xlsx_delivers_bytes() {
    let capture = Arc::new(CaptureDeliver {
        downloads: Mutex::new(Vec::new()),
    });
    let exp = exporter(
        Arc::new(ScriptedReader::new(vec![])),
        Some(CaptureDeliver::sink(&capture)),
        true,
    );
    let outcome = export(
        &exp,
        request(
            vec![provided_table("p", &["a"], vec![vec![json!(1)]])],
            ExportMode::DataOnly,
            ExportDataFormat::Xlsx,
            ExportOutputMode::Single,
        ),
    )
    .expect("export delivers");
    assert_eq!(outcome, ExportTablesOutcome::Saved(1));
    let downloads = capture.downloads();
    assert_eq!(downloads.len(), 1);
    assert_eq!(
        downloads[0].file_name,
        "export_data_2026-11-19-10-23-14.xlsx"
    );
    assert_eq!(
        downloads[0].mime_type,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet;charset=utf-8"
    );
    match &downloads[0].contents {
        DeliveredContent::Bytes(bytes) => assert!(bytes.starts_with(b"PK\x03\x04")),
        DeliveredContent::Text(_) => panic!("xlsx must deliver bytes"),
    }
}

#[test]
fn export_tables_single_ddl_plan_merges_with_text_csv_mime() {
    // (c): React's merge path calls `deliver(chunks, fileName)` with no
    // mime, so `downloadBlob`'s default `text/csv` applies — the merge
    // must pass "text/csv" explicitly.
    let capture = Arc::new(CaptureDeliver {
        downloads: Mutex::new(Vec::new()),
    });
    let exp = exporter(
        Arc::new(ScriptedReader::new(vec![])),
        Some(CaptureDeliver::sink(&capture)),
        true,
    );
    let outcome = export(
        &exp,
        request(
            vec![table("t")],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("export delivers");
    assert_eq!(outcome, ExportTablesOutcome::Saved(1));
    assert_eq!(
        capture.downloads(),
        vec![BlobDownload {
            contents: DeliveredContent::Text("-- DDL unavailable for t".to_owned()),
            file_name: "export_structure_2026-11-19-10-23-14.sql".to_owned(),
            mime_type: "text/csv;charset=utf-8".to_owned(),
        }]
    );
}

#[test]
fn export_tables_multi_ddl_merges_with_separators() {
    let capture = Arc::new(CaptureDeliver {
        downloads: Mutex::new(Vec::new()),
    });
    let exp = exporter(
        Arc::new(ScriptedReader::new(vec![])),
        Some(CaptureDeliver::sink(&capture)),
        true,
    );
    let outcome = export(
        &exp,
        request(
            vec![table("t"), table("u")],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("export delivers");
    assert_eq!(outcome, ExportTablesOutcome::Saved(2));
    assert_eq!(
        capture.downloads(),
        vec![BlobDownload {
            contents: DeliveredContent::Text(
                "-- ===== t.sql =====\n\n-- DDL unavailable for t\n\n-- ===== u.sql =====\n\n-- DDL unavailable for u\n\n"
                    .to_owned()
            ),
            file_name: "export_structure_2026-11-19-10-23-14.sql".to_owned(),
            mime_type: "text/csv;charset=utf-8".to_owned(),
        }]
    );
}

#[test]
fn export_tables_multi_sql_data_plans_merge_with_ddl_prefixes() {
    let capture = Arc::new(CaptureDeliver {
        downloads: Mutex::new(Vec::new()),
    });
    let exp = exporter(
        Arc::new(ScriptedReader::new(vec![])),
        Some(CaptureDeliver::sink(&capture)),
        true,
    );
    let mut p = provided_table("p", &["a"], vec![]);
    p.ddl = Some("CREATE TABLE p (a INT)".to_owned());
    let mut q = provided_table("q", &["a"], vec![]);
    q.ddl = Some("CREATE TABLE q (a INT)".to_owned());
    let outcome = export(
        &exp,
        request(
            vec![p, q],
            ExportMode::DataAndStructure,
            ExportDataFormat::SqlInsert,
            ExportOutputMode::Single,
        ),
    )
    .expect("export delivers");
    assert_eq!(outcome, ExportTablesOutcome::Saved(2));
    assert_eq!(
        capture.downloads(),
        vec![BlobDownload {
            contents: DeliveredContent::Text(
                "-- ===== p.sql =====\n\nCREATE TABLE p (a INT)\n\nBEGIN;\nCOMMIT;\n\n-- ===== q.sql =====\n\nCREATE TABLE q (a INT)\n\nBEGIN;\nCOMMIT;\n\n"
                    .to_owned()
            ),
            file_name: "export_full_2026-11-19-10-23-14.sql".to_owned(),
            mime_type: "text/csv;charset=utf-8".to_owned(),
        }]
    );
}

#[test]
fn export_tables_table_source_reads_pages_and_delivers() {
    let reader = Arc::new(ScriptedReader::new(vec![page(
        &["id"],
        vec![vec![json!(1)], vec![json!(2)]],
        None,
    )]));
    let capture = Arc::new(CaptureDeliver {
        downloads: Mutex::new(Vec::new()),
    });
    let exp = exporter(
        reader,
        Some(CaptureDeliver::sink(&capture)),
        true,
    );
    let outcome = export(
        &exp,
        request(
            vec![table("t")],
            ExportMode::DataOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect("export delivers");
    assert_eq!(outcome, ExportTablesOutcome::Saved(1));
    assert_eq!(
        capture.downloads(),
        vec![BlobDownload {
            contents: DeliveredContent::Text("id\n1\n2\n".to_owned()),
            file_name: "export_data_2026-11-19-10-23-14.csv".to_owned(),
            mime_type: "text/csv;charset=utf-8".to_owned(),
        }]
    );
}

#[test]
fn emit_planned_refuses_a_binary_plan_in_a_merge() {
    let capture = Arc::new(CaptureDeliver {
        downloads: Mutex::new(Vec::new()),
    });
    let exp = exporter(
        Arc::new(ScriptedReader::new(vec![])),
        Some(CaptureDeliver::sink(&capture)),
        true,
    );
    // A binary plan in a merge cannot be produced through `plan_export`
    // (the zip it would need is refused), so the merge is driven with a
    // hand-built plan: a DDL plan plus a provided-XLSX data plan.
    let planned = PlannedExport {
        request: request(
            vec![provided_table("p", &["a"], vec![vec![json!(1)]])],
            ExportMode::DataOnly,
            ExportDataFormat::Xlsx,
            ExportOutputMode::Single,
        ),
        plans: vec![
            ExportFilePlan::Ddl {
                filename: "t.sql".to_owned(),
                text: "-- DDL unavailable for t".to_owned(),
            },
            ExportFilePlan::Data {
                filename: "p.xlsx".to_owned(),
                table: provided_table("p", &["a"], vec![vec![json!(1)]]),
                format: ExportDataFormat::Xlsx,
                ddl_prefix: None,
            },
        ],
        file_name: "export_data_2026-11-19-10-23-14.xlsx".to_owned(),
        request_context: TablePageRequestContext {
            db_session_id: "sess-1".to_owned(),
            database: Some("db".to_owned()),
            schema: Some("public".to_owned()),
            driver_type: Some("postgres".to_owned()),
        },
    };
    let err = futures::executor::block_on(exp.emit_planned(&planned))
        .expect_err("a binary plan in a merge must fail");
    assert_eq!(err, BlobTablesExportError::BinaryCannotMerge);
    assert_eq!(
        err.to_string(),
        "this export produces a binary file that cannot be merged into a single download"
    );
    assert!(capture.downloads().is_empty());
}

#[test]
fn export_tables_without_a_deliver_sink_fails_loudly() {
    let exp = exporter(Arc::new(ScriptedReader::new(vec![])), None, false);
    let err = export(
        &exp,
        request(
            vec![table("t")],
            ExportMode::StructureOnly,
            ExportDataFormat::Csv,
            ExportOutputMode::Single,
        ),
    )
    .expect_err("no sink must fail");
    assert_eq!(err, BlobTablesExportError::NoDownloadSink);
}

// ── error display ───────────────────────────────────────────────────────────

#[test]
fn error_display_covers_s2_variants() {
    assert_eq!(
        BlobTablesExportError::Format(ExportFormatError::NoColumns).to_string(),
        "export needs at least one column"
    );
    assert_eq!(
        BlobTablesExportError::Format(ExportFormatError::CannotFormatAsText {
            format: ExportDataFormat::Xlsx
        })
        .to_string(),
        "formatRowsAsText cannot format 'xlsx' as text"
    );
    assert_eq!(
        BlobTablesExportError::Format(ExportFormatError::XlsxFailed(
            "XLSX write failed: boom".to_owned()
        ))
        .to_string(),
        "XLSX write failed: boom"
    );
}

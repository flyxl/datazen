//! Minimal mirror of the provided-rows formatter (`lib/exportFormat.ts`).
//!
//! Mirrors `packages/frontend/src/lib/exportFormat.ts` — `formatRowsAsText`
//! / `formatRowsAsXlsx` plus the SQL dialect table they carry — for the
//! one consumer this slice has: the web-shaped tables exporter's `provided`
//! branch ([`super::blob_tables_exporter`]).
//!
//! ## Minimal on purpose — T500 owns the full port
//!
//! `lib/exportFormat.ts` has its own later task (T500); this module is
//! only the smallest surface S2 needs, so the full port can expand or
//! replace it without touching the exporter:
//!
//! * [`format_rows_as_text`] mirrors the `formatRowsAsText` switch 1:1
//!   in rule and in error wording (csv / tsv / json / markdown /
//!   `sql_insert` / `sql_update`; xlsx and parquet refuse with the
//!   React default-branch message). The dialect table
//!   (`sql_quote_char`, backtick / bracket / no-quote sets, the
//!   MySQL-family literal escaper) is copied from the driver metas, the
//!   same static table React keeps so this module stays a leaf — it must
//!   NOT import the driver registry.
//! * [`format_rows_as_xlsx`] mirrors `formatRowsAsXlsx`: header row plus
//!   rows on a sheet named `Sheet1`, returned as base64 so the exporter
//!   can hand it to [`super::blob_tables_exporter::base64_to_bytes`]
//!   exactly as React does. The workbook is built with
//!   `rust_xlsxwriter` (the same writer the Rust export kernel uses) in
//!   its default in-memory mode — provided rows are already loaded, so
//!   there is nothing to stream. XLSX bytes differ from SheetJS by
//!   design (the same note React's module header carries), so parity is
//!   structural: the workbook is a valid zip whose sheet holds the same
//!   cells.
//! * The CSV branch of [`format_rows_as_text`] exists for mirror
//!   completeness, but the exporter never calls it for CSV: the exporter
//!   emits `\n`-terminated [`super::blob_tables_exporter::csv_cell`]
//!   lines (the streaming shape the Rust writer produces), while
//!   `formatRowsAsText` joins lines with no trailing newline.
//! * The Markdown branch mirrors the `serializeCells` markdown branch
//!   (`gridClipboard.ts`, DB-10) inline — the one serializer's pipe
//!   table — pending T523's port of `lib/gridClipboard.ts`.
//!
//! ## Known divergences from the JS formatter (pinned, not fixed here)
//!
//! * Numbers render through `serde_json` (ryu), as in
//!   [`super::blob_tables_exporter::cell_to_text`]: integral floats
//!   render as `1.0` where JS `String(1)` renders `1`; exponents match
//!   JS exactly (`1e21` renders as `"1e+21"` in both). This bites the
//!   TSV / SQL-literal / JSON paths the same way it bites CSV.
//! * JSON records keep the columns in order (the top level is built by
//!   hand for that), but a *nested* object inside a cell renders through
//!   `serde_json`, whose map order is sorted — the same limitation
//!   `cell_to_text`'s JSON rendering already carries. The wire
//!   `Value` has already lost the original key order, so no mirror can
//!   recover it.
//! * `sql_update` key selection: React falls back to the column whose
//!   `isPrimaryKey` is set before the first column; [`ProvidedColumn`]
//!   is the name-only slice of `ColumnInfo`, so that middle step is
//!   unavailable here and the mirror falls back from the caller's
//!   `primary_key` straight to the first column. The full `ColumnInfo`
//!   shape arrives with T298 / T500.
//! * Bytes: on this wire a byte array is a JSON number array,
//!   indistinguishable from a real array (see
//!   [`super::export_plan::ExportTableSource::Provided`]), so a byte
//!   cell renders as JSON text in XLSX / JSON / SQL instead of the bare
//!   hex React's `Uint8Array` branch writes. Dates likewise arrive as
//!   strings on the wire, so there is no date cell — a timestamp is a
//!   string cell, as in the Rust export kernel.

#![allow(dead_code)]
// The formatter mirror is driven by the exporter's `provided` branch
// (and its tests) today and by T500's full port tomorrow; the lib
// itself has no other caller yet. Same allowance as
// `blob_tables_exporter.rs` (S1).

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rust_xlsxwriter::{Workbook, Worksheet, XlsxError};
use serde_json::Value;

use super::blob_tables_exporter::{cell_to_text, csv_cell};
use super::export_plan::{format_extension, ExportDataFormat, ProvidedColumn};

/// Cell value at a positional index; a short row reads as `null`, extras
/// are ignored (`cellAt` in `exportFormat.ts`).
fn cell_at<'a>(row: &'a [Value], index: usize) -> &'a Value {
    row.get(index).unwrap_or(&Value::Null)
}

/// Column names, or the `requireColumns` refusal
/// (`'export needs at least one column'`).
fn require_columns(columns: &[ProvidedColumn]) -> Result<Vec<String>, ExportFormatError> {
    if columns.is_empty() {
        return Err(ExportFormatError::NoColumns);
    }
    Ok(columns.iter().map(|column| column.name.clone()).collect())
}

/// Why the provided-rows formatter refused or failed. Every message is
/// the TS `Error` message verbatim, except `XlsxFailed`, which carries
/// the `rust_xlsxwriter` failure (there is no React counterpart — SheetJS
/// throws its own errors, and the kernel maps the same two limits to
/// actionable text, which this mirror copies).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExportFormatError {
    /// `formatRowsAsText` / `formatRowsAsXlsx` with no columns
    /// (`'export needs at least one column'`).
    NoColumns,
    /// `sql_update` with a single-column table: every column is the key,
    /// so there is nothing to SET (`'sql_update export needs at least
    /// one non-key column to SET'`).
    SqlUpdateNeedsSetColumn,
    /// `formatRowsAsText` asked for a binary format
    /// (`"formatRowsAsText cannot format 'xlsx' as text"`).
    CannotFormatAsText { format: ExportDataFormat },
    /// The `rust_xlsxwriter` workbook build failed.
    XlsxFailed(String),
}

impl std::fmt::Display for ExportFormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportFormatError::NoColumns => f.write_str("export needs at least one column"),
            ExportFormatError::SqlUpdateNeedsSetColumn => {
                f.write_str("sql_update export needs at least one non-key column to SET")
            }
            ExportFormatError::CannotFormatAsText { format } => {
                write!(f, "formatRowsAsText cannot format '{format}' as text")
            }
            ExportFormatError::XlsxFailed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ExportFormatError {}

/// Map a `rust_xlsxwriter` failure onto the export error envelope,
/// turning the two limits users can hit into actionable validation
/// messages instead of library-internal text — the same mapping the
/// Rust export kernel applies (`crates/datazen/src/export/binary.rs`).
fn xlsx_error(err: XlsxError) -> ExportFormatError {
    match err {
        XlsxError::MaxStringLengthExceeded => ExportFormatError::XlsxFailed(
            "a cell exceeds Excel's 32767-character limit; shorten the value or export as CSV"
                .to_owned(),
        ),
        XlsxError::RowColumnLimitError => ExportFormatError::XlsxFailed(
            "export exceeds Excel's worksheet limits (1048576 rows / 16384 columns)".to_owned(),
        ),
        other => ExportFormatError::XlsxFailed(format!("XLSX write failed: {other}")),
    }
}

/// Text result — `TextExportResult` in `exportFormat.ts`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextExportResult {
    pub content: String,
    pub extension: &'static str,
    pub mime_type: &'static str,
}

/// Binary result — `BinaryExportResult` in `exportFormat.ts`: the
/// workbook as base64, so the caller can decode it without touching the
/// backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BinaryExportResult {
    pub data_base64: String,
    pub extension: &'static str,
    pub mime_type: &'static str,
}

/// `XLSX_MIME_TYPE` in `exportFormat.ts`.
pub(crate) const XLSX_MIME_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/// Multi-row INSERT batch size (TablePlus-style), same as Rust
/// `SQL_INSERT_BATCH_SIZE` (`SQL_INSERT_BATCH_SIZE` in `exportFormat.ts`).
const SQL_INSERT_BATCH_SIZE: usize = 500;

// MySQL-family drivers quote identifiers with backticks and escape string
// literals with backslashes (`mysqlEscapeSqlValue` in `mysql/ui/meta.ts`).
const BACKTICK_DIALECTS: &[&str] = &[
    "mysql",
    "mariadb",
    "doris",
    "starrocks",
    "manticore",
    "ob_oracle",
    "kiwi",
];
// SQL Server brackets identifiers; the no-SQL drivers below leave
// identifiers bare; everything else double-quotes (also the fallback for
// unknown types, matching `databaseTypes.escapeIdent`'s `?? '"'`).
const NO_QUOTE_DIALECTS: &[&str] = &[
    "elasticsearch",
    "hbase",
    "influxdb",
    "mongodb",
    "redis",
    "vector",
    "victoriametrics",
];

/// Drivers whose metas declare `escapeSqlValue` (the MySQL-family
/// escaper). Backtick quoting and backslash escaping are independent meta
/// facts — `kiwi` quotes with backticks but keeps the default escaper —
/// so this set is a strict subset of [`BACKTICK_DIALECTS`].
const MYSQL_ESCAPE_DIALECTS: &[&str] = &[
    "mysql",
    "mariadb",
    "doris",
    "starrocks",
    "manticore",
    "ob_oracle",
];

fn escape_sql_ident(name: &str, database_type: Option<&str>) -> String {
    let db = database_type.unwrap_or("");
    if BACKTICK_DIALECTS.contains(&db) {
        format!("`{}`", name.replace('`', "``"))
    } else if db == "sqlserver" {
        format!("[{}]", name.replace(']', "]]"))
    } else if NO_QUOTE_DIALECTS.contains(&db) {
        name.to_owned()
    } else {
        format!("\"{}\"", name.replace('"', "\"\""))
    }
}

/// Default SQL literal escaper (PostgreSQL-style), as in
/// `databaseTypes.defaultEscapeSqlValue`.
fn default_escape_sql_literal(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_owned(),
        Value::Bool(flag) => {
            if *flag {
                "TRUE".to_owned()
            } else {
                "FALSE".to_owned()
            }
        }
        Value::Number(number) => number.to_string(),
        _ => {
            // Strings pass through; objects and arrays render as JSON
            // (`cell_to_text`); the literal is quoted with `'` doubled.
            let text = cell_to_text(value);
            format!("'{}'", text.replace('\'', "''"))
        }
    }
}

/// MySQL-family literal escaper, as in `mysql/ui/meta.ts`: backslash
/// escapes in three passes — `\` → `\\`, `"` → `\"`, `'` → `''`.
fn mysql_escape_sql_literal(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_owned(),
        Value::Bool(flag) => {
            if *flag {
                "TRUE".to_owned()
            } else {
                "FALSE".to_owned()
            }
        }
        Value::Number(number) => number.to_string(),
        _ => {
            let text = cell_to_text(value);
            format!(
                "'{}'",
                text.replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\'', "''")
            )
        }
    }
}

fn escape_sql_literal(value: &Value, database_type: Option<&str>) -> String {
    if database_type.map_or(false, |db| MYSQL_ESCAPE_DIALECTS.contains(&db)) {
        mysql_escape_sql_literal(value)
    } else {
        default_escape_sql_literal(value)
    }
}

fn sql_begin_transaction(database_type: Option<&str>) -> &'static str {
    if database_type == Some("sqlserver") {
        "BEGIN TRANSACTION;"
    } else {
        "BEGIN;"
    }
}

/// Batched INSERT statements wrapped in a transaction (old
/// `formatSqlInsertScript`).
fn format_sql_insert_script(
    table_name: &str,
    cols: &[String],
    rows: &[Vec<Value>],
    database_type: Option<&str>,
) -> String {
    let mut lines: Vec<String> = vec![sql_begin_transaction(database_type).to_owned()];
    if rows.is_empty() {
        lines.push("COMMIT;".to_owned());
        return lines.join("\n");
    }
    let col_list: Vec<String> = cols
        .iter()
        .map(|col| escape_sql_ident(col, database_type))
        .collect();
    let header = format!(
        "INSERT INTO {} ({}) VALUES",
        escape_sql_ident(table_name, database_type),
        col_list.join(", ")
    );
    for chunk in rows.chunks(SQL_INSERT_BATCH_SIZE) {
        let tuples: Vec<String> = chunk
            .iter()
            .map(|row| {
                let literals: Vec<String> = cols
                    .iter()
                    .enumerate()
                    .map(|(i, _)| escape_sql_literal(cell_at(row, i), database_type))
                    .collect();
                format!("  ({})", literals.join(", "))
            })
            .collect();
        lines.push(format!("{header}\n{};", tuples.join(",\n")));
    }
    lines.push("COMMIT;".to_owned());
    lines.join("\n")
}

/// Key column for `sql_update`: the caller's `primaryKey` when it is
/// among the exported columns, else the first column — the same fallback
/// the Rust `StreamFormatter::update_key` uses. React also checks a
/// column's `isPrimaryKey` between the two; [`ProvidedColumn`] is
/// name-only, so that step is unavailable here (see the module docs).
/// `None` when there are no columns at all.
fn update_key_column(cols: &[String], primary_key: Option<&str>) -> Option<String> {
    if let Some(pk) = primary_key {
        if cols.iter().any(|col| col == pk) {
            return Some(pk.to_owned());
        }
    }
    cols.first().cloned()
}

fn format_sql_update_script(
    table_name: &str,
    columns: &[ProvidedColumn],
    rows: &[Vec<Value>],
    database_type: Option<&str>,
    primary_key: Option<&str>,
) -> Result<String, ExportFormatError> {
    let cols = require_columns(columns)?;
    // Empty columns are rejected upstream by `require_columns` in
    // `format_rows_as_text`, so a key column always exists here.
    let pk_name = match update_key_column(&cols, primary_key) {
        Some(name) => name,
        None => return Err(ExportFormatError::NoColumns),
    };
    let pk_index = cols.iter().position(|col| *col == pk_name).unwrap_or(0);
    let set_indexes: Vec<usize> = (0..cols.len()).filter(|i| *i != pk_index).collect();
    if set_indexes.is_empty() {
        return Err(ExportFormatError::SqlUpdateNeedsSetColumn);
    }
    let mut lines: Vec<String> = vec![sql_begin_transaction(database_type).to_owned()];
    for row in rows {
        let set_clauses: Vec<String> = set_indexes
            .iter()
            .map(|i| {
                format!(
                    "{} = {}",
                    escape_sql_ident(&cols[*i], database_type),
                    escape_sql_literal(cell_at(row, *i), database_type)
                )
            })
            .collect();
        let where_clause = format!(
            "{} = {}",
            escape_sql_ident(&pk_name, database_type),
            escape_sql_literal(cell_at(row, pk_index), database_type)
        );
        lines.push(format!(
            "UPDATE {} SET {} WHERE {};",
            escape_sql_ident(table_name, database_type),
            set_clauses.join(", "),
            where_clause
        ));
    }
    lines.push("COMMIT;".to_owned());
    Ok(lines.join("\n"))
}

/// One TSV field: null is empty, objects render as JSON, the rest as JS
/// `String(value)` — then tabs and line breaks become spaces
/// (`escapeTsvField` in `exportFormat.ts`).
fn escape_tsv_field(value: &Value) -> String {
    cell_to_text(value)
        .replace('\t', " ")
        .replace('\n', " ")
        .replace('\r', "")
}

/// Escape one Markdown table cell (`escapeMarkdownCellText` in
/// `gridClipboard.ts`, DB-10): a pipe would end the cell, a line break
/// would end the **row**. `\|` and `<br>` are what GitHub and Notion
/// both render back into the original characters.
fn escape_markdown_cell_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "<br>")
}

/// Write one JSON value pretty-printed at `indent` levels of two spaces
/// — the `JSON.stringify(value, null, 2)` shape. Object keys render in
/// iteration order; for the top-level records the caller builds the
/// object by hand so the keys follow the column order.
fn write_json_value(value: &Value, indent: usize, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => out.push_str(&number.to_string()),
        Value::String(text) => match serde_json::to_string(text) {
            Ok(escaped) => out.push_str(&escaped),
            Err(_) => out.push_str("\"\""),
        },
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                out.push_str(&"  ".repeat(indent + 1));
                write_json_value(item, indent + 1, out);
                if i + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&"  ".repeat(indent));
            out.push(']');
        }
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            let len = map.len();
            for (i, (key, item)) in map.iter().enumerate() {
                out.push_str(&"  ".repeat(indent + 1));
                match serde_json::to_string(key) {
                    Ok(escaped) => out.push_str(&escaped),
                    Err(_) => out.push_str("\"\""),
                }
                out.push_str(": ");
                write_json_value(item, indent + 1, out);
                if i + 1 < len {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&"  ".repeat(indent));
            out.push('}');
        }
    }
}

/// Rows as `{ column: value }` records, pretty-printed with two-space
/// indent (`rowsAsRecords` + `JSON.stringify(records, null, 2)`). The
/// record keys follow the column order — a `serde_json::Map` would sort
/// them, so the records are built by hand here.
fn rows_as_json_records(cols: &[String], rows: &[Vec<Value>]) -> String {
    if rows.is_empty() {
        return "[]".to_owned();
    }
    let mut out = String::from("[\n");
    for (ri, row) in rows.iter().enumerate() {
        out.push_str("  {\n");
        for (ci, col) in cols.iter().enumerate() {
            out.push_str("    ");
            match serde_json::to_string(col) {
                Ok(escaped) => out.push_str(&escaped),
                Err(_) => out.push_str("\"\""),
            }
            out.push_str(": ");
            write_json_value(cell_at(row, ci), 2, &mut out);
            if ci + 1 < cols.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  }");
        if ri + 1 < rows.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push(']');
    out
}

/// Format already-loaded rows as text. Row values are the `Value`
/// positional shape over `columns`; a missing cell reads as `null`, an
/// over-long row's extras are ignored (`formatRowsAsText` in
/// `exportFormat.ts`).
pub(crate) fn format_rows_as_text(
    table_name: &str,
    columns: &[ProvidedColumn],
    rows: &[Vec<Value>],
    format: ExportDataFormat,
    database_type: Option<&str>,
    primary_key: Option<&str>,
) -> Result<TextExportResult, ExportFormatError> {
    // Unreachable through the type in React (`ExportTextFormat` has
    // neither `'xlsx'` nor `'parquet'`), but a caller that smuggles one
    // in must fail loudly, not silently return the wrong shape — so
    // both matches below return the refusal for those arms instead of
    // claiming they cannot happen.
    let cols = require_columns(columns)?;
    let extension = format_extension(format);
    let mime_type = match format {
        ExportDataFormat::Csv => "text/csv",
        ExportDataFormat::Tsv => "text/tab-separated-values",
        ExportDataFormat::Json => "application/json",
        ExportDataFormat::Markdown => "text/markdown",
        ExportDataFormat::SqlInsert | ExportDataFormat::SqlUpdate => "text/sql",
        ExportDataFormat::Xlsx | ExportDataFormat::Parquet => {
            return Err(ExportFormatError::CannotFormatAsText { format });
        }
    };
    let content = match format {
        ExportDataFormat::Csv => {
            // The exporter's own CSV path never calls this (it emits
            // `\n`-terminated `csv_cell` lines instead); this branch
            // exists so the mirror stays complete.
            let header: Vec<String> = cols
                .iter()
                .map(|col| csv_cell(&Value::String(col.clone())))
                .collect();
            let mut lines = vec![header.join(",")];
            for row in rows {
                let cells: Vec<String> = cols
                    .iter()
                    .enumerate()
                    .map(|(i, _)| csv_cell(cell_at(row, i)))
                    .collect();
                lines.push(cells.join(","));
            }
            lines.join("\n")
        }
        ExportDataFormat::Tsv => {
            let header: Vec<String> = cols
                .iter()
                .map(|col| escape_tsv_field(&Value::String(col.clone())))
                .collect();
            let mut lines = vec![header.join("\t")];
            for row in rows {
                let cells: Vec<String> = cols
                    .iter()
                    .enumerate()
                    .map(|(i, _)| escape_tsv_field(cell_at(row, i)))
                    .collect();
                lines.push(cells.join("\t"));
            }
            lines.join("\n")
        }
        ExportDataFormat::Json => rows_as_json_records(&cols, rows),
        ExportDataFormat::Markdown => {
            // The one serializer's Markdown branch (DB-10): same
            // escaping as the clipboard's "Copy as Markdown Table".
            let head: Vec<String> =
                cols.iter().map(|col| escape_markdown_cell_text(col)).collect();
            let separator: Vec<&str> = cols.iter().map(|_| "---").collect();
            let mut lines = vec![
                format!("| {} |", head.join(" | ")),
                format!("| {} |", separator.join(" | ")),
            ];
            for row in rows {
                let cells: Vec<String> = cols
                    .iter()
                    .enumerate()
                    .map(|(i, _)| escape_markdown_cell_text(&cell_to_text(cell_at(row, i))))
                    .collect();
                lines.push(format!("| {} |", cells.join(" | ")));
            }
            lines.join("\n")
        }
        ExportDataFormat::SqlInsert => {
            format_sql_insert_script(table_name, &cols, rows, database_type)
        }
        ExportDataFormat::SqlUpdate => {
            format_sql_update_script(table_name, columns, rows, database_type, primary_key)?
        }
        ExportDataFormat::Xlsx | ExportDataFormat::Parquet => {
            return Err(ExportFormatError::CannotFormatAsText { format });
        }
    };
    Ok(TextExportResult {
        content,
        extension,
        mime_type,
    })
}

/// One XLSX cell (`toXlsxCell` in `exportFormat.ts`): numbers / booleans
/// / strings pass through, `null` stays a blank cell, objects become
/// their JSON text (as the Rust writer does). Bytes and dates have no
/// wire form here (see the module docs): a JSON array renders as JSON
/// text, a timestamp as a string cell.
fn write_xlsx_cell(
    worksheet: &mut Worksheet,
    row: u32,
    col: u16,
    value: &Value,
) -> Result<(), ExportFormatError> {
    match value {
        Value::Null => Ok(()),
        Value::Bool(flag) => {
            worksheet
                .write_boolean(row, col, *flag)
                .map_err(xlsx_error)?;
            Ok(())
        }
        Value::Number(number) => {
            // JS numbers are f64; a huge u64 rounds the same way here.
            // `as_f64` is `Some` for every JSON number, but the
            // `else` keeps the no-`unwrap` rule honest.
            if let Some(float) = number.as_f64() {
                worksheet
                    .write_number(row, col, float)
                    .map_err(xlsx_error)?;
            } else {
                worksheet
                    .write_string(row, col, &number.to_string())
                    .map_err(xlsx_error)?;
            }
            Ok(())
        }
        Value::String(text) => {
            worksheet
                .write_string(row, col, text)
                .map_err(xlsx_error)?;
            Ok(())
        }
        Value::Array(_) | Value::Object(_) => {
            worksheet
                .write_string(row, col, &value.to_string())
                .map_err(xlsx_error)?;
            Ok(())
        }
    }
}

/// Format already-loaded rows as an XLSX workbook, returned as base64
/// so the caller can hand it to a Blob download without touching the
/// backend (`formatRowsAsXlsx` in `exportFormat.ts`). React takes the
/// table name but never uses it — the sheet is always `Sheet1` — so
/// this mirror takes only the columns and rows it actually formats.
pub(crate) fn format_rows_as_xlsx(
    columns: &[ProvidedColumn],
    rows: &[Vec<Value>],
) -> Result<BinaryExportResult, ExportFormatError> {
    let cols = require_columns(columns)?;
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("Sheet1").map_err(xlsx_error)?;
    for (ci, name) in cols.iter().enumerate() {
        let col = u16::try_from(ci).map_err(|_| {
            ExportFormatError::XlsxFailed(
                "export exceeds Excel's worksheet limits (1048576 rows / 16384 columns)"
                    .to_owned(),
            )
        })?;
        worksheet
            .write_string(0, col, name)
            .map_err(xlsx_error)?;
    }
    for (ri, row) in rows.iter().enumerate() {
        let row_index = u32::try_from(ri + 1).map_err(|_| {
            ExportFormatError::XlsxFailed(
                "export exceeds Excel's worksheet limits (1048576 rows / 16384 columns)"
                    .to_owned(),
            )
        })?;
        for (ci, _) in cols.iter().enumerate() {
            // `ci` fits in `u16` here: the header write above already
            // failed on a column that does not.
            let col = u16::try_from(ci).map_err(|_| {
                ExportFormatError::XlsxFailed(
                    "export exceeds Excel's worksheet limits (1048576 rows / 16384 columns)"
                        .to_owned(),
                )
            })?;
            write_xlsx_cell(worksheet, row_index, col, cell_at(row, ci))?;
        }
    }
    let bytes = workbook.save_to_buffer().map_err(xlsx_error)?;
    Ok(BinaryExportResult {
        data_base64: STANDARD.encode(&bytes),
        extension: format_extension(ExportDataFormat::Xlsx),
        mime_type: XLSX_MIME_TYPE,
    })
}

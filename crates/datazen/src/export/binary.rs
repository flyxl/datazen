//! Binary (non-text) export formatters — the parallel branch to
//! [`StreamFormatter`](super::format::StreamFormatter).
//!
//! A text formatter returns `String` per batch, which an XLSX file can never
//! be: the ZIP central directory is only written when the workbook is
//! finished. Binary formatters therefore own their destination and write
//! incrementally (`begin` → batched `rows` → `finish`), reusing the same
//! upstream row stream (`StatementStart` + batched `Rows`) as the text path.
//!
//! Deviation from the XLSX/Parquet plan (§3.1): the plan sketches
//! `finish(self)`, but the shared export callback holds the formatter behind
//! `Arc<Mutex<…>>` and calls `tail()`-style completion twice (once on `Done`,
//! once as a post-fetch fallback when `Done` never fires). A consuming
//! `finish` cannot express that; `finish(&mut self)` with a `finished` flag
//! inside the formatter gives the same exactly-once guarantee and fits the
//! existing callback shape.
//!
//! Item 3a ships XLSX only; Parquet is a second implementor of this trait.

#[cfg(feature = "export-xlsx")]
use std::path::PathBuf;

#[cfg(feature = "export-xlsx")]
use super::bytes_to_hex;
use super::ExportError;
use datazen_driver_api::{ColumnInfo, Value};
#[cfg(feature = "export-xlsx")]
use rust_xlsxwriter::{Workbook, XlsxError};

/// Excel worksheet limit, header row included: at most this many rows total.
#[cfg(feature = "export-xlsx")]
pub const XLSX_MAX_ROWS: u32 = 1_048_576;
/// Excel worksheet limit on columns.
#[cfg(feature = "export-xlsx")]
pub const XLSX_MAX_COLUMNS: usize = 16_384;
/// Excel per-cell string limit, in characters.
#[cfg(feature = "export-xlsx")]
pub const XLSX_MAX_CELL_CHARS: usize = 32_767;
/// Excel worksheet-name limit, in characters.
#[cfg(feature = "export-xlsx")]
pub const XLSX_MAX_SHEET_NAME_CHARS: usize = 31;
/// Excel numbers are f64; integers beyond ±this lose precision silently,
/// so they are written as strings instead.
#[cfg(feature = "export-xlsx")]
pub const XLSX_MAX_EXACT_INTEGER: i64 = 999_999_999_999_999;

/// A formatter that writes one binary file incrementally.
pub trait BinaryFormatter: Send {
    /// Called on `StatementStart`, when the full schema is known.
    fn begin(&mut self, columns: &[ColumnInfo]) -> Result<(), ExportError>;
    /// Called once per streamed batch (at most `QUERY_STREAM_BATCH_SIZE` rows).
    fn rows(&mut self, rows: &[Vec<Option<Value>>]) -> Result<(), ExportError>;
    /// Called once at the end (on `Done`, or as a post-fetch fallback):
    /// flushes and closes the file. Safe to call only once per formatter.
    fn finish(&mut self) -> Result<(), ExportError>;
}

/// Map a `rust_xlsxwriter` failure onto the export error envelope, turning
/// the two limits users can hit into actionable validation messages instead
/// of library-internal text.
#[cfg(feature = "export-xlsx")]
fn xlsx_error(err: XlsxError) -> ExportError {
    match err {
        XlsxError::MaxStringLengthExceeded => ExportError::Validation(format!(
            "a cell exceeds Excel's {XLSX_MAX_CELL_CHARS}-character limit; \
             shorten the value or export as CSV"
        )),
        XlsxError::RowColumnLimitError => ExportError::Validation(format!(
            "export exceeds Excel's worksheet limits ({XLSX_MAX_ROWS} rows / \
             {XLSX_MAX_COLUMNS} columns)"
        )),
        other => ExportError::Internal(format!("XLSX write failed: {other}")),
    }
}

/// Clean a table name into a legal Excel worksheet name: forbidden
/// characters (`[]:*?/\`) become `_`, leading/trailing apostrophes are
/// stripped, over-long names truncate to 31 characters, and the reserved
/// name `History` is renamed. An empty result becomes `Sheet1`.
#[cfg(feature = "export-xlsx")]
pub fn sanitize_sheet_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut cleaned = cleaned.trim_matches('\'').to_string();
    if cleaned.chars().count() > XLSX_MAX_SHEET_NAME_CHARS {
        cleaned = cleaned.chars().take(XLSX_MAX_SHEET_NAME_CHARS).collect();
        cleaned = cleaned.trim_matches('\'').to_string();
    }
    if cleaned.is_empty() {
        cleaned = "Sheet1".to_string();
    } else if cleaned.eq_ignore_ascii_case("history") {
        cleaned = "History_".to_string();
    }
    cleaned
}

/// XLSX writer over `rust_xlsxwriter` in constant-memory mode: rows are
/// flushed to a temp file as they arrive, so memory stays flat no matter
/// how large the table is. The trade-off is the mode's own: rows must be
/// written in order (exactly what the stream gives us) and there is no
/// column-width autofit — v1 does not attempt one.
#[cfg(feature = "export-xlsx")]
pub struct XlsxFormatter {
    workbook: Workbook,
    sheet_name: String,
    dest: PathBuf,
    columns: Vec<ColumnInfo>,
    /// Next row index to write; 0 is the header row.
    next_row: u32,
    begun: bool,
    finished: bool,
}

#[cfg(feature = "export-xlsx")]
impl XlsxFormatter {
    pub fn new(table_name: &str, dest: PathBuf) -> Self {
        let sheet_name = sanitize_sheet_name(table_name);
        let mut workbook = Workbook::new();
        // Constant-memory mode fails only if its temp file cannot be created;
        // surface that lazily through begin()/rows() instead of panicking
        // here — the constructor is infallible by design.
        let _ = workbook
            .add_worksheet_with_constant_memory()
            .map(|ws| ws.set_name(sheet_name.clone()));
        Self {
            workbook,
            sheet_name,
            dest,
            columns: Vec::new(),
            next_row: 0,
            begun: false,
            finished: false,
        }
    }

    fn worksheet(&mut self) -> Result<&mut rust_xlsxwriter::Worksheet, ExportError> {
        self.workbook.worksheet_from_index(0).map_err(xlsx_error)
    }

    /// Write one cell. `Null` leaves the cell blank; `Timestamp` and `Json`
    /// are strings (the `Value` already carries timestamps as text, and v1
    /// does not pretend to parse them into Excel dates); `Bytes` is the bare
    /// hex string — the `datazen:bytes:hex:` prefix is a CSV round-trip
    /// marker that would look like noise in a spreadsheet.
    fn write_cell(
        worksheet: &mut rust_xlsxwriter::Worksheet,
        row: u32,
        col: u16,
        value: &Option<Value>,
    ) -> Result<(), ExportError> {
        match value {
            None | Some(Value::Null) => Ok(()),
            Some(Value::Bool(b)) => {
                worksheet.write_boolean(row, col, *b).map_err(xlsx_error)?;
                Ok(())
            }
            Some(Value::Integer(n)) => {
                // Range check, not abs(): n.abs() panics on i64::MIN in
                // debug builds (and wraps in release).
                if *n >= -XLSX_MAX_EXACT_INTEGER && *n <= XLSX_MAX_EXACT_INTEGER {
                    worksheet
                        .write_number(row, col, *n as f64)
                        .map_err(xlsx_error)?;
                } else {
                    worksheet
                        .write_string(row, col, n.to_string())
                        .map_err(xlsx_error)?;
                }
                Ok(())
            }
            Some(Value::Float(n)) => {
                worksheet.write_number(row, col, *n).map_err(xlsx_error)?;
                Ok(())
            }
            Some(Value::String(s)) | Some(Value::Timestamp(s)) => {
                check_cell_length(s)?;
                worksheet.write_string(row, col, s).map_err(xlsx_error)?;
                Ok(())
            }
            Some(Value::Bytes(b)) => {
                let hex = bytes_to_hex(b);
                check_cell_length(&hex)?;
                worksheet.write_string(row, col, hex).map_err(xlsx_error)?;
                Ok(())
            }
            Some(Value::Json(j)) => {
                let text = j.to_string();
                check_cell_length(&text)?;
                worksheet.write_string(row, col, text).map_err(xlsx_error)?;
                Ok(())
            }
        }
    }
}

/// Friendly pre-check for the 32,767-character cell limit: the library
/// reports the same failure, but naming the limit and the CSV fallback here
/// keeps the message actionable.
#[cfg(feature = "export-xlsx")]
fn check_cell_length(text: &str) -> Result<(), ExportError> {
    if text.chars().count() > XLSX_MAX_CELL_CHARS {
        return Err(ExportError::Validation(format!(
            "a cell exceeds Excel's {XLSX_MAX_CELL_CHARS}-character limit \
             ({} characters); shorten the value or export as CSV",
            text.chars().count()
        )));
    }
    Ok(())
}

#[cfg(feature = "export-xlsx")]
impl BinaryFormatter for XlsxFormatter {
    fn begin(&mut self, columns: &[ColumnInfo]) -> Result<(), ExportError> {
        if self.begun {
            return Err(ExportError::Validation(
                "XLSX export received a second column header".into(),
            ));
        }
        if columns.len() > XLSX_MAX_COLUMNS {
            return Err(ExportError::Validation(format!(
                "export has {} columns, over Excel's {XLSX_MAX_COLUMNS} limit",
                columns.len()
            )));
        }
        self.columns = columns.to_vec();
        let sheet_name = self.sheet_name.clone();
        {
            let worksheet = self.worksheet()?;
            // The name was sanitized in new(); re-apply here so a workbook
            // whose constant-memory worksheet failed to be created in new()
            // still reports the failure instead of writing under "Sheet1".
            worksheet.set_name(sheet_name).map_err(xlsx_error)?;
            for (col, column) in columns.iter().enumerate() {
                let col = u16::try_from(col).map_err(|_| {
                    ExportError::Validation(format!(
                        "export exceeds Excel's {XLSX_MAX_COLUMNS}-column limit"
                    ))
                })?;
                check_cell_length(&column.name)?;
                worksheet
                    .write_string(0, col, column.name.clone())
                    .map_err(xlsx_error)?;
            }
        }
        self.next_row = 1;
        self.begun = true;
        Ok(())
    }

    fn rows(&mut self, rows: &[Vec<Option<Value>>]) -> Result<(), ExportError> {
        if !self.begun {
            return Err(ExportError::Validation(
                "XLSX export received rows before the column header".into(),
            ));
        }
        for row in rows {
            if self.next_row >= XLSX_MAX_ROWS {
                return Err(ExportError::Validation(format!(
                    "export exceeds Excel's {XLSX_MAX_ROWS}-row limit \
                     (header included); split the table or export as CSV"
                )));
            }
            let row_index = self.next_row;
            {
                let worksheet = self.worksheet()?;
                for (col, cell) in row.iter().enumerate() {
                    let col = u16::try_from(col).map_err(|_| {
                        ExportError::Validation(format!(
                            "export exceeds Excel's {XLSX_MAX_COLUMNS}-column limit"
                        ))
                    })?;
                    Self::write_cell(worksheet, row_index, col, cell)?;
                }
            }
            self.next_row += 1;
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), ExportError> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        let dest = self.dest.clone();
        self.workbook.save(dest).map_err(xlsx_error)
    }
}

/// The sheet name the formatter will use (sanitized table name), for tests.
#[cfg(all(test, feature = "export-xlsx"))]
impl XlsxFormatter {
    pub(crate) fn test_sheet_name(&self) -> &str {
        &self.sheet_name
    }

    /// Move the write cursor (test-only): lets the 1,048,576-row limit be
    /// exercised without writing a million rows.
    pub(crate) fn test_set_next_row(&mut self, row: u32) {
        self.next_row = row;
    }

    /// Current write cursor (test-only): 0 before begin(), 1 once the
    /// header row is written.
    pub(crate) fn test_next_row(&self) -> u32 {
        self.next_row
    }
}

#[cfg(all(test, feature = "export-xlsx"))]
mod tests {
    use super::*;
    use std::io::Read;

    fn col(name: &str, data_type: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: data_type.into(),
            nullable: true,
        }
    }

    /// Read one entry out of a produced .xlsx (an XLSX file is a ZIP).
    fn xlsx_entry(path: &PathBuf, name: &str) -> String {
        let file = std::fs::File::open(path).expect("xlsx exists");
        let mut zip = zip::ZipArchive::new(file).expect("xlsx is a zip");
        let mut entry = zip
            .by_name(name)
            .unwrap_or_else(|_| panic!("xlsx entry {name} exists"));
        let mut text = String::new();
        entry.read_to_string(&mut text).expect("entry is text");
        text
    }

    /// All text the workbook stores for cells: the sheet XML plus the
    /// shared-strings table when one exists (constant-memory mode writes
    /// inline strings, but the assertion must not depend on that).
    fn xlsx_cell_text(path: &PathBuf) -> String {
        let file = std::fs::File::open(path).expect("xlsx exists");
        let mut zip = zip::ZipArchive::new(file).expect("xlsx is a zip");
        let mut out = String::new();
        for name in ["xl/worksheets/sheet1.xml", "xl/sharedStrings.xml"] {
            if let Ok(mut entry) = zip.by_name(name) {
                let mut text = String::new();
                entry.read_to_string(&mut text).expect("entry is text");
                out.push_str(&text);
            }
        }
        out
    }

    fn xlsx_entry_names(path: &PathBuf) -> Vec<String> {
        let file = std::fs::File::open(path).expect("xlsx exists");
        let zip = zip::ZipArchive::new(file).expect("xlsx is a zip");
        zip.file_names().map(str::to_string).collect()
    }

    fn temp_xlsx(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("datazen-export-xlsx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("tempdir");
        dir.join(name)
    }

    #[test]
    fn sheet_name_cleaning_covers_excel_rules() {
        assert_eq!(sanitize_sheet_name("users"), "users");
        assert_eq!(sanitize_sheet_name("a:b*c?d[e]f/g\\h"), "a_b_c_d_e_f_g_h");
        assert_eq!(sanitize_sheet_name("'quoted'"), "quoted");
        assert_eq!(sanitize_sheet_name("History"), "History_");
        assert_eq!(sanitize_sheet_name("history"), "History_");
        assert_eq!(sanitize_sheet_name(""), "Sheet1");
        assert_eq!(sanitize_sheet_name("'''"), "Sheet1");
        let long = "a".repeat(40);
        assert_eq!(sanitize_sheet_name(&long).chars().count(), 31);
        // Truncation must not leave a trailing apostrophe behind.
        assert_eq!(
            sanitize_sheet_name(&format!("{}'", "b".repeat(31))),
            "b".repeat(31)
        );
    }

    #[test]
    fn xlsx_roundtrip_header_rows_and_value_kinds() {
        let dest = temp_xlsx("roundtrip.xlsx");
        let mut fmt = XlsxFormatter::new("users", dest.clone());
        fmt.begin(&[
            col("id", "int8"),
            col("score", "float8"),
            col("name", "text"),
            col("active", "bool"),
            col("payload", "bytea"),
            col("created", "timestamp"),
            col("meta", "jsonb"),
            col("note", "text"),
        ])
        .unwrap();
        fmt.rows(&[vec![
            Some(Value::Integer(42)),
            Some(Value::Float(1.5)),
            Some(Value::String("a,b".into())),
            Some(Value::Bool(true)),
            Some(Value::Bytes(vec![0x00, 0xff])),
            Some(Value::Timestamp("2026-11-09 12:00:00".into())),
            Some(Value::Json(serde_json::json!({"k": 1}))),
            None,
        ]])
        .unwrap();
        fmt.finish().unwrap();

        let names = xlsx_entry_names(&dest);
        assert!(names.contains(&"xl/worksheets/sheet1.xml".to_string()));
        assert!(names.contains(&"xl/workbook.xml".to_string()));
        assert!(names.contains(&"[Content_Types].xml".to_string()));
        let text = xlsx_cell_text(&dest);
        for header in [
            "id", "score", "name", "active", "payload", "created", "meta", "note",
        ] {
            assert!(text.contains(header), "missing header {header} in {text}");
        }
        assert!(text.contains("<v>42</v>"), "integer as number: {text}");
        assert!(text.contains("<v>1.5</v>"), "float as number: {text}");
        assert!(text.contains("a,b"), "string cell: {text}");
        // Bool is stored as <v>1</v> with t="b"; the bare hex has no marker.
        assert!(text.contains("00ff"), "bytes as bare hex: {text}");
        assert!(
            !text.contains("datazen:bytes:hex:"),
            "no CSV marker in xlsx: {text}"
        );
        assert!(
            text.contains("2026-11-09 12:00:00"),
            "timestamp as string: {text}"
        );
        assert!(text.contains("k"), "json as string: {text}");
        // The workbook names the sheet after the table.
        assert!(
            xlsx_entry(&dest, "xl/workbook.xml").contains("users"),
            "sheet name in workbook.xml"
        );
    }

    #[test]
    fn xlsx_calamine_readback_matches_written_values() {
        // Real-parser acceptance: the file the Rust writer produces must
        // round-trip through calamine, not just string-match sheet XML.
        // Numbers come back as Float (XLSX stores numbers as f64);
        // Bytes come back as the bare hex string; the over-limit integer
        // comes back as a string, not a rounded number.
        let dest = temp_xlsx("calamine.xlsx");
        let mut fmt = XlsxFormatter::new("users", dest.clone());
        fmt.begin(&[
            col("id", "int8"),
            col("score", "float8"),
            col("name", "text"),
            col("active", "bool"),
            col("payload", "bytea"),
            col("created", "timestamp"),
            col("meta", "jsonb"),
            col("note", "text"),
            col("big", "int8"),
        ])
        .unwrap();
        fmt.rows(&[vec![
            Some(Value::Integer(42)),
            Some(Value::Float(1.5)),
            Some(Value::String("a,b".into())),
            Some(Value::Bool(true)),
            Some(Value::Bytes(vec![0x00, 0xff])),
            Some(Value::Timestamp("2026-11-09 12:00:00".into())),
            Some(Value::Json(serde_json::json!({"k": 1}))),
            None,
            Some(Value::Integer(1_000_000_000_000_000)),
        ]])
        .unwrap();
        fmt.finish().unwrap();

        let mut workbook: calamine::Xlsx<_> =
            calamine::open_workbook(&dest).expect("calamine opens the produced file");
        use calamine::{DataType, Reader};
        let range = workbook
            .worksheet_range("users")
            .expect("sheet users exists");
        let rows: Vec<&[calamine::Data]> = range.rows().collect();
        assert_eq!(rows.len(), 2, "header + one data row");
        let header: Vec<String> = rows[0]
            .iter()
            .map(|c| c.as_string().expect("header is a string"))
            .collect();
        assert_eq!(
            header,
            ["id", "score", "name", "active", "payload", "created", "meta", "note", "big"]
        );
        let row = rows[1];
        assert_eq!(row[0], calamine::Data::Float(42.0), "integer read back");
        assert_eq!(row[1], calamine::Data::Float(1.5), "float read back");
        assert_eq!(
            row[2],
            calamine::Data::String("a,b".to_string()),
            "string read back"
        );
        assert_eq!(row[3], calamine::Data::Bool(true), "bool read back");
        assert_eq!(
            row[4],
            calamine::Data::String("00ff".to_string()),
            "bytes read back as bare hex"
        );
        assert_eq!(
            row[5],
            calamine::Data::String("2026-11-09 12:00:00".to_string()),
            "timestamp read back as string"
        );
        assert_eq!(
            row[6],
            calamine::Data::String("{\"k\":1}".to_string()),
            "json read back as string"
        );
        assert_eq!(row[7], calamine::Data::Empty, "null read back as empty");
        assert_eq!(
            row[8],
            calamine::Data::String("1000000000000000".to_string()),
            "over-limit integer read back as a string, not a rounded number"
        );
    }

    #[test]
    fn xlsx_big_integers_become_strings_not_rounded_numbers() {
        let dest = temp_xlsx("bigint.xlsx");
        let mut fmt = XlsxFormatter::new("t", dest.clone());
        fmt.begin(&[col("n", "int8")]).unwrap();
        fmt.rows(&[vec![Some(Value::Integer(999_999_999_999_999))]])
            .unwrap();
        fmt.rows(&[vec![Some(Value::Integer(1_000_000_000_000_000))]])
            .unwrap();
        fmt.finish().unwrap();
        let text = xlsx_cell_text(&dest);
        assert!(
            text.contains("<v>999999999999999</v>"),
            "at-limit integer stays numeric: {text}"
        );
        assert!(
            text.contains("1000000000000000") && !text.contains("<v>1000000000000000</v>"),
            "over-limit integer stored as string, not a rounded number: {text}"
        );
    }

    #[test]
    fn xlsx_i64_min_becomes_string_without_panic_or_precision_loss() {
        // i64::MIN used to panic in write_cell via n.abs(); it must now
        // degrade to a string cell, keeping every digit.
        let dest = temp_xlsx("i64min.xlsx");
        let mut fmt = XlsxFormatter::new("t", dest.clone());
        fmt.begin(&[col("n", "int8")]).unwrap();
        fmt.rows(&[vec![Some(Value::Integer(i64::MIN))]]).unwrap();
        fmt.rows(&[vec![Some(Value::Integer(-999_999_999_999_999))]])
            .unwrap();
        fmt.finish().unwrap();
        let text = xlsx_cell_text(&dest);
        assert!(
            text.contains("-9223372036854775808") && !text.contains("<v>-9223372036854775808</v>"),
            "i64::MIN stored as string, not a number: {text}"
        );
        assert!(
            text.contains("<v>-999999999999999</v>"),
            "at-limit negative integer stays numeric: {text}"
        );
    }

    #[test]
    fn xlsx_rejects_second_header_and_rows_before_header() {
        let dest = temp_xlsx("order.xlsx");
        let mut fmt = XlsxFormatter::new("t", dest.clone());
        let err = fmt.rows(&[vec![Some(Value::Integer(1))]]).unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(ref m) if m.contains("before the column header")),
            "unexpected: {err:?}"
        );
        fmt.begin(&[col("id", "int4")]).unwrap();
        let err = fmt.begin(&[col("id", "int4")]).unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(ref m) if m.contains("second column header")),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn xlsx_rejects_overlong_cells_and_too_many_columns() {
        let dest = temp_xlsx("limits.xlsx");
        let mut fmt = XlsxFormatter::new("t", dest.clone());
        fmt.begin(&[col("text", "text")]).unwrap();
        let err = fmt
            .rows(&[vec![Some(Value::String("x".repeat(32_768)))]])
            .unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(ref m) if m.contains("32767")),
            "unexpected: {err:?}"
        );

        let dest2 = temp_xlsx("wide.xlsx");
        let mut wide = XlsxFormatter::new("t", dest2);
        let cols: Vec<ColumnInfo> = (0..16_385).map(|i| col(&format!("c{i}"), "text")).collect();
        let err = wide.begin(&cols).unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(ref m) if m.contains("16384")),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn xlsx_row_limit_counts_header_and_rejects_row_1048577() {
        let dest = temp_xlsx("rowlimit.xlsx");
        let mut fmt = XlsxFormatter::new("t", dest.clone());
        fmt.begin(&[col("id", "int4")]).unwrap();
        // begin() writes the header at row 0, so the next data row is 1.
        assert_eq!(fmt.test_next_row(), 1, "header occupies row 0");
        fmt.test_set_next_row(XLSX_MAX_ROWS - 1);
        fmt.rows(&[vec![Some(Value::Integer(1))]]).unwrap();
        let err = fmt.rows(&[vec![Some(Value::Integer(2))]]).unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(ref m) if m.contains("1048576")),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn xlsx_finish_is_idempotent_and_empty_export_still_saves() {
        let dest = temp_xlsx("empty.xlsx");
        let mut fmt = XlsxFormatter::new("t", dest.clone());
        fmt.begin(&[col("id", "int4")]).unwrap();
        fmt.finish().unwrap();
        fmt.finish().unwrap();
        assert!(dest.exists(), "empty export still produces a file");
        assert!(xlsx_cell_text(&dest).contains("id"), "header present");
    }

    #[test]
    fn xlsx_sheet_name_uses_sanitized_table_name() {
        let dest = temp_xlsx("name.xlsx");
        let fmt = XlsxFormatter::new("we:ird/name", dest);
        assert_eq!(fmt.test_sheet_name(), "we_ird_name");
    }

    #[test]
    fn xlsx_finish_to_unwritable_path_is_an_internal_error() {
        // The save failure maps through the catch-all xlsx_error arm, not
        // the two friendly limit arms.
        let dest = std::path::PathBuf::from("/nonexistent-dir-xyz/out.xlsx");
        let mut fmt = XlsxFormatter::new("t", dest);
        fmt.begin(&[col("id", "int4")]).unwrap();
        let err = fmt.finish().unwrap_err();
        assert!(
            matches!(err, ExportError::Internal(ref m) if m.contains("XLSX write failed")),
            "unexpected: {err:?}"
        );
    }
}

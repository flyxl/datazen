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

use std::path::PathBuf;

use datazen_driver_api::{ColumnInfo, Value};
use rust_xlsxwriter::{Workbook, XlsxError};

use super::{bytes_to_hex, ExportError};

/// Excel worksheet limit, header row included: at most this many rows total.
pub const XLSX_MAX_ROWS: u32 = 1_048_576;
/// Excel worksheet limit on columns.
pub const XLSX_MAX_COLUMNS: usize = 16_384;
/// Excel per-cell string limit, in characters.
pub const XLSX_MAX_CELL_CHARS: usize = 32_767;
/// Excel worksheet-name limit, in characters.
pub const XLSX_MAX_SHEET_NAME_CHARS: usize = 31;
/// Excel numbers are f64; integers beyond ±this lose precision silently,
/// so they are written as strings instead.
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
                if n.abs() <= XLSX_MAX_EXACT_INTEGER {
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
#[cfg(test)]
impl XlsxFormatter {
    pub(crate) fn test_sheet_name(&self) -> &str {
        &self.sheet_name
    }
}

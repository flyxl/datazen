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

#[cfg(any(feature = "export-xlsx", feature = "export-parquet"))]
use std::path::PathBuf;

#[cfg(any(feature = "export-xlsx", feature = "export-parquet"))]
use super::bytes_to_hex;
use super::ExportError;
use datazen_driver_api::{ColumnInfo, Value};
#[cfg(feature = "export-xlsx")]
use rust_xlsxwriter::{Workbook, XlsxError};

#[cfg(feature = "export-parquet")]
use std::fs::File;
#[cfg(feature = "export-parquet")]
use std::sync::Arc;

#[cfg(feature = "export-parquet")]
use super::parquet_schema::infer_schema;
#[cfg(feature = "export-parquet")]
use arrow::array::{
    ArrayRef, BinaryArray, BooleanArray, Float32Array, Float64Array, Int16Array, Int32Array,
    Int64Array, Int8Array, StringArray, UInt8Array,
};
#[cfg(feature = "export-parquet")]
use arrow::datatypes::{DataType, Field, SchemaRef};
#[cfg(feature = "export-parquet")]
use arrow::error::ArrowError;
#[cfg(feature = "export-parquet")]
use arrow::record_batch::RecordBatch;
#[cfg(feature = "export-parquet")]
use parquet::arrow::arrow_writer::ArrowWriter;
#[cfg(feature = "export-parquet")]
use parquet::basic::Compression;
#[cfg(feature = "export-parquet")]
use parquet::errors::ParquetError;
#[cfg(feature = "export-parquet")]
use parquet::file::properties::WriterProperties;

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

/// Row-group ceiling for the Parquet writer (PRD F4): far below the 1,048,576
/// default so the writer's buffering stays bounded even though upstream feeds
/// it in ≤500-row batches. Row groups split at this boundary.
#[cfg(feature = "export-parquet")]
pub const PARQUET_ROW_GROUP_ROWS: usize = 100_000;

/// Map an `ArrowWriter` failure onto the export error envelope (Parquet has no
/// user-hittable limits of its own; library failures are internal).
#[cfg(feature = "export-parquet")]
fn parquet_error(err: ParquetError) -> ExportError {
    ExportError::Internal(format!("Parquet write failed: {err}"))
}

/// Map a `RecordBatch` construction failure the same way (arrow-side errors
/// share the envelope; the batch cannot disagree with the schema that built
/// its arrays, so this is belt-and-braces).
#[cfg(feature = "export-parquet")]
fn arrow_error(err: ArrowError) -> ExportError {
    ExportError::Internal(format!("Parquet write failed: {err}"))
}

/// Human name of a `Value` variant, for type-conflict messages.
#[cfg(feature = "export-parquet")]
fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "NULL",
        Value::Bool(_) => "Bool",
        Value::Integer(_) => "Integer",
        Value::Float(_) => "Float",
        Value::String(_) => "String",
        Value::Bytes(_) => "Bytes",
        Value::Timestamp(_) => "Timestamp",
        Value::Json(_) => "Json",
    }
}

/// The schema was fixed from the first batch; a later batch that cannot fit
/// it fails the export naming the column and the offending kind (PRD F3:
/// never silently rewrite, never silently NULL out).
#[cfg(feature = "export-parquet")]
fn type_conflict(column: &str, expected: &str, value: &Value) -> ExportError {
    ExportError::Validation(format!(
        "Parquet column {column:?} expects {expected} but a later batch carries a {} value; \
         the schema was fixed from the first batch",
        value_kind(value)
    ))
}

/// One column of one batch → one Arrow array, transposed per the fixed schema
/// type. NULL (and a short row's missing tail) becomes a null slot; a value
/// that cannot fit the fixed type is a conflict error, not a silent rewrite.
#[cfg(feature = "export-parquet")]
fn parquet_column_array(
    index: usize,
    field: &Field,
    rows: &[Vec<Option<Value>>],
) -> Result<ArrayRef, ExportError> {
    let name = field.name().as_str();
    match field.data_type() {
        DataType::Int16 => {
            let mut values = Vec::with_capacity(rows.len());
            for row in rows {
                values.push(
                    int_cell(name, "int16", row.get(index))?
                        .map(|n| i16::try_from(n).map_err(|_| out_of_range(name, "int16", n)))
                        .transpose()?,
                );
            }
            Ok(Arc::new(Int16Array::from(values)))
        }
        DataType::Int32 => {
            let mut values = Vec::with_capacity(rows.len());
            for row in rows {
                values.push(
                    int_cell(name, "int32", row.get(index))?
                        .map(|n| i32::try_from(n).map_err(|_| out_of_range(name, "int32", n)))
                        .transpose()?,
                );
            }
            Ok(Arc::new(Int32Array::from(values)))
        }
        DataType::Int64 => {
            let mut values = Vec::with_capacity(rows.len());
            for row in rows {
                values.push(int_cell(name, "int64", row.get(index))?);
            }
            Ok(Arc::new(Int64Array::from(values)))
        }
        DataType::Int8 => {
            let mut values = Vec::with_capacity(rows.len());
            for row in rows {
                values.push(
                    int_cell(name, "int8", row.get(index))?
                        .map(|n| i8::try_from(n).map_err(|_| out_of_range(name, "int8", n)))
                        .transpose()?,
                );
            }
            Ok(Arc::new(Int8Array::from(values)))
        }
        DataType::UInt8 => {
            let mut values = Vec::with_capacity(rows.len());
            for row in rows {
                values.push(
                    int_cell(name, "uint8", row.get(index))?
                        .map(|n| u8::try_from(n).map_err(|_| out_of_range(name, "uint8", n)))
                        .transpose()?,
                );
            }
            Ok(Arc::new(UInt8Array::from(values)))
        }
        DataType::Float32 => {
            let mut values = Vec::with_capacity(rows.len());
            for row in rows {
                values.push(float_cell(name, "float32", row.get(index))?);
            }
            Ok(Arc::new(Float32Array::from(
                values
                    .into_iter()
                    .map(|v| v.map(|f| f as f32))
                    .collect::<Vec<_>>(),
            )))
        }
        DataType::Float64 => {
            let mut values = Vec::with_capacity(rows.len());
            for row in rows {
                values.push(float_cell(name, "float64", row.get(index))?);
            }
            Ok(Arc::new(Float64Array::from(values)))
        }
        DataType::Boolean => {
            let mut values = Vec::with_capacity(rows.len());
            for row in rows {
                values.push(match row.get(index).and_then(|v| v.as_ref()) {
                    None | Some(Value::Null) => None,
                    Some(Value::Bool(b)) => Some(*b),
                    Some(other) => return Err(type_conflict(name, "boolean", other)),
                });
            }
            Ok(Arc::new(BooleanArray::from(values)))
        }
        DataType::Binary => {
            let mut values: Vec<Option<&[u8]>> = Vec::with_capacity(rows.len());
            for row in rows {
                values.push(match row.get(index).and_then(|v| v.as_ref()) {
                    None | Some(Value::Null) => None,
                    Some(Value::Bytes(b)) => Some(b.as_slice()),
                    Some(other) => return Err(type_conflict(name, "binary", other)),
                });
            }
            Ok(Arc::new(BinaryArray::from(values)))
        }
        // Utf8 is the everything-fits fallback (PRD F3): every scalar renders
        // as text — numbers via Display, Bytes as the bare hex string (same
        // convention as the XLSX cells, no CSV round-trip marker), Json via
        // to_string. No value kind conflicts with Utf8.
        DataType::Utf8 => {
            let mut values = Vec::with_capacity(rows.len());
            for row in rows {
                values.push(match row.get(index).and_then(|v| v.as_ref()) {
                    None | Some(Value::Null) => None,
                    Some(Value::String(s)) | Some(Value::Timestamp(s)) => Some(s.clone()),
                    Some(Value::Json(j)) => Some(j.to_string()),
                    Some(Value::Integer(n)) => Some(n.to_string()),
                    Some(Value::Float(f)) => Some(f.to_string()),
                    Some(Value::Bool(b)) => Some(b.to_string()),
                    Some(Value::Bytes(b)) => Some(bytes_to_hex(b)),
                });
            }
            Ok(Arc::new(StringArray::from(values)))
        }
        // `infer_schema` only ever produces the types above; anything else is
        // an internal invariant break, not a user error.
        other => Err(ExportError::Internal(format!(
            "Parquet write failed: unsupported inferred column type {other:?}"
        ))),
    }
}

/// The integer a typed column expects, or a conflict error naming the column
/// and the offending value kind (PRD F3).
#[cfg(feature = "export-parquet")]
fn int_cell(
    column: &str,
    expected: &str,
    cell: Option<&Option<Value>>,
) -> Result<Option<i64>, ExportError> {
    match cell.and_then(|value| value.as_ref()) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Integer(n)) => Ok(Some(*n)),
        Some(other) => Err(type_conflict(column, expected, other)),
    }
}

/// The float a typed column expects, or a conflict error.
#[cfg(feature = "export-parquet")]
fn float_cell(
    name: &str,
    expected: &str,
    cell: Option<&Option<Value>>,
) -> Result<Option<f64>, ExportError> {
    match cell.and_then(|value| value.as_ref()) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Float(f)) => Ok(Some(*f)),
        Some(other) => Err(type_conflict(name, expected, other)),
    }
}

/// An integer outside the fixed width (only reachable via driver anomalies;
/// real drivers never send out-of-range integers for their own column types).
#[cfg(feature = "export-parquet")]
fn out_of_range(column: &str, expected: &str, n: i64) -> ExportError {
    ExportError::Validation(format!(
        "Parquet column {column:?} ({expected}) cannot hold the integer {n}"
    ))
}

/// Parquet writer over `parquet::arrow::ArrowWriter`, lazily created: `begin()`
/// only sees column metadata, never values, so the schema — and the writer —
/// cannot be fixed before the first batch (PRD F3). Memory stays bounded by
/// the explicit row-group limit, not the 1,048,576 default.
#[cfg(feature = "export-parquet")]
pub struct ParquetFormatter {
    dialect: Option<String>,
    dest: PathBuf,
    columns: Vec<ColumnInfo>,
    /// Fixed together with the writer: on the first non-empty `rows()` batch,
    /// or in `finish()` for a zero-row table.
    schema: Option<SchemaRef>,
    writer: Option<ArrowWriter<File>>,
    begun: bool,
    finished: bool,
}

#[cfg(feature = "export-parquet")]
impl ParquetFormatter {
    /// `dialect` is `ExportTablesRequest.database_type`, verbatim — the schema
    /// inference's dialect key (None when the request omits it).
    pub fn new(dialect: Option<String>, dest: PathBuf) -> Self {
        Self {
            dialect,
            dest,
            columns: Vec::new(),
            schema: None,
            writer: None,
            begun: false,
            finished: false,
        }
    }

    /// Fix the schema from `first_batch` and create the `ArrowWriter` over the
    /// destination. Called on the first non-empty `rows()` batch — and from
    /// `finish()` with an empty batch for a zero-row table, where the schema
    /// is the dialect mapping plus the Utf8 fallback (PRD F2).
    fn ensure_writer(&mut self, first_batch: &[Vec<Option<Value>>]) -> Result<(), ExportError> {
        if self.writer.is_some() {
            return Ok(());
        }
        let schema: SchemaRef = Arc::new(infer_schema(
            &self.columns,
            self.dialect.as_deref(),
            first_batch,
        ));
        let file = File::create(&self.dest).map_err(|err| {
            ExportError::Internal(format!(
                "Parquet write failed: cannot create {}: {err}",
                self.dest.display()
            ))
        })?;
        let properties = WriterProperties::builder()
            .set_max_row_group_row_count(Some(PARQUET_ROW_GROUP_ROWS))
            .set_compression(Compression::SNAPPY)
            .build();
        let writer =
            ArrowWriter::try_new(file, schema.clone(), Some(properties)).map_err(parquet_error)?;
        self.schema = Some(schema);
        self.writer = Some(writer);
        Ok(())
    }
}

#[cfg(feature = "export-parquet")]
impl BinaryFormatter for ParquetFormatter {
    fn begin(&mut self, columns: &[ColumnInfo]) -> Result<(), ExportError> {
        if self.begun {
            return Err(ExportError::Validation(
                "Parquet export received a second column header".into(),
            ));
        }
        self.columns = columns.to_vec();
        self.begun = true;
        Ok(())
    }

    fn rows(&mut self, rows: &[Vec<Option<Value>>]) -> Result<(), ExportError> {
        if !self.begun {
            return Err(ExportError::Validation(
                "Parquet export received rows before the column header".into(),
            ));
        }
        // An empty batch carries no inference signal: never let it fix the
        // schema (that is what the Utf8 fallback exists for, and an empty
        // table must keep the mapping-based schema in finish()).
        if rows.is_empty() {
            return Ok(());
        }
        self.ensure_writer(rows)?;
        let schema = self.schema.clone().expect("ensure_writer fixed the schema");
        let arrays = schema
            .fields()
            .iter()
            .enumerate()
            .map(|(index, field)| parquet_column_array(index, field, rows))
            .collect::<Result<Vec<_>, _>>()?;
        let batch = RecordBatch::try_new(schema, arrays).map_err(arrow_error)?;
        self.writer
            .as_mut()
            .expect("ensure_writer created the writer")
            .write(&batch)
            .map_err(parquet_error)?;
        Ok(())
    }

    fn finish(&mut self) -> Result<(), ExportError> {
        if self.finished {
            return Ok(());
        }
        if !self.begun {
            // A Parquet file needs at least one column; unlike XLSX there is
            // no such thing as a column-less workbook to save. This is only
            // reachable when the stream never produced a statement at all.
            return Err(ExportError::Validation(
                "Parquet export finished without a column header".into(),
            ));
        }
        self.finished = true;
        // Zero-row table: fix the schema from the mapping + Utf8 fallback
        // and close — a legal schema-only file, not an empty byte stream.
        self.ensure_writer(&[])?;
        let writer = self
            .writer
            .take()
            .expect("ensure_writer created the writer");
        writer.close().map_err(parquet_error)?;
        Ok(())
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

/// Parquet-side tests (separate from the XLSX `tests` module so each feature
/// gate compiles only what it can build). Read-back goes through the `parquet`
/// crate's own arrow reader — the same parser family pyarrow implements, per
/// the plan's "读回用 parquet crate 的 reader" requirement.
#[cfg(all(test, feature = "export-parquet"))]
mod parquet_tests {
    use super::*;
    use arrow::array::{Array, BinaryArray, BooleanArray, Float64Array, Int64Array, StringArray};
    use arrow::datatypes::DataType;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    fn col(name: &str, data_type: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: data_type.into(),
            nullable: true,
        }
    }

    fn temp_parquet(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("datazen-export-parquet-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("tempdir");
        dir.join(name)
    }

    fn pg_dialect() -> Option<String> {
        Some("postgresql".into())
    }

    fn cleanup(dest: &PathBuf) {
        let _ = std::fs::remove_dir_all(dest.parent().expect("tempdir parent"));
    }

    /// Read a written file back: field list plus every record batch.
    fn read_parquet(path: &PathBuf) -> (Vec<arrow::datatypes::Field>, Vec<RecordBatch>) {
        let file = std::fs::File::open(path).expect("parquet file exists");
        let builder = ParquetRecordBatchReaderBuilder::try_new(file).expect("open parquet");
        let fields = builder
            .schema()
            .fields()
            .iter()
            .map(|field| field.as_ref().clone())
            .collect();
        let reader = builder.build().expect("build parquet reader");
        let batches = reader
            .collect::<Result<Vec<_>, _>>()
            .expect("read parquet batches");
        (fields, batches)
    }

    fn total_rows(batches: &[RecordBatch]) -> usize {
        batches.iter().map(|batch| batch.num_rows()).sum()
    }

    #[test]
    fn parquet_roundtrip_columns_values_and_nulls() {
        let dest = temp_parquet("roundtrip.parquet");
        let mut fmt = ParquetFormatter::new(pg_dialect(), dest.clone());
        fmt.begin(&[
            col("id", "int8"),
            col("score", "float8"),
            col("name", "text"),
            col("active", "bool"),
            col("payload", "bytea"),
            col("created", "timestamptz"),
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

        let (fields, batches) = read_parquet(&dest);
        let expected = [
            (0, "id", DataType::Int64),
            (1, "score", DataType::Float64),
            (2, "name", DataType::Utf8),
            (3, "active", DataType::Boolean),
            (4, "payload", DataType::Binary),
            (5, "created", DataType::Utf8),
            (6, "meta", DataType::Utf8),
            (7, "note", DataType::Utf8),
        ];
        for (index, name, data_type) in expected {
            assert_eq!(fields[index].name(), name, "column {index} name");
            assert_eq!(fields[index].data_type(), &data_type, "column {name}");
        }
        assert_eq!(total_rows(&batches), 1);
        let batch = &batches[0];
        assert_eq!(
            batch
                .column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .expect("int64")
                .value(0),
            42
        );
        assert_eq!(
            batch
                .column(1)
                .as_any()
                .downcast_ref::<Float64Array>()
                .expect("float64")
                .value(0),
            1.5
        );
        assert_eq!(
            batch
                .column(2)
                .as_any()
                .downcast_ref::<StringArray>()
                .expect("utf8")
                .value(0),
            "a,b"
        );
        assert_eq!(
            batch
                .column(3)
                .as_any()
                .downcast_ref::<BooleanArray>()
                .expect("bool")
                .value(0),
            true
        );
        assert_eq!(
            batch
                .column(4)
                .as_any()
                .downcast_ref::<BinaryArray>()
                .expect("binary")
                .value(0),
            &[0x00, 0xff][..]
        );
        assert_eq!(
            batch
                .column(5)
                .as_any()
                .downcast_ref::<StringArray>()
                .expect("utf8")
                .value(0),
            "2026-11-09 12:00:00"
        );
        assert_eq!(
            batch
                .column(6)
                .as_any()
                .downcast_ref::<StringArray>()
                .expect("utf8")
                .value(0),
            "{\"k\":1}"
        );
        // No CSV round-trip marker anywhere in the rendered text.
        assert!(!batch
            .column(6)
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("utf8")
            .value(0)
            .contains("datazen:bytes:hex:"));
        assert!(batch.column(7).is_null(0), "the None cell is a null slot");
        cleanup(&dest);
    }

    #[test]
    fn parquet_empty_table_writes_schema_only_file() {
        let dest = temp_parquet("empty.parquet");
        let mut fmt = ParquetFormatter::new(pg_dialect(), dest.clone());
        fmt.begin(&[col("id", "int8"), col("weird", "money")])
            .unwrap();
        fmt.finish().unwrap();
        assert!(dest.exists(), "a schema-only file, not zero bytes");
        let bytes = std::fs::read(&dest).expect("read parquet");
        assert_eq!(&bytes[..4], b"PAR1", "parquet magic at head");
        assert_eq!(&bytes[bytes.len() - 4..], b"PAR1", "parquet magic at tail");
        let (fields, batches) = read_parquet(&dest);
        assert_eq!(total_rows(&batches), 0, "zero rows");
        // The schema is the dialect mapping plus the Utf8 fallback, with no
        // values to consult.
        assert_eq!(fields[0].name(), "id");
        assert_eq!(fields[0].data_type(), &DataType::Int64);
        assert_eq!(fields[1].name(), "weird");
        assert_eq!(fields[1].data_type(), &DataType::Utf8);
        cleanup(&dest);
    }

    #[test]
    fn parquet_first_batch_all_null_then_strings() {
        let dest = temp_parquet("allnull.parquet");
        let mut fmt = ParquetFormatter::new(pg_dialect(), dest.clone());
        fmt.begin(&[col("a", "money"), col("b", "money")]).unwrap();
        // An all-NULL first batch fixes Utf8 for both columns (no signal).
        fmt.rows(&[vec![None, None]]).unwrap();
        fmt.rows(&[vec![Some(Value::String("x".into())), None]])
            .unwrap();
        fmt.finish().unwrap();
        let (fields, batches) = read_parquet(&dest);
        assert_eq!(fields[0].data_type(), &DataType::Utf8);
        assert_eq!(fields[1].data_type(), &DataType::Utf8);
        assert_eq!(total_rows(&batches), 2);
        let batch = &batches[0];
        assert!(batch.column(0).is_null(0));
        assert!(batch.column(1).is_null(0));
        assert_eq!(
            batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .expect("utf8")
                .value(1),
            "x"
        );
        assert!(batch.column(1).is_null(1));
        cleanup(&dest);
    }

    #[test]
    fn parquet_multiple_batches_write_all_rows_in_order() {
        let dest = temp_parquet("batches.parquet");
        let mut fmt = ParquetFormatter::new(pg_dialect(), dest.clone());
        fmt.begin(&[col("id", "int8")]).unwrap();
        for batch_index in 0..3u64 {
            let rows: Vec<Vec<Option<Value>>> = (0..3)
                .map(|i| vec![Some(Value::Integer((batch_index * 3 + i) as i64))])
                .collect();
            fmt.rows(&rows).unwrap();
        }
        fmt.finish().unwrap();
        let (fields, batches) = read_parquet(&dest);
        assert_eq!(fields[0].data_type(), &DataType::Int64);
        assert_eq!(total_rows(&batches), 9);
        // Whatever batch boundaries the reader produces, the values arrive in
        // write order.
        let mut seen = Vec::new();
        for batch in &batches {
            let column = batch
                .column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .expect("int64");
            for row in 0..column.len() {
                seen.push(column.value(row));
            }
        }
        assert_eq!(seen, (0..9).collect::<Vec<i64>>());
        cleanup(&dest);
    }

    #[test]
    fn parquet_row_group_limit_and_snappy_compression() {
        let dest = temp_parquet("rowgroups.parquet");
        let mut fmt = ParquetFormatter::new(pg_dialect(), dest.clone());
        fmt.begin(&[col("id", "int8")]).unwrap();
        // 200 full 500-row batches + 1 final row = 100,001 rows: the explicit
        // 100,000-row ceiling must split this into two row groups.
        for chunk in 0..200i64 {
            let rows: Vec<Vec<Option<Value>>> = (0..500i64)
                .map(|i| vec![Some(Value::Integer(chunk * 500 + i))])
                .collect();
            fmt.rows(&rows).unwrap();
        }
        fmt.rows(&[vec![Some(Value::Integer(-1))]]).unwrap();
        fmt.finish().unwrap();

        let file = std::fs::File::open(&dest).expect("parquet exists");
        let builder = ParquetRecordBatchReaderBuilder::try_new(file).expect("open parquet");
        let metadata = builder.metadata();
        assert_eq!(metadata.num_row_groups(), 2, "split at the 100,000 limit");
        assert_eq!(metadata.row_group(0).num_rows(), 100_000);
        assert_eq!(metadata.row_group(1).num_rows(), 1);
        assert_eq!(
            metadata.row_group(0).column(0).compression(),
            Compression::SNAPPY
        );
        drop(builder);

        let (_, batches) = read_parquet(&dest);
        assert_eq!(total_rows(&batches), 100_001);
        let mut first: Option<i64> = None;
        let mut last = 0;
        for batch in &batches {
            let column = batch
                .column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .expect("int64");
            if column.len() > 0 {
                if first.is_none() {
                    first = Some(column.value(0));
                }
                last = column.value(column.len() - 1);
            }
        }
        assert_eq!(first, Some(0));
        assert_eq!(last, -1);
        cleanup(&dest);
    }

    #[test]
    fn parquet_finish_is_idempotent() {
        let dest = temp_parquet("idempotent.parquet");
        let mut fmt = ParquetFormatter::new(pg_dialect(), dest.clone());
        fmt.begin(&[col("id", "int8")]).unwrap();
        fmt.rows(&[vec![Some(Value::Integer(7))]]).unwrap();
        fmt.finish().unwrap();
        // The post-fetch fallback in write_binary_file may call finish() a
        // second time; that must be a no-op, not an error.
        fmt.finish().unwrap();
        let (_, batches) = read_parquet(&dest);
        assert_eq!(total_rows(&batches), 1);
        cleanup(&dest);
    }

    #[test]
    fn parquet_protocol_errors_are_validations() {
        let dest = temp_parquet("protocol.parquet");
        let mut fmt = ParquetFormatter::new(None, dest.clone());
        // Rows before the header.
        let err = fmt.rows(&[vec![Some(Value::Integer(1))]]).unwrap_err();
        assert!(matches!(err, ExportError::Validation(_)), "got {err:?}");
        fmt.begin(&[col("id", "int8")]).unwrap();
        // Second header.
        let err = fmt.begin(&[col("id", "int8")]).unwrap_err();
        assert!(matches!(err, ExportError::Validation(_)), "got {err:?}");
        // finish() without a header: no columns, no file.
        let mut fresh = ParquetFormatter::new(None, dest.clone());
        let err = fresh.finish().unwrap_err();
        assert!(matches!(err, ExportError::Validation(_)), "got {err:?}");
        assert!(!dest.exists(), "a column-less finish writes nothing");
        cleanup(&dest);
    }

    #[test]
    fn parquet_type_conflict_names_column_and_value_kind() {
        let dest = temp_parquet("conflict.parquet");
        let mut fmt = ParquetFormatter::new(pg_dialect(), dest.clone());
        fmt.begin(&[col("id", "int8"), col("name", "text")])
            .unwrap();
        fmt.rows(&[vec![
            Some(Value::Integer(1)),
            Some(Value::String("a".into())),
        ]])
        .unwrap();
        let err = fmt
            .rows(&[vec![Some(Value::String("boom".into())), None]])
            .unwrap_err();
        match err {
            ExportError::Validation(message) => {
                assert!(message.contains("id"), "names the column: {message}");
                assert!(message.contains("int64"), "names the type: {message}");
                assert!(message.contains("String"), "names the kind: {message}");
            }
            other => panic!("expected a Validation conflict, got {other:?}"),
        }
        // write_binary_file (T5) fails the whole run and removes the partial
        // file — that contract is covered there, not here.
        cleanup(&dest);
    }

    #[test]
    fn parquet_utf8_columns_render_every_value_kind() {
        let dest = temp_parquet("utf8render.parquet");
        let mut fmt = ParquetFormatter::new(pg_dialect(), dest.clone());
        // "text" maps to Utf8; "money" falls to value inference, and its mixed
        // first batch fixes Utf8 for it too.
        fmt.begin(&[col("blob_col", "text"), col("misc", "money")])
            .unwrap();
        fmt.rows(&[
            vec![Some(Value::Integer(42)), Some(Value::Float(1.5))],
            vec![
                Some(Value::Bool(true)),
                Some(Value::Bytes(vec![0x00, 0xff])),
            ],
            vec![
                Some(Value::Timestamp("2026-11-09".into())),
                Some(Value::Json(serde_json::json!({"k": 1}))),
            ],
            vec![Some(Value::String("plain".into())), Some(Value::Null)],
        ])
        .unwrap();
        fmt.finish().unwrap();
        let (fields, batches) = read_parquet(&dest);
        assert_eq!(fields[0].data_type(), &DataType::Utf8);
        assert_eq!(fields[1].data_type(), &DataType::Utf8);
        assert_eq!(total_rows(&batches), 4);
        // The reader may re-batch; collect through every batch instead of one.
        let mut column_values = [Vec::new(), Vec::new()];
        for batch in &batches {
            for (column_index, bucket) in column_values.iter_mut().enumerate() {
                let column = batch
                    .column(column_index)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .expect("utf8");
                for row in 0..column.len() {
                    if column.is_null(row) {
                        bucket.push(None);
                    } else {
                        bucket.push(Some(column.value(row).to_string()));
                    }
                }
            }
        }
        assert_eq!(
            column_values[0],
            vec![
                Some("42".to_string()),
                Some("true".to_string()),
                Some("2026-11-09".to_string()),
                Some("plain".to_string()),
            ]
        );
        assert_eq!(
            column_values[1],
            vec![
                Some("1.5".to_string()),
                Some("00ff".to_string()),
                Some("{\"k\":1}".to_string()),
                None,
            ]
        );
        cleanup(&dest);
    }

    #[test]
    fn parquet_no_dialect_infers_unmapped_types_from_values() {
        let dest = temp_parquet("infer.parquet");
        let mut fmt = ParquetFormatter::new(None, dest.clone());
        fmt.begin(&[col("id", "totally_unknown")]).unwrap();
        fmt.rows(&[vec![Some(Value::Integer(5))]]).unwrap();
        fmt.finish().unwrap();
        let (fields, batches) = read_parquet(&dest);
        assert_eq!(fields[0].data_type(), &DataType::Int64);
        assert_eq!(total_rows(&batches), 1);
        cleanup(&dest);
    }

    /// Manual acceptance helper (plan §T7): writes one representative file
    /// covering every column type and value kind — plus nulls, unicode and
    /// row-group compression — to a stable path under the temp dir, for
    /// read-back by pyarrow and DuckDB outside the Rust suite. `#[ignore]`d
    /// so normal runs and CI suites skip it; run explicitly with
    /// `cargo test parquet_acceptance -- --ignored --nocapture` and point the
    /// external readers at the printed path. The file self-verifies through
    /// the parquet reader first, so a manual run is itself an acceptance.
    #[test]
    #[ignore = "manual acceptance helper (plan §T7): run with --ignored to (re)write the file"]
    fn parquet_acceptance_dump_writes_representative_file() {
        let dir = std::env::temp_dir().join("datazen-parquet-acceptance");
        std::fs::create_dir_all(&dir).expect("create acceptance dir");
        let dest = dir.join("representative.parquet");
        let mut fmt = ParquetFormatter::new(pg_dialect(), dest.clone());
        fmt.begin(&[
            col("id", "int4"),
            col("score", "float8"),
            col("name", "text"),
            col("active", "bool"),
            col("payload", "bytea"),
            col("created", "timestamptz"),
            col("meta", "jsonb"),
        ])
        .unwrap();
        fmt.rows(&[
            vec![
                Some(Value::Integer(1)),
                Some(Value::Float(1.5)),
                Some(Value::String("alpha, βeta".into())),
                Some(Value::Bool(true)),
                Some(Value::Bytes(vec![0x00, 0xff, 0xfe])),
                Some(Value::Timestamp("2026-11-12 08:00:00".into())),
                Some(Value::Json(serde_json::json!({"k": 1}))),
            ],
            vec![
                Some(Value::Integer(-42)),
                Some(Value::Float(-0.25)),
                Some(Value::String("quote \" and | pipe".into())),
                Some(Value::Bool(false)),
                None,
                Some(Value::Timestamp("2026-11-12 09:30:00".into())),
                None,
            ],
            vec![None, None, None, None, None, None, None],
        ])
        .unwrap();
        fmt.finish().unwrap();
        let (fields, batches) = read_parquet(&dest);
        assert_eq!(
            total_rows(&batches),
            3,
            "three rows including the all-null row"
        );
        assert_eq!(fields[0].data_type(), &DataType::Int32);
        assert_eq!(fields[4].data_type(), &DataType::Binary);
        let last = batches.last().expect("at least one batch");
        for column in 0..7 {
            assert!(
                last.column(column).is_null(2),
                "column {column} row 3 is null"
            );
        }
        println!("parquet acceptance file: {}", dest.display());
        // Deliberately not cleaned up: the point is handing it to pyarrow /
        // DuckDB, which delete it after recording their read-back results.
    }
}

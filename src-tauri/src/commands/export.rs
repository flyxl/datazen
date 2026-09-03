//! Rust-side streaming table export.
//!
//! After the user picks a destination file, this module streams each selected
//! table's query directly on the Rust side (`driver.query_stream`), formats rows
//! in batches (CSV / JSON / SQL INSERT) and writes them straight to disk —
//! avoiding holding the whole table in memory and the JS↔Rust data round-trip.
//!
//! Product behavior:
//! - Output is a single file or a ZIP archive (multiple files).
//! - ZIP/multi-file: each file is written to a temp dir, then zipped, then the
//!   temp dir is deleted.
//! - DDL/create-table text is kept when the mode includes structure.
//! - SQL export wraps each table in a single BEGIN/COMMIT (never per-INSERT).

use std::collections::HashSet;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use datazen_driver_api::{QueryStreamCallback, QueryStreamEvent, Value};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::error::{CmdExt, CommandError};
use super::AppState;

pub const SQL_INSERT_BATCH_SIZE: usize = 500;
const APPEND_FLUSH_BYTES: usize = 64 * 1024;
const ALLOWED_EXPORT_EXTS: &[&str] = &["sql", "csv", "json", "zip"];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableExportInput {
    pub table_name: String,
    /// Column names to SELECT (empty => SELECT * and derive from result).
    #[serde(default)]
    pub columns: Vec<String>,
    /// DDL text (CREATE TABLE …); used when the mode includes structure.
    #[serde(default)]
    pub ddl: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportMode {
    StructureOnly,
    DataOnly,
    DataAndStructure,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DataFormat {
    Csv,
    Json,
    SqlInsert,
}

impl DataFormat {
    fn extension(self) -> &'static str {
        match self {
            DataFormat::Csv => "csv",
            DataFormat::Json => "json",
            DataFormat::SqlInsert => "sql",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    Single,
    Zip,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportTablesRequest {
    pub db_session_id: String,
    pub database_type: Option<String>,
    pub mode: ExportMode,
    pub data_format: DataFormat,
    pub output_mode: OutputMode,
    pub tables: Vec<TableExportInput>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportTablesResult {
    Saved(u64),
    Cancelled,
}

/// Progress events emitted while streaming the export to disk. The frontend
/// subscribes to `batch-export-progress` to show live feedback during the
/// (potentially long) data-write phase.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgressEvent {
    /// Table currently being streamed.
    pub table: String,
    /// Cumulative row count written for *this* table so far.
    pub rows_written: u64,
}

// ---------------------------------------------------------------------------
// Pure value/identifier escaping (mirrors src/lib/exportData.ts)
// ---------------------------------------------------------------------------

fn fmt_ident(name: &str, db_type: Option<&str>) -> String {
    let quote = match db_type.map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("mysql") | Some("mariadb") => '`',
        _ => '"',
    };
    format!(
        "{quote}{}{quote}",
        name.replace(quote, &format!("{quote}{quote}"))
    )
}

fn value_as_string(value: &Option<Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::Bool(true)) => "true".into(),
        Some(Value::Bool(false)) => "false".into(),
        Some(Value::Integer(n)) => n.to_string(),
        Some(Value::Float(n)) => n.to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bytes(b)) => String::from_utf8_lossy(b).into_owned(),
        Some(Value::Timestamp(s)) => s.clone(),
        Some(Value::Json(j)) => j.to_string(),
    }
}

fn escape_csv_field(value: &Option<Value>) -> String {
    let s = value_as_string(value);
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        return format!("\"{}\"", s.replace('"', "\"\""));
    }
    s
}

fn json_value(value: Option<Value>) -> serde_json::Value {
    match value {
        None | Some(Value::Null) => serde_json::Value::Null,
        Some(Value::Bool(b)) => serde_json::Value::Bool(b),
        Some(Value::Integer(n)) => serde_json::Value::from(n),
        Some(Value::Float(n)) => serde_json::Value::from(n),
        Some(Value::String(s)) => serde_json::Value::String(s),
        Some(Value::Bytes(b)) => {
            serde_json::Value::String(String::from_utf8_lossy(&b).into_owned())
        }
        Some(Value::Timestamp(s)) => serde_json::Value::String(s),
        Some(Value::Json(j)) => j,
    }
}

/// Build the SELECT SQL for a table export.
pub fn build_select_sql(table: &str, columns: &[String], db_type: Option<&str>) -> String {
    let qtable = fmt_ident(table, db_type);
    if columns.is_empty() {
        return format!("SELECT * FROM {qtable}");
    }
    let cols = columns
        .iter()
        .map(|c| fmt_ident(c, db_type))
        .collect::<Vec<_>>()
        .join(", ");
    format!("SELECT {cols} FROM {qtable}")
}

// ---------------------------------------------------------------------------
// Streamable text formatter
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum FormatterState {
    Csv,
    Json { started: bool },
    SqlInsert { pending: Vec<Vec<Option<Value>>> },
}

/// Formats row batches to text, preserving streaming state across calls.
struct StreamFormatter {
    format: DataFormat,
    table: String,
    database_type: Option<String>,
    state: FormatterState,
}

impl StreamFormatter {
    fn new(format: DataFormat, table: String, database_type: Option<String>) -> Self {
        let state = match format {
            DataFormat::Csv => FormatterState::Csv,
            DataFormat::Json => FormatterState::Json { started: false },
            DataFormat::SqlInsert => FormatterState::SqlInsert {
                pending: Vec::new(),
            },
        };
        StreamFormatter {
            format,
            table,
            database_type,
            state,
        }
    }

    /// Text to write at the very start of a table (JSON `[`, SQL `BEGIN`).
    fn header(&mut self, columns: &[String]) -> String {
        match self.format {
            DataFormat::Json => "[".into(),
            DataFormat::SqlInsert => {
                let re = if matches!(self.database_type.as_deref(), Some("sqlserver")) {
                    "BEGIN TRANSACTION;"
                } else {
                    "BEGIN;"
                };
                format!("{re}\n")
            }
            DataFormat::Csv => {
                let mut out = String::new();
                for (i, c) in columns.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&escape_csv_field(&Some(Value::String(c.clone()))));
                }
                out.push('\n');
                out
            }
        }
    }

    /// Format a batch of rows into text.
    fn rows(&mut self, rows: &[Vec<Option<Value>>], columns: &[String]) -> String {
        match &mut self.state {
            FormatterState::Csv => {
                let mut out = String::new();
                for row in rows {
                    for (i, cell) in row.iter().enumerate() {
                        if i > 0 {
                            out.push(',');
                        }
                        out.push_str(&escape_csv_field(cell));
                    }
                    out.push('\n');
                }
                out
            }
            FormatterState::Json { started } => {
                let mut out = String::new();
                for row in rows {
                    let mut map = serde_json::Map::new();
                    for (i, col) in columns.iter().enumerate() {
                        map.insert(col.clone(), json_value(row.get(i).and_then(|v| v.clone())));
                    }
                    let obj = serde_json::to_string(&serde_json::Value::Object(map))
                        .unwrap_or_else(|_| "null".into());
                    if *started {
                        out.push(',');
                    }
                    *started = true;
                    out.push('\n');
                    out.push_str(&obj);
                }
                out
            }
            FormatterState::SqlInsert { pending } => {
                pending.extend(rows.iter().cloned());
                self.flush_sql(columns)
            }
        }
    }

    fn flush_sql(&mut self, columns: &[String]) -> String {
        let FormatterState::SqlInsert { pending } = &mut self.state else {
            return String::new();
        };
        if pending.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        let mut batch: Vec<Vec<Option<Value>>> = Vec::new();
        for row in pending.drain(..) {
            batch.push(row);
            if batch.len() >= SQL_INSERT_BATCH_SIZE {
                out.push_str(&emit_insert_batch(
                    columns,
                    &self.table,
                    &self.database_type,
                    &batch,
                ));
                batch.clear();
            }
        }
        if !batch.is_empty() {
            out.push_str(&emit_insert_batch(
                columns,
                &self.table,
                &self.database_type,
                &batch,
            ));
        }
        out
    }

    /// Text to write at the very end of a table dump (JSON `]`, SQL `COMMIT`).
    fn tail(&mut self, columns: &[String]) -> String {
        match self.format {
            DataFormat::Json => {
                let FormatterState::Json { started } = &mut self.state else {
                    return String::new();
                };
                if *started {
                    "\n]".into()
                } else {
                    "]".into()
                }
            }
            DataFormat::SqlInsert => {
                let batched = self.flush_sql(columns);
                format!("{batched}COMMIT;\n")
            }
            DataFormat::Csv => String::new(),
        }
    }
}

fn emit_insert_batch(
    columns: &[String],
    table: &str,
    database_type: &Option<String>,
    rows: &[Vec<Option<Value>>],
) -> String {
    let db_type = database_type.as_deref();
    let col_list = columns
        .iter()
        .map(|c| fmt_ident(c, db_type))
        .collect::<Vec<_>>()
        .join(", ");
    let mut out = format!(
        "INSERT INTO {} ({col_list}) VALUES\n",
        fmt_ident(table, db_type)
    );
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            out.push_str(",\n  ");
        } else {
            out.push_str("  ");
        }
        let values = row
            .iter()
            .map(|v| crate::data_sync::sql::format_literal(v))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("({values})"));
    }
    out.push_str(";\n");
    out
}

// ---------------------------------------------------------------------------
// File output plumbing
// ---------------------------------------------------------------------------

/// A buffered file writer that flushes to disk every ~64KB.
struct FileSink {
    writer: BufWriter<std::fs::File>,
    buffer: Vec<u8>,
}

impl FileSink {
    fn create(path: &Path) -> Result<Self, CommandError> {
        let file = std::fs::File::create(path).map_err(CommandError::Io)?;
        Ok(FileSink {
            writer: BufWriter::new(file),
            buffer: Vec::new(),
        })
    }

    fn append_str(&mut self, text: &str) -> Result<(), CommandError> {
        self.append_bytes(text.as_bytes())
    }

    fn append_bytes(&mut self, bytes: &[u8]) -> Result<(), CommandError> {
        self.buffer.extend_from_slice(bytes);
        if self.buffer.len() >= APPEND_FLUSH_BYTES {
            self.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), CommandError> {
        if !self.buffer.is_empty() {
            self.writer
                .write_all(&self.buffer)
                .map_err(CommandError::Io)?;
            self.buffer.clear();
        }
        self.writer.flush().map_err(CommandError::Io)
    }
}

// ---------------------------------------------------------------------------
// File planning (mirrors src/lib/batchExport.ts buildBatchExportFiles)
// ---------------------------------------------------------------------------

/// A planned output file.
enum FilePlan<'a> {
    Ddl {
        filename: String,
        text: String,
    },
    Data {
        filename: String,
        table: &'a TableExportInput,
        format: DataFormat,
        /// DDL text prefixed to the same data file (only for sql_insert + structure).
        ddl_prefix: Option<String>,
    },
}

impl<'a> FilePlan<'a> {
    fn filename(&self) -> &str {
        match self {
            FilePlan::Ddl { filename, .. } | FilePlan::Data { filename, .. } => filename,
        }
    }
}

fn resolve_ddl(table: &TableExportInput) -> String {
    match table.ddl.as_deref() {
        Some(d) if !d.trim().is_empty() => d.to_string(),
        _ => format!("-- DDL unavailable for {}", table.table_name),
    }
}

fn build_file_plans(request: &ExportTablesRequest) -> Vec<FilePlan<'_>> {
    let mut seen = HashSet::new();
    let mut plans = Vec::new();
    for table in &request.tables {
        let base = table.table_name.clone();
        match request.mode {
            ExportMode::StructureOnly => {
                plans.push(FilePlan::Ddl {
                    filename: unique_filename(&mut seen, format!("{base}.sql")),
                    text: resolve_ddl(table),
                });
            }
            ExportMode::DataOnly => {
                plans.push(FilePlan::Data {
                    filename: format!("{base}.{}", request.data_format.extension()),
                    table,
                    format: request.data_format,
                    ddl_prefix: None,
                });
            }
            ExportMode::DataAndStructure => {
                if request.data_format == DataFormat::SqlInsert {
                    plans.push(FilePlan::Data {
                        filename: unique_filename(&mut seen, format!("{base}.sql")),
                        table,
                        format: DataFormat::SqlInsert,
                        ddl_prefix: Some(resolve_ddl(table)),
                    });
                } else {
                    plans.push(FilePlan::Ddl {
                        filename: unique_filename(&mut seen, format!("{base}.sql")),
                        text: resolve_ddl(table),
                    });
                    plans.push(FilePlan::Data {
                        filename: format!("{base}.{}", request.data_format.extension()),
                        table,
                        format: request.data_format,
                        ddl_prefix: None,
                    });
                }
            }
        }
    }
    plans
}

fn unique_filename(seen: &mut HashSet<String>, name: String) -> String {
    if seen.insert(name.clone()) {
        return name;
    }
    let stem = name.trim_end_matches(".sql");
    let mut n = 1;
    loop {
        let candidate = format!("{stem}.{n}.sql");
        if seen.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

fn default_export_name(request: &ExportTablesRequest, use_zip: bool) -> String {
    let ts = chrono::Utc::now().format("%Y-%m-%d-%H-%M-%S");
    let prefix = match request.mode {
        ExportMode::StructureOnly => "export_structure",
        ExportMode::DataOnly => "export_data",
        ExportMode::DataAndStructure => "export_full",
    };
    let ext = if use_zip {
        "zip"
    } else {
        request.data_format.extension()
    };
    format!("{prefix}_{ts}.{ext}")
}

fn validate_export_path(path: &Path) -> Result<(), CommandError> {
    if path.to_string_lossy().contains("..") {
        return Err(CommandError::Validation(
            "Path traversal not allowed".into(),
        ));
    }
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return Err(CommandError::Validation(
            "File must have an extension".into(),
        ));
    };
    let ext = ext.to_lowercase();
    if !ALLOWED_EXPORT_EXTS
        .iter()
        .any(|a| a.eq_ignore_ascii_case(&ext))
    {
        return Err(CommandError::Validation(format!(
            "File extension '.{ext}' not allowed"
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Streaming write helpers
// ---------------------------------------------------------------------------

fn lock_export<T>(mutex: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, CommandError> {
    mutex
        .lock()
        .map_err(|e| CommandError::Internal(format!("export lock poisoned: {e}")))
}

/// Query stream callbacks are `Fn` and cannot return `Result`; recover from poison.
fn lock_export_stream<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| {
        tracing::error!("export stream lock poisoned; continuing with inner value");
        poisoned.into_inner()
    })
}

/// Stream a table's data to `dest`, formatted as `format`. Returns row count.
async fn write_data_file(
    app: &AppHandle,
    state: &AppState,
    request: &ExportTablesRequest,
    table: &TableExportInput,
    format: DataFormat,
    ddl_prefix: Option<&str>,
    dest: &Path,
) -> Result<u64, CommandError> {
    let sql = build_select_sql(
        &table.table_name,
        &table.columns,
        request.database_type.as_deref(),
    );

    let (driver, handle) = state
        .connection_manager
        .get_session(&request.db_session_id)
        .await
        .cmd_err("export")?;

    let read_only = state
        .connection_manager
        .get_session_config(&handle.id)
        .await
        .map(|c| c.read_only)
        .unwrap_or(false);
    let safe_mode = state.store.get_settings().await.safe_mode;
    crate::sql_guard::check_sql(&sql, read_only, safe_mode).map_err(CommandError::Validation)?;

    let mut sink = FileSink::create(dest)?;
    if let Some(prefix) = ddl_prefix {
        sink.append_str(prefix)?;
        sink.append_str("\n\n")?;
    }
    sink.flush()?;
    let sink = Arc::new(Mutex::new(sink));
    let formatter = Arc::new(Mutex::new(StreamFormatter::new(
        format,
        table.table_name.clone(),
        request.database_type.clone(),
    )));

    let columns = Arc::new(Mutex::new(table.columns.clone()));
    let header_done = Arc::new(Mutex::new(false));
    let rows_written = Arc::new(AtomicU64::new(0));
    let tail_stated = Arc::new(AtomicU64::new(0));
    let last_emit_rows = Arc::new(AtomicU64::new(0));

    let cb_sink = Arc::clone(&sink);
    let cb_fmt = Arc::clone(&formatter);
    let cb_cols = Arc::clone(&columns);
    let cb_header = Arc::clone(&header_done);
    let cb_rows = Arc::clone(&rows_written);
    let cb_tail = Arc::clone(&tail_stated);
    let cb_last_emit = Arc::clone(&last_emit_rows);
    let cb_app = app.clone();
    let cb_table = table.table_name.clone();

    let callback: QueryStreamCallback = Arc::new(move |event: QueryStreamEvent| match event {
        QueryStreamEvent::ExecutionStarted { .. } => {}
        QueryStreamEvent::StatementStart { columns: cols, .. } => {
            let derived: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();
            {
                let mut cur = lock_export_stream(&cb_cols);
                if cur.is_empty() {
                    *cur = derived;
                }
            }
            ensure_header(&cb_cols, &cb_fmt, &cb_sink, &cb_header, true);
        }
        QueryStreamEvent::Rows { rows, .. } => {
            ensure_header(&cb_cols, &cb_fmt, &cb_sink, &cb_header, false);
            let cols = lock_export_stream(&cb_cols).clone();
            let text = lock_export_stream(&cb_fmt).rows(&rows, &cols);
            if !text.is_empty() {
                if let Err(e) = lock_export_stream(&cb_sink).append_str(&text) {
                    // Cannot return an error through the Fn callback; the query
                    // will surface via query_stream's error propagation instead.
                    let _ = e;
                }
            }
            let total = cb_rows.fetch_add(rows.len() as u64, Ordering::Relaxed) + rows.len() as u64;
            // Throttle IPC: emit once the visible count has grown meaningfully.
            if total >= cb_last_emit.load(Ordering::Relaxed) + 5000 || total < 5000 {
                cb_last_emit.store(total, Ordering::Relaxed);
                let _ = cb_app.emit(
                    "batch-export-progress",
                    ExportProgressEvent {
                        table: cb_table.clone(),
                        rows_written: total,
                    },
                );
            }
        }
        QueryStreamEvent::Done { .. } => {
            // Final progress so the last row count is always delivered.
            let total = cb_rows.load(Ordering::Relaxed);
            let _ = cb_app.emit(
                "batch-export-progress",
                ExportProgressEvent {
                    table: cb_table.clone(),
                    rows_written: total,
                },
            );
            if cb_tail.fetch_add(1, Ordering::Relaxed) == 0 {
                let cols = lock_export_stream(&cb_cols).clone();
                let text = lock_export_stream(&cb_fmt).tail(&cols);
                if !text.is_empty() {
                    if let Err(e) = lock_export_stream(&cb_sink).append_str(&text) {
                        let _ = e;
                    }
                }
            }
        }
        QueryStreamEvent::StatementEnd { .. } => {}
    });

    // Capture column type errors from formatting writes if the driver is not
    // streaming rows (they are written inside the callback above).
    driver
        .query_stream(&handle, &sql, None, callback)
        .await
        .cmd_err("export")?;

    // Ensure tail is written even if no Done event fired.
    if tail_stated.load(Ordering::Relaxed) == 0 {
        let cols = lock_export(&columns)?.clone();
        let text = lock_export(&formatter)?.tail(&cols);
        if !text.is_empty() {
            lock_export(&sink)?.append_str(&text)?;
        }
    }
    lock_export(&sink)?.flush()?;

    Ok(rows_written.load(Ordering::Relaxed))
}

#[allow(clippy::type_complexity)]
fn ensure_header(
    columns: &Arc<Mutex<Vec<String>>>,
    formatter: &Arc<Mutex<StreamFormatter>>,
    sink: &Arc<Mutex<FileSink>>,
    header_done: &Arc<Mutex<bool>>,
    _from_start: bool,
) {
    let mut done = lock_export_stream(header_done);
    if *done {
        return;
    }
    let cols = lock_export_stream(columns);
    if cols.is_empty() {
        return;
    }
    let text = lock_export_stream(formatter).header(&cols);
    if !text.is_empty() {
        if let Err(e) = lock_export_stream(sink).append_str(&text) {
            let _ = e;
        }
    }
    *done = true;
}

fn temp_export_dir() -> PathBuf {
    std::env::temp_dir().join(format!("datazen-export-{}", uuid::Uuid::new_v4()))
}

async fn run_zip_export(
    app: &AppHandle,
    state: &AppState,
    request: &ExportTablesRequest,
    plans: &[FilePlan<'_>],
    target: &Path,
) -> Result<u64, CommandError> {
    let temp_dir = temp_export_dir();
    std::fs::create_dir_all(&temp_dir).map_err(CommandError::Io)?;

    let mut rows_total: u64 = 0;
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    let mut build_err: Option<CommandError> = None;

    for plan in plans {
        let dest = temp_dir.join(plan.filename());
        match plan {
            FilePlan::Ddl { text, .. } => {
                if let Err(e) = std::fs::write(&dest, text.as_bytes()) {
                    build_err = Some(CommandError::Io(e));
                    break;
                }
            }
            FilePlan::Data {
                table,
                format,
                ddl_prefix,
                ..
            } => match write_data_file(
                app,
                state,
                request,
                table,
                *format,
                ddl_prefix.as_deref(),
                &dest,
            )
            .await
            {
                Ok(n) => rows_total += n,
                Err(e) => {
                    build_err = Some(e);
                    break;
                }
            },
        }
        entries.push((plan.filename().to_string(), dest));
    }

    if build_err.is_none() {
        let file = std::fs::File::create(target).map_err(CommandError::Io)?;
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, path) in &entries {
            if let Err(e) = zip.start_file(name, options) {
                build_err = Some(CommandError::Io(e.into()));
                break;
            }
            // Stream the temp file into the zip in bounded chunks so a single
            // huge table is never loaded into memory at zip time.
            if let Err(e) = stream_file_into_zip(&mut zip, path) {
                build_err = Some(e);
                break;
            }
        }
        if build_err.is_none() {
            if let Err(e) = zip.finish() {
                build_err = Some(CommandError::Io(e.into()));
            }
        }
    }

    let _ = std::fs::remove_dir_all(&temp_dir);

    if let Some(e) = build_err {
        return Err(e);
    }
    Ok(rows_total)
}

/// Copy a file into a `ZipWriter` in bounded chunks.
fn stream_file_into_zip<W: std::io::Write + std::io::Seek>(
    zip: &mut ZipWriter<W>,
    path: &Path,
) -> Result<(), CommandError> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(CommandError::Io)?;
    let mut reader = std::io::BufReader::new(file);
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(CommandError::Io)?;
        if n == 0 {
            break;
        }
        zip.write_all(&buf[..n]).map_err(CommandError::Io)?;
    }
    Ok(())
}

async fn run_single_export(
    app: &AppHandle,
    state: &AppState,
    request: &ExportTablesRequest,
    plans: &[FilePlan<'_>],
    target: &Path,
) -> Result<u64, CommandError> {
    if plans.len() == 1 {
        return match &plans[0] {
            FilePlan::Ddl { text, .. } => {
                std::fs::write(target, text.as_bytes()).map_err(CommandError::Io)?;
                Ok(0)
            }
            FilePlan::Data {
                table,
                format,
                ddl_prefix,
                ..
            } => {
                write_data_file(
                    app,
                    state,
                    request,
                    table,
                    *format,
                    ddl_prefix.as_deref(),
                    target,
                )
                .await
            }
        };
    }

    // Multiple files -> combine into one target, streaming each table's temp
    // file through the sink in bounded chunks so the whole export is never
    // held in memory (only one table's temp file at a time, on disk).
    let mut sink = FileSink::create(target)?;
    let mut rows_total: u64 = 0;
    for plan in plans {
        sink.append_str(&format!("-- ===== {} =====\n\n", plan.filename()))?;
        match plan {
            FilePlan::Ddl { text, .. } => {
                sink.append_str(text)?;
            }
            FilePlan::Data {
                table,
                format,
                ddl_prefix,
                ..
            } => {
                let tmp = temp_export_dir();
                let n = match write_data_file(
                    app,
                    state,
                    request,
                    table,
                    *format,
                    ddl_prefix.as_deref(),
                    &tmp,
                )
                .await
                {
                    Ok(n) => n,
                    Err(e) => {
                        let _ = std::fs::remove_file(&tmp);
                        return Err(e);
                    }
                };
                rows_total += n;
                stream_bytes_to_sink(&tmp, &mut sink)?;
                let _ = std::fs::remove_file(&tmp);
            }
        }
        sink.append_str("\n\n")?;
    }
    sink.flush()?;
    Ok(rows_total)
}

/// Stream a whole temp file into an [`FileSink`] in bounded chunks (copies the
/// temp file onto the final target without ever holding it in RAM).
fn stream_bytes_to_sink(path: &Path, sink: &mut FileSink) -> Result<(), CommandError> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(CommandError::Io)?;
    let mut reader = std::io::BufReader::new(file);
    let mut buf = vec![0u8; 128 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(CommandError::Io)?;
        if n == 0 {
            break;
        }
        sink.append_bytes(&buf[..n])?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Main command
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn export_tables_stream(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ExportTablesRequest,
) -> Result<ExportTablesResult, CommandError> {
    let plans = build_file_plans(&request);
    if plans.is_empty() {
        return Err(CommandError::Validation("no tables selected".into()));
    }

    let use_zip = matches!(request.output_mode, OutputMode::Zip)
        || (plans.len() > 1 && !plans.iter().all(|p| p.filename().ends_with(".sql")));

    let ext = if use_zip {
        "zip"
    } else {
        request.data_format.extension()
    };
    let picked = super::dialog::save_file(
        &app,
        ("Export".into(), vec![ext.to_string()]),
        default_export_name(&request, use_zip),
    )
    .await?;
    let Some(target) = picked else {
        return Ok(ExportTablesResult::Cancelled);
    };
    validate_export_path(&target)?;

    let rows_total = if use_zip {
        run_zip_export(&app, &state, &request, &plans, &target).await?
    } else {
        run_single_export(&app, &state, &request, &plans, &target).await?
    };

    Ok(ExportTablesResult::Saved(rows_total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::ColumnInfo;

    fn v_str(s: &str) -> Option<Value> {
        Some(Value::String(s.to_string()))
    }

    fn rows2() -> Vec<Vec<Option<Value>>> {
        vec![
            vec![Some(Value::Integer(1)), v_str("a,b")],
            vec![Some(Value::Integer(2)), Some(Value::Null)],
        ]
    }

    #[test]
    fn csv_escapes_commas_quotes_newlines() {
        let mut f = StreamFormatter::new(DataFormat::Csv, "t".into(), None);
        let cols = vec!["id".to_string(), "name".to_string()];
        let header = f.header(&cols);
        assert_eq!(header, "id,name\n");
        let text = f.rows(&rows2(), &cols);
        assert_eq!(text, "1,\"a,b\"\n2,\n");
        assert_eq!(f.tail(&cols), "");
    }

    #[test]
    fn json_streams_objects_with_comma() {
        let mut f = StreamFormatter::new(DataFormat::Json, "t".into(), None);
        let cols = vec!["id".to_string(), "name".to_string()];
        assert_eq!(f.header(&cols), "[");
        let rows: Vec<Vec<Option<Value>>> = vec![
            vec![Some(Value::Integer(1)), v_str("x")],
            vec![Some(Value::Integer(2)), Some(Value::Null)],
        ];
        // First call writes object 1 with a leading newline, no comma yet.
        let a = f.rows(&rows[..1], &cols);
        assert_eq!(a, "\n{\"id\":1,\"name\":\"x\"}");
        // Second call adds a comma.
        let b = f.rows(&rows[1..], &cols);
        assert_eq!(b, ",\n{\"id\":2,\"name\":null}");
        assert_eq!(f.tail(&cols), "\n]");
    }

    #[test]
    fn sql_insert_single_transaction_never_per_insert() {
        let mut f = StreamFormatter::new(
            DataFormat::SqlInsert,
            "users".into(),
            Some("postgres".into()),
        );
        let cols = vec!["id".to_string(), "email".to_string()];
        let header = f.header(&cols);
        assert_eq!(header, "BEGIN;\n");
        // Two rows -> single batched INSERT with two tuples.
        let text = f.rows(&rows2(), &cols);
        assert_eq!(
            text,
            "INSERT INTO \"users\" (\"id\", \"email\") VALUES\n  (1, 'a,b'),\n  (2, NULL);\n"
        );
        let tail = f.tail(&cols);
        assert_eq!(tail, "COMMIT;\n");
    }

    #[test]
    fn sql_insert_batches_at_limit() {
        let mut f = StreamFormatter::new(DataFormat::SqlInsert, "t".into(), None);
        let cols = vec!["id".to_string()];
        let rows: Vec<Vec<Option<Value>>> =
            (0..600).map(|i| vec![Some(Value::Integer(i))]).collect();
        let text = f.rows(&rows, &cols);
        // 600 rows -> two batched INSERT statements (500 + 100 rows), all in
        // this rows call since flush drains every pending row.
        assert_eq!(text.matches("INSERT INTO").count(), 2);
        // The last row (599) is in the second batch.
        assert!(text.contains("(599);"));
        let tail = f.tail(&cols);
        // No pending rows left; tail only adds COMMIT (single transaction).
        assert_eq!(tail, "COMMIT;\n");
        assert_eq!(tail.matches("INSERT INTO").count(), 0);
    }

    #[test]
    fn build_select_sql_quotes_mysql_backtick() {
        assert_eq!(
            build_select_sql("users", &["id".into(), "name".into()], Some("mysql")),
            "SELECT `id`, `name` FROM `users`"
        );
        assert_eq!(
            build_select_sql("users", &["id".into()], Some("postgres")),
            "SELECT \"id\" FROM \"users\""
        );
        assert_eq!(
            build_select_sql("users", &[], None),
            "SELECT * FROM \"users\""
        );
    }

    #[test]
    fn build_file_plans_matches_modes() {
        let mk = |name: &str, ddl: Option<String>| TableExportInput {
            table_name: name.into(),
            columns: vec!["id".into()],
            ddl,
        };
        // data_and_structure + csv -> structure.sql + data.csv
        let req = ExportTablesRequest {
            db_session_id: "c".into(),
            database_type: Some("postgres".into()),
            mode: ExportMode::DataAndStructure,
            data_format: DataFormat::Csv,
            output_mode: OutputMode::Zip,
            tables: vec![mk("users", Some("CREATE TABLE ...;".into()))],
        };
        let plans = build_file_plans(&req);
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].filename(), "users.sql");
        assert_eq!(plans[1].filename(), "users.csv");

        // data_and_structure + sql_insert -> one combined .sql
        let req2 = ExportTablesRequest {
            data_format: DataFormat::SqlInsert,
            ..req
        };
        let plans2 = build_file_plans(&req2);
        assert_eq!(plans2.len(), 1);
        assert_eq!(plans2[0].filename(), "users.sql");
    }

    #[test]
    fn resolve_ddl_fallback() {
        let t = TableExportInput {
            table_name: "t".into(),
            columns: vec![],
            ddl: None,
        };
        assert!(resolve_ddl(&t).contains("DDL unavailable"));
        let t2 = TableExportInput {
            table_name: "t".into(),
            columns: vec![],
            ddl: Some("CREATE TABLE;".into()),
        };
        assert_eq!(resolve_ddl(&t2), "CREATE TABLE;");
    }

    #[test]
    fn column_info_used() {
        let ci = ColumnInfo {
            name: "a".into(),
            data_type: "int".into(),
            nullable: true,
        };
        assert_eq!(ci.name, "a");
    }

    fn poison_mutex<T>(mutex: &Mutex<T>) {
        let target = mutex as *const Mutex<T> as usize;
        let handle = std::thread::spawn(move || {
            let m = unsafe { &*(target as *const Mutex<T>) };
            let _guard = m.lock().unwrap();
            panic!("tester: intentional mutex poison");
        });
        assert!(handle.join().is_err());
    }

    /// [tester] Non-callback export locks propagate poison as `CommandError::Internal`.
    #[test]
    fn test_tester_lock_export_poison_returns_internal() {
        let mutex = Mutex::new(0_i32);
        poison_mutex(&mutex);
        let err = lock_export(&mutex).unwrap_err();
        match err {
            CommandError::Internal(msg) => {
                assert!(
                    msg.contains("export lock poisoned"),
                    "unexpected internal message: {msg}"
                );
            }
            other => panic!("expected CommandError::Internal, got {other:?}"),
        }
    }

    /// [tester] Stream callback locks recover from poison via `into_inner`.
    #[test]
    fn test_tester_lock_export_stream_recovers_from_poison() {
        let mutex = Mutex::new(vec!["col".to_string()]);
        poison_mutex(&mutex);
        let cols = lock_export_stream(&mutex);
        assert_eq!(*cols, vec!["col".to_string()]);
    }

    /// [tester] Concurrent `lock_export` calls serialize without error on a healthy mutex.
    #[test]
    fn test_tester_lock_export_concurrent_access_succeeds() {
        use std::sync::Arc;
        use std::thread;

        let mutex = Arc::new(Mutex::new(0_i32));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let m = Arc::clone(&mutex);
                thread::spawn(move || {
                    for _ in 0..100 {
                        let mut guard = lock_export(&m).expect("lock should succeed");
                        *guard += 1;
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(*lock_export(&mutex).unwrap(), 800);
    }
}

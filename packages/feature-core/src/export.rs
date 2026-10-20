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

use datazen_driver_api::{DatabaseDriver, QueryStreamCallback, QueryStreamEvent, Value};
use serde::{Deserialize, Serialize};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::error::FeatureError as CommandError;

#[async_trait::async_trait]
pub trait ExportSource: Send + Sync {
    async fn resolve_source(
        &self,
        session_id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, bool, bool), CommandError>;
    async fn stream_query(
        &self,
        session_id: &str,
        sql: &str,
        target: datazen_driver_api::SqlTarget<'_>,
        callback: QueryStreamCallback,
    ) -> Result<(), CommandError> {
        let _ = (session_id, sql, target, callback);
        Err(CommandError::Validation(
            "export source does not provide query streaming".into(),
        ))
    }
}
pub trait ExportEvents: Send + Sync {
    fn progress_callback(&self) -> Arc<dyn Fn(ExportProgressEvent) + Send + Sync>;
}

pub const SQL_INSERT_BATCH_SIZE: usize = 500;
const APPEND_FLUSH_BYTES: usize = 64 * 1024;
pub const ALLOWED_EXPORT_EXTS: &[&str] = &["sql", "csv", "json", "zip"];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableExportInput {
    pub table_name: String,
    /// Schema the table lives in (PostgreSQL family). Resolved per table by the
    /// caller, because a batch can span schemas.
    #[serde(default)]
    pub schema: Option<String>,
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
    pub fn extension(self) -> &'static str {
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
    /// Database to read from. The session's own database is only a fallback:
    /// the tree lets the user export a table from any database it lists.
    #[serde(default)]
    pub database: Option<String>,
    /// Fallback schema for tables that do not carry one.
    #[serde(default)]
    pub schema: Option<String>,
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

mod format;
pub use format::*;

// ---------------------------------------------------------------------------
// File output plumbing
// ---------------------------------------------------------------------------

/// A buffered file writer that flushes to disk every ~64KB.
pub struct FileSink {
    writer: BufWriter<std::fs::File>,
    buffer: Vec<u8>,
}

impl FileSink {
    pub fn create(path: &Path) -> Result<Self, CommandError> {
        let file = std::fs::File::create(path).map_err(CommandError::Io)?;
        Ok(FileSink {
            writer: BufWriter::new(file),
            buffer: Vec::new(),
        })
    }

    pub fn append_str(&mut self, text: &str) -> Result<(), CommandError> {
        self.append_bytes(text.as_bytes())
    }

    pub fn append_bytes(&mut self, bytes: &[u8]) -> Result<(), CommandError> {
        self.buffer.extend_from_slice(bytes);
        if self.buffer.len() >= APPEND_FLUSH_BYTES {
            self.flush()?;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), CommandError> {
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
pub enum FilePlan<'a> {
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
    pub fn filename(&self) -> &str {
        match self {
            FilePlan::Ddl { filename, .. } | FilePlan::Data { filename, .. } => filename,
        }
    }
}

pub fn resolve_ddl(table: &TableExportInput) -> String {
    match table.ddl.as_deref() {
        Some(d) if !d.trim().is_empty() => d.to_string(),
        _ => format!("-- DDL unavailable for {}", table.table_name),
    }
}

pub fn build_file_plans(request: &ExportTablesRequest) -> Vec<FilePlan<'_>> {
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

/// Plan the output container before the host chooses its destination.
pub fn prepare_export(
    request: &ExportTablesRequest,
) -> Result<(Vec<FilePlan<'_>>, bool), CommandError> {
    let plans = build_file_plans(request);
    if plans.is_empty() {
        return Err(CommandError::Validation("no tables selected".into()));
    }
    let use_zip = matches!(request.output_mode, OutputMode::Zip)
        || (plans.len() > 1 && !plans.iter().all(|plan| plan.filename().ends_with(".sql")));
    Ok((plans, use_zip))
}

pub fn unique_filename(seen: &mut HashSet<String>, name: String) -> String {
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

pub fn default_export_name(request: &ExportTablesRequest, use_zip: bool) -> String {
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

pub fn validate_export_path(path: &Path) -> Result<(), CommandError> {
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

pub fn lock_export<T>(mutex: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, CommandError> {
    mutex
        .lock()
        .map_err(|e| CommandError::Internal(format!("export lock poisoned: {e}")))
}

/// Query stream callbacks are `Fn` and cannot return `Result`; recover from poison.
pub fn lock_export_stream<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| {
        tracing::error!("export stream lock poisoned; continuing with inner value");
        poisoned.into_inner()
    })
}

/// Stream a table's data to `dest`, formatted as `format`. Returns row count.
pub async fn write_data_file(
    app: &dyn ExportEvents,
    state: &dyn ExportSource,
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

    let (driver, read_only, safe_mode) = state.resolve_source(&request.db_session_id).await?;
    datazen_runtime::sql_guard::check_sql(&sql, read_only, safe_mode)
        .map_err(CommandError::Validation)?;

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
        driver.clone(),
    )));

    let columns = Arc::new(Mutex::new(table.columns.clone()));
    let header_done = Arc::new(Mutex::new(false));
    let rows_written = Arc::new(AtomicU64::new(0));
    let tail_stated = Arc::new(AtomicU64::new(0));
    let last_emit_rows = Arc::new(AtomicU64::new(0));
    let format_error = Arc::new(Mutex::new(None::<String>));

    let cb_sink = Arc::clone(&sink);
    let cb_fmt = Arc::clone(&formatter);
    let cb_cols = Arc::clone(&columns);
    let cb_header = Arc::clone(&header_done);
    let cb_rows = Arc::clone(&rows_written);
    let cb_tail = Arc::clone(&tail_stated);
    let cb_last_emit = Arc::clone(&last_emit_rows);
    let cb_format_error = Arc::clone(&format_error);
    let cb_app = app.progress_callback();
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
            match lock_export_stream(&cb_fmt).rows(&rows, &cols) {
                Ok(text) if !text.is_empty() => {
                    if let Err(e) = lock_export_stream(&cb_sink).append_str(&text) {
                        let _ = e;
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    *lock_export_stream(&cb_format_error) = Some(error.to_string());
                }
            }
            let total = cb_rows.fetch_add(rows.len() as u64, Ordering::Relaxed) + rows.len() as u64;
            // Throttle IPC: emit once the visible count has grown meaningfully.
            if total >= cb_last_emit.load(Ordering::Relaxed) + 5000 || total < 5000 {
                cb_last_emit.store(total, Ordering::Relaxed);
                cb_app(ExportProgressEvent {
                    table: cb_table.clone(),
                    rows_written: total,
                });
            }
        }
        QueryStreamEvent::Done { .. } => {
            // Final progress so the last row count is always delivered.
            let total = cb_rows.load(Ordering::Relaxed);
            cb_app(ExportProgressEvent {
                table: cb_table.clone(),
                rows_written: total,
            });
            if cb_tail.fetch_add(1, Ordering::Relaxed) == 0 {
                let cols = lock_export_stream(&cb_cols).clone();
                match lock_export_stream(&cb_fmt).tail(&cols) {
                    Ok(text) if !text.is_empty() => {
                        if let Err(e) = lock_export_stream(&cb_sink).append_str(&text) {
                            let _ = e;
                        }
                    }
                    Ok(_) => {}
                    Err(error) => {
                        *lock_export_stream(&cb_format_error) = Some(error.to_string());
                    }
                }
            }
        }
        QueryStreamEvent::StatementEnd { .. } => {}
    });

    // Capture column type errors from formatting writes if the driver is not
    // streaming rows (they are written inside the callback above).
    //
    // The statement carries its target: PostgreSQL cannot reference another
    // database in one statement, so a bare `query_stream` would have streamed
    // from the session's own database (the "second database exports the first
    // database's rows" bug). A per-table schema wins over the request default.
    state
        .stream_query(
            &request.db_session_id,
            &sql,
            datazen_driver_api::SqlTarget::new(
                request.database.as_deref(),
                table.schema.as_deref().or(request.schema.as_deref()),
            ),
            callback,
        )
        .await?;

    if let Some(error) = lock_export(&format_error)?.take() {
        return Err(CommandError::Validation(format!(
            "SQL export could not safely render a value: {error}"
        )));
    }

    // Ensure tail is written even if no Done event fired.
    if tail_stated.load(Ordering::Relaxed) == 0 {
        let cols = lock_export(&columns)?.clone();
        let text = lock_export(&formatter)?.tail(&cols)?;
        if !text.is_empty() {
            lock_export(&sink)?.append_str(&text)?;
        }
    }
    lock_export(&sink)?.flush()?;

    Ok(rows_written.load(Ordering::Relaxed))
}

#[allow(clippy::type_complexity)]
pub fn ensure_header(
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

pub fn temp_export_dir() -> PathBuf {
    std::env::temp_dir().join(format!("datazen-export-{}", uuid::Uuid::new_v4()))
}

pub async fn run_zip_export(
    app: &dyn ExportEvents,
    state: &dyn ExportSource,
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
pub fn stream_file_into_zip<W: std::io::Write + std::io::Seek>(
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

pub async fn run_single_export(
    app: &dyn ExportEvents,
    state: &dyn ExportSource,
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
pub fn stream_bytes_to_sink(path: &Path, sink: &mut FileSink) -> Result<(), CommandError> {
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

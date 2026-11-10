//! Rust-side streaming table export.
//!
//! After the user picks a destination file, this module streams each selected
//! table's query directly on the Rust side (`driver.query_stream`), formats rows
//! in batches (CSV / TSV / JSON / Markdown / SQL INSERT / SQL UPDATE, or XLSX
//! via the parallel [`BinaryFormatter`](binary::BinaryFormatter) branch) and
//! writes them straight to disk — avoiding holding the whole table in memory
//! and the JS↔Rust data round-trip.
//!
//! Each item's rows come from one SQL statement, streamed through
//! [`stream_sql_source`](fetcher::stream_sql_source) (see `fetcher.rs`):
//! the named table (as a `SELECT *` / selected-columns query) or one
//! arbitrary SQL statement. Both feed the same internal row stream
//! (`StatementStart` + batched `Rows`) into the formatters.
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

use datazen_driver_api::{ColumnInfo, DatabaseDriver, QueryStreamCallback, QueryStreamEvent};
use serde::{Deserialize, Serialize};

use crate::jobs::CancelToken;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use error::ExportError as CommandError;

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
pub const ALLOWED_EXPORT_EXTS: &[&str] = &["sql", "csv", "tsv", "json", "md", "xlsx", "zip"];

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
    /// Key column for `sql_update` output (`UPDATE … SET … WHERE <pk> = …`).
    /// The caller supplies the primary key when it knows one; when absent
    /// or not among the exported columns, the first column is used (the
    /// same fallback the frontend `sql_update` branch uses).
    #[serde(default)]
    pub primary_key: Option<String>,
    /// Where this item's rows come from. Defaults to the named table, so
    /// requests written before data sources existed keep parsing unchanged.
    /// The `type` tag is explicit and serializable so the frontend (item 3)
    /// can send SQL sources over the same IPC shape. Rows the caller
    /// already holds never reach Rust: they are formatted and saved in the
    /// frontend, so there is no provided-data variant here.
    #[serde(default)]
    pub source: TableDataSource,
}

/// Where one export item's rows come from.
///
/// Serialized with an explicit `type` tag (`{"type":"table"}`,
/// `{"type":"sql","sql":"..."}`) so callers and the frontend can tell the
/// sources apart without guessing from the other fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum TableDataSource {
    /// Default: the named table, exported as `SELECT *` (or the selected
    /// columns). Implemented as a SQL source — see `fetcher.rs`.
    Table,
    /// One arbitrary read-only SQL statement (e.g. a filtered export),
    /// executed against the backend. `sql_guard` applies at fetch time.
    Sql { sql: String },
}

impl Default for TableDataSource {
    fn default() -> Self {
        Self::Table
    }
}

impl TableDataSource {
    /// Fail fast on a malformed source, before any file is planned or
    /// written. SQL emptiness is checked here; the full `sql_guard` check
    /// needs backend flags and runs at fetch time instead.
    pub fn validate(&self, name: &str) -> Result<(), ExportError> {
        match self {
            TableDataSource::Table => Ok(()),
            TableDataSource::Sql { sql } => {
                if sql.trim().is_empty() {
                    return Err(ExportError::Validation(format!(
                        "SQL data source '{name}' must not be empty"
                    )));
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportMode {
    StructureOnly,
    DataOnly,
    DataAndStructure,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DataFormat {
    Csv,
    Tsv,
    Json,
    Markdown,
    SqlInsert,
    SqlUpdate,
    Xlsx,
}

impl DataFormat {
    pub fn extension(self) -> &'static str {
        match self {
            DataFormat::Csv => "csv",
            DataFormat::Tsv => "tsv",
            DataFormat::Json => "json",
            DataFormat::Markdown => "md",
            DataFormat::SqlInsert | DataFormat::SqlUpdate => "sql",
            DataFormat::Xlsx => "xlsx",
        }
    }

    /// Binary formats go through the parallel [`BinaryFormatter`](binary::BinaryFormatter)
    /// branch (`write_binary_file`); they can never be concatenated as text.
    pub fn is_binary(self) -> bool {
        matches!(self, DataFormat::Xlsx)
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
    /// Opaque per-run cancellation token (frontend `crypto.randomUUID`),
    /// carried only by SQL-path requests. The desktop host registers it
    /// before the save dialog; `cancel_export_tables` flips it. Provided
    /// exports never reach Rust, so they never carry one.
    #[serde(default)]
    pub cancel_token: Option<String>,
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

/// Wire shape is the TS mirror (`{ Saved: number } | { Cancelled: null }
/// | { Aborted: { partialDeleted: boolean } }` in
/// `packages/frontend/src/commands/file.ts`): variant names serialize
/// as-is, so do NOT re-add `rename_all = "camelCase"` here — it would
/// lowercase the variant names (`{"saved": …}`) and the frontend's
/// `'Saved' in result` checks would never match (a successful export
/// would report as cancelled).
#[derive(Debug, Clone, serde::Serialize)]
pub enum ExportTablesResult {
    Saved(u64),
    Cancelled,
    /// The run was cancelled mid-flight; `partial_deleted` says whether
    /// the in-progress output was removed (an abort that could not delete
    /// it is reported instead of assumed).
    Aborted {
        #[serde(rename = "partialDeleted")]
        partial_deleted: bool,
    },
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

pub mod binary;
pub mod error;
pub mod fetcher;
pub mod file_job;
pub mod format;
pub use binary::{sanitize_sheet_name, BinaryFormatter, XlsxFormatter};
pub use error::ExportError;
pub use fetcher::stream_sql_source;
pub use file_job::{FileArtifactJob, FileJobOperation, FileJobOutput};
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
// File planning for the Rust batch export. This module is the single source
// of truth for export file layout; the old TS buildBatchExportFiles builder
// was deleted in the export cleanup — do not reintroduce a second planner.
// ---------------------------------------------------------------------------

/// A planned output file.
#[derive(Debug)]
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

/// DDL text for a structure-mode plan.
///
/// DDL semantics, by source (chosen deliberately — see below):
///
/// * **Table source**: DDL comes from `table.ddl` when the caller supplied
///   it, else the `"-- DDL unavailable …"` placeholder. A real table exists,
///   so the file at least names what could not be retrieved.
/// * **SQL source**: there is no table whose DDL could be merged, so a
///   generated file would be fake DDL. Structure modes therefore require
///   the caller to supply explicit `ddl` text for these items (the `ddl`
///   field already exists for exactly this); without it, planning fails
///   with a clear `Validation` error instead of emitting a placeholder
///   that pretends to be a table definition.
///
/// The alternative — refusing structure modes for non-table sources
/// outright — was rejected: a filtered SQL export whose caller knows the
/// table's DDL is a legitimate structure export, and the explicit-`ddl`
/// rule keeps that working without inventing DDL.
pub fn ddl_for_structure(table: &TableExportInput) -> Result<String, CommandError> {
    match &table.source {
        TableDataSource::Table => Ok(resolve_ddl(table)),
        _ => match table.ddl.as_deref() {
            Some(d) if !d.trim().is_empty() => Ok(d.to_string()),
            _ => Err(CommandError::Validation(format!(
                "structure export for non-table source '{}' needs an explicit ddl: \
                 SQL sources have no table DDL to include",
                table.table_name
            ))),
        },
    }
}

pub fn build_file_plans(request: &ExportTablesRequest) -> Result<Vec<FilePlan<'_>>, CommandError> {
    // Fail fast on malformed sources before any file is planned.
    for table in &request.tables {
        table.source.validate(&table.table_name)?;
    }
    let mut seen = HashSet::new();
    let mut plans = Vec::new();
    for table in &request.tables {
        let base = table.table_name.clone();
        match request.mode {
            ExportMode::StructureOnly => {
                plans.push(FilePlan::Ddl {
                    filename: unique_filename(&mut seen, format!("{base}.sql")),
                    text: ddl_for_structure(table)?,
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
                // SQL data formats carry their DDL as a prefix in the same
                // `.sql` file (one file, no name collision between the DDL
                // plan and the data plan, which would otherwise both be
                // `{table}.sql`); every other format splits DDL and data.
                if matches!(
                    request.data_format,
                    DataFormat::SqlInsert | DataFormat::SqlUpdate
                ) {
                    plans.push(FilePlan::Data {
                        filename: unique_filename(&mut seen, format!("{base}.sql")),
                        table,
                        format: request.data_format,
                        ddl_prefix: Some(ddl_for_structure(table)?),
                    });
                } else {
                    plans.push(FilePlan::Ddl {
                        filename: unique_filename(&mut seen, format!("{base}.sql")),
                        text: ddl_for_structure(table)?,
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
    Ok(plans)
}

/// Plan the output container before the host chooses its destination.
pub fn prepare_export(
    request: &ExportTablesRequest,
) -> Result<(Vec<FilePlan<'_>>, bool), CommandError> {
    let plans = build_file_plans(request)?;
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

/// Extension of the file [`default_export_name`] produces — decided by the
/// actual product, not by the data format alone. A structure-only export
/// is always DDL text (see [`build_file_plans`]: every plan is
/// `{base}.sql`), so it is `.sql` whatever the data format says; a zip
/// container is `.zip`; everything else keeps the data format's extension.
pub fn default_export_extension(request: &ExportTablesRequest, use_zip: bool) -> &'static str {
    if use_zip {
        "zip"
    } else if matches!(request.mode, ExportMode::StructureOnly) {
        "sql"
    } else {
        request.data_format.extension()
    }
}

pub fn default_export_name(request: &ExportTablesRequest, use_zip: bool) -> String {
    let ts = chrono::Utc::now().format("%Y-%m-%d-%H-%M-%S");
    let prefix = match request.mode {
        ExportMode::StructureOnly => "export_structure",
        ExportMode::DataOnly => "export_data",
        ExportMode::DataAndStructure => "export_full",
    };
    let ext = default_export_extension(request, use_zip);
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

/// Stream an item's data to `dest`, formatted as `format`. Returns row count.
///
/// Rows come from the item's SQL source — the named table (as a SQL
/// source) or one SQL statement — resolved by
/// [`fetcher::resolve_sql_source`] and streamed by
/// [`fetcher::stream_sql_source`] into the callback below. `sql_guard`
/// runs inside the SQL stream.
pub async fn write_data_file(
    app: &dyn ExportEvents,
    state: &dyn ExportSource,
    request: &ExportTablesRequest,
    table: &TableExportInput,
    format: DataFormat,
    ddl_prefix: Option<&str>,
    dest: &Path,
    cancel: &CancelToken,
) -> Result<u64, CommandError> {
    if format.is_binary() {
        return Err(CommandError::Validation(format!(
            "{format:?} is a binary format; use write_binary_file"
        )));
    }
    // A cancel that landed before the first byte: never create the file.
    if cancel.is_cancelled() {
        return Err(CommandError::Cancelled);
    }
    let (sql, database, schema) = fetcher::resolve_sql_source(request, table)?;

    // The driver fetched here only feeds StreamFormatter's SQL-literal
    // dialect (SQL INSERT rendering); the SQL stream resolves its own
    // source for guard flags + streaming.
    let (driver, _read_only, _safe_mode) = state.resolve_source(&request.db_session_id).await?;

    let mut sink = FileSink::create(dest)?;
    if let Some(prefix) = ddl_prefix {
        sink.append_str(prefix)?;
        sink.append_str("\n\n")?;
    }
    sink.flush()?;
    let sink = Arc::new(Mutex::new(sink));
    let formatter = Arc::new(Mutex::new(
        StreamFormatter::new(
            format,
            table.table_name.clone(),
            request.database_type.clone(),
            driver.clone(),
        )
        .with_primary_key(table.primary_key.clone()),
    ));

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
    // The stream callback is `Fn` and `'static`, so it polls the shared
    // atomic bit behind the token (`CancelToken::flag`), not the token.
    let cb_cancel_flag = cancel.flag();
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
            // A source that keeps delivering after the cancel must not get
            // its rows written: stop here; the post-stream check below
            // turns the run into `Cancelled`.
            if cb_cancel_flag.load(Ordering::Relaxed) {
                return;
            }
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
            if cb_cancel_flag.load(Ordering::Relaxed) {
                return;
            }
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
    if let Err(error) = fetcher::stream_sql_source(
        state,
        &request.db_session_id,
        &sql,
        database.as_deref(),
        schema.as_deref(),
        &callback,
    )
    .await
    {
        // Cancel wins over the stream's own error: the run stopped because
        // the user asked it to, and the caller removes the partial file.
        if cancel.is_cancelled() {
            return Err(CommandError::Cancelled);
        }
        return Err(error);
    }

    if cancel.is_cancelled() {
        return Err(CommandError::Cancelled);
    }

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

/// Stream an item's data to `dest` as a binary format (item 3a: XLSX).
/// Parallel branch to [`write_data_file`]: same SQL row stream and
/// progress events, but the formatter owns the destination file and no
/// text sink is involved. Binary formatters need no SQL-literal dialect,
/// so no driver is resolved here — the SQL stream resolves its own source
/// inside `stream_sql_source`.
pub async fn write_binary_file(
    app: &dyn ExportEvents,
    state: &dyn ExportSource,
    request: &ExportTablesRequest,
    table: &TableExportInput,
    format: DataFormat,
    dest: &Path,
    cancel: &CancelToken,
) -> Result<u64, CommandError> {
    if !format.is_binary() {
        return Err(CommandError::Validation(format!(
            "{format:?} is a text format; use write_data_file"
        )));
    }
    // A cancel that landed before the first byte: never create the file.
    if cancel.is_cancelled() {
        return Err(CommandError::Cancelled);
    }
    let (sql, database, schema) = fetcher::resolve_sql_source(request, table)?;

    let formatter: Box<dyn BinaryFormatter> = match format {
        DataFormat::Xlsx => Box::new(XlsxFormatter::new(&table.table_name, dest.to_path_buf())),
        _ => {
            return Err(CommandError::Validation(format!(
                "{format:?} is marked binary but has no binary formatter"
            )));
        }
    };
    let formatter = Arc::new(Mutex::new(formatter));

    // Full ColumnInfo (not just names): the binary formatter needs it for
    // the header row, and a future Parquet formatter needs data types.
    let columns = Arc::new(Mutex::new(Vec::<ColumnInfo>::new()));
    let begun = Arc::new(Mutex::new(false));
    let finished = Arc::new(AtomicU64::new(0));
    let rows_written = Arc::new(AtomicU64::new(0));
    let last_emit_rows = Arc::new(AtomicU64::new(0));
    let format_error = Arc::new(Mutex::new(None::<String>));

    let cb_fmt = Arc::clone(&formatter);
    let cb_cols = Arc::clone(&columns);
    let cb_begun = Arc::clone(&begun);
    let cb_finished = Arc::clone(&finished);
    let cb_rows = Arc::clone(&rows_written);
    let cb_last_emit = Arc::clone(&last_emit_rows);
    let cb_format_error = Arc::clone(&format_error);
    // The stream callback is `Fn` and `'static`, so it polls the shared
    // atomic bit behind the token (`CancelToken::flag`), not the token.
    let cb_cancel_flag = cancel.flag();
    let cb_app = app.progress_callback();
    let cb_table = table.table_name.clone();
    let cb_fallback_cols = table.columns.clone();

    let callback: QueryStreamCallback = Arc::new(move |event: QueryStreamEvent| match event {
        QueryStreamEvent::ExecutionStarted { .. } => {}
        QueryStreamEvent::StatementStart { columns: cols, .. } => {
            {
                let mut cur = lock_export_stream(&cb_cols);
                if cur.is_empty() {
                    *cur = cols;
                }
            }
            let cols = lock_export_stream(&cb_cols).clone();
            if !*lock_export_stream(&cb_begun) {
                match lock_export_stream(&cb_fmt).begin(&cols) {
                    Ok(()) => *lock_export_stream(&cb_begun) = true,
                    Err(error) => {
                        *lock_export_stream(&cb_format_error) = Some(error.to_string());
                    }
                }
            }
        }
        QueryStreamEvent::Rows { rows, .. } => {
            // A source that keeps delivering after the cancel must not get
            // its rows written: stop here; the post-stream check below
            // turns the run into `Cancelled`.
            if cb_cancel_flag.load(Ordering::Relaxed) {
                return;
            }
            // A stream that skips StatementStart still gets a header: fall
            // back to the requested column names, as the text path does.
            if !*lock_export_stream(&cb_begun) {
                let fallback: Vec<ColumnInfo> = if cb_fallback_cols.is_empty() {
                    rows.first()
                        .map(|row| {
                            (0..row.len())
                                .map(|i| ColumnInfo {
                                    name: format!("col_{i}"),
                                    data_type: "text".into(),
                                    nullable: true,
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                } else {
                    cb_fallback_cols
                        .iter()
                        .map(|name| ColumnInfo {
                            name: name.clone(),
                            data_type: "text".into(),
                            nullable: true,
                        })
                        .collect()
                };
                match lock_export_stream(&cb_fmt).begin(&fallback) {
                    Ok(()) => *lock_export_stream(&cb_begun) = true,
                    Err(error) => {
                        *lock_export_stream(&cb_format_error) = Some(error.to_string());
                    }
                }
            }
            if let Err(error) = lock_export_stream(&cb_fmt).rows(&rows) {
                *lock_export_stream(&cb_format_error) = Some(error.to_string());
            }
            let total = cb_rows.fetch_add(rows.len() as u64, Ordering::Relaxed) + rows.len() as u64;
            if total >= cb_last_emit.load(Ordering::Relaxed) + 5000 || total < 5000 {
                cb_last_emit.store(total, Ordering::Relaxed);
                cb_app(ExportProgressEvent {
                    table: cb_table.clone(),
                    rows_written: total,
                });
            }
        }
        QueryStreamEvent::Done { .. } => {
            if cb_cancel_flag.load(Ordering::Relaxed) {
                return;
            }
            let total = cb_rows.load(Ordering::Relaxed);
            cb_app(ExportProgressEvent {
                table: cb_table.clone(),
                rows_written: total,
            });
            if cb_finished.fetch_add(1, Ordering::Relaxed) == 0 {
                if let Err(error) = lock_export_stream(&cb_fmt).finish() {
                    *lock_export_stream(&cb_format_error) = Some(error.to_string());
                }
            }
        }
        QueryStreamEvent::StatementEnd { .. } => {}
    });

    if let Err(error) = fetcher::stream_sql_source(
        state,
        &request.db_session_id,
        &sql,
        database.as_deref(),
        schema.as_deref(),
        &callback,
    )
    .await
    {
        // Cancel wins over the stream's own error: the run stopped because
        // the user asked it to, and the caller removes the partial file.
        if cancel.is_cancelled() {
            return Err(CommandError::Cancelled);
        }
        return Err(error);
    }

    if cancel.is_cancelled() {
        return Err(CommandError::Cancelled);
    }

    if let Some(error) = lock_export(&format_error)?.take() {
        return Err(CommandError::Validation(format!(
            "binary export failed: {error}"
        )));
    }

    // Ensure finish runs even if no Done event fired (same tail fallback
    // as the text path); the formatter itself guards against a second call.
    if finished.load(Ordering::Relaxed) == 0 {
        lock_export(&formatter)?.finish()?;
    }

    Ok(rows_written.load(Ordering::Relaxed))
}

/// One entry point for a planned data file: text formats stream through
/// [`write_data_file`], binary formats through [`write_binary_file`]. A DDL
/// prefix only ever accompanies a SQL (text) format — a binary file cannot
/// carry one, so requesting it is a programming error.
pub async fn write_plan_data_file(
    app: &dyn ExportEvents,
    state: &dyn ExportSource,
    request: &ExportTablesRequest,
    table: &TableExportInput,
    format: DataFormat,
    ddl_prefix: Option<&str>,
    dest: &Path,
    cancel: &CancelToken,
) -> Result<u64, CommandError> {
    if format.is_binary() {
        if ddl_prefix.is_some() {
            return Err(CommandError::Validation(format!(
                "{format:?} export cannot carry a DDL prefix"
            )));
        }
        write_binary_file(app, state, request, table, format, dest, cancel).await
    } else {
        write_data_file(app, state, request, table, format, ddl_prefix, dest, cancel).await
    }
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
    cancel: &CancelToken,
) -> Result<u64, CommandError> {
    // A cancel that landed before the first byte: never create the temp dir.
    if cancel.is_cancelled() {
        return Err(CommandError::Cancelled);
    }
    let temp_dir = temp_export_dir();
    std::fs::create_dir_all(&temp_dir).map_err(CommandError::Io)?;

    let mut rows_total: u64 = 0;
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    let mut build_err: Option<CommandError> = None;

    for plan in plans {
        // Stop the batch between tables: a cancel ends the whole run, not
        // just the table in flight.
        if cancel.is_cancelled() {
            build_err = Some(CommandError::Cancelled);
            break;
        }
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
            } => match write_plan_data_file(
                app,
                state,
                request,
                table,
                *format,
                ddl_prefix.as_deref(),
                &dest,
                cancel,
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
        // A cancel that landed while the last table finished must not
        // assemble a zip the user asked to stop.
        if cancel.is_cancelled() {
            build_err = Some(CommandError::Cancelled);
        }
    }
    if build_err.is_none() {
        let file = std::fs::File::create(target).map_err(CommandError::Io)?;
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, path) in &entries {
            if cancel.is_cancelled() {
                build_err = Some(CommandError::Cancelled);
                break;
            }
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
        // A cancelled zip leaves no partial archive behind: the staging
        // dir is gone above, and the half-written target goes here.
        if matches!(e, CommandError::Cancelled) {
            let _ = std::fs::remove_file(target);
        }
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
    cancel: &CancelToken,
) -> Result<u64, CommandError> {
    // A cancel that landed before the first byte: never create the file.
    if cancel.is_cancelled() {
        return Err(CommandError::Cancelled);
    }
    if plans.len() == 1 {
        return match &plans[0] {
            FilePlan::Ddl { text, .. } => {
                if cancel.is_cancelled() {
                    return Err(CommandError::Cancelled);
                }
                std::fs::write(target, text.as_bytes()).map_err(CommandError::Io)?;
                Ok(0)
            }
            FilePlan::Data {
                table,
                format,
                ddl_prefix,
                ..
            } => {
                match write_plan_data_file(
                    app,
                    state,
                    request,
                    table,
                    *format,
                    ddl_prefix.as_deref(),
                    target,
                    cancel,
                )
                .await
                {
                    Ok(n) => {
                        if cancel.is_cancelled() {
                            let _ = std::fs::remove_file(target);
                            Err(CommandError::Cancelled)
                        } else {
                            Ok(n)
                        }
                    }
                    Err(e) => {
                        // Only a cancel removes the partial file: a real
                        // failure keeps today's behaviour (the partial
                        // file stays for inspection).
                        if cancel.is_cancelled() {
                            let _ = std::fs::remove_file(target);
                            Err(CommandError::Cancelled)
                        } else {
                            Err(e)
                        }
                    }
                }
            }
        };
    }

    // Binary + multiple plans must never reach the text-merge branch below:
    // concatenating XLSX bytes would corrupt the file. `prepare_export`
    // already routes every multi-plan non-SQL export to zip; this guard is
    // the second line of defence for direct callers.
    if plans
        .iter()
        .any(|plan| matches!(plan, FilePlan::Data { format, .. } if format.is_binary()))
    {
        return Err(CommandError::Validation(
            "binary export with multiple files must use zip output".into(),
        ));
    }

    // Multiple files -> combine into one target, streaming each table's temp
    // file through the sink in bounded chunks so the whole export is never
    // held in memory (only one table's temp file at a time, on disk).
    let mut sink = FileSink::create(target)?;
    let mut rows_total: u64 = 0;
    for plan in plans {
        // Stop the batch between tables: a cancel ends the whole run, not
        // just the table in flight.
        if cancel.is_cancelled() {
            let _ = std::fs::remove_file(target);
            return Err(CommandError::Cancelled);
        }
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
                let n = match write_plan_data_file(
                    app,
                    state,
                    request,
                    table,
                    *format,
                    ddl_prefix.as_deref(),
                    &tmp,
                    cancel,
                )
                .await
                {
                    Ok(n) => n,
                    Err(e) => {
                        let _ = std::fs::remove_file(&tmp);
                        if cancel.is_cancelled() {
                            let _ = std::fs::remove_file(target);
                            return Err(CommandError::Cancelled);
                        }
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
    if cancel.is_cancelled() {
        let _ = std::fs::remove_file(target);
        return Err(CommandError::Cancelled);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn request(mode: ExportMode, data_format: DataFormat) -> ExportTablesRequest {
        ExportTablesRequest {
            db_session_id: "sess-1".into(),
            cancel_token: None,
            database_type: None,
            database: None,
            schema: None,
            mode,
            data_format,
            output_mode: OutputMode::Single,
            tables: vec![TableExportInput {
                table_name: "users".into(),
                schema: None,
                columns: vec![],
                ddl: Some("CREATE TABLE users (id INT);".into()),
                source: TableDataSource::Table,
                primary_key: None,
            }],
        }
    }

    #[test]
    fn default_export_name_uses_sql_for_structure_only() {
        // A structure-only export only ever produces DDL text, whatever the
        // data format says — naming it .xlsx would mislabel DDL as a workbook.
        let name =
            default_export_name(&request(ExportMode::StructureOnly, DataFormat::Xlsx), false);
        assert!(name.starts_with("export_structure_"), "{name}");
        assert!(name.ends_with(".sql"), "{name}");

        let name = default_export_name(&request(ExportMode::StructureOnly, DataFormat::Csv), false);
        assert!(name.ends_with(".sql"), "{name}");

        // Multi-table structure_only also stays a single merged .sql file
        // (every plan is {base}.sql, so prepare_export never routes to zip).
        let mut multi = request(ExportMode::StructureOnly, DataFormat::Xlsx);
        multi.tables.push(TableExportInput {
            table_name: "orders".into(),
            schema: None,
            columns: vec![],
            ddl: Some("CREATE TABLE orders (id INT);".into()),
            source: TableDataSource::Table,
            primary_key: None,
        });
        let (_, use_zip) = prepare_export(&multi).unwrap();
        assert!(!use_zip);
        let name = default_export_name(&multi, use_zip);
        assert!(name.ends_with(".sql"), "{name}");
    }

    #[test]
    fn default_export_name_keeps_format_extension_for_data_exports() {
        let name = default_export_name(&request(ExportMode::DataOnly, DataFormat::Xlsx), false);
        assert!(name.starts_with("export_data_"), "{name}");
        assert!(name.ends_with(".xlsx"), "{name}");
    }

    #[test]
    fn default_export_name_uses_zip_when_zipped() {
        let name = default_export_name(
            &request(ExportMode::DataAndStructure, DataFormat::Xlsx),
            true,
        );
        assert!(name.starts_with("export_full_"), "{name}");
        assert!(name.ends_with(".zip"), "{name}");
    }
}

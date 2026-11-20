//! The three orchestration functions of `lib/batchExportJob.ts` (T326).
//!
//! React's options are injected async functions; here each becomes a `FnMut`
//! seam and each `await` becomes `Result` propagation (the `ai_sessions` T040
//! precedent). The only shared state is the idempotent cancel flag, held in an
//! [`Rc<RefCell<_>>`] so a handle published before the work can be called by a
//! host from outside the runner (e.g. the dialog's Cancel button).

use std::cell::RefCell;
use std::rc::Rc;

use super::types::{
    BatchExportDataFormat, BatchExportMode, BatchExportOutputMode, BatchExportProgress,
    ExportTableInput, ExportTablesRequest, ExportTablesResult, LoadedTableExportData,
    RunBatchExportJobResult, SingleFetchOutcome, SingleTableExportResult, SingleTableFetchConfig,
    SingleTableFetchScope, SingleTableStructureExportResult,
};

/// React's `cancel` closure, shared between the runner and the caller that
/// received it through `onCancelHandle`. `cancel()` is idempotent: the first
/// call flips the flag and performs the cancel; every later call is a no-op.
/// The injected closure swallows its own error, mirroring React's
/// `void cancelExport(token).catch(() => undefined)`.
#[derive(Clone)]
pub struct CancelHandle {
    inner: Rc<RefCell<CancelInner>>,
}

struct CancelInner {
    requested: bool,
    cancel: Box<dyn FnMut()>,
}

impl CancelHandle {
    pub(crate) fn new(cancel: Box<dyn FnMut()>) -> Self {
        Self {
            inner: Rc::new(RefCell::new(CancelInner { requested: false, cancel })),
        }
    }

    /// The host-facing `cancel()` — safe to call any number of times.
    pub fn cancel(&self) {
        let mut inner = self.inner.borrow_mut();
        if inner.requested {
            return;
        }
        inner.requested = true;
        (inner.cancel)();
    }

    /// `cancelRequested` in React's loop guards — read before every table and
    /// once more before the exporter is called.
    pub fn is_requested(&self) -> bool {
        self.inner.borrow().requested
    }
}

/// React `RunBatchExportJobOptions`. Every injected function is required in
/// Rust: React's defaults reach the platform ports (`requireTablesExporter`,
/// `crypto.randomUUID`) that this crate deliberately does not own — the host
/// binds them when it constructs the options.
pub struct RunBatchExportJobOptions {
    pub table_names: Vec<String>,
    pub mode: BatchExportMode,
    pub data_format: BatchExportDataFormat,
    pub output_mode: BatchExportOutputMode,
    pub database_type: Option<String>,
    pub db_session_id: String,
    pub database: Option<String>,
    pub schema: Option<String>,
    /// `loadTableExportData` — awaited once per table, in order.
    pub load_table_export_data: Box<dyn FnMut(&str) -> Result<LoadedTableExportData, String>>,
    pub on_progress: Option<Box<dyn FnMut(&BatchExportProgress)>>,
    /// The bound tables exporter. `Ok(None)` is React's `null` result.
    pub export_tables:
        Box<dyn FnMut(&ExportTablesRequest) -> Result<Option<ExportTablesResult>, String>>,
    /// The bound `cancelExport(token)` port.
    pub cancel_export: Box<dyn FnMut(&str) -> Result<bool, String>>,
    /// Receives the [`CancelHandle`] before any table is loaded.
    pub on_cancel_handle: Option<Box<dyn FnOnce(CancelHandle)>>,
    /// React `crypto.randomUUID()` — one token for the whole combined request.
    pub new_id: Box<dyn FnMut() -> String>,
}

/// React `runBatchExportJob`. The stage sequence is fixed:
/// validate → mint token + publish handle → per-table metadata (with progress)
/// → final cancel guard → one combined exporter call → result map. Every
/// terminal branch is one-shot; no resume/retry is added (the kernel's job
/// system owns recovery, per the repo invariants).
pub fn run_batch_export_job(
    options: RunBatchExportJobOptions,
) -> Result<RunBatchExportJobResult, String> {
    let RunBatchExportJobOptions {
        table_names,
        mode,
        data_format,
        output_mode,
        database_type,
        db_session_id,
        database,
        schema,
        mut load_table_export_data,
        mut on_progress,
        mut export_tables,
        cancel_export,
        on_cancel_handle,
        mut new_id,
    } = options;

    if table_names.is_empty() {
        return Err("no_tables_selected".to_owned());
    }
    if db_session_id.is_empty() {
        return Err("Missing connection".to_owned());
    }

    // One token for the whole batch — it is a single combined Rust request,
    // so cancelling interrupts the table in flight and stops the batch.
    let cancel_token = (new_id)();
    let token_for_cancel = cancel_token.clone();
    let mut cancel_export = cancel_export;
    let handle = CancelHandle::new(Box::new(move || {
        let _ = cancel_export(&token_for_cancel);
    }));
    // The handle goes out before any table is loaded, so a cancel during
    // metadata loading stops the batch before the exporter is ever called.
    if let Some(cb) = on_cancel_handle {
        cb(handle.clone());
    }

    let total = table_names.len();
    let mut tables = Vec::with_capacity(total);
    for (index, table_name) in table_names.iter().enumerate() {
        if handle.is_requested() {
            return Ok(RunBatchExportJobResult::Aborted { partial_deleted: true });
        }
        if let Some(cb) = on_progress.as_mut() {
            cb(&BatchExportProgress {
                current: index + 1,
                total,
                table_name: table_name.clone(),
            });
        }
        let loaded = (load_table_export_data)(table_name)?;
        tables.push(ExportTableInput {
            // React `meta.tableName ?? tableName` — a dirty double that omits
            // the name falls back to the requested one. `Some("")` is kept.
            table_name: loaded.table_name.unwrap_or_else(|| table_name.clone()),
            schema: loaded.schema,
            columns: loaded.columns.into_iter().map(|column| column.name).collect(),
            ddl: loaded.ddl,
        });
    }

    // A cancel during the last table's load has no further loop iteration to
    // catch it — React checks again before calling the exporter so the batch
    // never starts after a cancel.
    if handle.is_requested() {
        return Ok(RunBatchExportJobResult::Aborted { partial_deleted: true });
    }

    let request = ExportTablesRequest {
        db_session_id,
        cancel_token: Some(cancel_token),
        database_type,
        database,
        schema,
        mode,
        data_format,
        output_mode,
        tables,
    };

    let result = (export_tables)(&request)?;
    Ok(match result {
        Some(ExportTablesResult::Aborted { partial_deleted }) => {
            RunBatchExportJobResult::Aborted { partial_deleted }
        }
        Some(ExportTablesResult::Saved(_)) => RunBatchExportJobResult::Saved,
        Some(ExportTablesResult::Cancelled) | None => RunBatchExportJobResult::Cancelled,
    })
}

/// React `SingleTableExportOptions`. `SqlDataFetcher` is a cross-task port
/// (`lib/dataFetcher.ts`), so this file owns only the wiring *around* it:
/// `fetch` is the bound `new SqlDataFetcher(config).fetch()` and `cancel_fetch`
/// is its `cancel()` (which calls the fetcher's own minted token). The React
/// `exportTables` / `cancelExport` / `subscribeProgress` fields are consumed by
/// that fetcher adapter, not re-injected here.
pub struct SingleTableExportOptions {
    pub db_session_id: String,
    pub database_type: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table_name: String,
    pub columns: Vec<String>,
    pub data_format: BatchExportDataFormat,
    pub sql: Option<String>,
    pub primary_key: Option<String>,
    /// The bound fetcher's `fetch()`, receiving the derived config.
    pub fetch: Box<dyn FnMut(&SingleTableFetchConfig) -> Result<SingleFetchOutcome, String>>,
    /// The bound fetcher's `cancel()`.
    pub cancel_fetch: Box<dyn FnMut() -> Result<bool, String>>,
    pub on_cancel_handle: Option<Box<dyn FnOnce(CancelHandle)>>,
}

/// React `exportSingleTableViaRust`: build the fetcher config, publish its
/// cancel handle, then map the fetch outcome. `aborted` reports the fetcher's
/// partial-deletion flag with React's `?? false` fallback.
pub fn export_single_table_via_rust(
    options: SingleTableExportOptions,
) -> Result<SingleTableExportResult, String> {
    let SingleTableExportOptions {
        db_session_id,
        database_type,
        database,
        schema,
        table_name,
        columns,
        data_format,
        sql,
        primary_key,
        mut fetch,
        cancel_fetch,
        on_cancel_handle,
    } = options;

    // React's `sql ? 'filtered_table' : 'entire_table'` uses JS truthiness:
    // an absent *or empty* statement means the entire table.
    let scope = match sql.as_deref() {
        Some(statement) if !statement.is_empty() => SingleTableFetchScope::FilteredTable,
        _ => SingleTableFetchScope::EntireTable,
    };
    let config = SingleTableFetchConfig {
        table_name,
        scope,
        db_session_id,
        database_type,
        database,
        schema,
        sql,
        mode: BatchExportMode::DataOnly,
        data_format,
        output_mode: BatchExportOutputMode::Single,
        column_names: columns,
        primary_key,
    };

    let mut cancel_fetch = cancel_fetch;
    let handle = CancelHandle::new(Box::new(move || {
        let _ = cancel_fetch();
    }));
    if let Some(cb) = on_cancel_handle {
        cb(handle);
    }

    let outcome = (fetch)(&config)?;
    Ok(match outcome {
        SingleFetchOutcome::Saved => SingleTableExportResult::Saved,
        SingleFetchOutcome::Cancelled => SingleTableExportResult::Cancelled,
        SingleFetchOutcome::Aborted { partial_deleted } => SingleTableExportResult::Aborted {
            partial_deleted: partial_deleted.unwrap_or(false),
        },
    })
}

/// React `SingleTableStructureExportOptions`. The exporter is injected rather
/// than defaulted to `requireTablesExporter()` (a commands-layer port).
pub struct SingleTableStructureExportOptions {
    pub db_session_id: String,
    pub database_type: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table_name: String,
    pub ddl: Option<String>,
    pub export_tables:
        Box<dyn FnMut(&ExportTablesRequest) -> Result<Option<ExportTablesResult>, String>>,
}

/// React `exportSingleTableStructureViaRust`: one `structure_only` request,
/// `sql_insert` format (so the native dialog offers `.sql`), `single` output,
/// exactly one table with no columns. A missing session rejects; every result
/// but `Saved` maps to `cancelled`.
pub fn export_single_table_structure_via_rust(
    options: SingleTableStructureExportOptions,
) -> Result<SingleTableStructureExportResult, String> {
    let SingleTableStructureExportOptions {
        db_session_id,
        database_type,
        database,
        schema,
        table_name,
        ddl,
        mut export_tables,
    } = options;

    if db_session_id.is_empty() {
        return Err("Missing connection".to_owned());
    }

    let request = ExportTablesRequest {
        db_session_id,
        cancel_token: None,
        database_type,
        database,
        schema: schema.clone(),
        mode: BatchExportMode::StructureOnly,
        data_format: BatchExportDataFormat::SqlInsert,
        output_mode: BatchExportOutputMode::Single,
        tables: vec![ExportTableInput {
            table_name,
            schema,
            columns: Vec::new(),
            ddl,
        }],
    };

    let result = (export_tables)(&request)?;
    Ok(match result {
        Some(ExportTablesResult::Saved(_)) => SingleTableStructureExportResult::Saved,
        _ => SingleTableStructureExportResult::Cancelled,
    })
}

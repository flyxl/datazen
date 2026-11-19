//! T043-S1: the web-shaped tables exporter — types, capability gates,
//! DDL resolution and shared planning.
//!
//! Rune port of `packages/frontend/src/platform/blobTablesExporter.ts`
//! (376 lines), slice 1 of 2 (1.0d). The other half of the form-split F4
//! dual-flow demonstration: the same `exportTables(request)` signature,
//! answered without a host. Table pages come from the (transport-agnostic)
//! table reader the caller injects, and the assembled CSV is delivered
//! with a Blob download, the browser's own path-free equivalent of the
//! save-dialog token writer.
//!
//! ## Slice boundary — S1 (this file) vs S2
//!
//! S1 owns everything up to the emission stage:
//!
//! * types: [`TablePage`], [`TablePageRequestContext`],
//!   [`TablePageReader`];
//! * pure functions: [`csv_cell`] ([`escape_csv_field`] +
//!   [`apply_csv_formula_guard`]), [`download_blob`],
//!   [`base64_to_bytes`];
//! * the factory [`create_blob_tables_exporter`] and the gate + plan
//!   stage [`BlobTablesExporter::plan_export`]: capability gates (in
//!   React order), DDL resolution, shared planning through
//!   [`export_plan::prepare_export`], `useZip` refusal, `requestContext`
//!   assembly and the [`export_plan::default_export_name`] file name.
//!
//! S1 deliberately does **not** do S2's work: `emitDataPlan`'s two
//! branches, the single/multi plan merge, `deliver`, or the `Saved`
//! return. Instead [`BlobTablesExporter::plan_export`] returns the
//! gate + plan result as a [`PlannedExport`] value, which is the seam S2's
//! emission stage consumes. The factory already takes the pieces S2
//! needs (`readPage`, `deliver`, `now`); the reader and the deliver sink
//! are stored untouched by S1 and read by S2.
//!
//! ## The S2 emission contract (fixed here, implemented by S2)
//!
//! S2 adds one method to [`BlobTablesExporter`], mirroring React's
//! `emitDataPlan(plan: Extract<ExportFilePlan, { kind: 'data' }>)`:
//!
//! ```rust,ignore
//! // S2 sketch — the signature S2 implements against `PlannedExport`:
//! pub(crate) struct EmittedDataPlan {
//!     // Text for every format except XLSX, which delivers decoded
//!     // workbook bytes (`base64_to_bytes`).
//!     pub content: DeliveredContent,
//!     pub mime_type: String,
//! }
//!
//! impl BlobTablesExporter {
//!     async fn emit_data_plan(
//!         &self,
//!         planned: &PlannedExport,
//!         plan: &ExportFilePlan, // must be the `Data` variant
//!     ) -> Result<EmittedDataPlan, BlobTablesExportError>;
//! }
//! ```
//!
//! Its two branches, in React order:
//!
//! * `provided` source — the caller's own rows, formatted in JS instead
//!   of re-reading the table. CSV goes through the [`csv_cell`] emission
//!   below (not `formatRowsAsText`: this exporter writes `\n`-terminated
//!   lines, the streaming shape the Rust writer produces, while
//!   `formatRowsAsText` joins lines with no trailing newline); XLSX
//!   through `formatRowsAsXlsx` + [`base64_to_bytes`]; Parquet is
//!   refused (`"provided-data export cannot format 'parquet'"`); every
//!   other format through `formatRowsAsText`. Those formatters live in
//!   `lib/exportFormat.ts` (T500); until T500 lands, S2 formats the CSV
//!   branch from [`csv_cell`] and refuses the rest loudly rather than
//!   inventing a formatter.
//! * table source — page until the reader says the table is exhausted
//!   (`nextCursor: null`); the first page contributes the header line,
//!   every page contributes its rows as [`csv_cell`] lines; each page
//!   call overrides the request context's schema with the table's own
//!   (`schema: table.schema ?? requestContext.schema`).
//!
//! Then S2 merges like the desktop single-file merge (one plan is
//! written directly; several plans are concatenated with
//! `-- ===== {filename} =====` separators — several plans only reach
//! emission when they are all `.sql`, anything else needed a zip and
//! was refused in [`BlobTablesExporter::plan_export`]), refuses a binary
//! plan in a merge (`'this export produces a binary file that cannot be
//! merged into a single download'`), delivers
//! (`deliver(content, file_name, mime_type)`) and returns
//! `{ Saved: request.tables.len() }`.
//!
//! ## Dependency mirrors — what exists in Rune, what is local
//!
//! Checked before writing (T043-S1): none of the React dependencies has
//! a Rune port yet, so this slice carries the smallest local mirrors and
//! wiring ports, each documented where it lives:
//!
//! * `prepareExport` / `defaultExportName` / `ExportFilePlan`
//!   (`./exportPlan`) → [`export_plan`] in this directory: the minimal
//!   planner mirror S1 needs, pending T189's full port of
//!   `platform/exportPlan.ts`.
//! * `escapeCsvField` (`../lib/gridClipboard`) → [`escape_csv_field`]
//!   here: the single-field face of the CSV escaping kernel (RFC 4180
//!   quoting, objects via JSON, null as empty), pending T523's port of
//!   `lib/gridClipboard.ts`.
//! * `applyCsvFormulaGuard` / `formatRowsAsText` / `formatRowsAsXlsx`
//!   (`../lib/exportFormat`) → [`apply_csv_formula_guard`] here for the
//!   guard only; the two formatters are S2's and arrive with T500's port
//!   of `lib/exportFormat.ts`.
//! * `TablesExporterPort` / `TablesExportRequest` (`./contracts`) → the
//!   request types live in [`export_plan`] (minimal mirror of
//!   `commands/file.ts`, pending T298); the port trait itself is T131's
//!   (`platform/contracts.ts`) — S1's factory returns the concrete
//!   [`BlobTablesExporter`], and T131/S2 add the port implementation.
//!
//! ## The browser half is a wiring port, not a re-implementation
//!
//! Rune has no Blob / URL / anchor API in this crate (no `web-sys`
//! dependency; the platform's own JS uses `Blob` for clipboard and
//! fonts, but that is the platform, not an API the UI crate can call),
//! so [`download_blob`] is the pure half of React's `downloadBlob`: it
//! builds the [`BlobDownload`] request — contents, file name, and the
//! MIME type pinned to `{mime};charset=utf-8` (default `text/csv`) —
//! and the host performs the Blob + object URL + anchor click + revoke
//! from that request. Same precedent as `support/chart_export.rs`: the
//! pure port defines the request contract; the host effect stays with
//! the host.
//!
//! By the same token the factory's `deliver` has no Rune default:
//! React's default *is* the real Blob download, which cannot run here,
//! so an exporter built without an injected sink carries `None` and
//! S2's emission fails loudly with
//! [`BlobTablesExportError::NoDownloadSink`] instead of silently
//! dropping the file. The web assembly must inject its Blob-download
//! sink (built on [`download_blob`]) when it binds this exporter.
//!
//! ## The formula guard stays in the CSV export layer
//!
//! [`csv_cell`] is `escapeCsvField` + `applyCsvFormulaGuard`, exactly as
//! in React. The guard belongs to the CSV export layer only: it must
//! never move into a shared escaping kernel, where copy/paste treats a
//! formula prefix as inert text.
//!
//! ## Base64 stays local
//!
//! [`base64_to_bytes`] is the same decode `ProvidedDataFetcher` does
//! (`atob`: ASCII whitespace stripped first, then standard base64),
//! kept local here so this module never imports the data-fetcher port
//! (which would cycle back through this module's [`download_blob`]).

#![allow(dead_code)]
// S1 seam items consumed by S2's emission, not by S1 itself: the injected
// `reader` and `deliver` fields, `csv_cell`, `download_blob` and
// `base64_to_bytes`. The `NoDownloadSink` / `ReadFailed` error variants
// are likewise S2's (emission / page loop).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{DateTime, Utc};
use serde_json::Value;

use super::export_plan::{
    default_export_name, prepare_export, ExportDataFormat, ExportFilePlan, ExportMode,
    ExportOutputMode, ExportPlanError, ExportTableInput, ExportTableSource, TablesExportRequest,
};

/// Why a web-shaped export refused or failed. The gate / planner messages
/// are the React `Error` messages verbatim; the last three variants have
/// no React counterpart (they are Rune-side wiring failures) and say so.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BlobTablesExportError {
    /// No tables in the request (`'no tables selected'`).
    NoTablesSelected,
    /// A non-CSV data format for a source that is not already loaded.
    UnsupportedFormat { format: ExportDataFormat },
    /// `zip` output (`'zip output is not supported by the web exporter'`).
    ZipOutputUnsupported,
    /// A `sql` source outside `structure_only` (`'filtered export is not
    /// supported by the web exporter'`).
    FilteredExportUnsupported,
    /// The plan needs a zip (`'this export produces multiple files and
    /// needs a zip archive, …'`).
    MultiFileNeedsZip,
    /// Shared planning refused the request; the message is the planner's.
    Plan(ExportPlanError),
    /// `base64_to_bytes` met input `atob` would reject.
    InvalidBase64,
    /// The injected page reader failed (S2's page loop; the message is
    /// the reader's own — there is no React counterpart to mirror).
    ReadFailed(String),
    /// S2's emission is due a download but no deliver sink was wired
    /// (there is no React counterpart: the browser can always download).
    NoDownloadSink,
}

impl std::fmt::Display for BlobTablesExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlobTablesExportError::NoTablesSelected => f.write_str("no tables selected"),
            BlobTablesExportError::UnsupportedFormat { format } => {
                write!(f, "'{format}' format is not supported by the web exporter")
            }
            BlobTablesExportError::ZipOutputUnsupported => {
                f.write_str("zip output is not supported by the web exporter")
            }
            BlobTablesExportError::FilteredExportUnsupported => {
                f.write_str("filtered export is not supported by the web exporter")
            }
            BlobTablesExportError::MultiFileNeedsZip => f.write_str(
                "this export produces multiple files and needs a zip archive, \
                     which is not supported by the web exporter; export one table at a time \
                     in data-only mode, or use the desktop app",
            ),
            BlobTablesExportError::Plan(err) => std::fmt::Display::fmt(err, f),
            BlobTablesExportError::InvalidBase64 => f.write_str("invalid base64 payload"),
            BlobTablesExportError::ReadFailed(message) => f.write_str(message),
            BlobTablesExportError::NoDownloadSink => f.write_str(
                "no Blob download sink is wired for the web exporter; \
                     the host must inject a deliver sink",
            ),
        }
    }
}

impl std::error::Error for BlobTablesExportError {}

impl From<ExportPlanError> for BlobTablesExportError {
    fn from(err: ExportPlanError) -> Self {
        BlobTablesExportError::Plan(err)
    }
}

/// One page of a table, as the injected reader answers it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TablePage {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    /// Paging cursor the reader understands; `None` when the table is
    /// exhausted (React `nextCursor: null`).
    pub next_cursor: Option<Value>,
}

/// The request-scoped read target, forwarded to the reader on every page
/// call (WR-E3): `db_session_id` and the database/schema/driver hints
/// come from `exportTables(request)` — the reader cannot see the request,
/// so the exporter carries these across the port. The per-table `schema`
/// (the table's own or the request's fallback) overrides the request
/// default per call (applied by S2's page loop, not here).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TablePageRequestContext {
    pub db_session_id: String,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub driver_type: Option<String>,
}

/// Reads one page of one table. The kernel `getTableData` fits this shape
/// — the third argument is the request-scoped context the reader needs to
/// address the read (see [`TablePageRequestContext`]); readers that close
/// over their own target may ignore it. The first call passes
/// `cursor: None`; later calls pass the previous page's `next_cursor`.
/// A reader failure is reported as [`BlobTablesExportError::ReadFailed`].
pub(crate) trait TablePageReader: Send + Sync {
    fn read_page(
        &self,
        table_name: &str,
        cursor: Option<&Value>,
        context: &TablePageRequestContext,
    ) -> Pin<Box<dyn Future<Output = Result<TablePage, BlobTablesExportError>> + Send + '_>>;
}

/// Loads one table's DDL (structure modes). Answers `None` when it has no
/// DDL for the table — the exporter then leaves the request's own `ddl`
/// (or nothing, and the planner falls back to its placeholder).
pub(crate) trait DdlLoader: Send + Sync {
    fn load_ddl(
        &self,
        table_name: &str,
    ) -> Pin<Box<dyn Future<Output = Option<String>> + Send + '_>>;
}

/// One cell as a JSON value, as text — `cellToText(value, '')` in
/// `gridClipboard.ts`: null is empty, strings pass through, booleans and
/// numbers render as JS `String(value)`, objects and arrays render as
/// JSON.
///
/// Numbers render through `serde_json` (ryu): integral floats render as
/// `1.0` where JS `String(1)` renders `1`, and large exponents render as
/// `1e21` where JS renders `1e+21`. Integers and ordinary decimals match
/// JS exactly; the divergence only bites floats that arrive on the wire
/// as `1.0` / in exponent form.
fn cell_to_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

/// Quote one field — `escapeDelimited` in `gridClipboard.ts`: a field is
/// quoted when it contains the delimiter, a quote or a line break; inner
/// quotes are doubled.
fn escape_delimited(text: &str, delimiter: char) -> String {
    if text.contains(delimiter) || text.contains('"') || text.contains('\n') || text.contains('\r')
    {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

/// One value as an escaped CSV field — the single-field face of the kernel
/// (`escapeCsvField` in `gridClipboard.ts`, DB-10), for callers that
/// format one cell at a time instead of a matrix. Minimal mirror pending
/// T523's port of `lib/gridClipboard.ts`.
pub(crate) fn escape_csv_field(value: &Value) -> String {
    escape_delimited(&cell_to_text(value), ',')
}

/// The formula-injection guard — `applyCsvFormulaGuard` in
/// `exportFormat.ts`: a field starting with `=`, `+`, `-` or `@` is
/// wrapped in quotes unless it is already quoted. Minimal mirror pending
/// T500's port of `lib/exportFormat.ts`.
///
/// The guard stays out of the shared escaping kernel, where copy/paste
/// treats a formula prefix as inert text; it belongs to the CSV export
/// layer only.
pub(crate) fn apply_csv_formula_guard(escaped: &str) -> String {
    let starts_with_formula_prefix = matches!(escaped.chars().next(), Some('=' | '+' | '-' | '@'));
    if starts_with_formula_prefix && !escaped.starts_with('"') {
        format!("\"{escaped}\"")
    } else {
        escaped.to_owned()
    }
}

/// One cell as a CSV field: the standard escaping from `gridClipboard`
/// (`escapeCsvField` — RFC 4180 quoting, objects via JSON, null as empty),
/// plus the formula-injection guard shared with the other two CSV paths
/// (`applyCsvFormulaGuard` in `exportFormat.ts` — also the Rust
/// `StreamFormatter` rule).
pub(crate) fn csv_cell(value: &Value) -> String {
    apply_csv_formula_guard(&escape_csv_field(value))
}

/// What a download delivers — `BlobPart` in React: text for every format
/// except XLSX, which delivers decoded workbook bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DeliveredContent {
    Text(String),
    Bytes(Vec<u8>),
}

/// A Blob download, ready for the host to perform — the pure half of
/// React's `downloadBlob`. The host turns this into a `Blob`, an object
/// URL, an anchor click with `download = file_name`, and then revokes
/// the URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BlobDownload {
    pub contents: DeliveredContent,
    pub file_name: String,
    /// The MIME type with `;charset=utf-8` pinned, exactly as React's
    /// `new Blob([contents], { type: \`${mimeType};charset=utf-8\` })`.
    pub mime_type: String,
}

/// Build the Blob download request for `contents` as `file_name`.
/// `mime_type` defaults to `'text/csv'` in React; here it is explicit so
/// S2 always passes the emitted plan's MIME type.
pub(crate) fn download_blob(
    contents: DeliveredContent,
    file_name: &str,
    mime_type: &str,
) -> BlobDownload {
    BlobDownload {
        contents,
        file_name: file_name.to_owned(),
        mime_type: format!("{mime_type};charset=utf-8"),
    }
}

/// Base64 → bytes for an XLSX Blob download (the same decode
/// `ProvidedDataFetcher` does; kept local — importing it from the
/// data-fetcher port would cycle back through this module's
/// [`download_blob`]). `atob` strips ASCII whitespace before decoding,
/// and throws on anything that is not standard base64; both are mirrored
/// here.
pub(crate) fn base64_to_bytes(data_base64: &str) -> Result<Vec<u8>, BlobTablesExportError> {
    let cleaned: String = data_base64
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect();
    STANDARD
        .decode(cleaned.as_bytes())
        .map_err(|_| BlobTablesExportError::InvalidBase64)
}

/// The deliver sink: performs the [`BlobDownload`] the emission stage
/// hands it (Blob + object URL + anchor click + revoke on the web host).
/// `None` in the factory options means no sink is wired — see the module
/// docs.
pub(crate) type DeliverFn = Arc<dyn Fn(&BlobDownload) + Send + Sync>;

/// The clock for the default output name; tests pin it, production uses now.
pub(crate) type NowFn = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;

/// Factory options — the `options?` object of React's
/// `createBlobTablesExporter`.
#[derive(Default)]
pub(crate) struct BlobTablesExporterOptions {
    /// Loads one table's DDL (structure modes). Absent ⇒ structure modes
    /// emit a placeholder comment (via the planner's
    /// [`export_plan::resolve_ddl`]).
    pub load_ddl: Option<Arc<dyn DdlLoader>>,
    /// Override for tests / the host sink; the default performs the real
    /// Blob download — which has no Rune equivalent in this crate, so an
    /// absent sink means S2's emission fails loudly with
    /// [`BlobTablesExportError::NoDownloadSink`]. `contents` is text for
    /// every format except XLSX, which delivers decoded workbook bytes.
    pub deliver: Option<DeliverFn>,
    /// Clock for the default output name; tests pin it, production uses now.
    pub now: Option<NowFn>,
}

/// The gate + plan result S2's emission stage consumes — the seam between
/// S1 and S2. `request` is the *effective* request: the caller's request
/// with structure-mode DDL resolved into each table (`table.ddl` first,
/// then the injected loader, then nothing — the planner falls back to
/// its placeholder). `use_zip` is always false here: a plan that needs a
/// zip is refused in [`BlobTablesExporter::plan_export`], so S2 can
/// assume a single-file merge.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlannedExport {
    pub request: TablesExportRequest,
    pub plans: Vec<ExportFilePlan>,
    pub file_name: String,
    pub request_context: TablePageRequestContext,
}

/// Build the exporter over an injected page reader.
///
/// `data_only` streams the table's rows as CSV; `structure_only` emits
/// only the DDL plans; `data_and_structure` in CSV is a multi-file plan
/// on the desktop (DDL file + data file) and therefore needs a zip —
/// which the web exporter refuses (see the module docs) instead of
/// merging the two files. A `provided` source is the exception to the
/// CSV-only rule: its rows are already loaded, so every format goes
/// through the shared frontend formatter (`exportFormat.ts`) and
/// downloads as a Blob (plan §2.8).
pub(crate) fn create_blob_tables_exporter(
    read_page: Arc<dyn TablePageReader>,
    options: BlobTablesExporterOptions,
) -> BlobTablesExporter {
    let now: NowFn = options.now.unwrap_or_else(|| Arc::new(|| Utc::now()));
    BlobTablesExporter {
        reader: read_page,
        load_ddl: options.load_ddl,
        deliver: options.deliver,
        now,
    }
}

/// The web-shaped tables exporter (factory shell + gate/plan stage; S2
/// adds the emission stage — see the module docs).
pub(crate) struct BlobTablesExporter {
    /// S2: consumed by `emit_data_plan`'s page loop.
    #[allow(dead_code)]
    reader: Arc<dyn TablePageReader>,
    load_ddl: Option<Arc<dyn DdlLoader>>,
    /// S2: consumed by the emission stage (`deliver` + `Saved`).
    #[allow(dead_code)]
    deliver: Option<DeliverFn>,
    now: NowFn,
}

impl BlobTablesExporter {
    /// Capability gates + DDL resolution + shared planning, in React
    /// order — the first half of `exportTables(request)`, returned as a
    /// [`PlannedExport`] for S2's emission stage.
    ///
    /// The gates fail before any page is read or any file is delivered,
    /// so a refused export never leaves a half-written download behind:
    ///
    /// 1. empty request → `'no tables selected'`;
    /// 2. CSV-only: any data format other than CSV for a source that is
    ///    not already loaded — except a `provided` source (its rows are
    ///    already in the frontend, so every format is available) and
    ///    except `structure_only` (it plans no data file, so the format
    ///    is irrelevant there, exactly as on the desktop). A mixed
    ///    request (a provided table next to a table that still has to be
    ///    read) stays CSV-only: the unread table has no JS formatter to
    ///    ride;
    /// 3. `zip` output is refused;
    /// 4. per table, a `sql` source is refused — except `structure_only`,
    ///    which plans no data file (planning decides it via
    ///    `ddl_for_structure`: explicit `ddl` required, as on desktop).
    ///
    /// Then DDL is resolved for structure modes (the request's own
    /// `table.ddl` first, then the injected loader, then nothing — the
    /// planner falls back to the shared placeholder for real tables),
    /// the shared planner runs ([`prepare_export`]), a plan that needs a
    /// zip is refused, and the request context + default file name are
    /// assembled.
    pub async fn plan_export(
        &self,
        request: TablesExportRequest,
    ) -> Result<PlannedExport, BlobTablesExportError> {
        // 1. Capability gates first, in React order.
        if request.tables.is_empty() {
            return Err(BlobTablesExportError::NoTablesSelected);
        }
        let all_data_from_provided = request
            .tables
            .iter()
            .all(|table| matches!(table.source, ExportTableSource::Provided { .. }));
        if request.mode != ExportMode::StructureOnly
            && request.data_format != ExportDataFormat::Csv
            && !all_data_from_provided
        {
            return Err(BlobTablesExportError::UnsupportedFormat {
                format: request.data_format,
            });
        }
        if request.output_mode == ExportOutputMode::Zip {
            return Err(BlobTablesExportError::ZipOutputUnsupported);
        }
        for table in &request.tables {
            // Item 3c: a `sql` source cannot run here at all — refuse
            // honestly rather than export the unfiltered table. The
            // desktop Rust exporter handles it properly. `structure_only`
            // plans no data file, so it is exempt here.
            if request.mode != ExportMode::StructureOnly
                && matches!(table.source, ExportTableSource::Sql { .. })
            {
                return Err(BlobTablesExportError::FilteredExportUnsupported);
            }
        }

        // 2. Resolve DDL for structure modes before planning: the
        // request's own `table.ddl` first (what the batch job carries —
        // the desktop planner reads the same field), then the injected
        // loader, then nothing and the planner falls back to the shared
        // placeholder for real tables (`ddl_for_structure` still refuses
        // structure modes for SQL / provided-data sources without an
        // explicit `ddl`). Only non-blank strings count, on both steps.
        let mut tables: Vec<ExportTableInput> = Vec::with_capacity(request.tables.len());
        for table in &request.tables {
            if request.mode == ExportMode::DataOnly {
                tables.push(table.clone());
                continue;
            }
            match table.ddl.as_deref() {
                Some(own) if !own.trim().is_empty() => {
                    tables.push(table.clone());
                    continue;
                }
                _ => {}
            }
            let mut resolved = table.clone();
            if let Some(loader) = &self.load_ddl {
                if let Some(loaded) = loader.load_ddl(&table.table_name).await {
                    if !loaded.trim().is_empty() {
                        resolved.ddl = Some(loaded);
                    }
                }
            }
            tables.push(resolved);
        }
        let effective = TablesExportRequest { tables, ..request };

        // 3. Shared planning (same rules as the Rust side): validates
        // sources, plans the files, and decides zip-vs-single. The web
        // build has no zip writer, so a plan that needs a zip is refused
        // here — never a merged CSV masquerading as the requested
        // multi-file export.
        let prepared = prepare_export(&effective)?;
        if prepared.use_zip {
            return Err(BlobTablesExportError::MultiFileNeedsZip);
        }

        // 4. Request context + default file name for S2's emission.
        let request_context = TablePageRequestContext {
            db_session_id: effective.db_session_id.clone(),
            database: effective.database.clone(),
            schema: effective.schema.clone(),
            driver_type: effective.database_type.clone(),
        };
        let file_name = default_export_name(&effective, false, (self.now)());

        Ok(PlannedExport {
            request: effective,
            plans: prepared.plans,
            file_name,
            request_context,
        })
    }
}

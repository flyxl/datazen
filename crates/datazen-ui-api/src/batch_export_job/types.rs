//! Data shapes of the `lib/batchExportJob.ts` port (T326).
//!
//! Every type here mirrors a React export field-for-field. The two `commands/file.ts`
//! wire types ([`ExportTablesRequest`] / [`ExportTablesResult`]) are *local mirrors*:
//! `commands/file.ts` is a separate port task (T298) and this crate must not
//! depend on a type that has not landed here yet. When T298 lands, these mirrors
//! should be replaced by its types rather than forked (the same ownership rule
//! the `ai_streaming` seams use for cross-task vocabulary).

use serde_json::Value;

/// React `BatchExportOutputMode = 'single' | 'zip'`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BatchExportOutputMode {
    #[default]
    Single,
    Zip,
}

impl BatchExportOutputMode {
    pub fn as_str(self) -> &'static str {
        match self {
            BatchExportOutputMode::Single => "single",
            BatchExportOutputMode::Zip => "zip",
        }
    }
}

/// React `BatchExportMode = 'data_only' | 'structure_only' | 'data_and_structure'`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BatchExportMode {
    #[default]
    DataOnly,
    StructureOnly,
    DataAndStructure,
}

impl BatchExportMode {
    pub fn as_str(self) -> &'static str {
        match self {
            BatchExportMode::DataOnly => "data_only",
            BatchExportMode::StructureOnly => "structure_only",
            BatchExportMode::DataAndStructure => "data_and_structure",
        }
    }
}

/// React `BatchExportDataFormat` — the eight wire spellings the Rust exporter
/// accepts (`commands/file.ts::ExportDataFormat`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BatchExportDataFormat {
    #[default]
    Csv,
    Tsv,
    Json,
    Markdown,
    SqlInsert,
    SqlUpdate,
    Xlsx,
    Parquet,
}

impl BatchExportDataFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            BatchExportDataFormat::Csv => "csv",
            BatchExportDataFormat::Tsv => "tsv",
            BatchExportDataFormat::Json => "json",
            BatchExportDataFormat::Markdown => "markdown",
            BatchExportDataFormat::SqlInsert => "sql_insert",
            BatchExportDataFormat::SqlUpdate => "sql_update",
            BatchExportDataFormat::Xlsx => "xlsx",
            BatchExportDataFormat::Parquet => "parquet",
        }
    }
}

impl std::fmt::Display for BatchExportDataFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// React `SingleTableExportResult` (also the alias `RunBatchExportJobResult`):
/// the three honest end states of a Rust-side export run (DBR-06). React spells
/// them as `{ status: 'saved' }`, `{ status: 'cancelled' }` and
/// `{ status: 'aborted'; partialDeleted: boolean }`; a closed enum keeps the
/// union exhaustive here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SingleTableExportResult {
    Saved,
    Cancelled,
    Aborted { partial_deleted: bool },
}

impl SingleTableExportResult {
    /// The React `status` discriminant, kept for callers that read the field.
    pub fn status(self) -> &'static str {
        match self {
            SingleTableExportResult::Saved => "saved",
            SingleTableExportResult::Cancelled => "cancelled",
            SingleTableExportResult::Aborted { .. } => "aborted",
        }
    }
}

/// React `RunBatchExportJobResult = SingleTableExportResult`.
pub type RunBatchExportJobResult = SingleTableExportResult;

/// React `exportSingleTableStructureViaRust`'s return type `'saved' | 'cancelled'`
/// — deliberately *not* the three-state [`SingleTableExportResult`]: the
/// structure path has no partial-file outcome to report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SingleTableStructureExportResult {
    Saved,
    Cancelled,
}

impl SingleTableStructureExportResult {
    pub fn as_str(self) -> &'static str {
        match self {
            SingleTableStructureExportResult::Saved => "saved",
            SingleTableStructureExportResult::Cancelled => "cancelled",
        }
    }
}

/// React `BatchExportProgress` — emitted once per table, *before* its metadata
/// is loaded. `current` is 1-based; `total` is the original `tableNames.length`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchExportProgress {
    pub current: usize,
    pub total: usize,
    /// React's `tableName`.
    pub table_name: String,
}

/// React `{ name: string }` — one column of a `BatchExportTableInput`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BatchExportColumn {
    pub name: String,
}

/// React `BatchExportTableInput`. `rows` is carried for the wire shape (the
/// backend streams rows itself, so the job never buffers them); each entry is
/// a JS row object, mirrored as a `serde_json::Value`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BatchExportTableInput {
    pub table_name: String,
    pub schema: Option<String>,
    pub ddl: Option<String>,
    pub columns: Vec<BatchExportColumn>,
    pub rows: Vec<Value>,
}

/// The `loadTableExportData(tableName)` resolution. React declares
/// `tableName: string` but still guards it with `meta.tableName ?? tableName`;
/// modelling it as `Option` pins that defensive fallback for a dirty double.
/// `rows?: unknown[]` is optional in React and maps to an empty `Vec` here.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoadedTableExportData {
    pub table_name: Option<String>,
    pub schema: Option<String>,
    pub ddl: Option<String>,
    pub columns: Vec<BatchExportColumn>,
    pub rows: Vec<Value>,
}

/// The `tables[]` entry the job actually sends — React's `ExportTableInput`
/// narrowed to the four fields this file sets (`source`/`primaryKey` are owned
/// by the single-table path, not by the batch job).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportTableInput {
    pub table_name: String,
    pub schema: Option<String>,
    pub columns: Vec<String>,
    pub ddl: Option<String>,
}

/// Local mirror of React `ExportTablesRequest` (`commands/file.ts`). T298 owns
/// the real port; this carries exactly the fields this file builds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportTablesRequest {
    /// Runtime db session id (backend `ExportTablesRequest.db_session_id`).
    pub db_session_id: String,
    /// Opaque per-run cancel token (React `cancelToken`), set by the batch
    /// path and absent from the structure path.
    pub cancel_token: Option<String>,
    pub database_type: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub mode: BatchExportMode,
    pub data_format: BatchExportDataFormat,
    pub output_mode: BatchExportOutputMode,
    pub tables: Vec<ExportTableInput>,
}

/// Local mirror of React `ExportTablesResult` (`commands/file.ts`):
/// `{ Saved: number } | { Cancelled: null } | { Aborted: { partialDeleted } }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportTablesResult {
    Saved(u64),
    Cancelled,
    Aborted { partial_deleted: bool },
}

/// React `SqlDataFetcher`'s `scope` — `filtered_table` when `sql` is present
/// (truthy), otherwise `entire_table`. The fetcher itself is a seam (see
/// [`super::SingleTableFetchConfig`]); this is the one piece of its config this
/// file derives.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SingleTableFetchScope {
    #[default]
    EntireTable,
    FilteredTable,
}

impl SingleTableFetchScope {
    pub fn as_str(self) -> &'static str {
        match self {
            SingleTableFetchScope::EntireTable => "entire_table",
            SingleTableFetchScope::FilteredTable => "filtered_table",
        }
    }
}

/// The config `exportSingleTableViaRust` hands to `new SqlDataFetcher(...)`.
/// Every field is the React object literal verbatim: `mode` is always
/// `data_only` and `output_mode` always `single`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SingleTableFetchConfig {
    pub table_name: String,
    pub scope: SingleTableFetchScope,
    pub db_session_id: String,
    pub database_type: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    /// React passes `sql` verbatim (`undefined` when absent). An empty string
    /// is still `Some("")` here; only the derived `scope` treats it as falsy.
    pub sql: Option<String>,
    pub mode: BatchExportMode,
    pub data_format: BatchExportDataFormat,
    pub output_mode: BatchExportOutputMode,
    pub column_names: Vec<String>,
    pub primary_key: Option<String>,
}

/// `fetcher.fetch()`'s resolution, used by the single-table seam. `Aborted`
/// carries `abortedPartialDeleted` as an `Option<bool>` so the React
/// `fetcher.abortedPartialDeleted ?? false` fallback (a double that never set
/// the field reports "kept", not "deleted") stays observable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SingleFetchOutcome {
    Saved,
    Cancelled,
    Aborted { partial_deleted: Option<bool> },
}

//! Minimal mirror of the shared export file planner.
//!
//! Mirrors `packages/frontend/src/platform/exportPlan.ts` — itself the
//! TypeScript mirror of the Rust batch-export planner in
//! `crates/datazen/src/export/mod.rs` (`DataFormat::extension`,
//! `resolve_ddl`, `ddl_for_structure`, `build_file_plans`,
//! `prepare_export`, `unique_filename`, `default_export_name`) plus the
//! source validation in `TableDataSource::validate` — for the one consumer
//! this slice has: the web-shaped tables exporter
//! ([`super::blob_tables_exporter`]).
//!
//! ## Scope — the planner surface is complete (T187)
//!
//! `platform/exportPlan.ts`'s own task (T187) completed the planner
//! function surface (every export of the file, `isBinaryFormat` and
//! `planFilename` included); the request types still come from
//! `commands/file.ts` (T298), and only the name-only [`ProvidedColumn`]
//! slice stands in for `ColumnInfo` until that port widens it — so a
//! later request-type task can expand the types without touching the
//! exporter:
//!
//! * Request types ([`TablesExportRequest`], [`ExportTableInput`],
//!   [`ExportTableSource`], [`ExportMode`], [`ExportDataFormat`],
//!   [`ExportOutputMode`]) mirror `commands/file.ts` field for field,
//!   with two deliberate simplifications: `source` is a plain
//!   [`ExportTableSource`] (its `Table` variant is the default, standing
//!   in for the TS "absent means `table`"), and a provided source's
//!   columns are [`ProvidedColumn`] — the name-only slice of `ColumnInfo`
//!   that planning and validation touch. The full `ColumnInfo` shape
//!   arrives with T298 / T500.
//! * The planner functions mirror `exportPlan.ts` 1:1 in rule and in
//!   error wording: [`format_extension`], [`is_binary_format`],
//!   [`is_sql_data_format`], [`plan_filename`], [`resolve_ddl`],
//!   [`ddl_for_structure`], [`validate_table_source`],
//!   [`unique_filename`], [`build_file_plans`], [`prepare_export`],
//!   [`default_export_extension`], [`default_export_name`].
//!   Error wording mirrors the TS `Error` messages verbatim, so a web
//!   failure and a desktop failure for the same request read the same
//!   (see [`ExportPlanError`]). The Rust kernel
//!   (`crates/datazen/src/export/mod.rs`) stays the single source of
//!   truth for file layout; if a rule changes on either side, change
//!   both and say so in the commit.
//!
//! ## What this module does not do
//!
//! No reading, no downloading, no clock: [`prepare_export`] is pure
//! planning over the request, and [`default_export_name`] takes its
//! `now` as a parameter so tests can pin the timestamp (the exporter
//! passes its injected clock).

#![allow(dead_code)]
// Request-mirror fields (`columns`, `primary_key`, `cancel_token`) are
// carried for the wire shape and consumed by S2's emission (T500's
// formatter) and T298's full `commands/file.ts` port, not by planning itself.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde_json::Value;

/// Export mode — `ExportMode` in `commands/file.ts`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExportMode {
    StructureOnly,
    #[default]
    DataOnly,
    DataAndStructure,
}

/// Data format — `ExportDataFormat` in `commands/file.ts`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExportDataFormat {
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

impl ExportDataFormat {
    /// The wire name (`'csv'`, `'sql_insert'`, …) — the spelling the TS
    /// error messages and the file extensions use.
    pub fn as_str(self) -> &'static str {
        match self {
            ExportDataFormat::Csv => "csv",
            ExportDataFormat::Tsv => "tsv",
            ExportDataFormat::Json => "json",
            ExportDataFormat::Markdown => "markdown",
            ExportDataFormat::SqlInsert => "sql_insert",
            ExportDataFormat::SqlUpdate => "sql_update",
            ExportDataFormat::Xlsx => "xlsx",
            ExportDataFormat::Parquet => "parquet",
        }
    }
}

impl std::fmt::Display for ExportDataFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Output container — `ExportOutputMode` in `commands/file.ts`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExportOutputMode {
    #[default]
    Single,
    Zip,
}

/// One column of already-loaded rows — the name-only slice of `ColumnInfo`
/// (`commands/file.ts` imports it from `../types`) that planning and
/// validation touch. The full column shape arrives with T298 / T500.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProvidedColumn {
    pub name: String,
}

/// Where one export item's rows come from — `ExportTableSource` in
/// `commands/file.ts` (the Rust `TableDataSource`, serde `{"type": …}`
/// tag). `Table` is the default: the named table as a SELECT, which is
/// what an absent `source` means on the wire.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ExportTableSource {
    #[default]
    Table,
    /// One arbitrary read-only SQL statement (a filtered export).
    Sql { sql: String },
    /// Rows the caller already holds, with their schema — never touches
    /// the backend. Values are the wire `Value` shape: null / boolean /
    /// number / string, a JSON object for `Json`, and a number array for
    /// `Bytes` (a JSON array value is indistinguishable from bytes on
    /// this wire shape, the same ambiguity query results already carry).
    /// A cell that is `null` on the wire is [`Value::Null`] here.
    Provided {
        columns: Vec<ProvidedColumn>,
        rows: Vec<Vec<Value>>,
    },
}

/// One table in an export request — `ExportTableInput` in
/// `commands/file.ts`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExportTableInput {
    pub table_name: String,
    /// Schema the table lives in (PostgreSQL family); resolved per table.
    pub schema: Option<String>,
    /// Column names to SELECT (empty means SELECT * and derive from the
    /// result).
    pub columns: Vec<String>,
    /// DDL text (CREATE TABLE …); used when the mode includes structure.
    pub ddl: Option<String>,
    /// Row source; defaults to the named table when omitted.
    pub source: ExportTableSource,
    /// Key column for `sql_update` output; first column when omitted.
    pub primary_key: Option<String>,
}

/// An export request — `ExportTablesRequest` in `commands/file.ts`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TablesExportRequest {
    /// Runtime db session id (backend `ExportTablesRequest.db_session_id`).
    pub db_session_id: String,
    /// Opaque per-run cancellation token, carried only by SQL-path
    /// requests.
    pub cancel_token: Option<String>,
    pub database_type: Option<String>,
    /// Database to read from; the session's own database is only a fallback.
    pub database: Option<String>,
    /// Fallback schema for tables that do not carry one.
    pub schema: Option<String>,
    pub mode: ExportMode,
    pub data_format: ExportDataFormat,
    pub output_mode: ExportOutputMode,
    pub tables: Vec<ExportTableInput>,
}

/// One planned output file — `ExportFilePlan` in `exportPlan.ts`
/// (`FilePlan` in `export/mod.rs`).
#[derive(Clone, Debug, PartialEq)]
pub enum ExportFilePlan {
    Ddl {
        filename: String,
        text: String,
    },
    Data {
        filename: String,
        table: ExportTableInput,
        format: ExportDataFormat,
        /// DDL text prefixed to the same data file (only for sql +
        /// structure).
        ddl_prefix: Option<String>,
    },
}

impl ExportFilePlan {
    pub fn filename(&self) -> &str {
        match self {
            ExportFilePlan::Ddl { filename, .. } | ExportFilePlan::Data { filename, .. } => {
                filename
            }
        }
    }
}

/// Why planning refused a request. Every message is the TS `Error`
/// message verbatim, so a web failure and a desktop failure for the same
/// request read the same.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportPlanError {
    /// `prepare_export` planned zero files (`'no tables selected'`).
    NoTables,
    /// A `sql` source whose statement is blank.
    EmptySqlSource { table: String },
    /// A `provided` source with no columns.
    ProvidedNoColumns { table: String },
    /// A `provided` source with a repeated column name.
    ProvidedDuplicateColumn { table: String, column: String },
    /// A `provided` source row whose length differs from its schema.
    ProvidedRowLength {
        table: String,
        row: usize,
        values: usize,
        columns: usize,
    },
    /// A structure mode for a SQL / provided-data source without an
    /// explicit `ddl`.
    StructureNeedsExplicitDdl { table: String },
}

impl std::fmt::Display for ExportPlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportPlanError::NoTables => f.write_str("no tables selected"),
            ExportPlanError::EmptySqlSource { table } => {
                write!(f, "SQL data source '{table}' must not be empty")
            }
            ExportPlanError::ProvidedNoColumns { table } => {
                write!(
                    f,
                    "provided data source '{table}' has no columns (empty schema)"
                )
            }
            ExportPlanError::ProvidedDuplicateColumn { table, column } => {
                write!(
                    f,
                    "provided data source '{table}' has a duplicate column name '{column}'"
                )
            }
            ExportPlanError::ProvidedRowLength {
                table,
                row,
                values,
                columns,
            } => {
                write!(
                    f,
                    "provided data source '{table}' row {row} has {values} values \
                     but the schema has {columns} columns"
                )
            }
            ExportPlanError::StructureNeedsExplicitDdl { table } => {
                write!(
                    f,
                    "structure export for non-table source '{table}' needs an explicit ddl: \
                     SQL and provided-data sources have no table DDL to include"
                )
            }
        }
    }
}

impl std::error::Error for ExportPlanError {}

/// File extension of one data format — `DataFormat::extension`.
pub fn format_extension(format: ExportDataFormat) -> &'static str {
    match format {
        ExportDataFormat::Csv => "csv",
        ExportDataFormat::Tsv => "tsv",
        ExportDataFormat::Json => "json",
        ExportDataFormat::Markdown => "md",
        ExportDataFormat::SqlInsert | ExportDataFormat::SqlUpdate => "sql",
        ExportDataFormat::Xlsx => "xlsx",
        ExportDataFormat::Parquet => "parquet",
    }
}

/// SQL data formats carry their DDL as a prefix in the same `.sql` file.
pub fn is_sql_data_format(format: ExportDataFormat) -> bool {
    matches!(
        format,
        ExportDataFormat::SqlInsert | ExportDataFormat::SqlUpdate
    )
}

/// Binary formats ride the parallel `BinaryFormatter` branch and can
/// never be concatenated as text — `DataFormat::is_binary`: XLSX and
/// Parquet (T187's completion of the mirror; both are Rust-only on
/// desktop, but the planning mirror stays platform-free).
pub fn is_binary_format(format: ExportDataFormat) -> bool {
    matches!(format, ExportDataFormat::Xlsx | ExportDataFormat::Parquet)
}

/// `planFilename(plan)` — the planned file's name, for either plan kind
/// (the [`ExportFilePlan::filename`] accessor is the same mirror; the
/// free function keeps the TS file's 1:1 surface).
pub fn plan_filename(plan: &ExportFilePlan) -> &str {
    plan.filename()
}

/// DDL text for one table — `resolve_ddl`: the caller's `ddl` when it
/// carries non-blank text, else the shared placeholder. The placeholder
/// names the bare table, exactly as the Rust side does.
pub fn resolve_ddl(table: &ExportTableInput) -> String {
    match table.ddl.as_deref() {
        Some(ddl) if !ddl.trim().is_empty() => ddl.to_owned(),
        _ => format!("-- DDL unavailable for {}", table.table_name),
    }
}

/// DDL text for a structure-mode plan — `ddl_for_structure`.
///
/// A real table may fall back to the placeholder, but a SQL / provided-data
/// source has no table whose DDL could be merged: structure modes require
/// the caller to supply explicit `ddl` text for these items, and planning
/// fails with a clear error instead of emitting a placeholder that
/// pretends to be a table definition.
pub fn ddl_for_structure(table: &ExportTableInput) -> Result<String, ExportPlanError> {
    match &table.source {
        ExportTableSource::Table => Ok(resolve_ddl(table)),
        _ => match table.ddl.as_deref() {
            Some(ddl) if !ddl.trim().is_empty() => Ok(ddl.to_owned()),
            _ => Err(ExportPlanError::StructureNeedsExplicitDdl {
                table: table.table_name.clone(),
            }),
        },
    }
}

/// Fail fast on a malformed source, before any file is planned or written.
/// The SQL half mirrors the Rust `TableDataSource::validate`
/// (`export/mod.rs`); the provided half has no Rust counterpart any more
/// — the Rust provided source was deleted in task 5, so this is the only
/// copy of the provided check.
pub fn validate_table_source(table: &ExportTableInput) -> Result<(), ExportPlanError> {
    match &table.source {
        ExportTableSource::Table => Ok(()),
        ExportTableSource::Sql { sql } => {
            if sql.trim().is_empty() {
                return Err(ExportPlanError::EmptySqlSource {
                    table: table.table_name.clone(),
                });
            }
            Ok(())
        }
        ExportTableSource::Provided { columns, rows } => {
            if columns.is_empty() {
                return Err(ExportPlanError::ProvidedNoColumns {
                    table: table.table_name.clone(),
                });
            }
            let mut seen: HashSet<&str> = HashSet::new();
            for column in columns {
                if !seen.insert(column.name.as_str()) {
                    return Err(ExportPlanError::ProvidedDuplicateColumn {
                        table: table.table_name.clone(),
                        column: column.name.clone(),
                    });
                }
            }
            for (index, row) in rows.iter().enumerate() {
                if row.len() != columns.len() {
                    return Err(ExportPlanError::ProvidedRowLength {
                        table: table.table_name.clone(),
                        row: index,
                        values: row.len(),
                        columns: columns.len(),
                    });
                }
            }
            Ok(())
        }
    }
}

/// De-duplicated `.sql` filename — `unique_filename`. Only DDL plans and
/// SQL-format data plans go through it (a data plan's `{table}.{ext}` is
/// used verbatim, as on the Rust side).
pub fn unique_filename(seen: &mut HashSet<String>, name: &str) -> String {
    if seen.insert(name.to_owned()) {
        return name.to_owned();
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

/// Plan the output files for a request — `build_file_plans`.
///
/// - `structure_only`: one `{table}.sql` DDL file per table.
/// - `data_only`: one `{table}.{ext}` data file per table.
/// - `data_and_structure`: SQL data formats carry their DDL as a prefix in
///   the same `{table}.sql` file; every other format splits DDL
///   (`{table}.sql`) and data (`{table}.{ext}`).
pub fn build_file_plans(
    request: &TablesExportRequest,
) -> Result<Vec<ExportFilePlan>, ExportPlanError> {
    for table in &request.tables {
        validate_table_source(table)?;
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut plans: Vec<ExportFilePlan> = Vec::new();
    for table in &request.tables {
        let base = table.table_name.clone();
        match request.mode {
            ExportMode::StructureOnly => {
                plans.push(ExportFilePlan::Ddl {
                    filename: unique_filename(&mut seen, &format!("{base}.sql")),
                    text: ddl_for_structure(table)?,
                });
            }
            ExportMode::DataOnly => {
                plans.push(ExportFilePlan::Data {
                    filename: format!("{base}.{}", format_extension(request.data_format)),
                    table: table.clone(),
                    format: request.data_format,
                    ddl_prefix: None,
                });
            }
            ExportMode::DataAndStructure => {
                if is_sql_data_format(request.data_format) {
                    plans.push(ExportFilePlan::Data {
                        filename: unique_filename(&mut seen, &format!("{base}.sql")),
                        table: table.clone(),
                        format: request.data_format,
                        ddl_prefix: Some(ddl_for_structure(table)?),
                    });
                } else {
                    plans.push(ExportFilePlan::Ddl {
                        filename: unique_filename(&mut seen, &format!("{base}.sql")),
                        text: ddl_for_structure(table)?,
                    });
                    plans.push(ExportFilePlan::Data {
                        filename: format!("{base}.{}", format_extension(request.data_format)),
                        table: table.clone(),
                        format: request.data_format,
                        ddl_prefix: None,
                    });
                }
            }
        }
    }
    Ok(plans)
}

/// The planned container — `prepareExport`'s `{ plans, useZip }`.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedExport {
    pub plans: Vec<ExportFilePlan>,
    pub use_zip: bool,
}

/// Plan the output container — `prepare_export`.
///
/// `use_zip` is true when the caller asked for zip output, or when the plan
/// has more than one file and they are not all `.sql` (a multi-file
/// non-SQL export has nowhere to go as a single file). A single file, or
/// several `.sql` files, merge into one target instead.
pub fn prepare_export(
    request: &TablesExportRequest,
) -> Result<PreparedExport, ExportPlanError> {
    let plans = build_file_plans(request)?;
    if plans.is_empty() {
        return Err(ExportPlanError::NoTables);
    }
    let use_zip = request.output_mode == ExportOutputMode::Zip
        || (plans.len() > 1 && !plans.iter().all(|plan| plan.filename().ends_with(".sql")));
    Ok(PreparedExport { plans, use_zip })
}

/// Extension of the file `default_export_name` produces —
/// `default_export_extension`: decided by the actual product, not by the
/// data format alone. A structure-only export is always DDL text
/// (`build_file_plans` emits only `{table}.sql` plans), so it is `.sql`
/// whatever the data format says; a zip container is `.zip`; everything
/// else keeps the data format's extension.
pub fn default_export_extension(
    request: &TablesExportRequest,
    use_zip: bool,
) -> &'static str {
    if use_zip {
        "zip"
    } else if request.mode == ExportMode::StructureOnly {
        "sql"
    } else {
        format_extension(request.data_format)
    }
}

/// Default output name — `default_export_name`: `{prefix}_{UTC
/// timestamp}.{ext}` with `export_structure` / `export_data` /
/// `export_full` by mode and [`default_export_extension`] as the extension
/// (`zip` for a zip container, `sql` for a structure-only export, else the
/// data format's extension). `now` is injectable so tests can pin the
/// timestamp; production passes the current time (UTC, as `chrono::Utc`
/// does on the Rust side).
pub fn default_export_name(
    request: &TablesExportRequest,
    use_zip: bool,
    now: DateTime<Utc>,
) -> String {
    let ts = now.format("%Y-%m-%d-%H-%M-%S");
    let prefix = match request.mode {
        ExportMode::StructureOnly => "export_structure",
        ExportMode::DataOnly => "export_data",
        ExportMode::DataAndStructure => "export_full",
    };
    let ext = default_export_extension(request, use_zip);
    format!("{prefix}_{ts}.{ext}")
}

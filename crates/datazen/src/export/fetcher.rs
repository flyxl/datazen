//! Unified export data source (`DataFetcher`).
//!
//! A batch export item can get its rows in two ways:
//!
//! * (a) **caller-provided data** — the caller already holds the rows and
//!   their schema (e.g. the current page / selected rows);
//! * (b) **backend query** — the caller passes one SQL statement (a filtered
//!   export, an arbitrary read) and the backend streams it back.
//!
//! The historical third shape — "export this table" — is not a separate
//! fetch path: a table source is expressed as a SQL source whose statement
//! is the `SELECT *` / selected-columns query built by
//! [`build_select_sql`](super::build_select_sql) (`SqlDataFetcher::for_table`).
//! There is exactly one way to pull rows from the backend.
//!
//! Whatever the origin, a fetcher emits the same internal row stream into
//! the caller's callback: one `StatementStart` carrying the full
//! [`ColumnInfo`] (not just names), `Rows` events in batches of at most
//! [`QUERY_STREAM_BATCH_SIZE`], then `StatementEnd` and `Done`. The existing
//! text formatters consume that stream unchanged, so no formatter knows
//! where its rows came from.
//!
//! Validation is split by source, deliberately:
//!
//! * SQL sources go through `sql_guard` (read-only / safe-mode rules) at
//!   fetch time, when the backend flags are known. An empty statement is
//!   rejected before it reaches the backend.
//! * Provided-data sources never touch `sql_guard` — there is no SQL to
//!   guard. Instead the schema and the row shapes are validated up front
//!   (non-empty schema, unique column names, every row as long as the
//!   schema). A cell whose [`Value`] variant is a poor fit for the column's
//!   declared `data_type` (e.g. a non-numeric string in an `int` column)
//!   is still exported as-is — the `Value` variants are the permissive
//!   wire type — but the first such mismatch per column is recorded with
//!   `tracing::warn` so a bad caller schema does not fail silently.

use std::sync::Arc;

use datazen_driver_api::{
    ColumnInfo, QueryStreamCallback, QueryStreamEvent, SqlTarget, Value, QUERY_STREAM_BATCH_SIZE,
};

use super::{
    build_select_sql, ExportError, ExportSource, ExportTablesRequest, TableDataSource,
    TableExportInput,
};

/// One way to supply rows to an export item.
///
/// Implementations must emit the full row stream into `callback`:
/// `StatementStart` (full [`ColumnInfo`]) → `Rows` batches → `StatementEnd`
/// → `Done`. `source` / `session_id` are only used by backend-querying
/// fetchers; in-memory fetchers ignore them.
#[async_trait::async_trait]
pub trait DataFetcher: Send + Sync {
    async fn fetch(
        &self,
        source: &dyn ExportSource,
        session_id: &str,
        callback: &QueryStreamCallback,
    ) -> Result<(), ExportError>;
}

/// A fetcher that runs one SQL statement against the backend and streams
/// the result back. Also the single implementation behind table exports:
/// [`SqlDataFetcher::for_table`] builds the table's `SELECT` for it.
pub struct SqlDataFetcher {
    /// Display name used in error messages (table name or SQL-source label).
    name: String,
    sql: String,
    database: Option<String>,
    schema: Option<String>,
}

impl SqlDataFetcher {
    pub fn new(
        name: impl Into<String>,
        sql: impl Into<String>,
        database: Option<String>,
        schema: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            sql: sql.into(),
            database,
            schema,
        }
    }

    /// The table source, expressed as a SQL source: `SELECT *` (or the
    /// selected columns) for the named table.
    pub fn for_table(request: &ExportTablesRequest, table: &TableExportInput) -> Self {
        Self::new(
            table.table_name.clone(),
            build_select_sql(
                &table.table_name,
                &table.columns,
                request.database_type.as_deref(),
            ),
            request.database.clone(),
            // A per-table schema wins over the request default. The database
            // is carried as the statement's target: PostgreSQL cannot
            // reference another database in one statement, so a bare
            // `query_stream` would have streamed from the session's own
            // database (the "second database exports the first database's
            // rows" bug).
            table.schema.clone().or(request.schema.clone()),
        )
    }

    pub fn sql(&self) -> &str {
        &self.sql
    }
}

#[async_trait::async_trait]
impl DataFetcher for SqlDataFetcher {
    async fn fetch(
        &self,
        source: &dyn ExportSource,
        session_id: &str,
        callback: &QueryStreamCallback,
    ) -> Result<(), ExportError> {
        if self.sql.trim().is_empty() {
            return Err(ExportError::Validation(format!(
                "SQL data source '{}' must not be empty",
                self.name
            )));
        }
        let (_driver, read_only, safe_mode) = source.resolve_source(session_id).await?;
        crate::execution::domain::sql_guard::check_sql(&self.sql, read_only, safe_mode)
            .map_err(ExportError::Validation)?;
        source
            .stream_query(
                session_id,
                &self.sql,
                SqlTarget::new(self.database.as_deref(), self.schema.as_deref()),
                Arc::clone(callback),
            )
            .await
    }
}

/// A fetcher over rows the caller already holds, plus their schema.
///
/// Validation happens in [`ProvidedDataFetcher::new`], so a malformed
/// payload fails before any file is planned or written; [`DataFetcher::fetch`]
/// only replays the validated rows as stream events.
pub struct ProvidedDataFetcher {
    name: String,
    columns: Vec<ColumnInfo>,
    rows: Vec<Vec<Option<Value>>>,
}

impl ProvidedDataFetcher {
    pub fn new(
        name: impl Into<String>,
        columns: Vec<ColumnInfo>,
        rows: Vec<Vec<Option<Value>>>,
    ) -> Result<Self, ExportError> {
        let name = name.into();
        validate_provided_data(&name, &columns, &rows)?;
        Ok(Self {
            name,
            columns,
            rows,
        })
    }

    pub fn columns(&self) -> &[ColumnInfo] {
        &self.columns
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Record (once per column, at `warn` level) the cells whose `Value`
    /// variant plainly contradicts the column's declared `data_type`.
    /// The values are still exported as-is; this only makes a bad caller
    /// schema visible instead of silently accepted.
    fn warn_type_mismatches(&self) {
        let mut warned: Vec<bool> = vec![false; self.columns.len()];
        for (row_index, row) in self.rows.iter().enumerate() {
            for (col_index, cell) in row.iter().enumerate() {
                if warned[col_index] {
                    continue;
                }
                if let Some(Value::String(text)) = cell {
                    let declared = self.columns[col_index].data_type.to_ascii_lowercase();
                    let mismatch = if declared.contains("int") {
                        text.trim().parse::<i64>().is_err()
                    } else if declared.contains("float")
                        || declared.contains("double")
                        || declared.contains("decimal")
                        || declared.contains("numeric")
                        || declared.contains("real")
                    {
                        text.trim().parse::<f64>().is_err()
                    } else if declared == "bool" || declared == "boolean" {
                        !matches!(text.trim().to_ascii_lowercase().as_str(), "true" | "false")
                    } else {
                        false
                    };
                    if mismatch {
                        warned[col_index] = true;
                        tracing::warn!(
                            source = "provided-data",
                            table = %self.name,
                            column = %self.columns[col_index].name,
                            declared_type = %self.columns[col_index].data_type,
                            row = row_index,
                            "provided row value does not match the declared column type; exporting it as-is"
                        );
                    }
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl DataFetcher for ProvidedDataFetcher {
    async fn fetch(
        &self,
        _source: &dyn ExportSource,
        _session_id: &str,
        callback: &QueryStreamCallback,
    ) -> Result<(), ExportError> {
        self.warn_type_mismatches();
        callback(QueryStreamEvent::StatementStart {
            index: 0,
            sql: String::new(),
            columns: self.columns.clone(),
        });
        for chunk in self.rows.chunks(QUERY_STREAM_BATCH_SIZE) {
            callback(QueryStreamEvent::Rows {
                index: 0,
                rows: chunk.to_vec(),
            });
        }
        callback(QueryStreamEvent::StatementEnd {
            index: 0,
            rows_affected: Some(self.rows.len() as u64),
            execution_time_ms: 0,
            truncated: false,
        });
        callback(QueryStreamEvent::Done { total_time_ms: 0 });
        Ok(())
    }
}

/// Validate caller-provided rows against their schema. This is the
/// provided-data counterpart of `sql_guard`: there is no SQL to guard, so
/// shape is what gets checked — a non-empty schema, unique column names
/// (duplicate names would collapse in JSON output and confuse CSV), and
/// every row exactly as long as the schema.
pub fn validate_provided_data(
    name: &str,
    columns: &[ColumnInfo],
    rows: &[Vec<Option<Value>>],
) -> Result<(), ExportError> {
    if columns.is_empty() {
        return Err(ExportError::Validation(format!(
            "provided data source '{name}' has no columns (empty schema)"
        )));
    }
    for (i, column) in columns.iter().enumerate() {
        if columns[..i].iter().any(|c| c.name == column.name) {
            return Err(ExportError::Validation(format!(
                "provided data source '{name}' has a duplicate column name '{}'",
                column.name
            )));
        }
    }
    for (i, row) in rows.iter().enumerate() {
        if row.len() != columns.len() {
            return Err(ExportError::Validation(format!(
                "provided data source '{name}' row {i} has {} values but the schema has {} columns",
                row.len(),
                columns.len()
            )));
        }
    }
    Ok(())
}

/// Build the fetcher for one export item from its declared source.
/// SQL sources are validated for emptiness here only in the sense that
/// [`SqlDataFetcher::fetch`] rejects them (plus `sql_guard`); provided
/// data is fully validated now, so planning fails fast on a bad payload.
pub fn fetcher_for_table(
    request: &ExportTablesRequest,
    table: &TableExportInput,
) -> Result<Box<dyn DataFetcher>, ExportError> {
    match &table.source {
        TableDataSource::Table => Ok(Box::new(SqlDataFetcher::for_table(request, table))),
        TableDataSource::Sql { sql } => Ok(Box::new(SqlDataFetcher::new(
            table.table_name.clone(),
            sql.clone(),
            request.database.clone(),
            table.schema.clone().or(request.schema.clone()),
        ))),
        TableDataSource::Provided { columns, rows } => Ok(Box::new(ProvidedDataFetcher::new(
            table.table_name.clone(),
            columns.clone(),
            rows.clone(),
        )?)),
    }
}

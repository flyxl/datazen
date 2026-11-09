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
#[derive(Debug)]
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
#[derive(Debug)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{
        build_file_plans, prepare_export, write_data_file, DataFormat, ExportError, ExportEvents,
        ExportMode, ExportProgressEvent, ExportSource, ExportTablesRequest, FilePlan, OutputMode,
        TableDataSource, TableExportInput,
    };
    use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
    use datazen_driver_api::{
        ColumnInfo, DatabaseDriver, QueryStreamCallback, QueryStreamEvent, Value,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn col(name: &str, data_type: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: data_type.into(),
            nullable: true,
        }
    }

    fn two_cols() -> Vec<ColumnInfo> {
        vec![col("id", "int4"), col("name", "text")]
    }

    fn two_rows() -> Vec<Vec<Option<Value>>> {
        vec![
            vec![Some(Value::Integer(1)), Some(Value::String("a".into()))],
            vec![Some(Value::Integer(2)), Some(Value::Null)],
        ]
    }

    fn collect_events() -> (
        QueryStreamCallback,
        Arc<std::sync::Mutex<Vec<QueryStreamEvent>>>,
    ) {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let callback: QueryStreamCallback = Arc::new(move |event| sink.lock().unwrap().push(event));
        (callback, events)
    }

    fn test_driver() -> Arc<dyn DatabaseDriver> {
        MockDriver::new("sqlite", MockDriverOptions::default())
    }

    /// Stub backend: records the SQL it was asked to stream and replays
    /// canned columns/rows as stream events.
    struct StubSource {
        driver: Arc<dyn DatabaseDriver>,
        read_only: bool,
        safe_mode: bool,
        columns: Vec<ColumnInfo>,
        rows: Vec<Vec<Option<Value>>>,
        streamed_sql: Arc<std::sync::Mutex<Vec<String>>>,
        stream_calls: Arc<AtomicUsize>,
    }

    impl StubSource {
        fn new(
            read_only: bool,
            safe_mode: bool,
            columns: Vec<ColumnInfo>,
            rows: Vec<Vec<Option<Value>>>,
        ) -> Self {
            Self {
                driver: test_driver(),
                read_only,
                safe_mode,
                columns,
                rows,
                streamed_sql: Arc::new(std::sync::Mutex::new(Vec::new())),
                stream_calls: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    #[async_trait::async_trait]
    impl ExportSource for StubSource {
        async fn resolve_source(
            &self,
            _session_id: &str,
        ) -> Result<(Arc<dyn DatabaseDriver>, bool, bool), ExportError> {
            Ok((self.driver.clone(), self.read_only, self.safe_mode))
        }

        async fn stream_query(
            &self,
            _session_id: &str,
            sql: &str,
            _target: datazen_driver_api::SqlTarget<'_>,
            callback: QueryStreamCallback,
        ) -> Result<(), ExportError> {
            self.streamed_sql.lock().unwrap().push(sql.to_string());
            self.stream_calls.fetch_add(1, Ordering::Relaxed);
            callback(QueryStreamEvent::StatementStart {
                index: 0,
                sql: sql.to_string(),
                columns: self.columns.clone(),
            });
            if !self.rows.is_empty() {
                callback(QueryStreamEvent::Rows {
                    index: 0,
                    rows: self.rows.clone(),
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

    struct NoopEvents;

    impl ExportEvents for NoopEvents {
        fn progress_callback(&self) -> Arc<dyn Fn(ExportProgressEvent) + Send + Sync> {
            Arc::new(|_| {})
        }
    }

    fn request(
        mode: ExportMode,
        format: DataFormat,
        tables: Vec<TableExportInput>,
    ) -> ExportTablesRequest {
        ExportTablesRequest {
            db_session_id: "s".into(),
            database_type: Some("sqlite".into()),
            database: None,
            schema: None,
            mode,
            data_format: format,
            output_mode: OutputMode::Single,
            tables,
        }
    }

    fn table_item(name: &str, source: TableDataSource) -> TableExportInput {
        TableExportInput {
            table_name: name.into(),
            schema: None,
            columns: vec![],
            ddl: None,
            source,
        }
    }

    fn temp_file(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("datazen-export-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    // --- ProvidedDataFetcher: event shape ---------------------------------

    #[tokio::test]
    async fn provided_fetcher_emits_full_stream_in_batches() {
        let rows: Vec<Vec<Option<Value>>> = (0..1200)
            .map(|i| vec![Some(Value::Integer(i)), Some(Value::String("x".into()))])
            .collect();
        let fetcher = ProvidedDataFetcher::new("t", two_cols(), rows).expect("valid provided data");
        let (callback, events) = collect_events();
        let source = StubSource::new(false, false, vec![], vec![]);
        fetcher.fetch(&source, "s", &callback).await.unwrap();

        let events = events.lock().unwrap();
        // StatementStart carries the full ColumnInfo, not just names.
        assert!(matches!(
            &events[0],
            QueryStreamEvent::StatementStart { columns, .. } if *columns == two_cols()
        ));
        let row_events: Vec<&Vec<Vec<Option<Value>>>> = events
            .iter()
            .filter_map(|e| match e {
                QueryStreamEvent::Rows { rows, .. } => Some(rows),
                _ => None,
            })
            .collect();
        assert_eq!(
            row_events.iter().map(|r| r.len()).collect::<Vec<_>>(),
            vec![500, 500, 200],
            "rows are batched at QUERY_STREAM_BATCH_SIZE"
        );
        assert!(matches!(
            &events[events.len() - 2],
            QueryStreamEvent::StatementEnd {
                rows_affected: Some(1200),
                ..
            }
        ));
        assert!(matches!(
            events[events.len() - 1],
            QueryStreamEvent::Done { .. }
        ));
        assert_eq!(source.stream_calls.load(Ordering::Relaxed), 0);
    }

    // --- ProvidedDataFetcher: validation ----------------------------------

    #[test]
    fn provided_data_rejects_row_length_mismatch() {
        let err = ProvidedDataFetcher::new("t", two_cols(), vec![vec![Some(Value::Integer(1))]])
            .unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(ref m) if m.contains("row 0") && m.contains("1 values") && m.contains("2 columns")),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn provided_data_rejects_empty_schema() {
        let err = ProvidedDataFetcher::new("t", vec![], vec![]).unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(ref m) if m.contains("no columns")),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn provided_data_rejects_duplicate_column_names() {
        let err = ProvidedDataFetcher::new("t", vec![col("id", "int4"), col("id", "text")], vec![])
            .unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(ref m) if m.contains("duplicate column")),
            "unexpected error: {err:?}"
        );
    }

    #[tokio::test]
    async fn provided_data_type_mismatch_is_lenient_not_rejected() {
        // A non-numeric string in an int column is still exported as-is;
        // the mismatch is only recorded (warn), never a hard failure.
        let fetcher = ProvidedDataFetcher::new(
            "t",
            vec![col("id", "int4")],
            vec![vec![Some(Value::String("abc".into()))]],
        )
        .expect("lenient by design");
        let (callback, events) = collect_events();
        let source = StubSource::new(false, false, vec![], vec![]);
        fetcher.fetch(&source, "s", &callback).await.unwrap();
        let events = events.lock().unwrap();
        assert!(events.iter().any(|e| matches!(
            e,
            QueryStreamEvent::Rows { rows, .. }
                if rows == &[vec![Some(Value::String("abc".into()))]]
        )));
    }

    #[tokio::test]
    async fn provided_data_type_mismatch_float_and_bool_are_lenient() {
        // Same leniency for the float / bool branches of the mismatch
        // recorder: non-numeric strings are still exported as-is.
        let fetcher = ProvidedDataFetcher::new(
            "t",
            vec![col("amount", "float8"), col("flag", "bool")],
            vec![
                vec![
                    Some(Value::String("abc".into())),
                    Some(Value::String("maybe".into())),
                ],
                // Second bad row in the same columns: already warned, so the
                // per-column "warn once" skip is exercised too.
                vec![
                    Some(Value::String("def".into())),
                    Some(Value::String("nope".into())),
                ],
            ],
        )
        .expect("lenient by design");
        let (callback, events) = collect_events();
        let source = StubSource::new(false, false, vec![], vec![]);
        fetcher.fetch(&source, "s", &callback).await.unwrap();
        assert!(events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, QueryStreamEvent::Rows { .. })));
    }

    #[test]
    fn fetcher_accessors_expose_sql_columns_and_row_count() {
        let sql = SqlDataFetcher::new("q", "SELECT 1", None, None);
        assert_eq!(sql.sql(), "SELECT 1");
        let provided =
            ProvidedDataFetcher::new("t", two_cols(), two_rows()).expect("valid provided data");
        assert_eq!(provided.columns(), two_cols().as_slice());
        assert_eq!(provided.row_count(), 2);
    }

    // --- SqlDataFetcher: guard + streaming --------------------------------

    #[tokio::test]
    async fn sql_fetcher_streams_valid_sql() {
        let source = StubSource::new(true, true, two_cols(), two_rows());
        let fetcher =
            SqlDataFetcher::new("q", "SELECT id, name FROM users WHERE id > 1", None, None);
        let (callback, events) = collect_events();
        fetcher.fetch(&source, "s", &callback).await.unwrap();
        assert_eq!(
            *source.streamed_sql.lock().unwrap(),
            vec!["SELECT id, name FROM users WHERE id > 1".to_string()]
        );
        assert_eq!(events.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn sql_fetcher_rejects_drop_under_guard() {
        let source = StubSource::new(true, true, two_cols(), two_rows());
        let fetcher = SqlDataFetcher::new("q", "DROP TABLE users", None, None);
        let (callback, _) = collect_events();
        let err = fetcher.fetch(&source, "s", &callback).await.unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(_)),
            "unexpected error: {err:?}"
        );
        assert_eq!(source.stream_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn sql_fetcher_rejects_where_less_delete_in_safe_mode() {
        let source = StubSource::new(true, true, two_cols(), two_rows());
        let fetcher = SqlDataFetcher::new("q", "DELETE FROM users", None, None);
        let (callback, _) = collect_events();
        let err = fetcher.fetch(&source, "s", &callback).await.unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(_)),
            "unexpected: {err:?}"
        );
        assert_eq!(source.stream_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn sql_fetcher_rejects_empty_sql() {
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let fetcher = SqlDataFetcher::new("q", "   ", None, None);
        let (callback, _) = collect_events();
        let err = fetcher.fetch(&source, "s", &callback).await.unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(_)),
            "unexpected: {err:?}"
        );
        assert_eq!(source.stream_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn sql_fetcher_guard_only_applies_under_read_only_or_safe_mode() {
        // Same DROP passes the guard when neither flag is set: check_sql is
        // a no-op there, so the statement reaches the backend.
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let fetcher = SqlDataFetcher::new("q", "DROP TABLE users", None, None);
        let (callback, _) = collect_events();
        fetcher.fetch(&source, "s", &callback).await.unwrap();
        assert_eq!(source.stream_calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn table_source_is_a_sql_source_with_built_select() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![TableExportInput {
                table_name: "users".into(),
                schema: None,
                columns: vec!["id".into(), "name".into()],
                ddl: None,
                source: TableDataSource::Table,
            }],
        );
        let mut req = req;
        req.database_type = Some("mysql".into());
        let fetcher = fetcher_for_table(&req, &req.tables[0]).expect("table fetcher");
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let (callback, _) = collect_events();
        fetcher.fetch(&source, "s", &callback).await.unwrap();
        assert_eq!(
            *source.streamed_sql.lock().unwrap(),
            vec!["SELECT `id`, `name` FROM `users`".to_string()]
        );
    }

    // --- DTO: serde ---------------------------------------------------------

    #[test]
    fn old_request_without_source_parses_as_table() {
        let json = serde_json::json!({
            "dbSessionId": "s",
            "mode": "data_only",
            "dataFormat": "csv",
            "outputMode": "single",
            "tables": [{ "tableName": "users", "columns": ["id"] }],
        });
        let req: ExportTablesRequest = serde_json::from_value(json).unwrap();
        assert!(matches!(req.tables[0].source, TableDataSource::Table));
    }

    #[test]
    fn sql_and_provided_sources_round_trip_with_explicit_type_tag() {
        let sql = TableDataSource::Sql {
            sql: "SELECT 1".into(),
        };
        assert_eq!(
            serde_json::to_value(&sql).unwrap(),
            serde_json::json!({ "type": "sql", "sql": "SELECT 1" })
        );
        let json = serde_json::json!({
            "dbSessionId": "s",
            "mode": "data_only",
            "dataFormat": "json",
            "outputMode": "single",
            "tables": [{
                "tableName": "provided",
                "source": {
                    "type": "provided",
                    "columns": [{ "name": "id", "dataType": "int4", "nullable": false }],
                    "rows": [[1], [2]],
                },
            }],
        });
        let req: ExportTablesRequest = serde_json::from_value(json).unwrap();
        assert!(matches!(
            req.tables[0].source,
            TableDataSource::Provided { .. }
        ));
    }

    // --- File planning: DDL semantics --------------------------------------

    #[test]
    fn build_file_plans_table_source_regression() {
        let req = request(
            ExportMode::DataAndStructure,
            DataFormat::Csv,
            vec![TableExportInput {
                table_name: "users".into(),
                schema: None,
                columns: vec!["id".into()],
                ddl: Some("CREATE TABLE users (id INT);".into()),
                source: TableDataSource::Table,
            }],
        );
        let plans = build_file_plans(&req).unwrap();
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].filename(), "users.sql");
        assert_eq!(plans[1].filename(), "users.csv");
    }

    #[test]
    fn build_file_plans_sql_source_structure_needs_explicit_ddl() {
        let req = request(
            ExportMode::StructureOnly,
            DataFormat::Csv,
            vec![table_item(
                "q",
                TableDataSource::Sql {
                    sql: "SELECT 1".into(),
                },
            )],
        );
        let err = build_file_plans(&req).unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(ref m) if m.contains("explicit ddl")),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn build_file_plans_sql_source_with_explicit_ddl_uses_it() {
        let mut item = table_item(
            "q",
            TableDataSource::Sql {
                sql: "SELECT 1".into(),
            },
        );
        item.ddl = Some("CREATE TABLE q (id INT);".into());
        let req = request(
            ExportMode::DataAndStructure,
            DataFormat::SqlInsert,
            vec![item],
        );
        let plans = build_file_plans(&req).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].filename(), "q.sql");
    }

    #[test]
    fn build_file_plans_provided_source_data_only_needs_no_ddl() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Json,
            vec![table_item(
                "provided",
                TableDataSource::Provided {
                    columns: two_cols(),
                    rows: two_rows(),
                },
            )],
        );
        let plans = build_file_plans(&req).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].filename(), "provided.json");
    }

    #[test]
    fn build_file_plans_data_and_structure_without_ddl_fails_for_sql_source() {
        // The error-propagation arm of ddl_for_structure in both
        // DataAndStructure branches (SqlInsert and the DDL+data split).
        for format in [DataFormat::SqlInsert, DataFormat::Csv] {
            let req = request(
                ExportMode::DataAndStructure,
                format,
                vec![table_item(
                    "q",
                    TableDataSource::Sql {
                        sql: "SELECT 1".into(),
                    },
                )],
            );
            let err = build_file_plans(&req).unwrap_err();
            assert!(
                matches!(err, ExportError::Validation(ref m) if m.contains("explicit ddl")),
                "unexpected error for {format:?}: {err:?}"
            );
        }
    }

    #[test]
    fn build_file_plans_table_source_structure_without_ddl_uses_placeholder() {
        // Table sources keep the historical placeholder: a real table
        // exists, so the file names what could not be retrieved.
        let req = request(
            ExportMode::StructureOnly,
            DataFormat::Csv,
            vec![table_item("users", TableDataSource::Table)],
        );
        let plans = build_file_plans(&req).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].filename(), "users.sql");
        match &plans[0] {
            FilePlan::Ddl { text, .. } => {
                assert!(text.contains("DDL unavailable for users"), "{text}");
            }
            other => panic!("expected a DDL plan, got {other:?}"),
        }
    }

    #[test]
    fn build_file_plans_table_source_data_only_sql_insert() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::SqlInsert,
            vec![table_item("users", TableDataSource::Table)],
        );
        let plans = build_file_plans(&req).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].filename(), "users.sql");
    }

    #[test]
    fn prepare_export_rejects_malformed_sources_early() {
        let bad_sql = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![table_item("q", TableDataSource::Sql { sql: "  ".into() })],
        );
        assert!(prepare_export(&bad_sql).is_err());
        let bad_rows = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![table_item(
                "p",
                TableDataSource::Provided {
                    columns: two_cols(),
                    rows: vec![vec![Some(Value::Integer(1))]],
                },
            )],
        );
        assert!(prepare_export(&bad_rows).is_err());
        let empty = request(ExportMode::DataOnly, DataFormat::Csv, vec![]);
        assert!(matches!(
            prepare_export(&empty).unwrap_err(),
            ExportError::Validation(_)
        ));
    }

    // --- write_data_file end to end ----------------------------------------

    #[tokio::test]
    async fn write_data_file_sql_source_writes_csv() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![table_item(
                "filtered",
                TableDataSource::Sql {
                    sql: "SELECT id, name FROM users WHERE id > 1".into(),
                },
            )],
        );
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.csv");
        let n = write_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Csv,
            None,
            &dest,
        )
        .await
        .unwrap();
        assert_eq!(n, 2);
        assert_eq!(
            *source.streamed_sql.lock().unwrap(),
            vec!["SELECT id, name FROM users WHERE id > 1".to_string()]
        );
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            "id,name\n1,a\n2,\n"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_data_file_provided_source_never_queries_backend() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Json,
            vec![table_item(
                "provided",
                TableDataSource::Provided {
                    columns: two_cols(),
                    rows: two_rows(),
                },
            )],
        );
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.json");
        let n = write_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Json,
            None,
            &dest,
        )
        .await
        .unwrap();
        assert_eq!(n, 2);
        assert_eq!(source.stream_calls.load(Ordering::Relaxed), 0);
        let parsed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&dest).unwrap()).unwrap();
        assert_eq!(
            parsed,
            serde_json::json!([{ "id": 1, "name": "a" }, { "id": 2, "name": null }])
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_data_file_table_source_regression() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![table_item("users", TableDataSource::Table)],
        );
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.csv");
        let n = write_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Csv,
            None,
            &dest,
        )
        .await
        .unwrap();
        assert_eq!(n, 2);
        assert_eq!(
            *source.streamed_sql.lock().unwrap(),
            vec!["SELECT * FROM \"users\"".to_string()]
        );
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            "id,name\n1,a\n2,\n"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_data_file_illegal_sql_is_rejected() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![table_item(
                "q",
                TableDataSource::Sql {
                    sql: "DROP TABLE users".into(),
                },
            )],
        );
        let source = StubSource::new(true, true, two_cols(), two_rows());
        let dest = temp_file("out.csv");
        let err = write_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Csv,
            None,
            &dest,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(_)),
            "unexpected error: {err:?}"
        );
        assert_eq!(source.stream_calls.load(Ordering::Relaxed), 0);
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_data_file_provided_source_with_bad_rows_is_rejected() {
        // Malformed provided data fails while building the fetcher, before
        // any row is written (prepare_export would reject it even earlier).
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![table_item(
                "p",
                TableDataSource::Provided {
                    columns: two_cols(),
                    rows: vec![vec![Some(Value::Integer(1))]],
                },
            )],
        );
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.csv");
        let err = write_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Csv,
            None,
            &dest,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(_)),
            "unexpected error: {err:?}"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }
}

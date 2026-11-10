//! SQL-only export data source.
//!
//! A batch export item gets its rows from exactly one place: the backend.
//! The caller passes one SQL statement — either a filtered export's
//! arbitrary read, or the `SELECT *` / selected-columns query built by
//! [`build_select_sql`](super::build_select_sql) for a named table — and
//! [`stream_sql_source`] streams it back through `sql_guard`.
//!
//! This function is the Rust counterpart of the frontend `SqlDataFetcher`
//! (`packages/frontend/src/lib/dataFetcher.ts`): the frontend decides the
//! source and the SQL, Rust only executes it. There is no trait and no
//! second source: rows the caller already holds ("provided" data) are
//! formatted and saved entirely in the frontend and never reach this
//! module, so a request carrying a provided source fails to deserialize
//! (see [`TableDataSource`](super::TableDataSource)).
//!
//! The row stream into the caller's callback is always the same shape:
//! one `StatementStart` carrying the full [`ColumnInfo`] (not just
//! names), `Rows` events in batches of at most
//! [`QUERY_STREAM_BATCH_SIZE`], then `StatementEnd` and `Done`. The text
//! and binary formatters consume that stream unchanged.
//!
//! SQL sources go through `sql_guard` (read-only / safe-mode rules) at
//! fetch time, when the backend flags are known. An empty statement is
//! rejected before it reaches the backend.

use std::sync::Arc;

use datazen_driver_api::{QueryStreamCallback, SqlTarget};

use super::{
    build_select_sql, ExportError, ExportSource, ExportTablesRequest, TableDataSource,
    TableExportInput,
};

/// Stream one SQL statement against the backend, guarded and targeted.
///
/// This is the single row source for exports and the Rust counterpart of
/// the frontend `SqlDataFetcher`: the caller supplies the statement plus
/// its database / schema target, Rust runs `sql_guard` with the session's
/// flags and streams the result into `callback`.
///
/// `database` / `schema` are carried as the statement's target:
/// PostgreSQL cannot reference another database in one statement, so a
/// bare `query_stream` would have streamed from the session's own database
/// (the "second database exports the first database's rows" bug).
pub async fn stream_sql_source(
    source: &dyn ExportSource,
    session_id: &str,
    sql: &str,
    database: Option<&str>,
    schema: Option<&str>,
    callback: &QueryStreamCallback,
) -> Result<(), ExportError> {
    if sql.trim().is_empty() {
        return Err(ExportError::Validation(
            "SQL data source must not be empty".into(),
        ));
    }
    let (_driver, read_only, safe_mode) = source.resolve_source(session_id).await?;
    crate::execution::domain::sql_guard::check_sql(sql, read_only, safe_mode)
        .map_err(ExportError::Validation)?;
    source
        .stream_query(
            session_id,
            sql,
            SqlTarget::new(database, schema),
            Arc::clone(callback),
        )
        .await
}

/// Resolve one export item to the SQL statement to stream, plus its
/// database / schema target. A named table is expressed as a SQL source
/// (`SELECT *` or the selected columns); an arbitrary-SQL item streams
/// its statement as-is. A per-table schema wins over the request default.
pub(crate) fn resolve_sql_source(
    request: &ExportTablesRequest,
    table: &TableExportInput,
) -> Result<(String, Option<String>, Option<String>), ExportError> {
    let database = request.database.clone();
    let schema = table.schema.clone().or(request.schema.clone());
    match &table.source {
        TableDataSource::Table => Ok((
            build_select_sql(
                &table.table_name,
                &table.columns,
                request.database_type.as_deref(),
            ),
            database,
            schema,
        )),
        TableDataSource::Sql { sql } => Ok((sql.clone(), database, schema)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{
        build_file_plans, prepare_export, write_data_file, DataFormat, ExportError, ExportEvents,
        ExportMode, ExportProgressEvent, ExportSource, ExportTablesRequest, ExportTablesResult,
        FilePlan, OutputMode, TableDataSource, TableExportInput,
    };
    use crate::jobs::CancelToken;
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

    /// Stub backend: records the SQL it was asked to stream (and the
    /// database / schema target it was asked to stream it from) and replays
    /// canned columns/rows as stream events.
    struct StubSource {
        driver: Arc<dyn DatabaseDriver>,
        read_only: bool,
        safe_mode: bool,
        columns: Vec<ColumnInfo>,
        rows: Vec<Vec<Option<Value>>>,
        streamed_sql: Arc<std::sync::Mutex<Vec<String>>>,
        streamed_targets: Arc<std::sync::Mutex<Vec<(Option<String>, Option<String>)>>>,
        stream_calls: Arc<AtomicUsize>,
        /// Test seam: when false, `stream_query` emits `Rows` without a
        /// preceding `StatementStart`, so the exporter's header fallback
        /// (item 3a) can be exercised.
        emit_start: bool,
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
                streamed_targets: Arc::new(std::sync::Mutex::new(Vec::new())),
                stream_calls: Arc::new(AtomicUsize::new(0)),
                emit_start: true,
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
            target: datazen_driver_api::SqlTarget<'_>,
            callback: QueryStreamCallback,
        ) -> Result<(), ExportError> {
            self.streamed_sql.lock().unwrap().push(sql.to_string());
            self.streamed_targets.lock().unwrap().push((
                target.database.map(str::to_string),
                target.schema.map(str::to_string),
            ));
            self.stream_calls.fetch_add(1, Ordering::Relaxed);
            if self.emit_start {
                callback(QueryStreamEvent::StatementStart {
                    index: 0,
                    sql: sql.to_string(),
                    columns: self.columns.clone(),
                });
            }
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

    /// Test seam: replays two row batches, cancelling the shared token
    /// between them — the second batch must never be written, and the
    /// stream still returns `Ok` (a source that keeps delivering after
    /// the cancel), so the writer's own checks are what stop the run.
    struct CancellingSource {
        driver: Arc<dyn DatabaseDriver>,
        columns: Vec<ColumnInfo>,
        first: Vec<Vec<Option<Value>>>,
        second: Vec<Vec<Option<Value>>>,
        token: CancelToken,
    }

    impl CancellingSource {
        fn new(
            columns: Vec<ColumnInfo>,
            first: Vec<Vec<Option<Value>>>,
            second: Vec<Vec<Option<Value>>>,
            token: CancelToken,
        ) -> Self {
            Self {
                driver: test_driver(),
                columns,
                first,
                second,
                token,
            }
        }
    }

    #[async_trait::async_trait]
    impl ExportSource for CancellingSource {
        async fn resolve_source(
            &self,
            _session_id: &str,
        ) -> Result<(Arc<dyn DatabaseDriver>, bool, bool), ExportError> {
            Ok((self.driver.clone(), false, false))
        }

        async fn stream_query(
            &self,
            _session_id: &str,
            sql: &str,
            _target: datazen_driver_api::SqlTarget<'_>,
            callback: QueryStreamCallback,
        ) -> Result<(), ExportError> {
            callback(QueryStreamEvent::StatementStart {
                index: 0,
                sql: sql.to_string(),
                columns: self.columns.clone(),
            });
            callback(QueryStreamEvent::Rows {
                index: 0,
                rows: self.first.clone(),
            });
            self.token.cancel();
            callback(QueryStreamEvent::Rows {
                index: 0,
                rows: self.second.clone(),
            });
            callback(QueryStreamEvent::StatementEnd {
                index: 0,
                rows_affected: None,
                execution_time_ms: 0,
                truncated: false,
            });
            callback(QueryStreamEvent::Done { total_time_ms: 0 });
            Ok(())
        }
    }

    /// Test seam: a well-behaved source that cancels the shared token
    /// when its first stream finishes — the next table must never start.
    struct CancelAfterFirstStreamSource {
        inner: StubSource,
        token: CancelToken,
    }

    #[async_trait::async_trait]
    impl ExportSource for CancelAfterFirstStreamSource {
        async fn resolve_source(
            &self,
            session_id: &str,
        ) -> Result<(Arc<dyn DatabaseDriver>, bool, bool), ExportError> {
            self.inner.resolve_source(session_id).await
        }

        async fn stream_query(
            &self,
            session_id: &str,
            sql: &str,
            target: datazen_driver_api::SqlTarget<'_>,
            callback: QueryStreamCallback,
        ) -> Result<(), ExportError> {
            let result = self
                .inner
                .stream_query(session_id, sql, target, callback)
                .await;
            self.token.cancel();
            result
        }
    }

    impl ExportEvents for NoopEvents {
        fn progress_callback(&self) -> Arc<dyn Fn(ExportProgressEvent) + Send + Sync> {
            Arc::new(|_| {})
        }
    }

    struct CountingEvents {
        events: Arc<std::sync::Mutex<Vec<ExportProgressEvent>>>,
    }

    impl CountingEvents {
        fn new() -> Self {
            Self {
                events: Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }
    }

    impl ExportEvents for CountingEvents {
        fn progress_callback(&self) -> Arc<dyn Fn(ExportProgressEvent) + Send + Sync> {
            let events = Arc::clone(&self.events);
            Arc::new(move |event| events.lock().unwrap().push(event))
        }
    }

    fn request(
        mode: ExportMode,
        format: DataFormat,
        tables: Vec<TableExportInput>,
    ) -> ExportTablesRequest {
        ExportTablesRequest {
            db_session_id: "s".into(),
            cancel_token: None,
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
            primary_key: None,
        }
    }

    fn temp_file(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("datazen-export-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    // --- stream_sql_source: guard + streaming -----------------------------

    #[tokio::test]
    async fn sql_source_streams_valid_sql() {
        let source = StubSource::new(true, true, two_cols(), two_rows());
        let (callback, events) = collect_events();
        stream_sql_source(
            &source,
            "s",
            "SELECT id, name FROM users WHERE id > 1",
            None,
            None,
            &callback,
        )
        .await
        .unwrap();
        assert_eq!(
            *source.streamed_sql.lock().unwrap(),
            vec!["SELECT id, name FROM users WHERE id > 1".to_string()]
        );
        assert_eq!(events.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn sql_source_rejects_drop_under_guard() {
        let source = StubSource::new(true, true, two_cols(), two_rows());
        let (callback, _) = collect_events();
        let err = stream_sql_source(&source, "s", "DROP TABLE users", None, None, &callback)
            .await
            .unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(_)),
            "unexpected error: {err:?}"
        );
        assert_eq!(source.stream_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn sql_source_rejects_where_less_delete_in_safe_mode() {
        let source = StubSource::new(true, true, two_cols(), two_rows());
        let (callback, _) = collect_events();
        let err = stream_sql_source(&source, "s", "DELETE FROM users", None, None, &callback)
            .await
            .unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(_)),
            "unexpected: {err:?}"
        );
        assert_eq!(source.stream_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn sql_source_rejects_empty_sql() {
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let (callback, _) = collect_events();
        let err = stream_sql_source(&source, "s", "   ", None, None, &callback)
            .await
            .unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(_)),
            "unexpected: {err:?}"
        );
        assert_eq!(source.stream_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn sql_source_guard_only_applies_under_read_only_or_safe_mode() {
        // Same DROP passes the guard when neither flag is set: check_sql is
        // a no-op there, so the statement reaches the backend.
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let (callback, _) = collect_events();
        stream_sql_source(&source, "s", "DROP TABLE users", None, None, &callback)
            .await
            .unwrap();
        assert_eq!(source.stream_calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn table_source_resolves_to_sql_source_with_built_select() {
        let mut req = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![TableExportInput {
                table_name: "users".into(),
                schema: None,
                columns: vec!["id".into(), "name".into()],
                ddl: None,
                source: TableDataSource::Table,
                primary_key: None,
            }],
        );
        req.database_type = Some("mysql".into());
        let (sql, database, schema) = resolve_sql_source(&req, &req.tables[0]).unwrap();
        assert_eq!(sql, "SELECT `id`, `name` FROM `users`");
        assert_eq!(database, None);
        assert_eq!(schema, None);
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let (callback, _) = collect_events();
        stream_sql_source(
            &source,
            "s",
            &sql,
            database.as_deref(),
            schema.as_deref(),
            &callback,
        )
        .await
        .unwrap();
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
    fn sql_source_round_trips_with_explicit_type_tag() {
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
                "tableName": "q",
                "source": { "type": "sql", "sql": "SELECT 1" },
            }],
        });
        let req: ExportTablesRequest = serde_json::from_value(json).unwrap();
        assert!(matches!(
            req.tables[0].source,
            TableDataSource::Sql { ref sql } if sql == "SELECT 1"
        ));
    }

    #[test]
    fn provided_source_fails_to_deserialize() {
        // Rust no longer accepts a provided source at all: a request
        // carrying one fails in the deserialization layer, before any
        // planning or file work. The frontend formats provided rows
        // itself and never sends them here.
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
        let err = serde_json::from_value::<ExportTablesRequest>(json).unwrap_err();
        assert!(
            err.to_string().contains("unknown variant"),
            "unexpected error: {err}"
        );
        let source_json = serde_json::json!({
            "type": "provided",
            "columns": [],
            "rows": [],
        });
        assert!(serde_json::from_value::<TableDataSource>(source_json).is_err());
    }

    #[tokio::test]
    async fn resolve_sql_source_carries_database_and_schema_target() {
        // Cross-database routing: the request database and the winning
        // schema reach stream_query as the statement's SqlTarget, so a
        // second database never exports the session database's rows.
        let mut req = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![TableExportInput {
                table_name: "users".into(),
                schema: Some("s2".into()),
                columns: vec![],
                ddl: None,
                source: TableDataSource::Table,
                primary_key: None,
            }],
        );
        req.database = Some("db2".into());
        req.schema = Some("public".into());
        let (sql, database, schema) = resolve_sql_source(&req, &req.tables[0]).unwrap();
        assert_eq!(database.as_deref(), Some("db2"));
        // Per-table schema wins over the request default.
        assert_eq!(schema.as_deref(), Some("s2"));
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let (callback, _) = collect_events();
        stream_sql_source(
            &source,
            "s",
            &sql,
            database.as_deref(),
            schema.as_deref(),
            &callback,
        )
        .await
        .unwrap();
        assert_eq!(
            *source.streamed_targets.lock().unwrap(),
            vec![(Some("db2".to_string()), Some("s2".to_string()))]
        );

        // SQL source with no per-table schema falls back to the request
        // schema, with the request database still carried.
        let req2 = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![table_item(
                "q",
                TableDataSource::Sql {
                    sql: "SELECT 1".into(),
                },
            )],
        );
        let mut req2 = req2;
        req2.database = Some("db2".into());
        req2.schema = Some("public".into());
        let (sql2, database2, schema2) = resolve_sql_source(&req2, &req2.tables[0]).unwrap();
        assert_eq!(sql2, "SELECT 1");
        assert_eq!(database2.as_deref(), Some("db2"));
        assert_eq!(schema2.as_deref(), Some("public"));
        let source2 = StubSource::new(false, false, two_cols(), two_rows());
        let (callback2, _) = collect_events();
        stream_sql_source(
            &source2,
            "s",
            &sql2,
            database2.as_deref(),
            schema2.as_deref(),
            &callback2,
        )
        .await
        .unwrap();
        assert_eq!(
            *source2.streamed_targets.lock().unwrap(),
            vec![(Some("db2".to_string()), Some("public".to_string()))]
        );
    }

    #[tokio::test]
    async fn write_data_file_emits_progress_events() {
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
        let events = CountingEvents::new();
        let dest = temp_file("out.csv");
        let n = write_data_file(
            &events,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Csv,
            None,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(n, 2);
        let seen = events.events.lock().unwrap();
        assert!(!seen.is_empty(), "progress must fire while streaming");
        assert!(seen.iter().all(|e| e.table == "filtered"));
        assert_eq!(seen.last().unwrap().rows_written, 2);
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_binary_file_emits_progress_events() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Xlsx,
            vec![table_item(
                "t",
                TableDataSource::Sql {
                    sql: "SELECT 1".into(),
                },
            )],
        );
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let events = CountingEvents::new();
        let dest = temp_file("out.xlsx");
        let n = crate::export::write_binary_file(
            &events,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Xlsx,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(n, 2);
        let seen = events.events.lock().unwrap();
        assert!(!seen.is_empty(), "progress must fire while streaming");
        assert!(seen.iter().all(|e| e.table == "t"));
        assert_eq!(seen.last().unwrap().rows_written, 2);
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
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
                primary_key: None,
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
        let empty = request(ExportMode::DataOnly, DataFormat::Csv, vec![]);
        assert!(matches!(
            prepare_export(&empty).unwrap_err(),
            ExportError::Validation(_)
        ));
    }

    #[test]
    fn build_file_plans_sql_update_structure_merges_into_one_sql_file() {
        // Like SqlInsert: DDL is a prefix in the same .sql file, so the DDL
        // plan and the data plan never collide on "{table}.sql".
        let mut item = table_item("users", TableDataSource::Table);
        item.ddl = Some("CREATE TABLE users (id INT);".into());
        let req = request(
            ExportMode::DataAndStructure,
            DataFormat::SqlUpdate,
            vec![item],
        );
        let plans = build_file_plans(&req).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].filename(), "users.sql");
        match &plans[0] {
            FilePlan::Data { ddl_prefix, .. } => {
                assert!(ddl_prefix.as_deref().unwrap().contains("CREATE TABLE"));
            }
            other => panic!("expected a data plan, got {other:?}"),
        }
    }

    #[test]
    fn data_format_extensions_and_binary_flag() {
        assert_eq!(DataFormat::Csv.extension(), "csv");
        assert_eq!(DataFormat::Tsv.extension(), "tsv");
        assert_eq!(DataFormat::Json.extension(), "json");
        assert_eq!(DataFormat::Markdown.extension(), "md");
        assert_eq!(DataFormat::SqlInsert.extension(), "sql");
        assert_eq!(DataFormat::SqlUpdate.extension(), "sql");
        assert_eq!(DataFormat::Xlsx.extension(), "xlsx");
        assert!(DataFormat::Xlsx.is_binary());
        for format in [
            DataFormat::Csv,
            DataFormat::Tsv,
            DataFormat::Json,
            DataFormat::Markdown,
            DataFormat::SqlInsert,
            DataFormat::SqlUpdate,
        ] {
            assert!(!format.is_binary(), "{format:?} must be text");
        }
        // Serde names are the snake_case wire names the frontend sends.
        assert_eq!(
            serde_json::to_string(&DataFormat::SqlUpdate).unwrap(),
            "\"sql_update\""
        );
        assert_eq!(
            serde_json::from_str::<DataFormat>("\"xlsx\"").unwrap(),
            DataFormat::Xlsx
        );
    }

    #[test]
    fn validate_export_path_accepts_new_extensions() {
        for ext in ["tsv", "md", "xlsx", "csv", "json", "sql", "zip"] {
            let path = std::path::PathBuf::from(format!("export.{ext}"));
            assert!(
                crate::export::validate_export_path(&path).is_ok(),
                ".{ext} must be allowed"
            );
        }
        assert!(
            crate::export::validate_export_path(std::path::Path::new("export.parquet")).is_err()
        );
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
            &CancelToken::new(),
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

    // --- CSV formula-injection guard parity (issue 2) ----------------------
    //
    // Shared fixture with the JS parity tests
    // (`packages/frontend/src/lib/__tests__/exportFormat.test.ts` and
    // `packages/frontend/src/platform/__tests__/blobTablesExporter.test.ts`):
    // the same rows must produce byte-identical CSV in all three paths
    // (modulo the in-memory formatter's missing trailing newline — see
    // the JS test). Desktop CSV bytes change on purpose here: cells
    // starting with `=`, `+`, `-` or `@` are now quoted (security fix,
    // not a regression).

    fn formula_guard_cols() -> Vec<ColumnInfo> {
        vec![col("v", "text")]
    }

    fn formula_guard_rows() -> Vec<Vec<Option<Value>>> {
        vec![
            vec![Some(Value::String("=SUM(A1:B2)".into()))],
            vec![Some(Value::String("+2+2".into()))],
            vec![Some(Value::String("-x".into()))],
            vec![Some(Value::String("@user".into()))],
            vec![Some(Value::Integer(-5))],
            vec![Some(Value::String("=a,b".into()))],
            vec![Some(Value::String("say \"hi\"".into()))],
            vec![Some(Value::String("line1\nline2".into()))],
            vec![Some(Value::String("plain".into()))],
            vec![Some(Value::Integer(5))],
        ]
    }

    const FORMULA_GUARD_CSV: &str = "v\n\"=SUM(A1:B2)\"\n\"+2+2\"\n\"-x\"\n\"@user\"\n\"-5\"\n\"=a,b\"\n\"say \"\"hi\"\"\"\n\"line1\nline2\"\nplain\n5\n";

    #[test]
    fn apply_csv_formula_guard_quotes_formula_prefixes_only() {
        let guard = crate::export::apply_csv_formula_guard;
        assert_eq!(guard("=SUM(A1)"), "\"=SUM(A1)\"");
        assert_eq!(guard("+2+2"), "\"+2+2\"");
        assert_eq!(guard("-x"), "\"-x\"");
        assert_eq!(guard("@user"), "\"@user\"");
        // Negative numbers keep the quoting the rule gives them
        // (deliberate, matching the web exporter byte for byte).
        assert_eq!(guard("-5"), "\"-5\"");
        // Already-quoted values (delimiter / quote / line break inside)
        // start with `"` and are left alone — no double wrap.
        assert_eq!(guard("\"=a,b\""), "\"=a,b\"");
        assert_eq!(guard("\"say \"\"hi\"\"\""), "\"say \"\"hi\"\"\"");
        assert_eq!(guard("\"line1\nline2\""), "\"line1\nline2\"");
        // Plain text, plain numbers and empty cells are untouched.
        assert_eq!(guard("plain"), "plain");
        assert_eq!(guard("5"), "5");
        assert_eq!(guard(""), "");
    }

    #[test]
    fn stream_formatter_csv_applies_formula_guard() {
        let mut formatter =
            crate::export::StreamFormatter::new(DataFormat::Csv, "t".into(), None, test_driver());
        let columns = vec!["v".to_string()];
        let mut out = formatter.header(&columns);
        out.push_str(&formatter.rows(&formula_guard_rows(), &columns).unwrap());
        assert_eq!(out, FORMULA_GUARD_CSV);
        // The header branch guards too: a column name starting with `=`
        // is quoted instead of reaching the spreadsheet as a formula.
        let mut formatter =
            crate::export::StreamFormatter::new(DataFormat::Csv, "t".into(), None, test_driver());
        assert_eq!(formatter.header(&["=note".to_string()]), "\"=note\"\n");
    }

    #[tokio::test]
    async fn write_data_file_sql_source_applies_csv_formula_guard() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![table_item(
                "filtered",
                TableDataSource::Sql {
                    sql: "SELECT v FROM t".into(),
                },
            )],
        );
        let source = StubSource::new(false, false, formula_guard_cols(), formula_guard_rows());
        let dest = temp_file("out.csv");
        let n = write_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Csv,
            None,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(n, 10);
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), FORMULA_GUARD_CSV);
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_data_file_sql_source_writes_json() {
        // The JSON write path used to be covered by a provided-source
        // test; the same rows now come from the stub backend over SQL.
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Json,
            vec![table_item(
                "filtered",
                TableDataSource::Sql {
                    sql: "SELECT id, name FROM users WHERE id > 1".into(),
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
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(n, 2);
        assert_eq!(
            *source.streamed_sql.lock().unwrap(),
            vec!["SELECT id, name FROM users WHERE id > 1".to_string()]
        );
        let parsed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&dest).unwrap()).unwrap();
        assert_eq!(
            parsed,
            serde_json::json!([{ "id": 1, "name": "a" }, { "id": 2, "name": null }])
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_data_file_sql_insert_batches_rows() {
        // SqlInsert had no write-path test: header BEGIN, one INSERT
        // batch per flush, tail COMMIT.
        let req = request(
            ExportMode::DataOnly,
            DataFormat::SqlInsert,
            vec![table_item(
                "users",
                TableDataSource::Sql {
                    sql: "SELECT 1".into(),
                },
            )],
        );
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.sql");
        let n = write_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::SqlInsert,
            None,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(n, 2);
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            "BEGIN;\nINSERT INTO \"users\" (\"id\", \"name\") VALUES\n  (1, CAST(X'61' AS TEXT)),\n  (2, NULL);\nCOMMIT;\n"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_data_file_json_renders_all_value_kinds() {
        // Every json_value arm: bool / float / bytes marker / timestamp /
        // bytes-typed JSON marker / plain JSON, plus a null row.
        let columns = vec![
            col("n", "int4"),
            col("f", "float8"),
            col("b", "bool"),
            col("s", "text"),
            col("by", "bytea"),
            col("ts", "timestamptz"),
            col("j", "jsonb"),
            col("p", "jsonb"),
        ];
        let marker = serde_json::json!({
            "$datazenType": "bytes",
            "encoding": "hex",
            "value": "00",
        });
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Json,
            vec![table_item(
                "t",
                TableDataSource::Sql {
                    sql: "SELECT 1".into(),
                },
            )],
        );
        let source = StubSource::new(
            false,
            false,
            columns,
            vec![
                vec![
                    Some(Value::Integer(7)),
                    Some(Value::Float(1.5)),
                    Some(Value::Bool(true)),
                    Some(Value::String("x".into())),
                    Some(Value::Bytes(vec![0xde, 0xad])),
                    Some(Value::Timestamp("2026-11-09T16:00:00Z".into())),
                    Some(Value::Json(marker.clone())),
                    Some(Value::Json(serde_json::json!({"k": 1}))),
                ],
                vec![None, None, None, None, None, None, None, None],
            ],
        );
        let dest = temp_file("out.json");
        let n = write_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Json,
            None,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(n, 2);
        let parsed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&dest).unwrap()).unwrap();
        assert_eq!(
            parsed,
            serde_json::json!([
                {
                    "n": 7,
                    "f": 1.5,
                    "b": true,
                    "s": "x",
                    "by": { "$datazenType": "bytes", "encoding": "hex", "value": "dead" },
                    "ts": "2026-11-09T16:00:00Z",
                    "j": { "$datazenType": "json", "value": marker },
                    "p": { "k": 1 },
                },
                {
                    "n": null,
                    "f": null,
                    "b": null,
                    "s": null,
                    "by": null,
                    "ts": null,
                    "j": null,
                    "p": null,
                },
            ])
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_data_file_csv_renders_all_value_kinds() {
        // Every value_as_string arm plus the quoting branch and the
        // reserved-prefix branch (a timestamp that already starts with
        // the marker prefix is re-prefixed, never mistaken for bytes).
        let columns = vec![
            col("b", "bool"),
            col("f", "float8"),
            col("s", "text"),
            col("by", "bytea"),
            col("ts", "text"),
            col("j", "jsonb"),
        ];
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![table_item(
                "t",
                TableDataSource::Sql {
                    sql: "SELECT 1".into(),
                },
            )],
        );
        let source = StubSource::new(
            false,
            false,
            columns,
            vec![vec![
                Some(Value::Bool(true)),
                Some(Value::Float(1.5)),
                Some(Value::String("a,\"b\"".into())),
                Some(Value::Bytes(vec![0xde, 0xad])),
                Some(Value::Timestamp("datazen:text:hi".into())),
                Some(Value::Json(serde_json::json!({"k": 1}))),
            ]],
        );
        let dest = temp_file("out.csv");
        let n = write_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Csv,
            None,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(n, 1);
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            "b,f,s,by,ts,j\ntrue,1.5,\"a,\"\"b\"\"\",datazen:bytes:hex:dead,datazen:text:datazen:text:hi,\"{\"\"k\"\":1}\"\n"
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
            &CancelToken::new(),
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
            &CancelToken::new(),
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

    // --- Item 3a: new text formats (TSV / Markdown / SQL UPDATE) ---------
    // These used to carry their rows as a provided source; the rows now
    // come from the stub backend over a SQL source, so the formatter
    // behaviour they pin is unchanged.

    #[tokio::test]
    async fn write_data_file_tsv_flattens_tabs_and_newlines() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Tsv,
            vec![table_item(
                "t",
                TableDataSource::Sql {
                    sql: "SELECT 1".into(),
                },
            )],
        );
        let source = StubSource::new(
            false,
            false,
            two_cols(),
            vec![
                vec![
                    Some(Value::Integer(1)),
                    Some(Value::String("a\tb\nc".into())),
                ],
                vec![Some(Value::Integer(2)), Some(Value::Null)],
            ],
        );
        let dest = temp_file("out.tsv");
        let n = write_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Tsv,
            None,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(n, 2);
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            "id\tname\n1\ta b c\n2\t\n"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_data_file_markdown_escapes_pipes_and_breaks() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Markdown,
            vec![table_item(
                "t",
                TableDataSource::Sql {
                    sql: "SELECT 1".into(),
                },
            )],
        );
        let source = StubSource::new(
            false,
            false,
            two_cols(),
            vec![vec![
                Some(Value::Integer(1)),
                Some(Value::String("a|b\nc\\d".into())),
            ]],
        );
        let dest = temp_file("out.md");
        write_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Markdown,
            None,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            "| id | name |\n| --- | --- |\n| 1 | a\\|b<br>c\\\\d |\n"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_data_file_sql_update_uses_primary_key() {
        let mut item = table_item(
            "users",
            TableDataSource::Sql {
                sql: "SELECT 1".into(),
            },
        );
        item.primary_key = Some("id".into());
        let req = request(ExportMode::DataOnly, DataFormat::SqlUpdate, vec![item]);
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.sql");
        write_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::SqlUpdate,
            None,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            // The sqlite mock driver renders string literals as CAST(X'…'
            // AS TEXT); the UPDATE shape (SET … WHERE pk = …) is what this
            // test pins, not the dialect's literal style.
            "BEGIN;\nUPDATE \"users\" SET \"name\" = CAST(X'61' AS TEXT) WHERE \"id\" = 1;\nUPDATE \"users\" SET \"name\" = NULL WHERE \"id\" = 2;\nCOMMIT;\n"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_data_file_sql_update_falls_back_to_first_column() {
        // No primary_key supplied: the first column is the key, as in the
        // frontend sql_update branch.
        let req = request(
            ExportMode::DataOnly,
            DataFormat::SqlUpdate,
            vec![table_item(
                "users",
                TableDataSource::Sql {
                    sql: "SELECT 1".into(),
                },
            )],
        );
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.sql");
        write_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::SqlUpdate,
            None,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert!(
            std::fs::read_to_string(&dest)
                .unwrap()
                .contains("WHERE \"id\" = 1;"),
            "first column is the fallback key"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_data_file_sql_update_single_column_is_rejected() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::SqlUpdate,
            vec![table_item(
                "only_id",
                TableDataSource::Sql {
                    sql: "SELECT 1".into(),
                },
            )],
        );
        let source = StubSource::new(
            false,
            false,
            vec![col("id", "int4")],
            vec![vec![Some(Value::Integer(1))]],
        );
        let dest = temp_file("out.sql");
        let err = write_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::SqlUpdate,
            None,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(ref m) if m.contains("non-key column")),
            "unexpected error: {err:?}"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    // --- Item 3a: binary dispatch + multi-plan guard ----------------------

    #[tokio::test]
    async fn write_data_file_rejects_binary_format() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Xlsx,
            vec![table_item("t", TableDataSource::Table)],
        );
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.xlsx");
        let err = write_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Xlsx,
            None,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(ref m) if m.contains("write_binary_file")),
            "unexpected error: {err:?}"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_binary_file_rejects_text_format() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![table_item("t", TableDataSource::Table)],
        );
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.csv");
        let err = crate::export::write_binary_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Csv,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(ref m) if m.contains("write_data_file")),
            "unexpected error: {err:?}"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_binary_file_sql_source_writes_xlsx() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Xlsx,
            vec![table_item(
                "t",
                TableDataSource::Sql {
                    sql: "SELECT 1".into(),
                },
            )],
        );
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.xlsx");
        let n = crate::export::write_binary_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Xlsx,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(n, 2);
        assert_eq!(source.stream_calls.load(Ordering::Relaxed), 1);
        assert!(dest.exists(), "xlsx file written");
        let file = std::fs::File::open(&dest).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        assert!(zip.by_name("xl/worksheets/sheet1.xml").is_ok());
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[test]
    fn prepare_export_forces_zip_for_multi_plan_xlsx() {
        let req = request(
            ExportMode::DataAndStructure,
            DataFormat::Xlsx,
            vec![table_item("users", TableDataSource::Table)],
        );
        let (plans, use_zip) = prepare_export(&req).unwrap();
        assert_eq!(plans.len(), 2);
        assert!(use_zip, "DDL .sql + data .xlsx must zip");
    }

    #[tokio::test]
    async fn run_single_export_refuses_binary_multi_plan() {
        // Defence in depth: even a direct caller bypassing prepare_export
        // must never get concatenated XLSX bytes.
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Xlsx,
            vec![
                table_item("a", TableDataSource::Table),
                table_item("b", TableDataSource::Table),
            ],
        );
        let plans = build_file_plans(&req).unwrap();
        assert_eq!(plans.len(), 2);
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.xlsx");
        let err = crate::export::run_single_export(
            &NoopEvents,
            &source,
            &req,
            &plans,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(ref m) if m.contains("zip")),
            "unexpected error: {err:?}"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn run_zip_export_writes_xlsx_entries() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Xlsx,
            vec![
                table_item("a", TableDataSource::Table),
                table_item("b", TableDataSource::Table),
            ],
        );
        let plans = build_file_plans(&req).unwrap();
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.zip");
        let n = crate::export::run_zip_export(
            &NoopEvents,
            &source,
            &req,
            &plans,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(n, 4);
        let file = std::fs::File::open(&dest).unwrap();
        let zip = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = zip.file_names().map(str::to_string).collect();
        assert_eq!(names, vec!["a.xlsx".to_string(), "b.xlsx".to_string()]);
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    // --- Item 3a coverage: dispatch, fallbacks, defensive arms -----------

    #[tokio::test]
    async fn write_data_file_sql_update_sqlserver_transaction_header() {
        // The SqlInsert/SqlUpdate header shares the sqlserver branch.
        let mut item = table_item(
            "users",
            TableDataSource::Sql {
                sql: "SELECT 1".into(),
            },
        );
        item.primary_key = Some("id".into());
        let mut req = request(ExportMode::DataOnly, DataFormat::SqlUpdate, vec![item]);
        req.database_type = Some("sqlserver".into());
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.sql");
        write_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::SqlUpdate,
            None,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        let text = std::fs::read_to_string(&dest).unwrap();
        assert!(text.starts_with("BEGIN TRANSACTION;\n"), "{text}");
        assert!(text.ends_with("COMMIT;\n"), "{text}");
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[test]
    fn stream_formatter_xlsx_header_is_empty_by_construction() {
        // Binary formats never go through the text formatter (write_data_file
        // rejects them first); the Xlsx arms in StreamFormatter only exist
        // so construction cannot panic if one is ever routed here.
        let mut f =
            crate::export::StreamFormatter::new(DataFormat::Xlsx, "t".into(), None, test_driver());
        assert_eq!(f.header(&["id".to_string()]), "");
    }

    #[tokio::test]
    async fn write_binary_file_begin_failure_is_reported() {
        // The 16_385-column schema arrives over the SQL stream's
        // StatementStart, so the binary formatter's begin still fails.
        let wide: Vec<ColumnInfo> = (0..16_385).map(|i| col(&format!("c{i}"), "text")).collect();
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Xlsx,
            vec![table_item(
                "wide",
                TableDataSource::Sql {
                    sql: "SELECT 1".into(),
                },
            )],
        );
        let source = StubSource::new(false, false, wide, vec![]);
        let dest = temp_file("out.xlsx");
        let err = crate::export::write_binary_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Xlsx,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(ref m) if m.contains("16385 columns")),
            "unexpected error: {err:?}"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_binary_file_rows_before_start_fall_back_to_requested_columns() {
        // A stream that skips StatementStart still gets a header, from the
        // requested column names — and from col_0… when none were requested.
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Xlsx,
            vec![TableExportInput {
                table_name: "t".into(),
                schema: None,
                columns: vec!["id".into(), "name".into()],
                ddl: None,
                primary_key: None,
                source: TableDataSource::Sql {
                    sql: "SELECT 1".into(),
                },
            }],
        );
        let mut source = StubSource::new(false, false, two_cols(), two_rows());
        source.emit_start = false;
        let dest = temp_file("fallback.xlsx");
        let n = crate::export::write_binary_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Xlsx,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(n, 2);
        let text = {
            let file = std::fs::File::open(&dest).unwrap();
            let mut zip = zip::ZipArchive::new(file).unwrap();
            let mut out = String::new();
            for name in ["xl/worksheets/sheet1.xml", "xl/sharedStrings.xml"] {
                if let Ok(mut entry) = zip.by_name(name) {
                    use std::io::Read as _;
                    let mut s = String::new();
                    entry.read_to_string(&mut s).unwrap();
                    out.push_str(&s);
                }
            }
            out
        };
        assert!(
            text.contains("id") && text.contains("name"),
            "fallback header: {text}"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());

        // No requested columns either: col_0 / col_1 stand in.
        let req2 = request(
            ExportMode::DataOnly,
            DataFormat::Xlsx,
            vec![table_item(
                "t",
                TableDataSource::Sql {
                    sql: "SELECT 1".into(),
                },
            )],
        );
        let mut source2 = StubSource::new(false, false, two_cols(), two_rows());
        source2.emit_start = false;
        let dest2 = temp_file("fallback2.xlsx");
        crate::export::write_binary_file(
            &NoopEvents,
            &source2,
            &req2,
            &req2.tables[0],
            DataFormat::Xlsx,
            &dest2,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        let file = std::fs::File::open(&dest2).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let mut entry = zip.by_name("xl/worksheets/sheet1.xml").unwrap();
        let mut text = String::new();
        use std::io::Read as _;
        entry.read_to_string(&mut text).unwrap();
        assert!(
            text.contains("col_0") && text.contains("col_1"),
            "col_N header: {text}"
        );
        let _ = std::fs::remove_dir_all(dest2.parent().unwrap());
    }

    #[tokio::test]
    async fn write_binary_file_row_wider_than_excel_columns_is_rejected() {
        // The canned row is wider than the schema: the library's column
        // limit surfaces as a friendly Validation error, not a panic.
        let wide_row: Vec<Option<Value>> = (0..17_000).map(|i| Some(Value::Integer(i))).collect();
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Xlsx,
            vec![table_item(
                "t",
                TableDataSource::Sql {
                    sql: "SELECT 1".into(),
                },
            )],
        );
        let source = StubSource::new(false, false, two_cols(), vec![wide_row]);
        let dest = temp_file("wide.xlsx");
        let err = crate::export::write_binary_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Xlsx,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(ref m) if m.contains("worksheet limits")),
            "unexpected error: {err:?}"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_plan_data_file_rejects_ddl_prefix_for_binary() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Xlsx,
            vec![table_item("t", TableDataSource::Table)],
        );
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.xlsx");
        let err = crate::export::write_plan_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Xlsx,
            Some("CREATE TABLE t (id INT);"),
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, ExportError::Validation(ref m) if m.contains("DDL prefix")),
            "unexpected error: {err:?}"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn run_single_export_single_csv_plan_goes_through_dispatch() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![table_item("users", TableDataSource::Table)],
        );
        let plans = build_file_plans(&req).unwrap();
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.csv");
        let n = crate::export::run_single_export(
            &NoopEvents,
            &source,
            &req,
            &plans,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(n, 2);
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            "id,name\n1,a\n2,\n"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn run_single_export_merges_text_plans_with_banners() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![
                table_item("a", TableDataSource::Table),
                table_item("b", TableDataSource::Table),
            ],
        );
        let plans = build_file_plans(&req).unwrap();
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.csv");
        let n = crate::export::run_single_export(
            &NoopEvents,
            &source,
            &req,
            &plans,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(n, 4);
        let text = std::fs::read_to_string(&dest).unwrap();
        assert!(text.contains("-- ===== a.csv ====="), "{text}");
        assert!(text.contains("-- ===== b.csv ====="), "{text}");
        assert_eq!(text.matches("id,name").count(), 2, "{text}");
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn run_single_export_single_ddl_plan_writes_text() {
        let mut item = table_item("users", TableDataSource::Table);
        item.ddl = Some("CREATE TABLE users (id INT);".into());
        let req = request(ExportMode::StructureOnly, DataFormat::Csv, vec![item]);
        let plans = build_file_plans(&req).unwrap();
        assert_eq!(plans.len(), 1);
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.sql");
        let n = crate::export::run_single_export(
            &NoopEvents,
            &source,
            &req,
            &plans,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(n, 0);
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            "CREATE TABLE users (id INT);"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn run_zip_export_writes_csv_entries() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![
                table_item("a", TableDataSource::Table),
                table_item("b", TableDataSource::Table),
            ],
        );
        let plans = build_file_plans(&req).unwrap();
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.zip");
        let n = crate::export::run_zip_export(
            &NoopEvents,
            &source,
            &req,
            &plans,
            &dest,
            &CancelToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(n, 4);
        let file = std::fs::File::open(&dest).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        assert_eq!(zip.file_names().collect::<Vec<_>>(), vec!["a.csv", "b.csv"]);
        let mut entry = zip.by_name("a.csv").unwrap();
        let mut text = String::new();
        use std::io::Read as _;
        entry.read_to_string(&mut text).unwrap();
        assert_eq!(text, "id,name\n1,a\n2,\n");
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    // --- Cancellation -------------------------------------------------------

    #[test]
    fn export_tables_result_aborted_reports_partial_deleted_honestly() {
        let deleted = serde_json::to_value(ExportTablesResult::Aborted {
            partial_deleted: true,
        })
        .unwrap();
        assert_eq!(
            deleted,
            serde_json::json!({"Aborted": {"partialDeleted": true}})
        );
        let kept = serde_json::to_value(ExportTablesResult::Aborted {
            partial_deleted: false,
        })
        .unwrap();
        assert_eq!(
            kept,
            serde_json::json!({"Aborted": {"partialDeleted": false}})
        );
        // The wire shape must stay the TS mirror: PascalCase variants.
        let saved = serde_json::to_value(ExportTablesResult::Saved(3)).unwrap();
        assert_eq!(saved, serde_json::json!({"Saved": 3}));
        let cancelled = serde_json::to_value(ExportTablesResult::Cancelled).unwrap();
        assert_eq!(cancelled, serde_json::json!("Cancelled"));
    }

    #[test]
    fn export_tables_request_cancel_token_defaults_to_none() {
        let req: ExportTablesRequest = serde_json::from_value(serde_json::json!({
            "dbSessionId": "s",
            "mode": "data_only",
            "dataFormat": "csv",
            "outputMode": "single",
            "tables": [],
        }))
        .unwrap();
        assert_eq!(req.cancel_token, None);
        let req: ExportTablesRequest = serde_json::from_value(serde_json::json!({
            "dbSessionId": "s",
            "cancelToken": "tok-1",
            "mode": "data_only",
            "dataFormat": "csv",
            "outputMode": "single",
            "tables": [],
        }))
        .unwrap();
        assert_eq!(req.cancel_token.as_deref(), Some("tok-1"));
    }

    fn one_row(value: i64, name: &str) -> Vec<Vec<Option<Value>>> {
        vec![vec![
            Some(Value::Integer(value)),
            Some(Value::String(name.into())),
        ]]
    }

    #[tokio::test]
    async fn write_data_file_precancelled_token_writes_nothing() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![table_item("t", TableDataSource::Table)],
        );
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.csv");
        let token = CancelToken::new();
        token.cancel();
        let err = write_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Csv,
            None,
            &dest,
            &token,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, ExportError::Cancelled),
            "unexpected error: {err:?}"
        );
        assert!(
            !dest.exists(),
            "a cancelled export must not create its file"
        );
        assert_eq!(
            source.stream_calls.load(Ordering::Relaxed),
            0,
            "a pre-cancelled run must not stream at all"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_data_file_cancels_mid_stream_and_writes_no_further_rows() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![table_item("t", TableDataSource::Table)],
        );
        let token = CancelToken::new();
        let source =
            CancellingSource::new(two_cols(), one_row(1, "a"), one_row(2, "b"), token.clone());
        let dest = temp_file("out.csv");
        let err = write_data_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Csv,
            None,
            &dest,
            &token,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, ExportError::Cancelled),
            "unexpected error: {err:?}"
        );
        // The sink buffers below the 64KB flush watermark, so the first
        // batch may still sit in memory when the run stops — what matters
        // is that the batch delivered after the cancel never reaches disk
        // (and the caller removes the partial file; see the
        // run_single_export test below).
        assert!(
            !std::fs::read_to_string(&dest).unwrap().contains("2,b"),
            "the batch delivered after the cancel must not be written"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_binary_file_precancelled_token_writes_nothing() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Xlsx,
            vec![table_item("t", TableDataSource::Table)],
        );
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.xlsx");
        let token = CancelToken::new();
        token.cancel();
        let err = crate::export::write_binary_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Xlsx,
            &dest,
            &token,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, ExportError::Cancelled),
            "unexpected error: {err:?}"
        );
        assert!(
            !dest.exists(),
            "a cancelled export must not create its file"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn write_binary_file_cancels_mid_stream() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Xlsx,
            vec![table_item("t", TableDataSource::Table)],
        );
        let token = CancelToken::new();
        let source =
            CancellingSource::new(two_cols(), one_row(1, "a"), one_row(2, "b"), token.clone());
        let dest = temp_file("out.xlsx");
        let err = crate::export::write_binary_file(
            &NoopEvents,
            &source,
            &req,
            &req.tables[0],
            DataFormat::Xlsx,
            &dest,
            &token,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, ExportError::Cancelled),
            "unexpected error: {err:?}"
        );
        // The binary formatter owns the file from begin(); the run (not
        // the writer) removes the partial file — covered by the
        // run_single_export xlsx test below.
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn run_single_export_cancels_mid_stream_and_removes_partial_file() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![table_item("t", TableDataSource::Table)],
        );
        let plans = build_file_plans(&req).unwrap();
        let token = CancelToken::new();
        let source =
            CancellingSource::new(two_cols(), one_row(1, "a"), one_row(2, "b"), token.clone());
        let dest = temp_file("out.csv");
        let err =
            crate::export::run_single_export(&NoopEvents, &source, &req, &plans, &dest, &token)
                .await
                .unwrap_err();
        assert!(
            matches!(err, ExportError::Cancelled),
            "unexpected error: {err:?}"
        );
        assert!(
            !dest.exists(),
            "a cancelled export must not leave a partial file"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn run_single_export_cancels_mid_stream_and_removes_partial_xlsx() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Xlsx,
            vec![table_item("t", TableDataSource::Table)],
        );
        let plans = build_file_plans(&req).unwrap();
        let token = CancelToken::new();
        let source =
            CancellingSource::new(two_cols(), one_row(1, "a"), one_row(2, "b"), token.clone());
        let dest = temp_file("out.xlsx");
        let err =
            crate::export::run_single_export(&NoopEvents, &source, &req, &plans, &dest, &token)
                .await
                .unwrap_err();
        assert!(
            matches!(err, ExportError::Cancelled),
            "unexpected error: {err:?}"
        );
        assert!(
            !dest.exists(),
            "a cancelled export must not leave a partial file"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn run_single_export_precancelled_token_writes_nothing() {
        let req = request(
            ExportMode::DataOnly,
            DataFormat::Csv,
            vec![table_item("t", TableDataSource::Table)],
        );
        let plans = build_file_plans(&req).unwrap();
        let source = StubSource::new(false, false, two_cols(), two_rows());
        let dest = temp_file("out.csv");
        let token = CancelToken::new();
        token.cancel();
        let err =
            crate::export::run_single_export(&NoopEvents, &source, &req, &plans, &dest, &token)
                .await
                .unwrap_err();
        assert!(
            matches!(err, ExportError::Cancelled),
            "unexpected error: {err:?}"
        );
        assert!(
            !dest.exists(),
            "a cancelled export must not create its file"
        );
        assert_eq!(
            source.stream_calls.load(Ordering::Relaxed),
            0,
            "a pre-cancelled run must not stream at all"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    fn table_with_ddl(name: &str) -> TableExportInput {
        TableExportInput {
            table_name: name.into(),
            schema: None,
            columns: vec!["id".into()],
            ddl: Some(format!("CREATE TABLE {name} (id INT);")),
            source: TableDataSource::Table,
            primary_key: None,
        }
    }

    #[tokio::test]
    async fn run_single_export_stops_between_tables_on_cancel() {
        let req = request(
            ExportMode::DataAndStructure,
            DataFormat::Csv,
            vec![table_with_ddl("a"), table_with_ddl("b")],
        );
        let plans = build_file_plans(&req).unwrap();
        assert_eq!(plans.len(), 4, "ddl + data per table");
        let token = CancelToken::new();
        let inner = StubSource::new(false, false, two_cols(), two_rows());
        let calls = Arc::clone(&inner.stream_calls);
        let source = CancelAfterFirstStreamSource {
            inner,
            token: token.clone(),
        };
        let dest = temp_file("out.csv");
        let err =
            crate::export::run_single_export(&NoopEvents, &source, &req, &plans, &dest, &token)
                .await
                .unwrap_err();
        assert!(
            matches!(err, ExportError::Cancelled),
            "unexpected error: {err:?}"
        );
        assert!(
            !dest.exists(),
            "a cancelled export must not leave a partial file"
        );
        assert_eq!(
            calls.load(Ordering::Relaxed),
            1,
            "the second table must never start streaming"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[tokio::test]
    async fn run_zip_export_cancels_mid_stream_and_removes_target() {
        let mut req = request(
            ExportMode::DataAndStructure,
            DataFormat::Csv,
            vec![table_with_ddl("a")],
        );
        req.output_mode = OutputMode::Zip;
        let plans = build_file_plans(&req).unwrap();
        assert_eq!(plans.len(), 2, "ddl + data for one table");
        let token = CancelToken::new();
        let source =
            CancellingSource::new(two_cols(), one_row(1, "a"), one_row(2, "b"), token.clone());
        let dest = temp_file("out.zip");
        let err = crate::export::run_zip_export(&NoopEvents, &source, &req, &plans, &dest, &token)
            .await
            .unwrap_err();
        assert!(
            matches!(err, ExportError::Cancelled),
            "unexpected error: {err:?}"
        );
        assert!(
            !dest.exists(),
            "a cancelled export must not leave a partial archive"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }
}

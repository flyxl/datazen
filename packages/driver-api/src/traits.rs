//! Core driver traits.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::query_stream::{emit_multi_query_as_stream, QueryStreamCallback};
use crate::schema_migration::{MigrationCapabilities, MigrationRenderer, TypeNormalizer};
use crate::sql_target::SqlTarget;
use crate::types::*;
use crate::{
    execute_command_definition, query_command_definition, schema_catalog_command_definitions,
    try_execute_schema_catalog_command, CommandResult, DriverCommandDefinition,
};

#[async_trait]
pub trait DatabaseDriver: Send + Sync {
    fn driver_type(&self) -> DatabaseType;

    fn driver_category(&self) -> DriverCategory {
        DriverCategory::Sql
    }

    /// Sync/transfer pairing category. Defaults from [`Self::driver_category`];
    /// proxy/exploration drivers (e.g. kiwi, superset) should override to
    /// [`SyncCategory::Other`].
    fn sync_category(&self) -> SyncCategory {
        match self.driver_category() {
            DriverCategory::Sql => SyncCategory::Sql,
            DriverCategory::KeyValue => SyncCategory::Kv,
            DriverCategory::Document => SyncCategory::Document,
        }
    }

    /// Sync dialect family for pairing (same family → Direct path).
    /// Drivers should override when their family differs from [`Self::driver_type`]
    /// (e.g. mariadb → mysql). Reuse wrappers delegate to the inner driver.
    fn sync_family(&self) -> String {
        self.driver_type()
    }

    /// Dialect-specific schema migration renderer. The schema-diff domain
    /// produces database-neutral operations; the driver owns SQL syntax.
    fn migration_renderer(&self) -> Option<Arc<dyn MigrationRenderer>> {
        None
    }

    fn migration_capabilities(&self) -> Option<Arc<dyn MigrationCapabilities>> {
        None
    }

    fn type_normalizer(&self) -> Option<Arc<dyn TypeNormalizer>> {
        None
    }

    fn quote_char(&self) -> char {
        '"'
    }

    fn quote_ident(&self, name: &str) -> String {
        let q = self.quote_char();
        if q == '`' {
            format!("`{}`", name.replace('`', "``"))
        } else {
            format!("\"{}\"", name.replace('"', "\"\""))
        }
    }

    fn skip_count_query(&self) -> bool {
        false
    }

    /// Whether the driver's SQL dialect supports `OFFSET` in pagination.
    /// Drivers that don't (e.g. Presto/Hive via Superset) should return `false`.
    fn supports_offset(&self) -> bool {
        true
    }

    /// Whether the driver supports EXPLAIN query plan analysis.
    fn supports_explain(&self) -> bool {
        true
    }

    /// Whether this driver has a **real schema level** in its namespace hierarchy.
    ///
    /// `true` for engines whose relations are addressed as `schema.table`
    /// (PostgreSQL, SQL Server, DuckDB). `false` for engines where the database
    /// *is* the namespace (MySQL/MariaDB, ClickHouse) or that have no schema
    /// concept at all (SQLite, Redis, MongoDB, …).
    ///
    /// This drives [`validate_schema_target`]: a schema-aware driver **must**
    /// receive `Some(schema)` (including the default `public`/`dbo`), and a
    /// schema-less driver **must** receive `None`. Both mismatches are errors —
    /// there is no implicit fallback, because an implicit schema is exactly the
    /// kind of hidden session state that produced BUG-003.
    fn has_schema_level(&self) -> bool {
        false
    }

    /// Conventional schema this driver resolves unqualified relations in.
    ///
    /// Used only as the last resort when a caller has **no** schema to offer
    /// (a hand-typed MCP table name, a table name scraped out of SQL text) and
    /// the driver declares [`has_schema_level`](Self::has_schema_level). Every
    /// caller that already knows the schema — the object tree, the ER diagram,
    /// schema diff, sync and transfer all carry it on `TableInfo` — must pass
    /// that real value instead. Keeping the fallback here rather than in the
    /// host is what stops `public` / `dbo` from being hardcoded per driver
    /// outside the driver that owns the convention.
    fn default_schema(&self) -> Option<&'static str> {
        None
    }

    /// Whether multi-statement DDL can be wrapped in a single transaction.
    ///
    /// Override for SQL drivers with known semantics. The default is
    /// [`DdlAtomicity::Unknown`], which keeps legacy behavior for drivers
    /// that have not yet declared their DDL atomicity.
    fn ddl_atomicity(&self) -> DdlAtomicity {
        DdlAtomicity::Unknown
    }

    fn format_sql_literal(&self, value: &Option<Value>) -> String {
        match value {
            None | Some(Value::Null) => "NULL".to_string(),
            Some(Value::Bool(b)) => {
                if *b {
                    "TRUE".to_string()
                } else {
                    "FALSE".to_string()
                }
            }
            Some(Value::Integer(i)) => i.to_string(),
            Some(Value::Float(f)) => f.to_string(),
            Some(Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(Value::Bytes(b)) => {
                format!("'{}'", String::from_utf8_lossy(b).replace('\'', "''"))
            }
            Some(Value::Timestamp(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(Value::Json(j)) => format!("'{}'", j.to_string().replace('\'', "''")),
        }
    }

    fn build_update_sql(
        &self,
        table: &str,
        set_columns: &[(&str, Option<Value>)],
        pk_columns: &[(&str, Option<Value>)],
    ) -> String {
        let set_clauses: Vec<String> = set_columns
            .iter()
            .map(|(col, val)| {
                format!(
                    "{} = {}",
                    self.quote_ident(col),
                    self.format_sql_literal(val)
                )
            })
            .collect();
        let where_clauses: Vec<String> = pk_columns
            .iter()
            .map(|(col, val)| match val {
                None | Some(Value::Null) => format!("{} IS NULL", self.quote_ident(col)),
                Some(v) => format!(
                    "{} = {}",
                    self.quote_ident(col),
                    self.format_sql_literal(&Some(v.clone()))
                ),
            })
            .collect();
        format!(
            "UPDATE {} SET {} WHERE {}",
            self.quote_ident(table),
            set_clauses.join(", "),
            where_clauses.join(" AND ")
        )
    }

    /// Build `DELETE FROM … WHERE pk…` for row-level deletes (mirrors `build_update_sql`).
    fn build_delete_sql(&self, table: &str, pk_columns: &[(&str, Option<Value>)]) -> String {
        let where_clauses: Vec<String> = pk_columns
            .iter()
            .map(|(col, val)| match val {
                None | Some(Value::Null) => format!("{} IS NULL", self.quote_ident(col)),
                Some(v) => format!(
                    "{} = {}",
                    self.quote_ident(col),
                    self.format_sql_literal(&Some(v.clone()))
                ),
            })
            .collect();
        format!(
            "DELETE FROM {} WHERE {}",
            self.quote_ident(table),
            where_clauses.join(" AND ")
        )
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError>;

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError>;

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError>;

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError>;

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError>;

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<TableSchema, DriverError>;

    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        let table_schema = self
            .get_table_schema(handle, table, database, schema)
            .await?;
        let pks = table_schema.effective_primary_keys();
        Ok((table_schema.columns, pks))
    }

    /// Batch-fetch columns for all tables in the given database/schema.
    ///
    /// Returns a map of `table_name → (columns, primary_keys)`. Drivers that
    /// can issue a single SQL query covering every table should override this
    /// to avoid N per-table round-trips. The default returns
    /// [`DriverError::Unsupported`] so callers know to fall back to
    /// per-table [`Self::get_columns`].
    async fn get_all_columns(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
        _schema: Option<&str>,
    ) -> Result<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>, DriverError> {
        Err(DriverError::Unsupported(
            "get_all_columns not supported by this driver".into(),
        ))
    }

    async fn query(&self, handle: &ConnectionHandle, sql: &str)
        -> Result<QueryResult, DriverError>;

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError>;

    /// Stream query results as row batches.
    ///
    /// `limit` is the SQL result cap from the host "limit SELECT results"
    /// setting (`None` = do not rewrite/cap SQL). It is **not** the IPC batch
    /// size. Drivers that can stream from the wire should override this;
    /// the default materializes [`Self::query_multi`] then emits chunks.
    async fn query_stream(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        let result = self.query_multi(handle, sql, limit).await?;
        emit_multi_query_as_stream(result, &on_event);
        Ok(())
    }

    /// Register an opaque execution before the backend target is known.
    ///
    /// The default is a no-op so legacy drivers retain their query behavior;
    /// it does not make them cancellable. Drivers advertising precise cancel
    /// must retain a pending entry here and make a later cancel request win
    /// the race with target acquisition.
    async fn prepare_query_execution(
        &self,
        _handle: &ConnectionHandle,
        _execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        Ok(())
    }

    /// Stream one execution using its opaque identity.
    ///
    /// The compatibility default deliberately delegates only the *stream*
    /// operation to the old API. It never provides a cancellation fallback.
    async fn query_stream_with_execution(
        &self,
        handle: &ConnectionHandle,
        _execution_id: &QueryExecutionId,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        self.query_stream(handle, sql, limit, on_event).await
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<QueryResult, DriverError>;

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError>;

    /// Apply this driver's own target rewrite to `sql`.
    ///
    /// Drivers that inline the target into relation names (MySQL family:
    /// `` `db`.t ``; PG family: `"schema"."t"`) implement
    /// [`Self::qualify_sql_target`] and get this for free. A driver that keeps
    /// per-database resources instead resolves them in the `*_at` methods
    /// below, which override these defaults.
    fn qualified_sql(&self, sql: &str, target: SqlTarget<'_>) -> String {
        if !target.is_present() {
            return sql.to_string();
        }
        self.qualify_sql_target(sql, target.database, target.schema)
            .unwrap_or_else(|| sql.to_string())
    }

    /// Run `sql` against an explicit target.
    ///
    /// [`Self::query`] cannot express a target, which is why the database
    /// dimension used to live in session state. A driver whose connection is
    /// scoped to one database (PostgreSQL) would otherwise serve every read
    /// from the connection's own database; a driver that inlines the database
    /// into relation names (MySQL) would fail with "no database selected" on an
    /// unqualified statement. Host call sites that know their target must use
    /// these methods.
    ///
    /// The default rewrites the statement through [`Self::qualified_sql`] and
    /// delegates to [`Self::query`], which is correct for drivers whose
    /// connection is not database-scoped.
    async fn query_at(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        target: SqlTarget<'_>,
    ) -> Result<QueryResult, DriverError> {
        let sql = self.qualified_sql(sql, target);
        self.query(handle, &sql).await
    }

    /// [`Self::query_multi`] against an explicit target. See [`Self::query_at`].
    async fn query_multi_at(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        target: SqlTarget<'_>,
    ) -> Result<MultiQueryResult, DriverError> {
        let sql = self.qualified_sql(sql, target);
        self.query_multi(handle, &sql, limit).await
    }

    /// [`Self::query_with_params`] against an explicit target.
    /// See [`Self::query_at`].
    async fn query_with_params_at(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
        target: SqlTarget<'_>,
    ) -> Result<QueryResult, DriverError> {
        let sql = self.qualified_sql(sql, target);
        self.query_with_params(handle, &sql, params).await
    }

    /// [`Self::query_stream`] against an explicit target.
    /// See [`Self::query_at`].
    async fn query_stream_at(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        target: SqlTarget<'_>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        let sql = self.qualified_sql(sql, target);
        self.query_stream(handle, &sql, limit, on_event).await
    }

    /// [`Self::execute`] against an explicit target. See [`Self::query_at`].
    async fn execute_at(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        target: SqlTarget<'_>,
    ) -> Result<u64, DriverError> {
        let sql = self.qualified_sql(sql, target);
        self.execute(handle, &sql).await
    }

    /// Return commands supported by this driver.
    ///
    /// Existing SQL drivers get the standard `query` and `execute` commands.
    /// A driver with additional capabilities can override this method and append
    /// its own command definitions.
    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        let mut defs = vec![query_command_definition(), execute_command_definition()];
        defs.extend(schema_catalog_command_definitions());
        defs
    }

    /// Execute a driver command.
    ///
    /// The default implementation maps the existing SQL APIs to commands so
    /// existing drivers remain source-compatible. Driver plugins can override
    /// this method to implement driver-specific commands without adding another
    /// application-level dispatch path.
    async fn execute_command(
        &self,
        handle: &ConnectionHandle,
        command: &str,
        input: serde_json::Value,
    ) -> Result<CommandResult, DriverError> {
        match execute_standard_sql_command(self, handle, command, input.clone()).await {
            Ok(result) => return Ok(result),
            Err(DriverError::Unsupported(_)) => {}
            Err(err) => return Err(err),
        }
        if let Some(result) =
            try_execute_schema_catalog_command(self, handle, command, input).await?
        {
            return Ok(result);
        }
        Err(DriverError::Unsupported(format!(
            "unsupported driver command: {command}"
        )))
    }

    async fn begin_transaction(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        Err(DriverError::TransactionError(
            "Not supported for this driver type".into(),
        ))
    }

    async fn commit(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        Err(DriverError::TransactionError(
            "Not supported for this driver type".into(),
        ))
    }

    async fn rollback(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        Err(DriverError::TransactionError(
            "Not supported for this driver type".into(),
        ))
    }

    async fn explain(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        Err(DriverError::QueryFailed(
            "Not supported for this driver type".into(),
        ))
    }

    async fn cancel_query(&self, handle: &ConnectionHandle) -> Result<(), DriverError>;

    /// Cancel exactly the registered execution identified by `execution_id`.
    /// The default intentionally does not call legacy `cancel_query`.
    async fn cancel_query_with_execution(
        &self,
        _handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        Err(DriverError::Unsupported(format!(
            "precise query cancellation is not supported for execution {}",
            execution_id.as_str()
        )))
    }

    /// Remove a prepared execution from the driver's registry. Drivers should
    /// make this idempotent so Host cleanup remains safe on every terminal path.
    async fn cleanup_query_execution(
        &self,
        _handle: &ConnectionHandle,
        _execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        Ok(())
    }

    /// True only when this concrete driver overrides the exact execution-handle
    /// protocol. Factory metadata must also advertise the capability.
    fn supports_query_execution_cancel(&self) -> bool {
        false
    }

    /// Fetch server version info using an existing connection handle.
    /// Unlike `test_connection` which creates a temporary pool, this reuses the live connection.
    async fn get_server_info(&self, _handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
        Ok(ServerInfo {
            server_version: String::new(),
            server_type: self.driver_type(),
        })
    }

    /// Close whatever the driver opened for `database` on this handle — the
    /// right-click "close database connection" action.
    ///
    /// Drivers that hold a pool (or any other resource) per browsed database
    /// override this and return whether something was actually open. Drivers
    /// with no per-database resource keep the default `Ok(false)`: there is
    /// nothing to close, which is a normal outcome rather than an error.
    ///
    /// The handle's **own** database cannot be closed this way — use
    /// [`Self::disconnect`] for that.
    async fn close_database(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<bool, DriverError> {
        Ok(false)
    }

    /// Databases this handle currently holds an open resource for.
    ///
    /// The counterpart of [`Self::close_database`]: it lets the UI mark which
    /// database nodes have a live connection, so a user can see what the
    /// right-click "close database connection" action would actually release.
    /// A driver with no per-database resource keeps the default `Ok(vec![])`,
    /// which the UI reads as "nothing to show" rather than "nothing is open".
    ///
    /// Only databases the driver opened *itself* belong here. The handle's own
    /// database is included when the driver keeps a pool for it, because it is
    /// genuinely open even though it cannot be closed individually.
    async fn open_databases(&self, _handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Ok(Vec::new())
    }

    /// F7: dialect-aware SQL target qualification.
    ///
    /// Given optional targeting information (`database`, and for PG-family
    /// engines `schema`), rewrite unqualified table references in `sql` into
    /// dialect-qualified names (AST-level, see [`crate::sql_target`]). This
    /// lets a command land on the caller-selected target with no session
    /// switch and no `USE`.
    ///
    /// Return value:
    /// - `Some(qualified_sql)` — the driver can rewrite; parse failures pass
    ///   the original text back through (the rewrite is best-effort).
    /// - `None` — the driver has no rewrite capability (default). The SQL is
    ///   executed as-is and the database dimension is served by the driver's
    ///   own per-database resources in [`Self::query_at`] and friends.
    ///
    /// Implementations must be pure/stateless and idempotent (re-qualifying
    /// already-qualified SQL is a no-op).
    fn qualify_sql_target(
        &self,
        _sql: &str,
        _database: Option<&str>,
        _schema: Option<&str>,
    ) -> Option<String> {
        None
    }

    /// Return driver-specific prompt overrides for AI features.
    ///
    /// Templates can use `{{variable}}` placeholders. Available variables per
    /// scenario are documented in the main application's `PromptResolver`.
    /// The default implementation returns an empty map (use global defaults).
    fn prompt_overrides(&self) -> HashMap<PromptScenario, PromptTemplate> {
        HashMap::new()
    }

    /// Return dialect-specific notes for the AI prompt system.
    ///
    /// When non-empty, the prompt resolver replaces `{{dialect_notes}}` in
    /// system prompts with these notes, helping the LLM generate dialect-correct
    /// SQL. Drivers should describe key syntax differences (pagination, string
    /// operations, type quirks, etc.).
    fn dialect_notes(&self) -> Option<String> {
        None
    }

    /// Emit `CREATE TABLE` DDL for a single table.
    ///
    /// Default builds DDL from [`Self::get_table_schema`].
    async fn dump_table_ddl(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<String, DriverError> {
        crate::sql_dump::dump_table_ddl_from_schema::<Self>(self, handle, table, database, schema)
            .await
    }

    /// Emit `CREATE VIEW` (or equivalent) for a view / materialized view.
    ///
    /// Default is [`DriverError::NotSupported`]; backup then writes a comment
    /// and still skips `INSERT INTO` for view-like objects.
    async fn dump_view_ddl(
        &self,
        _handle: &ConnectionHandle,
        view: &str,
        _database: &str,
        _schema: Option<&str>,
    ) -> Result<String, DriverError> {
        Err(DriverError::NotSupported(format!(
            "View DDL dump is not supported for {view}"
        )))
    }

    /// Dump stored procedures and functions for the current database.
    ///
    /// Default is an empty string (driver has no routines, or they are skipped).
    async fn dump_routines(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<String, DriverError> {
        Ok(String::new())
    }

    /// Dump triggers for the current database.
    ///
    /// Default is an empty string.
    async fn dump_triggers(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<String, DriverError> {
        Ok(String::new())
    }

    /// Dump an entire database to SQL text.
    ///
    /// Default refuses `create_database` with [`DriverError::NotSupported`] and
    /// otherwise delegates to [`crate::sql_dump::dump_sql_database`].
    async fn dump_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
    ) -> Result<String, DriverError> {
        self.dump_database_with_progress(handle, database, opts, &mut |_| {})
            .await
    }

    /// Same as [`Self::dump_database`] with per-object progress.
    async fn dump_database_with_progress(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<String, DriverError> {
        if opts.create_database {
            return Err(DriverError::NotSupported(
                "Backup option 'create' (CREATE DATABASE) is not supported for this driver".into(),
            ));
        }
        crate::sql_dump::dump_sql_database_with_progress::<Self, _>(
            self,
            handle,
            database,
            opts,
            on_progress,
        )
        .await
    }

    /// When true (SQL drivers), the host streams the dump file into
    /// [`crate::sql_dump::RestoreSession`]. Override to `false` to take over
    /// the whole restore via [`Self::restore_sql_with_progress`].
    fn uses_sql_restore_pipeline(&self) -> bool {
        matches!(self.driver_category(), DriverCategory::Sql)
    }

    /// Statement scanner for the default restore pipeline.
    /// Override to disable `DELIMITER`, change quote rules, or swap splitters.
    fn new_sql_scanner(&self) -> crate::sql_split::SqlStatementScanner {
        crate::sql_split::SqlStatementScanner::new()
    }

    /// Split a complete SQL buffer. Default uses [`Self::new_sql_scanner`].
    fn split_restore_sql(&self, sql: &str) -> Vec<String> {
        let mut scanner = self.new_sql_scanner();
        let mut out = scanner.push(sql);
        out.extend(scanner.finish());
        out
    }

    /// After a restore statement fails: clear an aborted PG transaction,
    /// create missing `nextval` sequences, and when `overwrite` is set, drop an
    /// existing relation and ask the pipeline to retry `CREATE`.
    async fn recover_restore_statement(
        &self,
        handle: &ConnectionHandle,
        stmt: &str,
        error: &DriverError,
        overwrite: bool,
    ) -> Result<bool, DriverError> {
        crate::sql_dump::recover_restore_statement_default(self, handle, stmt, error, overwrite)
            .await
    }

    /// Restore a SQL dump by executing statements against the live connection.
    ///
    /// Default uses [`crate::sql_dump::RestoreSession`] (streaming-capable) and
    /// honors [`BackupRestoreOptions::single_transaction`] when the **user**
    /// requested it. Dump-header `-- Options: single-transaction` is dump-time
    /// snapshot only and is not treated as restore atomicity.
    /// Override this method to replace the entire restore pipeline.
    async fn restore_sql(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        opts: Option<&BackupRestoreOptions>,
    ) -> Result<(), DriverError> {
        self.restore_sql_with_progress(handle, sql, opts, &mut |_| {})
            .await
    }

    /// Same as [`Self::restore_sql`] with per-statement progress.
    async fn restore_sql_with_progress(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        opts: Option<&BackupRestoreOptions>,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<(), DriverError> {
        crate::sql_dump::restore_sql_statements_with_progress::<Self, _>(
            self,
            handle,
            sql,
            opts,
            on_progress,
        )
        .await
    }

    async fn structure_capabilities(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<StructureCapabilities, DriverError> {
        Ok(StructureCapabilities {
            dialect_id: self.driver_type(),
            ..Default::default()
        })
    }

    async fn plan_structure_changes(
        &self,
        _handle: &ConnectionHandle,
        _request: &StructureChangeRequest,
    ) -> Result<StructureChangePlan, DriverError> {
        Err(DriverError::Unsupported(
            "table structure planning is not supported by this driver".into(),
        ))
    }
}

/// Default `query` / `execute` command dispatch shared by SQL drivers.
///
/// F7: the input object may carry optional targeting fields `database` /
/// `schema` (injected by the host from the IPC envelope). When present, the
/// SQL is rewritten through [`DatabaseDriver::qualify_sql_target`] before
/// execution; drivers without the capability execute as-is (logged), keeping
/// the host session pin as fallback.
pub async fn execute_standard_sql_command<D: DatabaseDriver + ?Sized>(
    driver: &D,
    handle: &ConnectionHandle,
    command: &str,
    input: serde_json::Value,
) -> Result<CommandResult, DriverError> {
    match command {
        "query" => {
            let (sql, target) = sql_input_with_target(&input, "query")?;
            let limit = input
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|v| v.min(u32::MAX as u64) as u32);
            let result = driver.query_multi_at(handle, &sql, limit, target).await?;
            let data = serde_json::to_value(result).map_err(|e| {
                DriverError::QueryFailed(format!("failed to serialize query result: {e}"))
            })?;
            Ok(CommandResult::new(data))
        }
        "execute" => {
            let (sql, target) = sql_input_with_target(&input, "execute")?;
            let rows_affected = driver.execute_at(handle, &sql, target).await?;
            Ok(CommandResult::new(serde_json::json!({
                "rowsAffected": rows_affected
            })))
        }
        other => Err(DriverError::Unsupported(format!(
            "unsupported driver command: {other}"
        ))),
    }
}

/// How precisely a call site must pin the schema dimension.
///
/// Listing operations legitimately span every schema in a database (the
/// connection tree groups tables by their own schema), while single-table
/// resolution must be exact — an ambiguous table identity is what allowed
/// same-named tables from different schemas to be merged into one column set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaScope {
    /// Listing call: `None` means "every schema in the database".
    AnySchema,
    /// Resolution call: a schema-aware driver must receive `Some(schema)`.
    ExactSchema,
}

/// Validate the `(database, schema)` target pair against a driver's capability.
///
/// **This is the single source of truth for the schema-dimension rule**, and it
/// is deliberately a free function: the trait method is the only chokepoint
/// shared by *every* caller (GUI IPC, MCP server, Workflow, `sql_dump`, reuse
/// wrappers), so drivers must call it at the top of their
/// `get_tables`/`get_table_schema`/`get_columns`/`get_all_columns`
/// implementations. Host call sites may call it too, purely to fail earlier
/// with a friendlier message — that is an optimization, not the guarantee.
///
/// | `has_schema_level()` | `schema` | [`SchemaScope::AnySchema`] | [`SchemaScope::ExactSchema`] |
/// | --- | --- | --- | --- |
/// | `true` | `Some(s)` non-blank | `Ok` | `Ok` |
/// | `true` | `None` / blank | `Ok` (all schemas) | `Err(InvalidConfig)` |
/// | `false` | `None` / blank | `Ok` | `Ok` |
/// | `false` | `Some(s)` | `Err(InvalidConfig)` | `Err(InvalidConfig)` |
///
/// `database` is intentionally not validated here: engines differ on whether a
/// blank database means "the session's current catalog" (PostgreSQL's listing
/// path) or is simply invalid.
pub fn validate_schema_target<D: DatabaseDriver + ?Sized>(
    driver: &D,
    database: &str,
    schema: Option<&str>,
    scope: SchemaScope,
) -> Result<(), DriverError> {
    let schema = schema.map(str::trim).filter(|s| !s.is_empty());
    let driver_type = driver.driver_type();
    match (driver.has_schema_level(), schema, scope) {
        (false, Some(schema), _) => Err(DriverError::InvalidConfig(format!(
            "driver '{driver_type}' has no schema level: schema '{schema}' is not allowed \
             (database '{database}')"
        ))),
        (true, None, SchemaScope::ExactSchema) => Err(DriverError::InvalidConfig(format!(
            "driver '{driver_type}' has a schema level: an explicit schema is required \
             (database '{database}')"
        ))),
        _ => Ok(()),
    }
}

/// Extract the `sql` input of a standard SQL command together with the target
/// the host injected into the envelope.
///
/// The target is returned rather than applied here: qualification is only half
/// the story, because a driver that keeps per-database resources must also
/// route the statement to the right one. Callers pass this to
/// [`DatabaseDriver::query_multi_at`] / [`DatabaseDriver::execute_at`], which
/// apply the rewrite *and* the routing.
fn sql_input_with_target<'a>(
    input: &'a serde_json::Value,
    command: &str,
) -> Result<(String, SqlTarget<'a>), DriverError> {
    let sql = input
        .get("sql")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            DriverError::InvalidConfig(format!("command '{command}' requires string input 'sql'"))
        })?
        .to_string();

    let database = optional_target_field(input, "database");
    let schema = optional_target_field(input, "schema");
    Ok((sql, SqlTarget { database, schema }))
}

fn optional_target_field<'a>(input: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    input
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod structure_defaults_tests {
    use super::*;
    use crate::ReuseDriver;
    use std::sync::Arc;

    struct StubDriver;

    #[async_trait]
    impl DatabaseDriver for StubDriver {
        fn driver_type(&self) -> DatabaseType {
            "stub".to_string()
        }

        async fn connect(
            &self,
            _config: &ConnectionConfig,
        ) -> Result<ConnectionHandle, DriverError> {
            Ok(ConnectionHandle {
                id: "conn".into(),
                pool_id: "pool".into(),
            })
        }

        async fn test_connection(
            &self,
            _config: &ConnectionConfig,
        ) -> Result<ServerInfo, DriverError> {
            Ok(ServerInfo {
                server_version: String::new(),
                server_type: self.driver_type(),
            })
        }

        async fn disconnect(&self, _handle: ConnectionHandle) -> Result<(), DriverError> {
            Ok(())
        }

        async fn get_databases(
            &self,
            _handle: &ConnectionHandle,
        ) -> Result<Vec<String>, DriverError> {
            Ok(vec![])
        }

        async fn get_tables(
            &self,
            _handle: &ConnectionHandle,
            _database: &str,
            _schema: Option<&str>,
        ) -> Result<Vec<TableInfo>, DriverError> {
            Ok(vec![])
        }

        async fn get_table_schema(
            &self,
            _handle: &ConnectionHandle,
            _table: &str,
            _database: &str,
            _schema: Option<&str>,
        ) -> Result<TableSchema, DriverError> {
            Ok(TableSchema {
                table_name: String::new(),
                columns: vec![],
                primary_keys: vec![],
                indexes: vec![],
                foreign_keys: vec![],
            })
        }

        async fn query(
            &self,
            _handle: &ConnectionHandle,
            _sql: &str,
        ) -> Result<QueryResult, DriverError> {
            Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                rows_affected: None,
                execution_time_ms: 0,
            })
        }

        async fn query_multi(
            &self,
            _handle: &ConnectionHandle,
            _sql: &str,
            _limit: Option<u32>,
        ) -> Result<MultiQueryResult, DriverError> {
            Ok(MultiQueryResult {
                results: vec![],
                total_time_ms: 0,
            })
        }

        async fn query_with_params(
            &self,
            _handle: &ConnectionHandle,
            _sql: &str,
            _params: &[Value],
        ) -> Result<QueryResult, DriverError> {
            Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                rows_affected: None,
                execution_time_ms: 0,
            })
        }

        async fn execute(
            &self,
            _handle: &ConnectionHandle,
            _sql: &str,
        ) -> Result<u64, DriverError> {
            Ok(0)
        }

        async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
            Ok(())
        }
    }

    fn sample_request() -> StructureChangeRequest {
        StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: Some("public".into()),
            table: "users".into(),
            original_columns: vec![],
            current_columns: vec![],
            original_indexes: vec![],
            current_indexes: vec![],
        }
    }

    #[tokio::test]
    async fn default_ddl_atomicity_is_unknown() {
        let driver = StubDriver;
        assert_eq!(driver.ddl_atomicity(), DdlAtomicity::Unknown);
    }

    #[tokio::test]
    async fn default_structure_capabilities_are_disabled() {
        let driver = StubDriver;
        let handle = ConnectionHandle {
            id: "conn".into(),
            pool_id: "pool".into(),
        };

        let caps = driver.structure_capabilities(&handle).await.unwrap();
        assert_eq!(caps.dialect_id, "stub");
        assert_eq!(caps.alter_strategy, AlterStrategy::None);
        assert!(!caps.create_table);
        assert!(!caps.add_column);
        assert!(!caps.drop_column);
        assert!(!caps.rename_column);
        assert!(!caps.alter_type);
        assert!(!caps.alter_nullability);
        assert!(!caps.alter_default);
        assert!(!caps.alter_primary_key);
        assert!(!caps.reorder_column);
        assert!(!caps.comment);
        assert!(!caps.create_index);
        assert!(!caps.drop_index);
        assert!(!caps.rebuild_index);
        assert!(!caps.index_type);
        assert!(!caps.index_include);
        assert!(!caps.index_filter);
        assert!(!caps.index_comment);
        assert!(caps.index_methods.is_empty());
    }

    #[tokio::test]
    async fn default_plan_structure_changes_is_unsupported() {
        let driver = StubDriver;
        let handle = ConnectionHandle {
            id: "conn".into(),
            pool_id: "pool".into(),
        };
        let request = sample_request();

        let err = driver
            .plan_structure_changes(&handle, &request)
            .await
            .unwrap_err();
        assert!(
            matches!(err, DriverError::Unsupported(msg) if msg == "table structure planning is not supported by this driver")
        );
    }

    #[test]
    fn stub_driver_sync_defaults() {
        let driver = StubDriver;
        assert_eq!(driver.sync_category(), SyncCategory::Sql);
        assert_eq!(driver.sync_family(), "stub");
    }

    #[tokio::test]
    async fn reuse_driver_forwards_sync_taxonomy() {
        let inner: Arc<dyn DatabaseDriver> = Arc::new(StubDriver);
        let driver = ReuseDriver::new(inner, "reuse-stub");
        assert_eq!(driver.sync_category(), SyncCategory::Sql);
        assert_eq!(driver.sync_family(), "stub");
    }

    #[tokio::test]
    async fn reuse_driver_forwards_structure_methods() {
        let inner: Arc<dyn DatabaseDriver> = Arc::new(StubDriver);
        let driver = ReuseDriver::new(inner, "reuse-stub");
        let handle = ConnectionHandle {
            id: "conn".into(),
            pool_id: "pool".into(),
        };

        let caps = driver.structure_capabilities(&handle).await.unwrap();
        assert_eq!(caps.dialect_id, "stub");
        assert_eq!(driver.driver_type(), "reuse-stub");

        let err = driver
            .plan_structure_changes(&handle, &sample_request())
            .await
            .unwrap_err();
        assert!(matches!(err, DriverError::Unsupported(_)));
    }

    #[tokio::test]
    async fn get_columns_uses_effective_primary_keys_when_primary_keys_empty() {
        struct DriverWithColumnPkOnly;

        #[async_trait]
        impl DatabaseDriver for DriverWithColumnPkOnly {
            fn driver_type(&self) -> DatabaseType {
                "dummy".into()
            }

            async fn connect(&self, _: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
                unreachable!()
            }

            async fn test_connection(
                &self,
                _: &ConnectionConfig,
            ) -> Result<ServerInfo, DriverError> {
                unreachable!()
            }

            async fn disconnect(&self, _: ConnectionHandle) -> Result<(), DriverError> {
                Ok(())
            }

            async fn get_databases(
                &self,
                _: &ConnectionHandle,
            ) -> Result<Vec<String>, DriverError> {
                Ok(vec![])
            }

            async fn get_tables(
                &self,
                _: &ConnectionHandle,
                _: &str,
                _: Option<&str>,
            ) -> Result<Vec<TableInfo>, DriverError> {
                Ok(vec![])
            }

            async fn get_table_schema(
                &self,
                _: &ConnectionHandle,
                _: &str,
                _: &str,
                _: Option<&str>,
            ) -> Result<TableSchema, DriverError> {
                Ok(TableSchema {
                    table_name: "users".into(),
                    columns: vec![ColumnSchema {
                        name: "id".into(),
                        data_type: "int".into(),
                        nullable: false,
                        default_value: None,
                        comment: None,
                        is_primary_key: true,
                        is_auto_increment: true,
                    }],
                    primary_keys: vec![], // intentionally empty to test fallback
                    indexes: vec![],
                    foreign_keys: vec![],
                })
            }

            async fn query(
                &self,
                _: &ConnectionHandle,
                _: &str,
            ) -> Result<QueryResult, DriverError> {
                unreachable!()
            }

            async fn query_multi(
                &self,
                _: &ConnectionHandle,
                _: &str,
                _: Option<u32>,
            ) -> Result<MultiQueryResult, DriverError> {
                unreachable!()
            }

            async fn query_with_params(
                &self,
                _: &ConnectionHandle,
                _: &str,
                _: &[Value],
            ) -> Result<QueryResult, DriverError> {
                unreachable!()
            }

            async fn execute(&self, _: &ConnectionHandle, _: &str) -> Result<u64, DriverError> {
                unreachable!()
            }

            async fn cancel_query(&self, _: &ConnectionHandle) -> Result<(), DriverError> {
                Ok(())
            }
        }

        let driver = DriverWithColumnPkOnly;
        let handle = ConnectionHandle {
            id: "c".into(),
            pool_id: "p".into(),
        };
        let (_cols, pks) = driver
            .get_columns(&handle, "users", "app", None)
            .await
            .unwrap();
        assert_eq!(pks, vec!["id"]);
    }
    /// `has_schema_level` defaults to false, and the validator enforces the
    /// capability/schema pairing in both directions.
    mod validate_schema_target_tests {
        use super::*;

        struct SchemaLess;
        struct SchemaAware;

        macro_rules! stub_driver {
            ($name:ident, $schema_level:expr) => {
                #[async_trait]
                impl DatabaseDriver for $name {
                    fn driver_type(&self) -> DatabaseType {
                        stringify!($name).to_lowercase()
                    }
                    fn has_schema_level(&self) -> bool {
                        $schema_level
                    }
                    async fn test_connection(
                        &self,
                        _: &ConnectionConfig,
                    ) -> Result<ServerInfo, DriverError> {
                        unreachable!()
                    }
                    async fn connect(
                        &self,
                        _: &ConnectionConfig,
                    ) -> Result<ConnectionHandle, DriverError> {
                        unreachable!()
                    }
                    async fn disconnect(&self, _: ConnectionHandle) -> Result<(), DriverError> {
                        unreachable!()
                    }
                    async fn get_databases(
                        &self,
                        _: &ConnectionHandle,
                    ) -> Result<Vec<String>, DriverError> {
                        unreachable!()
                    }
                    async fn get_tables(
                        &self,
                        _: &ConnectionHandle,
                        _: &str,
                        _: Option<&str>,
                    ) -> Result<Vec<TableInfo>, DriverError> {
                        unreachable!()
                    }
                    async fn get_table_schema(
                        &self,
                        _: &ConnectionHandle,
                        _: &str,
                        _: &str,
                        _: Option<&str>,
                    ) -> Result<TableSchema, DriverError> {
                        unreachable!()
                    }
                    async fn query(
                        &self,
                        _: &ConnectionHandle,
                        _: &str,
                    ) -> Result<QueryResult, DriverError> {
                        unreachable!()
                    }
                    async fn query_multi(
                        &self,
                        _: &ConnectionHandle,
                        _: &str,
                        _: Option<u32>,
                    ) -> Result<MultiQueryResult, DriverError> {
                        unreachable!()
                    }
                    async fn query_with_params(
                        &self,
                        _: &ConnectionHandle,
                        _: &str,
                        _: &[Value],
                    ) -> Result<QueryResult, DriverError> {
                        unreachable!()
                    }
                    async fn execute(
                        &self,
                        _: &ConnectionHandle,
                        _: &str,
                    ) -> Result<u64, DriverError> {
                        unreachable!()
                    }
                    async fn cancel_query(&self, _: &ConnectionHandle) -> Result<(), DriverError> {
                        unreachable!()
                    }
                }
            };
        }

        stub_driver!(SchemaLess, false);
        stub_driver!(SchemaAware, true);

        #[test]
        fn schema_less_driver_accepts_only_none() {
            assert!(!SchemaLess.has_schema_level());
            assert!(
                validate_schema_target(&SchemaLess, "app", None, SchemaScope::AnySchema).is_ok()
            );
            assert!(
                validate_schema_target(&SchemaLess, "app", None, SchemaScope::ExactSchema).is_ok()
            );
            let err =
                validate_schema_target(&SchemaLess, "app", Some("public"), SchemaScope::AnySchema)
                    .expect_err("schema-less driver must reject a schema");
            assert!(err.to_string().contains("no schema level"), "{err}");
            // Blank is treated as absent, not as a schema literally named "".
            assert!(
                validate_schema_target(&SchemaLess, "app", Some("  "), SchemaScope::AnySchema)
                    .is_ok()
            );
        }

        #[test]
        fn schema_aware_driver_requires_schema_only_when_resolving() {
            assert!(SchemaAware.has_schema_level());
            // Listing spans every schema, so None is legitimate here.
            assert!(
                validate_schema_target(&SchemaAware, "app", None, SchemaScope::AnySchema).is_ok()
            );
            assert!(validate_schema_target(
                &SchemaAware,
                "app",
                Some("public"),
                SchemaScope::AnySchema
            )
            .is_ok());
            assert!(validate_schema_target(
                &SchemaAware,
                "app",
                Some("public"),
                SchemaScope::ExactSchema
            )
            .is_ok());
            let err = validate_schema_target(&SchemaAware, "app", None, SchemaScope::ExactSchema)
                .expect_err("schema-aware driver must require a schema when resolving one table");
            assert!(
                err.to_string().contains("explicit schema is required"),
                "{err}"
            );
            assert!(validate_schema_target(
                &SchemaAware,
                "app",
                Some(" "),
                SchemaScope::ExactSchema
            )
            .is_err());
        }
    }
}

#[async_trait]
pub trait KeyValueDriver: Send + Sync {
    fn driver_type(&self) -> DatabaseType;

    async fn scan_keys_with_info(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
        pattern: &str,
        cursor: u64,
        count: u32,
    ) -> Result<(u64, Vec<KeyEntry>, u64), DriverError>;

    async fn get_key_detail(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
        key: &str,
    ) -> Result<KeyDetail, DriverError>;
}

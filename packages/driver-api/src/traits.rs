//! Core driver traits.
//!
//! [`DatabaseDriver`] is the contract every database driver implements, kept as
//! one trait on purpose: it is the interface driver authors read, and splitting it
//! across files would make every signature unresolvable.
//!
//! Everything that is not the contract is split under `traits/` — SQL text, the
//! streaming defaults, command dispatch, the schema rule, the key/value contract
//! and the structural tests — and re-exported, so their public paths hold.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::query_stream::{emit_multi_query_as_stream, QueryStreamCallback};
use crate::schema_migration::{MigrationCapabilities, MigrationRenderer, TypeNormalizer};
use crate::sql_target::SqlTarget;
use crate::types::*;
use crate::{
    execute_command_definition, query_command_definition, schema_catalog_command_definitions,
    CommandResult, DriverCommandDefinition,
};

mod command_api;
mod key_value;
mod schema_target;
mod sql_text;
mod streaming;

pub use command_api::execute_standard_sql_command;
pub use key_value::KeyValueDriver;
pub use schema_target::{validate_schema_target, SchemaScope};

// The braces below are load-bearing — do not collapse this back to `mod structure_defaults_tests;`.
#[cfg(test)]
mod structure_defaults_tests {
    mod cases;
}

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
        sql_text::quote_ident(self, name)
    }

    fn skip_count_query(&self) -> bool {
        false
    }

    /// Whether the driver's SQL dialect supports `OFFSET` in pagination.
    /// Drivers that don't (e.g. Presto/Hive via Superset) should return `false`.
    fn supports_offset(&self) -> bool {
        true
    }

    /// Pagination syntax this dialect accepts for a `SELECT` returning at most
    /// `limit` rows starting at `offset`.
    ///
    /// The host builds every paged read through this method, so a dialect that
    /// has no `LIMIT` (SQL Server) or that needs an `ORDER BY` before `OFFSET`
    /// overrides it instead of the host special-casing the driver by name.
    /// The default keeps the historical `LIMIT …` / `LIMIT … OFFSET …` shape for
    /// dialects that opt out of `OFFSET` via [`Self::supports_offset`].
    fn pagination_syntax(&self, limit: u64, offset: u64) -> PaginationSyntax {
        sql_text::pagination_syntax(self, limit, offset)
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

    /// Whether one connection can address more than one `database`.
    ///
    /// Engines that resolve a relation against whatever database the session
    /// landed on (PostgreSQL silently defaults to `postgres`) need the target
    /// stated explicitly, because a missing value cannot be told apart from a
    /// deliberate one. Drivers with a single fixed database return `false`.
    fn has_multi_database(&self) -> bool {
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
        sql_text::format_sql_literal(value)
    }

    fn build_update_sql(
        &self,
        table: &str,
        set_columns: &[(&str, Option<Value>)],
        pk_columns: &[(&str, Option<Value>)],
    ) -> String {
        sql_text::build_update_sql(self, table, set_columns, pk_columns)
    }

    /// Build `DELETE FROM … WHERE pk…` for row-level deletes (mirrors `build_update_sql`).
    fn build_delete_sql(&self, table: &str, pk_columns: &[(&str, Option<Value>)]) -> String {
        sql_text::build_delete_sql(self, table, pk_columns)
    }

    /// The host this driver dials when the connection config leaves `host` unset.
    ///
    /// `connect` resolves that default internally, so a config that omits
    /// `host` and one that spells the same default out reach the *same*
    /// server. Anything that compares two endpoints — the transfer Job's
    /// self-overwrite guard, for instance — has to see them as one endpoint,
    /// and it can only do that by asking the driver, because the default is
    /// driver knowledge and the host must not hard-code a per-driver table.
    ///
    /// The implementation must return the very value `connect` substitutes.
    /// Drivers should back both with one constant so they cannot drift.
    /// Defaults to `None`, which means "this driver applies no implicit host".
    fn default_host(&self) -> Option<&'static str> {
        None
    }

    /// The port this driver dials when the connection config leaves `port`
    /// unset. Same contract as [`Self::default_host`]: return what `connect`
    /// actually substitutes, `None` when there is no implicit port.
    fn default_port(&self) -> Option<u16> {
        None
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError>;

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError>;

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError>;

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError>;

    /// Whether the current database identity can inspect the full foreign-key
    /// dependency catalog across every database on this server. Destructive
    /// planners must fail closed when this cannot be proven. Drivers that do
    /// not have a server-wide namespace or cannot prove complete visibility
    /// keep the default `false`.
    async fn has_complete_foreign_key_catalog_visibility(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<bool, DriverError> {
        Ok(false)
    }

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
        streaming::get_columns(self, handle, table, database, schema).await
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

    /// Stream a parameterized query. Drivers with a wire-level streaming
    /// implementation may override this; the compatibility default binds the
    /// values through `query_with_params` and emits the result in chunks.
    ///
    /// This is intentionally separate from `query_stream`: callers must never
    /// interpolate user-controlled filter values into a streaming SELECT.
    async fn query_stream_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        streaming::query_stream_with_params(self, handle, sql, params, limit, on_event).await
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

    /// [`Self::query_stream_with_execution`] against an explicit target.
    /// See [`Self::query_at`].
    ///
    /// The compatibility default drops `target` entirely. Drivers that route
    /// statements to a per-database pool must override this, otherwise a
    /// database chosen in the query panel is silently discarded and the
    /// statement runs on the session's default pool.
    async fn query_stream_with_execution_at(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
        sql: &str,
        limit: Option<u32>,
        target: SqlTarget<'_>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        let _ = target;
        self.query_stream_with_execution(handle, execution_id, sql, limit, on_event)
            .await
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<QueryResult, DriverError>;

    /// Render a parameter for this dialect. Unsupported drivers must fail before writes.
    /// `data_type` comes from the inspected target column metadata.
    fn parameter_placeholder(
        &self,
        _index: usize,
        _data_type: Option<&str>,
    ) -> Result<String, DriverError> {
        Err(DriverError::Unsupported(
            "parameterized migration writes are not supported".into(),
        ))
    }

    /// Maximum number of bound parameters accepted by one statement.
    /// Drivers should report the server's actual statement limit so bulk
    /// writers can split batches before execution. The default is a
    /// conservative portable ceiling for engines without a lower limit.
    fn max_bound_parameters(&self) -> usize {
        60_000
    }

    /// Execute bound DML and report actual affected rows; never serialize values as SQL.
    async fn execute_with_params(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
        _params: &[Value],
    ) -> Result<u64, DriverError> {
        Err(DriverError::Unsupported(
            "parameterized migration writes are not supported".into(),
        ))
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError>;

    /// Apply this driver's own target rewrite to `sql`.
    ///
    /// Drivers that inline the target into relation names (MySQL family:
    /// `` `db`.t ``; PG family: `"schema"."t"`) implement
    /// [`Self::qualify_sql_target`] and get this for free. A driver that keeps
    /// per-database resources instead resolves them in the `*_at` methods
    /// below, which override these defaults.
    fn qualified_sql(&self, sql: &str, target: SqlTarget<'_>) -> String {
        sql_text::qualified_sql(self, sql, target)
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
        command_api::execute_command(self, handle, command, input).await
    }

    async fn begin_transaction(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        Err(DriverError::TransactionError(
            "Not supported for this driver type".into(),
        ))
    }

    /// Synchronize generated identity/serial sequences after a Data Transfer
    /// table batch has inserted explicit values. The call runs inside the
    /// table's active data transaction, after every batch succeeded and before
    /// commit. Drivers whose generated-value state advances automatically may
    /// keep the default no-op implementation.
    async fn advance_transfer_identity_sequences(
        &self,
        _handle: &ConnectionHandle,
        _schema: Option<&str>,
        _table: &str,
        _columns: &[String],
    ) -> Result<(), DriverError> {
        Ok(())
    }

    /// Whether explicit identity inserts require a session-scoped mode on
    /// this connection. Shared migration paths must turn this mode off before
    /// committing, rolling back, switching target tables, or returning the
    /// session to general use.
    fn explicit_identity_insert_requires_session_toggle(&self) -> bool {
        false
    }

    /// Enable or disable explicit identity insertion for one target table.
    /// The caller attempts cleanup on every success, error, and cancellation
    /// path because the state may survive transaction rollback.
    async fn set_identity_insert(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
        _schema: Option<&str>,
        _table: &str,
        _enabled: bool,
    ) -> Result<(), DriverError> {
        Err(DriverError::Unsupported(
            "target does not implement session-scoped identity insertion".into(),
        ))
    }

    /// Drop a connection whose session-scoped identity mode could not be
    /// reset. The default fails closed so the host never reports that an
    /// un-dropped session is safe.
    async fn discard_connection(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Err(DriverError::Unsupported(
            "driver cannot discard this connection after session cleanup failed".into(),
        ))
    }

    /// Return the clause required by this dialect to accept explicit values
    /// for generated identity columns during Data Transfer. This is a
    /// transfer-only DML extension; the default keeps existing drivers and
    /// other migration paths unchanged.
    fn transfer_explicit_identity_insert_clause(&self) -> Option<&'static str> {
        None
    }

    /// Maximum row count for one Data Transfer SQL-file INSERT statement.
    /// Drivers may raise this above one when their SQL-file renderer can
    /// safely process multi-row VALUES statements in a single parse/execute.
    fn transfer_sql_file_insert_batch_size(&self) -> usize {
        1
    }

    /// Transaction delimiters for one atomic Data Transfer SQL-file artifact.
    /// Engines with different transaction batch syntax may override these.
    fn transfer_sql_file_begin_transaction(&self) -> &'static str {
        "BEGIN;"
    }

    fn transfer_sql_file_commit_transaction(&self) -> &'static str {
        "COMMIT;"
    }

    /// Render a Data Transfer SQL-file INSERT when the target schema cannot
    /// be inspected while exporting. The template may contain multiple
    /// VALUES rows. `identity_override_marker` is a unique placeholder
    /// immediately before `VALUES`; the default removes it.
    fn render_transfer_sql_file_insert(
        &self,
        insert_template: &str,
        identity_override_marker: &str,
    ) -> Result<String, DriverError> {
        if identity_override_marker.is_empty()
            || insert_template.matches(identity_override_marker).count() != 1
        {
            return Err(DriverError::Unsupported(
                "Data Transfer SQL-file INSERT has an invalid identity override marker".into(),
            ));
        }
        Ok(insert_template.replacen(&format!("{identity_override_marker} "), "", 1))
    }

    /// Wrap an SQL-file INSERT using its actual mapped target columns and
    /// target relation. Drivers can intersect those columns with the target
    /// identity metadata at script execution time. The target relation is
    /// already safely quoted by the host. Drivers without a session toggle
    /// return the statement unchanged.
    fn render_transfer_sql_file_identity_insert(
        &self,
        insert_sql: &str,
        _target_relation: &str,
        _mapped_target_columns: &[String],
    ) -> Result<String, DriverError> {
        Ok(insert_sql.to_string())
    }

    /// Render Data Transfer sequence synchronization statements for an SQL
    /// file targeting this driver's dialect. Drivers without such a concept
    /// may keep the empty default.
    fn render_transfer_identity_sequence_sync_sql(
        &self,
        _schema: Option<&str>,
        _table: &str,
        _columns: &[String],
    ) -> Result<Vec<String>, DriverError> {
        Ok(Vec::new())
    }

    /// Begin a read-only transaction with a stable snapshot for a multi-page
    /// comparison. Drivers must override this when their normal transaction
    /// isolation does not guarantee that every statement sees the same
    /// committed view. The default fails closed so a caller cannot silently
    /// downgrade a consistency-sensitive comparison to auto-commit reads.
    async fn begin_read_snapshot(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        Err(DriverError::Unsupported(
            "stable read snapshots are not supported by this driver".into(),
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

    /// Validate that target catalog snapshots used to prepare an atomic
    /// schema migration still match before the host executes its first write.
    /// The host invokes this inside the transaction only for reviewed plans
    /// that require atomic execution.
    async fn validate_schema_migration_plan(
        &self,
        _handle: &ConnectionHandle,
        _target: SqlTarget<'_>,
        _expected_target_schemas: &[TableSchema],
    ) -> Result<(), DriverError> {
        Ok(())
    }

    /// Validate driver-specific invariants before committing a reviewed
    /// schema migration. Drivers that need no extra validation may keep the
    /// default. The host invokes this only for plans whose statements require
    /// an atomic transaction.
    async fn validate_schema_migration(
        &self,
        _handle: &ConnectionHandle,
        _target: SqlTarget<'_>,
    ) -> Result<(), DriverError> {
        Ok(())
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

    /// Return a stable identity for the physical server and selected database
    /// when the driver can discover one. Schema Diff uses it to reject aliases
    /// that point both endpoints at the same database. This must not include
    /// credentials or other secrets.
    async fn physical_database_identity(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<Option<String>, DriverError> {
        Ok(None)
    }

    /// Return a catalog identity for one exact schema scope when the driver
    /// can prove it. The value is compared only when both endpoints have the
    /// same physical database identity; drivers must not include credentials
    /// or other secrets.
    async fn schema_scope_identity(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
        _schema: &str,
    ) -> Result<Option<String>, DriverError> {
        Ok(None)
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
        sql_text::split_restore_sql(self, sql)
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

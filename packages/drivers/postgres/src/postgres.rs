//! PostgreSQL driver backed by sqlx PgPool.

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

use async_trait::async_trait;
use datazen_driver_api::*;
use sqlx::pool::PoolConnection;
use sqlx::{PgPool, Postgres};
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::{Mutex, RwLock};

pub struct PostgresDriver {
    pub(crate) pools: RwLock<HashMap<String, PgPool>>,
    /// Template connection config (host/user/pass/timeout) for reconnecting to other databases.
    pub(crate) connect_configs: RwLock<HashMap<String, ConnectionConfig>>,
    /// Weak entries avoid retaining one lifecycle gate per historical pool.
    /// Active creators, closers and waiters each hold the same gate strongly.
    pub(crate) pool_lifecycle: Mutex<HashMap<String, std::sync::Weak<RwLock<()>>>>,
    /// Database the handle's pool is currently connected to, keyed by pool_id.
    pub(crate) active_databases: RwLock<HashMap<String, String>>,
    /// Pools for databases **other than** the handle's active one, keyed by
    /// `(pool_id, database)`. PostgreSQL cannot reference another catalog from a
    /// query, so reading a foreign database means selecting the pool connected
    /// to it — never re-pointing the handle's session (BUG-003).
    pub(crate) database_pools: RwLock<HashMap<(String, String), DatabasePoolEntry>>,
    /// Open transactions: connection held for the lifetime of BEGIN…COMMIT/ROLLBACK, keyed by handle.id.
    pub(crate) fixed_contexts: RwLock<HashMap<String, datazen_driver_api::session::SessionContext>>,
    pub(crate) fixed_sessions: RwLock<HashMap<String, i32>>,
    pub(crate) transactions:
        Mutex<HashMap<String, std::sync::Arc<Mutex<PoolConnection<Postgres>>>>>,
    /// Exact execution target registry.
    pub(crate) query_executions:
        Mutex<HashMap<QueryExecutionId, crate::execution::PgQueryExecution>>,
    /// One independently budgeted control socket per profile/server/auth identity.
    pub(crate) control_pools: RwLock<HashMap<String, crate::control::ControlPoolEntry>>,
}

/// A cached pool for one non-active database, plus when it was last handed out.
/// The timestamp drives both LRU eviction and the inactive-database reap.
pub(crate) struct DatabasePoolEntry {
    pub(crate) pool: PgPool,
    pub(crate) last_used: Instant,
}

impl PostgresDriver {
    pub fn new() -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
            connect_configs: RwLock::new(HashMap::new()),
            pool_lifecycle: Mutex::new(HashMap::new()),
            active_databases: RwLock::new(HashMap::new()),
            database_pools: RwLock::new(HashMap::new()),
            transactions: Mutex::new(HashMap::new()),
            fixed_sessions: RwLock::new(HashMap::new()),
            fixed_contexts: RwLock::new(HashMap::new()),
            query_executions: Mutex::new(HashMap::new()),
            control_pools: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl DatabaseDriver for PostgresDriver {
    fn supports_bound_writes(&self) -> bool {
        true
    }

    fn sql_literal_dialect(&self) -> Option<SqlLiteralDialect> {
        Some(SqlLiteralDialect::Postgres)
    }

    fn default_host(&self) -> Option<&'static str> {
        Some(crate::connection::DEFAULT_HOST)
    }

    fn default_port(&self) -> Option<u16> {
        Some(crate::connection::DEFAULT_PORT)
    }

    fn migration_renderer(
        &self,
    ) -> Option<std::sync::Arc<dyn datazen_driver_api::MigrationRenderer>> {
        Some(std::sync::Arc::new(super::PostgresMigrationRenderer))
    }

    /// PostgreSQL's own dialect SQL for the shared schema-object commands.
    fn schema_object_sql(
        &self,
    ) -> Option<std::sync::Arc<dyn datazen_driver_api::schema_object_sql::SchemaObjectSql>> {
        Some(std::sync::Arc::new(super::PostgresSchemaObjectSql))
    }

    fn migration_capabilities(
        &self,
    ) -> Option<std::sync::Arc<dyn datazen_driver_api::MigrationCapabilities>> {
        Some(std::sync::Arc::new(super::PostgresMigrationCapabilities))
    }

    fn type_normalizer(&self) -> Option<std::sync::Arc<dyn datazen_driver_api::TypeNormalizer>> {
        Some(std::sync::Arc::new(super::PostgresTypeNormalizer))
    }

    fn driver_type(&self) -> DatabaseType {
        "postgresql".to_string()
    }

    fn ddl_atomicity(&self) -> DdlAtomicity {
        DdlAtomicity::Transactional
    }

    /// PostgreSQL keeps a row estimate for every relation, so the grid can
    /// show a row count before the exact count has been paid for.
    fn count_strategy(&self) -> CountStrategy {
        CountStrategy::Estimated
    }

    /// The statement that estimates the row count of a table.
    ///
    /// The table name and the schema are the **unquoted** names the caller
    /// asked about; the driver writes them itself, escaped, and it locates
    /// the relation by name rather than by a session-dependent function.
    fn count_estimate_sql(
        &self,
        _database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Option<String> {
        crate::sql::count_estimate_sql(table, schema, &|value: &str| {
            self.format_sql_literal(&Some(Value::String(value.to_string())))
        })
    }

    fn sync_family(&self) -> String {
        "postgresql".into()
    }

    fn supports_keyset_pagination(&self) -> bool {
        true
    }

    /// The seek condition for one page boundary.
    ///
    /// PostgreSQL compares **row values**, so an order that runs in one
    /// direction is a single `(a, b) > (1, 2)` instead of a disjunction of
    /// `OR`ed predicates — one comparison the planner can answer from the
    /// index instead of a filter over a union. A mixed order (`a ASC, b
    /// DESC`) has no row-value form, and is spelled out lexicographically.
    fn keyset_predicate_sql(
        &self,
        order_columns: &[(String, bool)],
        boundary: &[(String, Value)],
    ) -> Option<String> {
        datazen_driver_api::keyset::row_value_seek_predicate(self, order_columns, boundary)
    }

    fn format_sql_literal(&self, value: &Option<Value>) -> String {
        match value {
            None | Some(Value::Null) => "NULL".into(),
            Some(Value::Bool(true)) => "TRUE".into(),
            Some(Value::Bool(false)) => "FALSE".into(),
            Some(Value::Integer(n)) => n.to_string(),
            Some(Value::Float(n)) => n.to_string(),
            Some(Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(Value::Timestamp(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(Value::Json(j)) => format!("'{}'", j.to_string().replace('\'', "''")),
            Some(Value::Bytes(bytes)) => format!(
                "'\\x{}'",
                bytes
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            ),
        }
    }

    fn dialect_notes(&self) -> Option<String> {
        Some(
            "PostgreSQL uses LIMIT/OFFSET for pagination, ILIKE for case-insensitive \
             string matching, || for string concatenation, ARRAY and JSONB types with \
             dedicated operators (->, ->>, @>, ?), window functions (ROW_NUMBER, LAG, LEAD), \
             CTEs with WITH, UPSERT via ON CONFLICT, and supports transactions for DDL."
                .into(),
        )
    }

    /// F7: qualify unqualified table references with the target schema
    /// (`"schema"."t"`). The database dimension is **not** inlined — PG cannot
    /// reference another database in one statement — so the `*_at` methods
    /// below serve it by routing to that database's pool instead. Parse
    /// failures pass SQL through unchanged. See `sql_target::qualify_sql`.
    fn qualify_sql_target(
        &self,
        sql: &str,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Option<String> {
        Some(crate::sql_target::qualify_sql(sql, database, schema))
    }

    async fn query_at(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        target: SqlTarget<'_>,
    ) -> Result<QueryResult, DriverError> {
        Self::query_at_impl(self, handle, sql, target).await
    }

    async fn query_multi_at(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        target: SqlTarget<'_>,
    ) -> Result<MultiQueryResult, DriverError> {
        Self::query_multi_at_impl(self, handle, sql, limit, target).await
    }

    async fn execute_at(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        target: SqlTarget<'_>,
    ) -> Result<u64, DriverError> {
        Self::execute_at_impl(self, handle, sql, target).await
    }

    async fn query_with_params_at(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
        target: SqlTarget<'_>,
    ) -> Result<QueryResult, DriverError> {
        Self::query_with_params_at_impl(self, handle, sql, params, target).await
    }

    async fn query_stream_at(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        target: SqlTarget<'_>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        Self::query_stream_at_impl(self, handle, sql, limit, target, on_event).await
    }

    async fn connect_fixed_session(
        &self,
        config: &ConnectionConfig,
    ) -> Result<ConnectionHandle, DriverError> {
        self.open_fixed_session(config).await
    }
    async fn observe_fixed_session(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<datazen_driver_api::session::SessionContext, DriverError> {
        self.read_fixed_session(handle).await
    }

    async fn query_to_sink(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
        sql: &str,
        limit: Option<u32>,
        parameters: Option<&[Value]>,
        sink: &dyn datazen_driver_api::resource::ResultSink,
    ) -> Result<Vec<StatementResult>, datazen_driver_api::resource::ResourceError> {
        self.stream_resource_query(handle, execution_id, sql, limit, parameters, sink)
            .await
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        Self::test_connection_impl(self, config).await
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        Self::connect_impl(self, config).await
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        Self::disconnect_impl(self, handle).await
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Self::get_databases_impl(self, handle).await
    }

    /// PostgreSQL addresses relations as `schema.table`, so an explicit schema
    /// is mandatory whenever a single table is resolved.
    fn has_schema_level(&self) -> bool {
        true
    }

    fn has_multi_database(&self) -> bool {
        true
    }

    /// PostgreSQL resolves unqualified names in the first schema of
    /// `search_path`, which defaults to `public`.
    fn default_schema(&self) -> Option<&'static str> {
        Some("public")
    }

    async fn close_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<bool, DriverError> {
        Self::close_database_pool(self, handle, database).await
    }

    async fn open_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Self::open_databases_impl(self, handle).await
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        Self::get_tables_impl(self, handle, database, schema).await
    }

    async fn get_schemas(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Option<Vec<String>>, DriverError> {
        let mut connection = crate::session_connection::for_session(self, handle, database).await?;
        let schemas = sqlx::query_scalar::<_, String>(
            "SELECT nspname::text FROM pg_namespace WHERE nspname NOT IN ('pg_catalog', 'information_schema') AND nspname NOT LIKE 'pg_toast%' AND nspname NOT LIKE 'pg_temp_%' AND has_schema_privilege(oid, 'USAGE') ORDER BY nspname"
        ).fetch_all(&mut *connection).await.map_err(|error| DriverError::QueryFailed(error.to_string()))?;
        Ok(Some(schemas))
    }

    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        Self::get_columns_impl(self, handle, table, database, schema).await
    }

    async fn get_all_columns(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>, DriverError> {
        Self::get_all_columns_impl(self, handle, database, schema).await
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        Self::get_table_schema_impl(self, handle, table, database, schema).await
    }

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        Self::query_impl(self, handle, sql).await
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        Self::query_multi_impl(self, handle, sql, limit).await
    }

    async fn query_stream(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        Self::query_stream_impl(self, handle, sql, limit, on_event).await
    }

    async fn prepare_query_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.prepare_query_execution_impl(handle, execution_id)
            .await
    }

    async fn query_stream_with_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        self.query_stream_with_execution_at(
            handle,
            execution_id,
            sql,
            limit,
            SqlTarget::new(None, None),
            on_event,
        )
        .await
    }

    async fn query_stream_with_execution_at(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
        sql: &str,
        limit: Option<u32>,
        target: SqlTarget<'_>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        // A transaction holds one connection bound to its own database, so
        // refuse a foreign target instead of silently reading another catalog.
        self.ensure_transaction_reaches(handle, target).await?;
        let sql = self.qualified_sql(sql, target);
        let pool = self.resolve_statement_pool(handle, target).await?;
        let statements = sql_dump::split_sql_statements(&sql);
        if statements.is_empty() {
            self.pin_query_execution_pool(handle, execution_id, pool)
                .await?;
            self.finish_query_execution(handle, execution_id).await?;
            on_event(QueryStreamEvent::Done { total_time_ms: 0 });
            return Ok(());
        }
        self.pin_query_execution_pool(handle, execution_id, pool)
            .await?;
        self.stream_registered_execution(handle, execution_id, &statements, limit, &on_event)
            .await
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        Self::query_with_params_impl(self, handle, sql, params).await
    }

    fn parameter_placeholder(
        &self,
        index: usize,
        data_type: Option<&str>,
    ) -> Result<String, DriverError> {
        let normalized = data_type.unwrap_or("").trim().to_ascii_lowercase();
        let base = normalized.split('(').next().unwrap_or("").trim();
        let cast = match base {
            "uuid" => Some("uuid"),
            "numeric" | "decimal" => Some("numeric"),
            "bigint" | "int8" | "bigserial" => Some("bigint"),
            "integer" | "int" | "int4" | "serial" => Some("integer"),
            "smallint" | "int2" | "smallserial" => Some("smallint"),
            "timestamp with time zone" | "timestamptz" => Some("timestamptz"),
            "timestamp without time zone" | "timestamp" => Some("timestamp"),
            "time with time zone" | "timetz" => Some("timetz"),
            "time without time zone" | "time" => Some("time"),
            "date" => Some("date"),
            "json" => Some("json"),
            "jsonb" => Some("jsonb"),
            "interval" => Some("interval"),
            "inet" => Some("inet"),
            "cidr" => Some("cidr"),
            _ => None,
        };
        Ok(match cast {
            Some(cast) => format!("${index}::{cast}"),
            None => format!("${index}"),
        })
    }

    async fn execute_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<u64, DriverError> {
        Self::execute_params_impl(self, handle, sql, params).await
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        Self::execute_impl(self, handle, sql).await
    }

    async fn begin_transaction(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        Self::begin_transaction_impl(self, handle).await
    }

    async fn advance_transfer_identity_sequences(
        &self,
        handle: &ConnectionHandle,
        schema: Option<&str>,
        table: &str,
        columns: &[String],
    ) -> Result<(), DriverError> {
        self.advance_transfer_identity_sequences_impl(handle, schema, table, columns)
            .await
    }

    fn transfer_explicit_identity_insert_clause(&self) -> Option<&'static str> {
        Some("OVERRIDING SYSTEM VALUE")
    }

    fn transfer_sql_file_insert_batch_size(&self) -> usize {
        500
    }

    fn render_transfer_sql_file_insert(
        &self,
        insert_template: &str,
        identity_override_marker: &str,
    ) -> Result<String, DriverError> {
        crate::transfer_identity::render_sql_file_insert(insert_template, identity_override_marker)
    }

    fn render_transfer_identity_sequence_sync_sql(
        &self,
        schema: Option<&str>,
        table: &str,
        columns: &[String],
    ) -> Result<Vec<String>, DriverError> {
        Ok(crate::transfer_identity::render_sync_sql(
            self, schema, table, columns,
        ))
    }

    async fn begin_read_snapshot(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        Self::begin_read_snapshot_impl(self, handle).await
    }

    async fn commit(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        Self::commit_impl(self, tx).await
    }

    async fn rollback(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        Self::rollback_impl(self, tx).await
    }

    async fn explain(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        Self::explain_impl(self, handle, sql).await
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Err(DriverError::Unsupported(
            "legacy session-wide query cancellation is disabled; use an execution handle".into(),
        ))
    }

    async fn cancel_query_with_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.cancel_query_with_execution_impl(handle, execution_id)
            .await
    }

    async fn cleanup_query_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.finish_query_execution(handle, execution_id).await
    }

    fn supports_query_execution_cancel(&self) -> bool {
        true
    }

    async fn get_server_info(&self, handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
        Self::get_server_info_impl(self, handle).await
    }

    async fn physical_database_identity(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Option<String>, DriverError> {
        Self::physical_database_identity_impl(self, handle, database).await
    }

    async fn dump_table_ddl(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<String, DriverError> {
        Self::dump_table_ddl_impl(self, handle, table, database, schema).await
    }

    async fn dump_view_ddl(
        &self,
        handle: &ConnectionHandle,
        view: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<String, DriverError> {
        Self::dump_view_ddl_impl(self, handle, view, database, schema).await
    }

    async fn dump_routines(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<String, DriverError> {
        Self::dump_routines_impl(self, handle, database).await
    }

    async fn dump_triggers(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<String, DriverError> {
        Self::dump_triggers_impl(self, handle, database).await
    }

    async fn dump_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
    ) -> Result<String, DriverError> {
        self.dump_database_with_progress(handle, database, opts, &mut |_| {})
            .await
    }

    fn new_sql_scanner(&self) -> sql_dump::SqlStatementScanner {
        sql_dump::SqlStatementScanner::new().recognize_delimiter_commands(false)
    }

    async fn dump_database_with_progress(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<String, DriverError> {
        Self::dump_database_with_progress_impl(self, handle, database, opts, on_progress).await
    }

    async fn structure_capabilities(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<StructureCapabilities, DriverError> {
        Self::structure_capabilities_impl(self, handle).await
    }

    async fn plan_structure_changes(
        &self,
        handle: &ConnectionHandle,
        request: &StructureChangeRequest,
    ) -> Result<StructureChangePlan, DriverError> {
        Self::plan_structure_changes_impl(self, handle, request).await
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        crate::admin_commands::pg_admin_command_definitions()
    }

    async fn execute_command(
        &self,
        handle: &ConnectionHandle,
        command: &str,
        input: serde_json::Value,
    ) -> Result<CommandResult, DriverError> {
        Self::execute_command_impl(self, handle, command, input).await
    }
}

#[cfg(test)]
mod count_estimate_tests {
    use super::*;

    fn driver() -> PostgresDriver {
        PostgresDriver::new()
    }

    #[test]
    fn postgres_declares_estimated_strategy() {
        assert_eq!(driver().count_strategy(), CountStrategy::Estimated);
    }

    #[test]
    fn postgres_estimate_names_the_requested_relation() {
        let sql = driver()
            .count_estimate_sql("app", Some("public"), "orders")
            .expect("the driver has an estimate");
        assert!(sql.contains("pg_catalog.pg_class"), "{sql}");
        assert!(sql.contains("c.relname = 'orders'"), "{sql}");
        assert!(sql.contains("n.nspname = 'public'"), "{sql}");
    }
}

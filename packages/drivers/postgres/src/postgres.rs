//! PostgreSQL driver backed by sqlx PgPool.

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

use async_trait::async_trait;
use datazen_driver_api::*;
use sqlx::pool::PoolConnection;
use sqlx::{PgPool, Postgres};
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use tokio::sync::{Mutex, RwLock};

pub struct PostgresDriver {
    pub(crate) pools: RwLock<HashMap<String, PgPool>>,
    /// Template connection config (host/user/pass/timeout) for reconnecting to other databases.
    pub(crate) connect_configs: RwLock<HashMap<String, ConnectionConfig>>,
    /// Database the handle's pool is currently connected to, keyed by pool_id.
    pub(crate) active_databases: RwLock<HashMap<String, String>>,
    /// Pools for databases **other than** the handle's active one, keyed by
    /// `(pool_id, database)`. PostgreSQL cannot reference another catalog from a
    /// query, so reading a foreign database means selecting the pool connected
    /// to it — never re-pointing the handle's session (BUG-003).
    pub(crate) database_pools: RwLock<HashMap<(String, String), DatabasePoolEntry>>,
    /// Monotonic clock backing the LRU eviction of [`Self::database_pools`].
    pub(crate) database_pool_clock: AtomicU64,
    /// Open transactions: connection held for the lifetime of BEGIN…COMMIT/ROLLBACK, keyed by handle.id.
    pub(crate) transactions: Mutex<HashMap<String, PoolConnection<Postgres>>>,
    /// Exact execution target registry.
    pub(crate) query_executions:
        Mutex<HashMap<QueryExecutionId, crate::execution::PgQueryExecution>>,
    /// Separate control connections are never used to execute user SQL.
    pub(crate) control_pools: RwLock<HashMap<String, PgPool>>,
}

/// A cached pool for one non-active database, plus its LRU stamp.
pub(crate) struct DatabasePoolEntry {
    pub(crate) pool: PgPool,
    pub(crate) last_used: u64,
}

impl PostgresDriver {
    pub fn new() -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
            connect_configs: RwLock::new(HashMap::new()),
            active_databases: RwLock::new(HashMap::new()),
            database_pools: RwLock::new(HashMap::new()),
            database_pool_clock: AtomicU64::new(0),
            transactions: Mutex::new(HashMap::new()),
            query_executions: Mutex::new(HashMap::new()),
            control_pools: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl DatabaseDriver for PostgresDriver {
    fn migration_renderer(
        &self,
    ) -> Option<std::sync::Arc<dyn datazen_driver_api::MigrationRenderer>> {
        Some(std::sync::Arc::new(super::PostgresMigrationRenderer))
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

    fn sync_family(&self) -> String {
        "postgresql".into()
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
        let statements = sql_dump::split_sql_statements(sql);
        if statements.is_empty() {
            self.finish_query_execution(handle, execution_id).await?;
            on_event(QueryStreamEvent::Done { total_time_ms: 0 });
            return Ok(());
        }
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

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        Self::execute_impl(self, handle, sql).await
    }

    async fn begin_transaction(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        Self::begin_transaction_impl(self, handle).await
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

//! Reuse drivers — thin wrappers that present an existing driver
//! under a different `DatabaseType` id.
//!
//! Used so community builds can keep a single implementation while still
//! advertising alternate type ids (e.g. MariaDB → MySQL driver).

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::query_stream::QueryStreamCallback;
use crate::schema_migration::{MigrationCapabilities, MigrationRenderer, TypeNormalizer};
use crate::types::*;
use crate::{CommandResult, DatabaseDriver, DriverCommandDefinition};

/// Wraps an existing [`DatabaseDriver`] and re-exposes it under a different type id.
pub struct ReuseDriver {
    inner: Arc<dyn DatabaseDriver>,
    advertised_type: DatabaseType,
}

impl ReuseDriver {
    pub fn new(inner: Arc<dyn DatabaseDriver>, advertised_type: impl Into<DatabaseType>) -> Self {
        Self {
            inner,
            advertised_type: advertised_type.into(),
        }
    }

    pub fn inner(&self) -> &Arc<dyn DatabaseDriver> {
        &self.inner
    }
}

#[async_trait]
impl DatabaseDriver for ReuseDriver {
    fn driver_type(&self) -> DatabaseType {
        self.advertised_type.clone()
    }

    fn driver_category(&self) -> DriverCategory {
        self.inner.driver_category()
    }

    fn sync_category(&self) -> SyncCategory {
        self.inner.sync_category()
    }

    fn sync_family(&self) -> String {
        self.inner.sync_family()
    }

    fn migration_renderer(&self) -> Option<Arc<dyn MigrationRenderer>> {
        self.inner.migration_renderer()
    }

    fn migration_capabilities(&self) -> Option<Arc<dyn MigrationCapabilities>> {
        self.inner.migration_capabilities()
    }

    fn type_normalizer(&self) -> Option<Arc<dyn TypeNormalizer>> {
        self.inner.type_normalizer()
    }

    fn quote_char(&self) -> char {
        self.inner.quote_char()
    }

    fn quote_ident(&self, name: &str) -> String {
        self.inner.quote_ident(name)
    }

    fn skip_count_query(&self) -> bool {
        self.inner.skip_count_query()
    }

    fn supports_offset(&self) -> bool {
        self.inner.supports_offset()
    }

    fn supports_explain(&self) -> bool {
        self.inner.supports_explain()
    }

    fn ddl_atomicity(&self) -> DdlAtomicity {
        self.inner.ddl_atomicity()
    }

    fn format_sql_literal(&self, value: &Option<Value>) -> String {
        self.inner.format_sql_literal(value)
    }

    fn build_update_sql(
        &self,
        table: &str,
        set_columns: &[(&str, Option<Value>)],
        pk_columns: &[(&str, Option<Value>)],
    ) -> String {
        self.inner.build_update_sql(table, set_columns, pk_columns)
    }

    fn build_delete_sql(&self, table: &str, pk_columns: &[(&str, Option<Value>)]) -> String {
        self.inner.build_delete_sql(table, pk_columns)
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        self.inner.connect(config).await
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        self.inner.test_connection(config).await
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        self.inner.disconnect(handle).await
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        self.inner.get_databases(handle).await
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        self.inner.get_tables(handle, database).await
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        self.inner.get_table_schema(handle, table).await
    }

    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        self.inner.get_columns(handle, table).await
    }

    async fn get_all_columns(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>, DriverError> {
        self.inner.get_all_columns(handle, database).await
    }

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        self.inner.query(handle, sql).await
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        self.inner.query_multi(handle, sql, limit).await
    }

    async fn query_stream(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        self.inner.query_stream(handle, sql, limit, on_event).await
    }

    async fn prepare_query_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.inner
            .prepare_query_execution(handle, execution_id)
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
        self.inner
            .query_stream_with_execution(handle, execution_id, sql, limit, on_event)
            .await
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        self.inner.query_with_params(handle, sql, params).await
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        self.inner.execute(handle, sql).await
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        self.inner.command_definitions()
    }

    async fn execute_command(
        &self,
        handle: &ConnectionHandle,
        command: &str,
        input: serde_json::Value,
    ) -> Result<CommandResult, DriverError> {
        self.inner.execute_command(handle, command, input).await
    }

    async fn begin_transaction(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        self.inner.begin_transaction(handle).await
    }

    async fn commit(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        self.inner.commit(tx).await
    }

    async fn rollback(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        self.inner.rollback(tx).await
    }

    async fn explain(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        self.inner.explain(handle, sql).await
    }

    async fn cancel_query(&self, handle: &ConnectionHandle) -> Result<(), DriverError> {
        self.inner.cancel_query(handle).await
    }

    async fn cancel_query_with_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.inner
            .cancel_query_with_execution(handle, execution_id)
            .await
    }

    async fn cleanup_query_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.inner
            .cleanup_query_execution(handle, execution_id)
            .await
    }

    fn supports_query_execution_cancel(&self) -> bool {
        self.inner.supports_query_execution_cancel()
    }

    async fn get_server_info(&self, handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
        self.inner.get_server_info(handle).await
    }

    async fn use_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<(), DriverError> {
        self.inner.use_database(handle, database).await
    }

    fn qualify_sql_target(
        &self,
        sql: &str,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Option<String> {
        self.inner.qualify_sql_target(sql, database, schema)
    }

    fn prompt_overrides(&self) -> HashMap<PromptScenario, PromptTemplate> {
        self.inner.prompt_overrides()
    }

    async fn dump_table_ddl(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<String, DriverError> {
        self.inner.dump_table_ddl(handle, table).await
    }

    async fn dump_view_ddl(
        &self,
        handle: &ConnectionHandle,
        view: &str,
    ) -> Result<String, DriverError> {
        self.inner.dump_view_ddl(handle, view).await
    }

    async fn dump_routines(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<String, DriverError> {
        self.inner.dump_routines(handle, database).await
    }

    async fn dump_triggers(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<String, DriverError> {
        self.inner.dump_triggers(handle, database).await
    }

    async fn dump_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
    ) -> Result<String, DriverError> {
        self.inner.dump_database(handle, database, opts).await
    }

    async fn dump_database_with_progress(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<String, DriverError> {
        self.inner
            .dump_database_with_progress(handle, database, opts, on_progress)
            .await
    }

    fn uses_sql_restore_pipeline(&self) -> bool {
        self.inner.uses_sql_restore_pipeline()
    }

    async fn restore_sql_with_progress(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        opts: &BackupRestoreOptions,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<(), DriverError> {
        self.inner
            .restore_sql_with_progress(handle, sql, opts, on_progress)
            .await
    }

    fn structure_capabilities(&self) -> StructureCapabilities {
        self.inner.structure_capabilities()
    }

    async fn plan_structure_changes(
        &self,
        handle: &ConnectionHandle,
        request: &StructureChangeRequest,
    ) -> Result<StructureChangePlan, DriverError> {
        self.inner.plan_structure_changes(handle, request).await
    }
}

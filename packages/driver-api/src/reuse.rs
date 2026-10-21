//! Reuse drivers — thin wrappers that present an existing driver
//! under a different `DatabaseType` id.
//!
//! Used for engines that speak a compatible wire protocol:
//! - MySQL protocol: Doris, StarRocks, ManticoreSearch, OceanBase (MySQL mode)
//! - PostgreSQL wire: QuestDB, Cloudberry
//!
//! **Invariant: every `DatabaseDriver` method is decided here.** A method the
//! wrapper does not mention inherits a default body written for a generic
//! driver, so driver-specific rules (`has_multi_database`, `get_schemas`,
//! `qualify_sql_target`, `default_host`/`default_port`, the migration
//! renderers, target-qualified streaming) silently disappear while the product
//! metadata keeps advertising them. `reuse_forwarding_tests.rs` fails the build
//! when the trait grows and this table does not, so the omission cannot return.
//! Only `driver_type` is deliberately *not* forwarded: the alias id is the
//! reason the wrapper exists. `test_connection` and the precise-cancel methods
//! override rather than forward, because they gate the alias' own contract.

use crate::filters::FilterOperator;
use crate::*;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

pub struct ReuseDriver {
    inner: Arc<dyn DatabaseDriver>,
    db_type: DatabaseType,
    precise_query_execution_cancel: bool,
}

impl ReuseDriver {
    pub fn new(inner: Arc<dyn DatabaseDriver>, db_type: &str) -> Self {
        Self::new_with_precise_cancel(inner, db_type, true)
    }

    /// Build a compatibility wrapper with an explicit precise-cancel
    /// capability. The capability can only be enabled when the wrapped driver
    /// actually implements the execution-handle protocol.
    pub fn new_with_precise_cancel(
        inner: Arc<dyn DatabaseDriver>,
        db_type: &str,
        enabled: bool,
    ) -> Self {
        let precise_query_execution_cancel = enabled && inner.supports_query_execution_cancel();
        Self {
            inner,
            db_type: db_type.to_string(),
            precise_query_execution_cancel,
        }
    }
}

#[async_trait]
impl DatabaseDriver for ReuseDriver {
    fn driver_type(&self) -> DatabaseType {
        self.db_type.clone()
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

    fn transfer_explicit_identity_insert_clause(&self) -> Option<&'static str> {
        self.inner.transfer_explicit_identity_insert_clause()
    }

    fn explicit_identity_insert_requires_session_toggle(&self) -> bool {
        self.inner
            .explicit_identity_insert_requires_session_toggle()
    }

    async fn advance_transfer_identity_sequences(
        &self,
        handle: &ConnectionHandle,
        schema: Option<&str>,
        table: &str,
        columns: &[String],
    ) -> Result<(), DriverError> {
        self.inner
            .advance_transfer_identity_sequences(handle, schema, table, columns)
            .await
    }

    async fn set_identity_insert(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
        table: &str,
        enabled: bool,
    ) -> Result<(), DriverError> {
        self.inner
            .set_identity_insert(handle, database, schema, table, enabled)
            .await
    }

    async fn discard_connection(&self, handle: &ConnectionHandle) -> Result<(), DriverError> {
        self.inner.discard_connection(handle).await
    }

    fn transfer_sql_file_insert_batch_size(&self) -> usize {
        self.inner.transfer_sql_file_insert_batch_size()
    }

    fn transfer_sql_file_begin_transaction(&self) -> &'static str {
        self.inner.transfer_sql_file_begin_transaction()
    }

    fn transfer_sql_file_commit_transaction(&self) -> &'static str {
        self.inner.transfer_sql_file_commit_transaction()
    }

    fn render_transfer_sql_file_insert(
        &self,
        insert_template: &str,
        identity_override_marker: &str,
    ) -> Result<String, DriverError> {
        self.inner
            .render_transfer_sql_file_insert(insert_template, identity_override_marker)
    }

    fn render_transfer_sql_file_identity_insert(
        &self,
        insert_sql: &str,
        target_relation: &str,
        mapped_target_columns: &[String],
    ) -> Result<String, DriverError> {
        self.inner.render_transfer_sql_file_identity_insert(
            insert_sql,
            target_relation,
            mapped_target_columns,
        )
    }

    fn type_normalizer(&self) -> Option<Arc<dyn TypeNormalizer>> {
        self.inner.type_normalizer()
    }

    fn migration_renderer(&self) -> Option<Arc<dyn MigrationRenderer>> {
        self.inner.migration_renderer()
    }

    fn migration_capabilities(&self) -> Option<Arc<dyn MigrationCapabilities>> {
        self.inner.migration_capabilities()
    }

    fn quote_char(&self) -> char {
        self.inner.quote_char()
    }

    fn quote_ident(&self, name: &str) -> String {
        self.inner.quote_ident(name)
    }

    fn default_host(&self) -> Option<&'static str> {
        self.inner.default_host()
    }

    fn default_port(&self) -> Option<u16> {
        self.inner.default_port()
    }

    fn skip_count_query(&self) -> bool {
        self.inner.skip_count_query()
    }

    fn count_strategy(&self) -> CountStrategy {
        self.inner.count_strategy()
    }

    fn count_estimate_sql(
        &self,
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Option<String> {
        self.inner.count_estimate_sql(database, schema, table)
    }

    fn default_order_columns(
        &self,
        columns: &[ColumnSchema],
        primary_keys: &[String],
    ) -> Vec<String> {
        self.inner.default_order_columns(columns, primary_keys)
    }

    fn allow_first_column_fallback_order(&self) -> bool {
        self.inner.allow_first_column_fallback_order()
    }

    fn supports_offset(&self) -> bool {
        self.inner.supports_offset()
    }

    fn pagination_syntax(&self, limit: u64, offset: u64) -> PaginationSyntax {
        self.inner.pagination_syntax(limit, offset)
    }

    fn supports_explain(&self) -> bool {
        self.inner.supports_explain()
    }

    fn supported_filter_operators(&self) -> &'static [FilterOperator] {
        self.inner.supported_filter_operators()
    }

    fn filter_operator_sql(
        &self,
        operator: FilterOperator,
        column_expr: &str,
        literal: &str,
    ) -> Option<String> {
        self.inner
            .filter_operator_sql(operator, column_expr, literal)
    }

    fn like_is_case_insensitive(&self) -> bool {
        self.inner.like_is_case_insensitive()
    }

    fn like_pattern_escape(&self) -> Option<char> {
        self.inner.like_pattern_escape()
    }

    fn like_escape_clause(&self, escape_char: char) -> Option<&'static str> {
        self.inner.like_escape_clause(escape_char)
    }

    fn has_schema_level(&self) -> bool {
        self.inner.has_schema_level()
    }

    fn has_multi_database(&self) -> bool {
        self.inner.has_multi_database()
    }

    fn default_schema(&self) -> Option<&'static str> {
        self.inner.default_schema()
    }

    async fn close_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<bool, DriverError> {
        self.inner.close_database(handle, database).await
    }

    fn ddl_atomicity(&self) -> DdlAtomicity {
        self.inner.ddl_atomicity()
    }

    fn format_sql_literal(&self, value: &Option<Value>) -> String {
        self.inner.format_sql_literal(value)
    }

    fn sql_literal_dialect(&self) -> Option<SqlLiteralDialect> {
        self.inner.sql_literal_dialect()
    }

    fn supports_bound_writes(&self) -> bool {
        self.inner.supports_bound_writes()
    }

    fn try_format_sql_literal(&self, value: &Option<Value>) -> Result<String, DriverError> {
        self.inner.try_format_sql_literal(value)
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

    fn build_update_statement(
        &self,
        table: &str,
        set_columns: &[(&str, Option<Value>)],
        pk_columns: &[(&str, Option<Value>)],
    ) -> Result<BoundSqlStatement, DriverError> {
        self.inner
            .build_update_statement(table, set_columns, pk_columns)
    }

    fn build_delete_statement(
        &self,
        table: &str,
        pk_columns: &[(&str, Option<Value>)],
    ) -> Result<BoundSqlStatement, DriverError> {
        self.inner.build_delete_statement(table, pk_columns)
    }

    fn build_insert_statement(
        &self,
        table: &str,
        values: &[(&str, Option<Value>)],
    ) -> Result<BoundSqlStatement, DriverError> {
        self.inner.build_insert_statement(table, values)
    }

    fn insert_returning_clause(&self, columns: &[&str]) -> Option<String> {
        self.inner.insert_returning_clause(columns)
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        self.inner.connect(config).await
    }

    async fn connect_fixed_session(
        &self,
        config: &ConnectionConfig,
    ) -> Result<ConnectionHandle, DriverError> {
        self.inner.connect_fixed_session(config).await
    }

    async fn observe_fixed_session(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<crate::session::SessionContext, DriverError> {
        self.inner.observe_fixed_session(handle).await
    }

    async fn query_to_sink(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
        sql: &str,
        limit: Option<u32>,
        parameters: Option<&[Value]>,
        sink: &dyn crate::resource::ResultSink,
    ) -> Result<Vec<StatementResult>, crate::resource::ResourceError> {
        self.inner
            .query_to_sink(handle, execution_id, sql, limit, parameters, sink)
            .await
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let info = self.inner.test_connection(config).await?;
        Ok(ServerInfo {
            server_type: self.db_type.clone(),
            ..info
        })
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        self.inner.disconnect(handle).await
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        self.inner.get_databases(handle).await
    }

    async fn has_complete_foreign_key_catalog_visibility(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<bool, DriverError> {
        self.inner
            .has_complete_foreign_key_catalog_visibility(handle)
            .await
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        self.inner.get_tables(handle, database, schema).await
    }

    async fn get_schemas(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Option<Vec<String>>, DriverError> {
        self.inner.get_schemas(handle, database).await
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        self.inner
            .get_table_schema(handle, table, database, schema)
            .await
    }

    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        self.inner
            .get_columns(handle, table, database, schema)
            .await
    }

    async fn get_all_columns(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>, DriverError> {
        self.inner.get_all_columns(handle, database, schema).await
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
        if !self.precise_query_execution_cancel {
            return Err(DriverError::Unsupported(
                "precise query cancellation is not supported for this compatibility driver".into(),
            ));
        }
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
        if !self.precise_query_execution_cancel {
            return Err(DriverError::Unsupported(
                "precise query cancellation is not supported for this compatibility driver".into(),
            ));
        }
        self.inner
            .query_stream_with_execution(handle, execution_id, sql, limit, on_event)
            .await
    }

    async fn query_stream_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        self.inner
            .query_stream_with_params(handle, sql, params, limit, on_event)
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

    fn parameter_placeholder(
        &self,
        index: usize,
        data_type: Option<&str>,
    ) -> Result<String, DriverError> {
        self.inner.parameter_placeholder(index, data_type)
    }

    fn max_bound_parameters(&self) -> usize {
        self.inner.max_bound_parameters()
    }

    async fn execute_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<u64, DriverError> {
        self.inner.execute_with_params(handle, sql, params).await
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        self.inner.execute(handle, sql).await
    }

    async fn open_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        self.inner.open_databases(handle).await
    }

    fn qualified_sql(&self, sql: &str, target: SqlTarget<'_>) -> String {
        self.inner.qualified_sql(sql, target)
    }

    async fn query_at(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        target: SqlTarget<'_>,
    ) -> Result<QueryResult, DriverError> {
        self.inner.query_at(handle, sql, target).await
    }

    async fn query_multi_at(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        target: SqlTarget<'_>,
    ) -> Result<MultiQueryResult, DriverError> {
        self.inner.query_multi_at(handle, sql, limit, target).await
    }

    async fn execute_at(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        target: SqlTarget<'_>,
    ) -> Result<u64, DriverError> {
        self.inner.execute_at(handle, sql, target).await
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
        if !self.precise_query_execution_cancel {
            return Err(DriverError::Unsupported(
                "precise query cancellation is not supported for this compatibility driver".into(),
            ));
        }
        self.inner
            .query_stream_with_execution_at(handle, execution_id, sql, limit, target, on_event)
            .await
    }

    async fn query_with_params_at(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
        target: SqlTarget<'_>,
    ) -> Result<QueryResult, DriverError> {
        self.inner
            .query_with_params_at(handle, sql, params, target)
            .await
    }

    async fn query_stream_at(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        target: SqlTarget<'_>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        self.inner
            .query_stream_at(handle, sql, limit, target, on_event)
            .await
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        self.inner.command_definitions()
    }

    fn target_requirements(
        &self,
        command: &str,
    ) -> Option<crate::namespace::CommandTargetRequirements> {
        self.inner.target_requirements(command)
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

    async fn begin_read_snapshot(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        self.inner.begin_read_snapshot(handle).await
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
        if !self.precise_query_execution_cancel {
            return Err(DriverError::Unsupported(
                "precise query cancellation is not supported for this compatibility driver".into(),
            ));
        }
        self.inner
            .cancel_query_with_execution(handle, execution_id)
            .await
    }

    async fn cleanup_query_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        if !self.precise_query_execution_cancel {
            return Ok(());
        }
        self.inner
            .cleanup_query_execution(handle, execution_id)
            .await
    }

    fn supports_query_execution_cancel(&self) -> bool {
        self.precise_query_execution_cancel
    }

    async fn get_server_info(&self, handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
        let info = self.inner.get_server_info(handle).await?;
        Ok(ServerInfo {
            server_type: self.db_type.clone(),
            ..info
        })
    }

    async fn physical_database_identity(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Option<String>, DriverError> {
        self.inner
            .physical_database_identity(handle, database)
            .await
    }

    async fn schema_scope_identity(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: &str,
    ) -> Result<Option<String>, DriverError> {
        self.inner
            .schema_scope_identity(handle, database, schema)
            .await
    }

    fn prompt_overrides(&self) -> HashMap<PromptScenario, PromptTemplate> {
        self.inner.prompt_overrides()
    }

    fn dialect_notes(&self) -> Option<String> {
        self.inner.dialect_notes()
    }

    fn qualify_sql_target(
        &self,
        sql: &str,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Option<String> {
        self.inner.qualify_sql_target(sql, database, schema)
    }

    async fn dump_table_ddl(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<String, DriverError> {
        self.inner
            .dump_table_ddl(handle, table, database, schema)
            .await
    }

    async fn dump_view_ddl(
        &self,
        handle: &ConnectionHandle,
        view: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<String, DriverError> {
        self.inner
            .dump_view_ddl(handle, view, database, schema)
            .await
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

    fn new_sql_scanner(&self) -> crate::sql_split::SqlStatementScanner {
        self.inner.new_sql_scanner()
    }

    async fn recover_restore_statement(
        &self,
        handle: &ConnectionHandle,
        stmt: &str,
        error: &DriverError,
        overwrite: bool,
    ) -> Result<bool, DriverError> {
        self.inner
            .recover_restore_statement(handle, stmt, error, overwrite)
            .await
    }

    async fn restore_sql(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        opts: Option<&BackupRestoreOptions>,
    ) -> Result<(), DriverError> {
        self.inner.restore_sql(handle, sql, opts).await
    }

    async fn restore_sql_with_progress(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        opts: Option<&BackupRestoreOptions>,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<(), DriverError> {
        self.inner
            .restore_sql_with_progress(handle, sql, opts, on_progress)
            .await
    }

    async fn validate_schema_migration(
        &self,
        handle: &ConnectionHandle,
        target: SqlTarget<'_>,
    ) -> Result<(), DriverError> {
        self.inner.validate_schema_migration(handle, target).await
    }

    async fn validate_schema_migration_plan(
        &self,
        handle: &ConnectionHandle,
        target: SqlTarget<'_>,
        expected_target_schemas: &[TableSchema],
    ) -> Result<(), DriverError> {
        self.inner
            .validate_schema_migration_plan(handle, target, expected_target_schemas)
            .await
    }

    fn render_transfer_identity_sequence_sync_sql(
        &self,
        schema: Option<&str>,
        table: &str,
        columns: &[String],
    ) -> Result<Vec<String>, DriverError> {
        self.inner
            .render_transfer_identity_sequence_sync_sql(schema, table, columns)
    }

    async fn structure_capabilities(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<StructureCapabilities, DriverError> {
        self.inner.structure_capabilities(handle).await
    }

    async fn plan_structure_changes(
        &self,
        handle: &ConnectionHandle,
        request: &StructureChangeRequest,
    ) -> Result<StructureChangePlan, DriverError> {
        self.inner.plan_structure_changes(handle, request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_stream::QUERY_STREAM_BATCH_SIZE;
    use crate::types::{ColumnInfo, QueryResult, StatementResult};
    use std::sync::Mutex;

    struct FakeDriver {
        streamed: Mutex<bool>,
        override_stream: bool,
        rows: usize,
        precise_cancel: bool,
        ddl_atomicity: Option<DdlAtomicity>,
        full_fk_catalog_visibility: bool,
        /// Sentinel returned by the [`DatabaseDriver::build_insert_statement`]
        /// override. `None` models a driver that keeps the trait default, so a
        /// completed seam has to be set explicitly.
        insert_sentinel: Option<String>,
        /// Sentinel returned by [`DatabaseDriver::insert_returning_clause`].
        returning_sentinel: Option<String>,
    }

    impl FakeDriver {
        fn new(rows: usize, override_stream: bool) -> Arc<Self> {
            Arc::new(Self {
                streamed: Mutex::new(false),
                override_stream,
                rows,
                precise_cancel: false,
                ddl_atomicity: None,
                full_fk_catalog_visibility: false,
                insert_sentinel: None,
                returning_sentinel: None,
            })
        }

        fn new_precise(rows: usize) -> Arc<Self> {
            Arc::new(Self {
                streamed: Mutex::new(false),
                override_stream: true,
                rows,
                precise_cancel: true,
                ddl_atomicity: None,
                full_fk_catalog_visibility: false,
                insert_sentinel: None,
                returning_sentinel: None,
            })
        }

        fn with_ddl_atomicity(rows: usize, atomicity: DdlAtomicity) -> Arc<Self> {
            Arc::new(Self {
                streamed: Mutex::new(false),
                override_stream: false,
                rows,
                precise_cancel: false,
                ddl_atomicity: Some(atomicity),
                full_fk_catalog_visibility: false,
                insert_sentinel: None,
                returning_sentinel: None,
            })
        }

        fn with_full_fk_catalog_visibility(mut self: Arc<Self>) -> Arc<Self> {
            Arc::get_mut(&mut self)
                .expect("fake driver is uniquely owned")
                .full_fk_catalog_visibility = true;
            self
        }

        /// Arms the two INSERT seams with values no default implementation can
        /// produce, so forwarding is observable rather than merely plausible.
        fn with_insert_seams(
            mut self: Arc<Self>,
            insert_sentinel: &str,
            returning_sentinel: Option<&str>,
        ) -> Arc<Self> {
            let this = Arc::get_mut(&mut self).expect("fake driver is uniquely owned");
            this.insert_sentinel = Some(insert_sentinel.to_string());
            this.returning_sentinel = returning_sentinel.map(str::to_string);
            self
        }

        fn statement(&self, sql: &str) -> StatementResult {
            StatementResult {
                sql: sql.to_string(),
                columns: vec![ColumnInfo {
                    name: "id".into(),
                    data_type: "int".into(),
                    nullable: false,
                }],
                rows: (0..self.rows as i64)
                    .map(|i| vec![Some(Value::Integer(i))])
                    .collect(),
                rows_affected: Some(self.rows as u64),
                execution_time_ms: 1,
                truncated: false,
            }
        }
    }

    #[async_trait]
    impl DatabaseDriver for FakeDriver {
        fn driver_type(&self) -> DatabaseType {
            "fake".into()
        }

        fn ddl_atomicity(&self) -> DdlAtomicity {
            self.ddl_atomicity.unwrap_or(DdlAtomicity::Unknown)
        }

        /// Mirrors a real driver that extends the default INSERT builder: the
        /// sentinel proves the wrapper reached *this* body instead of inheriting
        /// the trait default.
        fn build_insert_statement(
            &self,
            _table: &str,
            _values: &[(&str, Option<Value>)],
        ) -> Result<BoundSqlStatement, DriverError> {
            Ok(BoundSqlStatement {
                sql: self
                    .insert_sentinel
                    .clone()
                    .unwrap_or_else(|| "FAKE_DRIVER_DEFAULT_UNSET".into()),
                parameters: Vec::new(),
            })
        }

        fn insert_returning_clause(&self, _columns: &[&str]) -> Option<String> {
            self.returning_sentinel.clone()
        }

        async fn has_complete_foreign_key_catalog_visibility(
            &self,
            _handle: &ConnectionHandle,
        ) -> Result<bool, DriverError> {
            Ok(self.full_fk_catalog_visibility)
        }

        async fn physical_database_identity(
            &self,
            _handle: &ConnectionHandle,
            database: &str,
        ) -> Result<Option<String>, DriverError> {
            Ok(Some(format!("physical:{database}")))
        }

        async fn schema_scope_identity(
            &self,
            _handle: &ConnectionHandle,
            database: &str,
            schema: &str,
        ) -> Result<Option<String>, DriverError> {
            Ok(Some(format!("schema:{database}:{schema}")))
        }

        async fn connect(
            &self,
            _config: &ConnectionConfig,
        ) -> Result<ConnectionHandle, DriverError> {
            Ok(ConnectionHandle {
                id: "h".into(),
                pool_id: "p".into(),
            })
        }

        async fn test_connection(
            &self,
            _config: &ConnectionConfig,
        ) -> Result<ServerInfo, DriverError> {
            Ok(ServerInfo {
                server_version: String::new(),
                server_type: "fake".into(),
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
            table: &str,
            _database: &str,
            _schema: Option<&str>,
        ) -> Result<TableSchema, DriverError> {
            Ok(TableSchema {
                table_name: table.to_string(),
                columns: vec![],
                primary_keys: vec![],
                indexes: vec![],
                foreign_keys: vec![],
                check_constraints: vec![],
                table_options: TableOptions::default(),
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
            sql: &str,
            _limit: Option<u32>,
        ) -> Result<MultiQueryResult, DriverError> {
            Ok(MultiQueryResult {
                results: vec![self.statement(sql)],
                total_time_ms: 1,
            })
        }

        async fn query_stream(
            &self,
            handle: &ConnectionHandle,
            sql: &str,
            limit: Option<u32>,
            on_event: QueryStreamCallback,
        ) -> Result<(), DriverError> {
            *self.streamed.lock().unwrap() = true;
            if self.override_stream {
                emit_multi_query_as_stream(self.query_multi(handle, sql, limit).await?, &on_event);
                return Ok(());
            }
            let result = self.query_multi(handle, sql, limit).await?;
            emit_multi_query_as_stream(result, &on_event);
            Ok(())
        }

        async fn prepare_query_execution(
            &self,
            _handle: &ConnectionHandle,
            _execution_id: &QueryExecutionId,
        ) -> Result<(), DriverError> {
            Ok(())
        }

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
            _params: &[Value],
        ) -> Result<QueryResult, DriverError> {
            self.query(handle, sql).await
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

        async fn cancel_query_with_execution(
            &self,
            _handle: &ConnectionHandle,
            _execution_id: &QueryExecutionId,
        ) -> Result<(), DriverError> {
            Ok(())
        }

        async fn cleanup_query_execution(
            &self,
            _handle: &ConnectionHandle,
            _execution_id: &QueryExecutionId,
        ) -> Result<(), DriverError> {
            Ok(())
        }

        fn supports_query_execution_cancel(&self) -> bool {
            self.precise_cancel
        }
    }

    fn collect() -> (QueryStreamCallback, Arc<Mutex<Vec<QueryStreamEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_cb = Arc::clone(&events);
        (
            Arc::new(move |ev| {
                events_cb.lock().unwrap().push(ev);
            }),
            events,
        )
    }

    #[tokio::test]
    async fn default_query_stream_chunks_materialized_rows() {
        struct DefaultStreamDriver {
            inner: FakeDriver,
        }

        #[async_trait]
        impl DatabaseDriver for DefaultStreamDriver {
            fn driver_type(&self) -> DatabaseType {
                self.inner.driver_type()
            }
            async fn connect(
                &self,
                config: &ConnectionConfig,
            ) -> Result<ConnectionHandle, DriverError> {
                self.inner.connect(config).await
            }
            async fn test_connection(
                &self,
                config: &ConnectionConfig,
            ) -> Result<ServerInfo, DriverError> {
                self.inner.test_connection(config).await
            }
            async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
                self.inner.disconnect(handle).await
            }
            async fn get_databases(
                &self,
                handle: &ConnectionHandle,
            ) -> Result<Vec<String>, DriverError> {
                self.inner.get_databases(handle).await
            }
            async fn get_tables(
                &self,
                handle: &ConnectionHandle,
                database: &str,
                schema: Option<&str>,
            ) -> Result<Vec<TableInfo>, DriverError> {
                self.inner.get_tables(handle, database, schema).await
            }
            async fn get_table_schema(
                &self,
                handle: &ConnectionHandle,
                table: &str,
                database: &str,
                schema: Option<&str>,
            ) -> Result<TableSchema, DriverError> {
                self.inner
                    .get_table_schema(handle, table, database, schema)
                    .await
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
            async fn query_with_params(
                &self,
                handle: &ConnectionHandle,
                sql: &str,
                params: &[Value],
            ) -> Result<QueryResult, DriverError> {
                self.inner.query_with_params(handle, sql, params).await
            }
            async fn execute(
                &self,
                handle: &ConnectionHandle,
                sql: &str,
            ) -> Result<u64, DriverError> {
                self.inner.execute(handle, sql).await
            }
            async fn cancel_query(&self, handle: &ConnectionHandle) -> Result<(), DriverError> {
                self.inner.cancel_query(handle).await
            }
        }

        let driver = DefaultStreamDriver {
            inner: FakeDriver {
                streamed: Mutex::new(false),
                override_stream: false,
                rows: QUERY_STREAM_BATCH_SIZE + 2,
                precise_cancel: false,
                ddl_atomicity: None,
                full_fk_catalog_visibility: false,
                insert_sentinel: None,
                returning_sentinel: None,
            },
        };
        let handle = ConnectionHandle {
            id: "h".into(),
            pool_id: "p".into(),
        };
        let (cb, events) = collect();
        driver
            .query_stream(&handle, "SELECT 1", None, cb)
            .await
            .unwrap();
        let events = events.lock().unwrap();
        let chunks = events
            .iter()
            .filter(|e| matches!(e, QueryStreamEvent::Rows { .. }))
            .count();
        assert!(chunks >= 2);
        assert!(matches!(events.last(), Some(QueryStreamEvent::Done { .. })));
    }

    #[tokio::test]
    async fn reuse_driver_forwards_query_stream() {
        let inner = FakeDriver::new(3, true);
        let reuse = ReuseDriver::new(inner.clone(), "mariadb");
        let handle = ConnectionHandle {
            id: "h".into(),
            pool_id: "p".into(),
        };
        let (cb, events) = collect();
        reuse
            .query_stream(&handle, "SELECT 1", Some(10), cb)
            .await
            .unwrap();
        assert!(*inner.streamed.lock().unwrap());
        let events = events.lock().unwrap();
        assert!(events
            .iter()
            .any(|e| matches!(e, QueryStreamEvent::StatementStart { .. })));
        assert!(matches!(events.last(), Some(QueryStreamEvent::Done { .. })));
    }

    /// `ReuseDriver` must forward both INSERT seams to the wrapped driver.
    ///
    /// The sibling test in `traits/sql_text.rs` uses `MockDriver`, which does
    /// *not* override `build_insert_statement`; there the wrapper and the inner
    /// driver both fall through to the same default body, so that assertion
    /// cannot tell forwarding from inheritance. This one arms a real override
    /// with a sentinel, which is the only way to observe the wrapper's decision.
    #[test]
    fn reuse_driver_forwards_both_insert_seams_to_the_inner_driver() {
        let inner = FakeDriver::new(0, false)
            .with_insert_seams("SENTINEL INNER INSERT", Some("RETURNING `id`"));
        let reuse = ReuseDriver::new(inner, "doris");

        let statement = reuse
            .build_insert_statement("rows", &[("name", Some(Value::Integer(1)))])
            .expect("forwarded insert builder");
        assert_eq!(
            statement.sql, "SENTINEL INNER INSERT",
            "the wrapper inherited the generic builder instead of forwarding"
        );

        assert_eq!(
            reuse.insert_returning_clause(&["id"]),
            Some("RETURNING `id`".to_string()),
            "the wrapper inherited `None` instead of forwarding the inner clause"
        );
    }

    #[tokio::test]
    async fn reuse_driver_forwards_complete_fk_catalog_visibility() {
        let inner = FakeDriver::new(0, false).with_full_fk_catalog_visibility();
        let reuse = ReuseDriver::new(inner, "mariadb");
        let handle = ConnectionHandle {
            id: "h".into(),
            pool_id: "p".into(),
        };

        assert!(reuse
            .has_complete_foreign_key_catalog_visibility(&handle)
            .await
            .expect("forward visibility query"));
    }

    #[tokio::test]
    async fn reuse_driver_forwards_precise_execution_protocol_and_capability() {
        let inner = FakeDriver::new_precise(1);
        let reuse = ReuseDriver::new(inner, "cloudberry");
        let handle = ConnectionHandle {
            id: "h".into(),
            pool_id: "p".into(),
        };
        let execution_id = QueryExecutionId::new("exec-1");

        assert!(reuse.supports_query_execution_cancel());
        reuse
            .prepare_query_execution(&handle, &execution_id)
            .await
            .unwrap();
        reuse
            .cancel_query_with_execution(&handle, &execution_id)
            .await
            .unwrap();
        reuse
            .cleanup_query_execution(&handle, &execution_id)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn disabled_reuse_precise_protocol_does_not_call_inner_cancel() {
        let inner = FakeDriver::new_precise(1);
        let reuse = ReuseDriver::new_with_precise_cancel(inner, "questdb", false);
        let handle = ConnectionHandle {
            id: "h".into(),
            pool_id: "p".into(),
        };
        let error = reuse
            .cancel_query_with_execution(&handle, &QueryExecutionId::new("exec-1"))
            .await
            .unwrap_err();

        assert!(!reuse.supports_query_execution_cancel());
        assert!(
            matches!(error, DriverError::Unsupported(message) if message.contains("compatibility driver"))
        );
    }

    #[tokio::test]
    async fn reuse_driver_forwards_database_and_schema_scope_identities() {
        let reuse = ReuseDriver::new(FakeDriver::new(1, false), "sqlserver");
        let handle = ConnectionHandle {
            id: "h".into(),
            pool_id: "p".into(),
        };

        assert_eq!(
            reuse
                .physical_database_identity(&handle, "DataZen")
                .await
                .unwrap()
                .as_deref(),
            Some("physical:DataZen")
        );
        assert_eq!(
            reuse
                .schema_scope_identity(&handle, "DataZen", "sales")
                .await
                .unwrap()
                .as_deref(),
            Some("schema:DataZen:sales")
        );
    }

    #[test]
    fn test_tester_reuse_driver_forwards_ddl_atomicity() {
        let inner = FakeDriver::with_ddl_atomicity(1, DdlAtomicity::Transactional);
        let reuse = ReuseDriver::new(inner, "mariadb");
        assert_eq!(reuse.ddl_atomicity(), DdlAtomicity::Transactional);
    }
}

/// Enforces "the reuse wrapper decides on every driver method" in CI rather
/// than in review. Lives beside the wrapper because it reads `traits.rs` and
/// this file as text.
#[cfg(test)]
#[path = "reuse_forwarding_tests.rs"]
mod forwarding_tests;

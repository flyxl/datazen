//! The `DatabaseDriver` implementation of [`MysqlDriver`].
//!
//! Rust allows exactly one `impl DatabaseDriver for MysqlDriver`, so this
//! module owns it. The capability declarations stay here because they carry
//! no behaviour; every behavioural method forwards to the `<name>_inner`
//! method that its own module owns, so the concern modules stay the only
//! place a behaviour is written.

use super::*;
use async_trait::async_trait;

#[async_trait]
impl DatabaseDriver for MysqlDriver {
    /// This driver's own dialect SQL for the shared schema-object commands.
    fn schema_object_sql(
        &self,
    ) -> Option<std::sync::Arc<dyn datazen_driver_api::schema_object_sql::SchemaObjectSql>> {
        Some(std::sync::Arc::new(crate::MysqlSchemaObjectSql))
    }

    fn supports_bound_writes(&self) -> bool {
        true
    }

    /// MySQL has no `DEFAULT VALUES`; an all-default row is spelled with an
    /// empty column/value list. Non-empty inserts keep the shared
    /// parameterized rendering (backtick identifiers, positional `?`).
    fn build_insert_statement(
        &self,
        table: &str,
        values: &[(&str, Option<Value>)],
    ) -> Result<BoundSqlStatement, DriverError> {
        if !values.is_empty() {
            return datazen_driver_api::sql_builder::build_insert_statement(
                &datazen_driver_api::LegacySqlDialect::new(self),
                self.max_bound_parameters(),
                table,
                values,
            );
        }
        Ok(BoundSqlStatement {
            sql: format!(
                "INSERT INTO {} () VALUES ()",
                MysqlDriver::quote_identifier(table)
            ),
            parameters: Vec::new(),
        })
    }

    fn sql_literal_dialect(&self) -> Option<SqlLiteralDialect> {
        Some(SqlLiteralDialect::MySql)
    }

    fn default_host(&self) -> Option<&'static str> {
        Some(crate::mysql::connection::DEFAULT_HOST)
    }

    fn default_port(&self) -> Option<u16> {
        Some(crate::mysql::connection::DEFAULT_PORT)
    }

    fn has_multi_database(&self) -> bool {
        true
    }

    fn migration_renderer(
        &self,
    ) -> Option<std::sync::Arc<dyn datazen_driver_api::MigrationRenderer>> {
        Some(std::sync::Arc::new(super::MysqlMigrationRenderer))
    }

    fn migration_capabilities(
        &self,
    ) -> Option<std::sync::Arc<dyn datazen_driver_api::MigrationCapabilities>> {
        Some(std::sync::Arc::new(super::MysqlMigrationCapabilities))
    }

    fn type_normalizer(&self) -> Option<std::sync::Arc<dyn datazen_driver_api::TypeNormalizer>> {
        Some(std::sync::Arc::new(super::MysqlTypeNormalizer))
    }

    fn driver_type(&self) -> DatabaseType {
        if self.is_mariadb {
            "mariadb".to_string()
        } else {
            "mysql".to_string()
        }
    }

    fn ddl_atomicity(&self) -> DdlAtomicity {
        DdlAtomicity::AutoCommitPerStatement
    }

    fn supports_keyset_pagination(&self) -> bool {
        true
    }

    /// The seek condition for one page boundary.
    ///
    /// MySQL compares row values, so a one-direction order is a single
    /// `(a, b) > (1, 2)` the index can answer directly. A mixed order, and a
    /// boundary that holds a `NULL`, fall back to the lexicographic spelling.
    fn keyset_predicate_sql(
        &self,
        order_columns: &[(String, bool)],
        boundary: &[(String, super::Value)],
    ) -> Option<String> {
        datazen_driver_api::keyset::row_value_seek_predicate(self, order_columns, boundary)
    }

    fn sync_family(&self) -> String {
        "mysql".into()
    }

    fn dialect_notes(&self) -> Option<String> {
        Some(
            "MySQL uses LIMIT offset, count (reversed order from PostgreSQL), \
             CONCAT() for string concatenation, LIKE is case-insensitive by default \
             (collation-dependent), ENUM and SET types, INSERT IGNORE and \
             ON DUPLICATE KEY UPDATE for upsert, and each DDL statement auto-commits \
             (no transactional DDL)."
                .into(),
        )
    }

    /// F7: qualify unqualified table references with the target database
    /// (`` `db`.`t` ``), shared by the mysql/mariadb/doris/starrocks/
    /// manticore/ob_oracle variants. Parse failures pass SQL through
    /// unchanged; see `sql_execution::target::qualify_sql`.
    fn qualify_sql_target(
        &self,
        sql: &str,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Option<String> {
        Some(crate::sql_target::qualify_sql(sql, database, schema))
    }

    fn quote_char(&self) -> char {
        '`'
    }

    fn format_sql_literal(&self, value: &Option<super::Value>) -> String {
        match value {
            None | Some(super::Value::Null) => "NULL".to_string(),
            Some(super::Value::Bool(b)) => {
                if *b {
                    "1".to_string()
                } else {
                    "0".to_string()
                }
            }
            Some(super::Value::Integer(i)) => i.to_string(),
            Some(super::Value::Float(f)) => f.to_string(),
            Some(super::Value::String(s)) => {
                let escaped = s.replace('\\', "\\\\");
                format!("'{}'", escaped.replace('\'', "''"))
            }
            Some(super::Value::Bytes(b)) => format!(
                "X'{}'",
                b.iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            ),
            Some(super::Value::Timestamp(s)) => {
                let escaped = s.replace('\\', "\\\\");
                format!("'{}'", escaped.replace('\'', "''"))
            }
            Some(super::Value::Json(j)) => {
                let s = j.to_string();
                let escaped = s.replace('\\', "\\\\");
                format!("'{}'", escaped.replace('\'', "''"))
            }
        }
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
        self.test_connection_inner(config).await
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        self.connect_inner(config).await
    }

    /// MySQL serves every database from one pool by qualifying names, so it
    /// holds no per-database resource at all: `close_database` can never release
    /// one. Reporting the connection's own database here would make the tree
    /// mark a database the user never opened, so the default empty report
    /// applies — the tree's own expand state is what shows a database as open.
    async fn open_databases(&self, _handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        self.open_databases_inner(_handle).await
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        self.disconnect_inner(handle).await
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        self.get_databases_inner(handle).await
    }

    async fn has_complete_foreign_key_catalog_visibility(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<bool, DriverError> {
        self.has_complete_foreign_key_catalog_visibility_inner(handle)
            .await
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        self.get_tables_inner(handle, database, schema).await
    }

    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        self.get_columns_inner(handle, table, database, schema)
            .await
    }

    async fn get_all_columns(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>, DriverError> {
        self.get_all_columns_inner(handle, database, schema).await
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        self.get_table_schema_inner(handle, table, database, schema)
            .await
    }

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        self.query_inner(handle, sql).await
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        self.query_multi_inner(handle, sql, limit).await
    }

    async fn query_stream(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        self.query_stream_inner(handle, sql, limit, on_event).await
    }

    async fn prepare_query_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.prepare_query_execution_inner(handle, execution_id)
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
        self.query_stream_with_execution_inner(handle, execution_id, sql, limit, on_event)
            .await
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        self.query_with_params_inner(handle, sql, params).await
    }

    fn parameter_placeholder(
        &self,
        _index: usize,
        _data_type: Option<&str>,
    ) -> Result<String, DriverError> {
        self.parameter_placeholder_inner(_index, _data_type)
    }

    async fn execute_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<u64, DriverError> {
        self.execute_with_params_inner(handle, sql, params).await
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        self.execute_inner(handle, sql).await
    }

    async fn begin_transaction(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        self.begin_transaction_inner(handle).await
    }

    async fn begin_read_snapshot(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        self.begin_read_snapshot_inner(handle).await
    }

    async fn commit(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        self.commit_inner(tx).await
    }

    async fn rollback(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        self.rollback_inner(tx).await
    }

    async fn explain(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        self.explain_inner(handle, sql).await
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        self.cancel_query_inner(_handle).await
    }

    async fn cancel_query_with_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.cancel_query_with_execution_inner(handle, execution_id)
            .await
    }

    async fn cleanup_query_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.cleanup_query_execution_inner(handle, execution_id)
            .await
    }

    fn supports_query_execution_cancel(&self) -> bool {
        self.supports_query_execution_cancel_inner()
    }

    async fn get_server_info(&self, handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
        self.get_server_info_inner(handle).await
    }

    async fn physical_database_identity(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<Option<String>, DriverError> {
        self.physical_database_identity_inner(handle, _database)
            .await
    }

    async fn dump_table_ddl(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<String, DriverError> {
        self.dump_table_ddl_inner(handle, table, database, schema)
            .await
    }

    async fn dump_view_ddl(
        &self,
        handle: &ConnectionHandle,
        view: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<String, DriverError> {
        self.dump_view_ddl_inner(handle, view, database, schema)
            .await
    }

    async fn dump_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
    ) -> Result<String, DriverError> {
        self.dump_database_inner(handle, database, opts).await
    }

    async fn dump_database_with_progress(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<String, DriverError> {
        self.dump_database_with_progress_inner(handle, database, opts, on_progress)
            .await
    }

    async fn dump_routines(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<String, DriverError> {
        self.dump_routines_inner(handle, _database).await
    }

    async fn dump_triggers(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<String, DriverError> {
        self.dump_triggers_inner(handle, _database).await
    }

    async fn structure_capabilities(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<StructureCapabilities, DriverError> {
        self.structure_capabilities_inner(_handle).await
    }

    async fn plan_structure_changes(
        &self,
        handle: &ConnectionHandle,
        request: &StructureChangeRequest,
    ) -> Result<StructureChangePlan, DriverError> {
        self.plan_structure_changes_inner(handle, request).await
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        self.command_definitions_inner()
    }

    async fn execute_command(
        &self,
        handle: &ConnectionHandle,
        command: &str,
        input: serde_json::Value,
    ) -> Result<CommandResult, DriverError> {
        self.execute_command_inner(handle, command, input).await
    }
}

//! Configurable in-memory driver for service/cache unit tests.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::db::{
    ColumnInfo, ColumnSchema, ConnectionConfig, ConnectionHandle, DatabaseDriver, DatabaseType,
    DriverCategory, DriverError, ExplainResult, MultiQueryResult, QueryResult, ServerInfo,
    StatementResult, StructureChangePlan, StructureChangeRequest, TableInfo, TableSchema,
    TransactionHandle, Value,
};
use datazen_driver_api::{
    execute_command_definition, execute_schema_object_command, execute_standard_sql_command,
    is_schema_object_command, query_command_definition, query_stream_command_definition,
    schema_object_command_definitions, CommandResult, DriverCommandDefinition,
};

#[derive(Clone)]
pub struct MockDriverOptions {
    /// Driver category reported by `driver_category` (defaults to Sql).
    pub category: DriverCategory,
    pub columns: Vec<ColumnSchema>,
    pub primary_keys: Vec<String>,
    pub table_schema: Option<TableSchema>,
    pub query_rows: Vec<Vec<Option<Value>>>,
    pub count_total: i64,
    pub databases: Vec<String>,
    pub tables: Vec<TableInfo>,
    pub explain_plan: ExplainResult,
    pub server_version: String,
    pub extra_commands: Vec<DriverCommandDefinition>,
    pub query_error: Option<String>,
    /// F7: when true, `qualify_sql_target` rewrites SQL by appending a
    /// marker comment recording the requested target (capability simulation).
    pub rewrite_sql_target: bool,
}

impl Default for MockDriverOptions {
    fn default() -> Self {
        Self {
            category: DriverCategory::Sql,
            columns: Vec::new(),
            primary_keys: Vec::new(),
            table_schema: None,
            query_rows: Vec::new(),
            count_total: 0,
            databases: Vec::new(),
            tables: Vec::new(),
            explain_plan: ExplainResult {
                plan_text: String::new(),
                plan_json: None,
                plan_tree: None,
                total_cost: None,
                estimated_rows: None,
            },
            server_version: String::new(),
            extra_commands: Vec::new(),
            query_error: None,
            rewrite_sql_target: false,
        }
    }
}

pub struct MockDriver {
    db_type: DatabaseType,
    opts: MockDriverOptions,
    /// Monotonic counter so each `connect` returns a distinct session handle id.
    session_seq: AtomicU32,
    get_columns_calls: AtomicU32,
    get_schema_calls: AtomicU32,
    query_calls: AtomicU32,
    last_query_limit: Mutex<Option<Option<u32>>>,
    open_txs: Mutex<HashSet<String>>,
    use_database_calls: Mutex<Vec<String>>,
    qualify_calls: Mutex<Vec<(Option<String>, Option<String>)>>,
}

impl MockDriver {
    pub fn new(db_type: impl Into<DatabaseType>, opts: MockDriverOptions) -> Arc<Self> {
        Arc::new(Self {
            db_type: db_type.into(),
            opts,
            session_seq: AtomicU32::new(0),
            get_columns_calls: AtomicU32::new(0),
            get_schema_calls: AtomicU32::new(0),
            query_calls: AtomicU32::new(0),
            last_query_limit: Mutex::new(None),
            open_txs: Mutex::new(HashSet::new()),
            use_database_calls: Mutex::new(Vec::new()),
            qualify_calls: Mutex::new(Vec::new()),
        })
    }

    /// Databases passed to `use_database`, in call order (F1 session-switch tests).
    pub fn use_database_calls(&self) -> Vec<String> {
        self.use_database_calls
            .lock()
            .ok()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// (database, schema) pairs passed to `qualify_sql_target`, in call
    /// order (F7 envelope passthrough tests).
    pub fn qualify_calls(&self) -> Vec<(Option<String>, Option<String>)> {
        self.qualify_calls
            .lock()
            .ok()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    pub fn last_query_limit(&self) -> Option<Option<u32>> {
        self.last_query_limit.lock().ok().and_then(|g| *g)
    }

    pub fn get_columns_calls(&self) -> u32 {
        self.get_columns_calls.load(Ordering::Relaxed)
    }

    pub fn get_schema_calls(&self) -> u32 {
        self.get_schema_calls.load(Ordering::Relaxed)
    }

    pub fn query_calls(&self) -> u32 {
        self.query_calls.load(Ordering::Relaxed)
    }

    pub fn reset_columns_calls(&self) {
        self.get_columns_calls.store(0, Ordering::Relaxed);
    }

    fn sample_columns() -> Vec<ColumnSchema> {
        vec![
            ColumnSchema {
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: true,
            },
            ColumnSchema {
                name: "name".into(),
                data_type: "text".into(),
                nullable: true,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            },
        ]
    }

    pub fn default_table_schema(table: &str) -> TableSchema {
        TableSchema {
            table_name: table.to_string(),
            columns: Self::sample_columns(),
            primary_keys: vec!["id".into()],
            indexes: vec![],
            foreign_keys: vec![],
        }
    }
}

#[async_trait]
impl DatabaseDriver for MockDriver {
    fn driver_type(&self) -> DatabaseType {
        self.db_type.clone()
    }

    fn driver_category(&self) -> DriverCategory {
        self.opts.category.clone()
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let seq = self.session_seq.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(ConnectionHandle {
            id: format!("mock-{}-{}", config.id, seq),
            pool_id: format!("pool-{}", config.id),
        })
    }

    async fn test_connection(&self, _config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        Ok(ServerInfo {
            server_version: self.opts.server_version.clone(),
            server_type: self.db_type.clone(),
        })
    }

    async fn disconnect(&self, _handle: ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }

    async fn get_databases(&self, _handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Ok(self.opts.databases.clone())
    }

    async fn get_tables(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        Ok(self.opts.tables.clone())
    }

    async fn get_table_schema(
        &self,
        _handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        self.get_schema_calls.fetch_add(1, Ordering::Relaxed);
        Ok(self
            .opts
            .table_schema
            .clone()
            .unwrap_or_else(|| Self::default_table_schema(table)))
    }

    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        self.get_columns_calls.fetch_add(1, Ordering::Relaxed);
        if !self.opts.columns.is_empty() {
            return Ok((self.opts.columns.clone(), self.opts.primary_keys.clone()));
        }
        let schema = self.get_table_schema(handle, table).await?;
        Ok((schema.columns, schema.primary_keys))
    }

    async fn query(
        &self,
        _handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        self.query_calls.fetch_add(1, Ordering::Relaxed);
        if let Some(msg) = &self.opts.query_error {
            return Err(DriverError::QueryFailed(msg.clone()));
        }
        if sql.contains("COUNT(*)") {
            return Ok(QueryResult {
                columns: vec![ColumnInfo {
                    name: "count".into(),
                    data_type: "bigint".into(),
                    nullable: false,
                }],
                rows: vec![vec![Some(Value::Integer(self.opts.count_total))]],
                rows_affected: None,
                execution_time_ms: 0,
            });
        }
        let columns: Vec<ColumnInfo> = self
            .opts
            .columns
            .iter()
            .map(|c| ColumnInfo {
                name: c.name.clone(),
                data_type: c.data_type.clone(),
                nullable: c.nullable,
            })
            .collect();
        Ok(QueryResult {
            columns,
            rows: self.opts.query_rows.clone(),
            rows_affected: None,
            execution_time_ms: 0,
        })
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        let result = self.query(handle, sql).await?;
        if let Ok(mut g) = self.last_query_limit.lock() {
            *g = Some(limit);
        }
        Ok(MultiQueryResult {
            results: vec![StatementResult {
                sql: sql.to_string(),
                columns: result.columns,
                rows: result.rows,
                rows_affected: result.rows_affected,
                execution_time_ms: result.execution_time_ms,
                truncated: false,
            }],
            total_time_ms: 0,
        })
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        _params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        self.query(handle, sql).await
    }

    async fn execute(&self, _handle: &ConnectionHandle, _sql: &str) -> Result<u64, DriverError> {
        Ok(0)
    }

    async fn explain(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        Ok(self.opts.explain_plan.clone())
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }

    async fn use_database(
        &self,
        _handle: &ConnectionHandle,
        database: &str,
    ) -> Result<(), DriverError> {
        if let Ok(mut calls) = self.use_database_calls.lock() {
            calls.push(database.to_string());
        }
        Ok(())
    }

    /// F7 capability simulation: appends a `/* target: db=… schema=… */`
    /// marker so host tests can assert which SQL actually reached the driver.
    fn qualify_sql_target(
        &self,
        sql: &str,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Option<String> {
        if !self.opts.rewrite_sql_target {
            return None;
        }
        if let Ok(mut calls) = self.qualify_calls.lock() {
            calls.push((database.map(str::to_string), schema.map(str::to_string)));
        }
        let mut marker = String::from("/* target:");
        if let Some(database) = database {
            marker.push_str(&format!(" db={database}"));
        }
        if let Some(schema) = schema {
            marker.push_str(&format!(" schema={schema}"));
        }
        marker.push_str(" */");
        Some(format!("{sql} {marker}"))
    }

    /// No-op planning support so host-side structure-command tests can assert
    /// session/database pinning around `plan_structure_changes`.
    async fn plan_structure_changes(
        &self,
        _handle: &ConnectionHandle,
        _request: &StructureChangeRequest,
    ) -> Result<StructureChangePlan, DriverError> {
        Ok(StructureChangePlan::default())
    }

    async fn get_server_info(&self, _handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
        Ok(ServerInfo {
            server_version: self.opts.server_version.clone(),
            server_type: self.db_type.clone(),
        })
    }

    async fn begin_transaction(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        let mut txs = self.open_txs.lock().expect("mock open_txs");
        if !txs.insert(handle.id.clone()) {
            return Err(DriverError::TransactionError(
                "A transaction is already open on this connection".into(),
            ));
        }
        Ok(TransactionHandle {
            id: format!("mock_tx_{}", handle.id),
            connection_id: handle.id.clone(),
        })
    }

    async fn commit(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        let mut txs = self.open_txs.lock().expect("mock open_txs");
        if !txs.remove(&tx.connection_id) {
            return Err(DriverError::TransactionError(
                "Transaction not found or already ended".into(),
            ));
        }
        Ok(())
    }

    async fn rollback(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        let mut txs = self.open_txs.lock().expect("mock open_txs");
        if !txs.remove(&tx.connection_id) {
            return Err(DriverError::TransactionError(
                "Transaction not found or already ended".into(),
            ));
        }
        Ok(())
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        let mut definitions = vec![
            query_command_definition(),
            query_stream_command_definition(),
            execute_command_definition(),
        ];
        definitions.extend(schema_object_command_definitions());
        definitions.extend(self.opts.extra_commands.clone());
        definitions
    }

    async fn execute_command(
        &self,
        handle: &ConnectionHandle,
        command: &str,
        input: serde_json::Value,
    ) -> Result<CommandResult, DriverError> {
        if self
            .opts
            .extra_commands
            .iter()
            .any(|definition| definition.id == command)
        {
            return Ok(CommandResult::new(serde_json::json!({
                "command": command,
                "input": input,
            })));
        }
        if is_schema_object_command(command) {
            return execute_schema_object_command(self, &self.db_type, handle, command, input)
                .await;
        }
        execute_standard_sql_command(self, handle, command, input).await
    }
}

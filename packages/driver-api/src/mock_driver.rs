//! Configurable in-memory driver for service/cache unit tests.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::{
    execute_command_definition, execute_schema_object_command, execute_standard_sql_command,
    is_schema_object_command, query_command_definition, query_stream_command_definition,
    schema_catalog_command_definitions, schema_object_command_definitions,
    try_execute_schema_catalog_command, validate_schema_target, CommandResult, DdlAtomicity,
    DriverCommandDefinition, SchemaScope,
};
use crate::{
    ColumnInfo, ColumnSchema, ConnectionConfig, ConnectionHandle, DatabaseDriver, DatabaseType,
    DriverCategory, DriverError, ExplainResult, MultiQueryResult, QueryResult, ServerInfo,
    StatementResult, StructureChangePlan, StructureChangeRequest, TableInfo, TableSchema,
    TransactionHandle, Value,
};

/// Models "another task registered its own transaction for the same
/// `dbSessionId` while this one was being opened".
///
/// `begin_transaction` publishes `winner` into `slot` *before* it returns, so a
/// caller that re-reads its own bookkeeping after `await`ing the driver — which
/// is exactly what `begin_session_transaction_impl` does — deterministically
/// loses the race. Nothing else can reach that branch: the map is empty when the
/// function starts, and the only writer between the two reads is the driver.
#[derive(Clone)]
pub struct BeginRace {
    /// The caller's own `dbSessionId → TransactionHandle` bookkeeping.
    pub slot: Arc<tokio::sync::Mutex<HashMap<String, TransactionHandle>>>,
    /// The handle the competing task won with. Behind an `Arc` so it can be
    /// taken on first use while `MockDriverOptions` stays `Clone`
    /// (`TransactionHandle` is deliberately not `Clone`).
    pub winner: Arc<Mutex<Option<TransactionHandle>>>,
}

#[derive(Clone)]
pub struct MockDriverOptions {
    /// Driver category reported by `driver_category` (defaults to Sql).
    pub category: DriverCategory,
    /// Host this mock declares as its implicit default, reported by
    /// `DatabaseDriver::default_host`. `None` keeps the trait's own default
    /// ("this driver applies no implicit host"), which is what a mock standing
    /// in for e.g. SQLite should say.
    pub default_host: Option<&'static str>,
    /// Port this mock declares as its implicit default, reported by
    /// `DatabaseDriver::default_port`.
    pub default_port: Option<u16>,
    pub columns: Vec<ColumnSchema>,
    pub primary_keys: Vec<String>,
    /// Per-relation read failures for metadata batch partial-success tests.
    pub column_errors_by_table: HashMap<String, String>,
    pub table_schema: Option<TableSchema>,
    pub query_rows: Vec<Vec<Option<Value>>>,
    /// Keyset pages after the first are empty for resume-executor tests.
    pub empty_keyset_after_cursor: bool,
    pub count_total: i64,
    pub databases: Vec<String>,
    pub tables: Vec<TableInfo>,
    /// Per-database table schemas for Schema Diff command tests. When a
    /// database is present, a missing table resolves to an empty schema.
    pub table_schemas_by_database: HashMap<String, HashMap<String, TableSchema>>,
    /// Simulate a MySQL identity with proven server-wide catalog visibility.
    pub complete_foreign_key_catalog_visibility: bool,
    pub explain_plan: ExplainResult,
    pub server_version: String,
    pub extra_commands: Vec<DriverCommandDefinition>,
    pub command_results: HashMap<String, serde_json::Value>,
    pub query_error: Option<String>,
    /// When set, precise execution-handle cancellation returns a driver-level
    /// error for command tests. The legacy session-wide method remains a
    /// separate counter and is never used by the Host cancel path.
    pub cancel_error: Option<String>,
    /// Rows affected by the generic execute path (zero by default so callers
    /// that do not model mutations retain the old mock behavior).
    pub execute_rows_affected: u64,
    /// Enable the parameterized DML contract for command integration tests.
    /// The default remains unsupported so tests that exercise capability
    /// gating keep their original behavior.
    pub parameterized_writes: bool,
    /// Fail the selected commit call to model a lost acknowledgement. When
    /// `commit_error_after_effect` is true, the transaction is removed before
    /// returning the error; otherwise it remains open and unapplied.
    pub commit_error_on_call: Option<u32>,
    pub commit_error_after_effect: bool,
    /// Fail the selected rollback call to model an unknown rollback outcome.
    pub rollback_error_on_call: Option<u32>,
    pub rollback_error: Option<String>,
    /// Make `begin_transaction` hand the caller's race to a competing task —
    /// see [`BeginRace`]. `None` (the default) keeps the single-writer model.
    pub begin_race: Option<BeginRace>,
    /// Fail every `disconnect` to model a driver that cannot confirm the
    /// physical connection is gone. Used to pin that the Host reports such a
    /// teardown instead of claiming a clean disconnect.
    pub disconnect_error: Option<String>,
    /// Fail parameterized migration DML before it reaches the target.
    pub execute_with_params_error: Option<String>,
    /// Fail target DDL or another unparameterized mutation.
    pub execute_error: Option<String>,
    /// F7: when true, `qualify_sql_target` rewrites SQL by appending a
    /// marker comment recording the requested target (capability simulation).
    pub rewrite_sql_target: bool,
    /// When set, overrides the default [`DdlAtomicity::Unknown`] for transaction-scope tests.
    pub ddl_atomicity: Option<DdlAtomicity>,
    /// Per-database table lists, modelling `get_tables(handle, database)` —
    /// the driver answers for the *requested* database. Empty keeps the flat
    /// [`Self::tables`] behavior.
    pub tables_by_database: HashMap<String, Vec<TableInfo>>,
    /// Columns per database → table, modelling `get_table_schema` /
    /// `get_columns` under the **explicit-target** contract: the answer is
    /// looked up under the `database` argument the caller passed, and an
    /// unknown table yields an empty column list — the silent-empty case the
    /// Host must never cache. Empty keeps the flat [`Self::columns`] /
    /// [`Self::table_schema`] behavior.
    pub columns_by_database: HashMap<String, HashMap<String, Vec<ColumnSchema>>>,
    /// Declared schema level, forwarded to `has_schema_level`. When true the
    /// mock enforces the same rule the real drivers do: a metadata call with no
    /// schema is rejected, so a host call site that forgets the schema fails
    /// loudly in tests instead of silently reading the wrong namespace.
    pub has_schema_level: bool,
    /// Forwarded to `default_schema` (only meaningful with
    /// [`Self::has_schema_level`]).
    pub default_schema: Option<&'static str>,
    /// Databases reported as holding an open resource by `open_databases`.
    /// Empty models a driver with no per-database resource.
    pub open_databases: Vec<String>,
    /// When true, `query_with_params` models a **keyed equality read**: the
    /// leading cells of each configured row must equal the leading bound
    /// params, so a read for key 2 never answers with the row for key 1.
    ///
    /// Opt-in because the default mock answers every query with
    /// [`Self::query_rows`], and keyset pagination (`> ?1` seek predicates)
    /// shares this method: filtering those would drop later pages. Only a
    /// pure equality read (`… = ?1`, no `>` seek) is filtered, so a test opts
    /// in exactly for the keyed-read shape it needs to prove.
    pub filter_rows_by_key_equality: bool,
}

impl Default for MockDriverOptions {
    fn default() -> Self {
        Self {
            category: DriverCategory::Sql,
            default_host: None,
            default_port: None,
            columns: Vec::new(),
            primary_keys: Vec::new(),
            column_errors_by_table: HashMap::new(),
            table_schema: None,
            query_rows: Vec::new(),
            empty_keyset_after_cursor: false,
            count_total: 0,
            databases: Vec::new(),
            tables: Vec::new(),
            table_schemas_by_database: HashMap::new(),
            complete_foreign_key_catalog_visibility: false,
            explain_plan: ExplainResult {
                plan_text: String::new(),
                plan_json: None,
                plan_tree: None,
                total_cost: None,
                estimated_rows: None,
            },
            server_version: String::new(),
            extra_commands: Vec::new(),
            command_results: HashMap::new(),
            query_error: None,
            cancel_error: None,
            execute_rows_affected: 0,
            parameterized_writes: false,
            commit_error_on_call: None,
            commit_error_after_effect: false,
            rollback_error_on_call: None,
            rollback_error: None,
            begin_race: None,
            disconnect_error: None,
            execute_with_params_error: None,
            execute_error: None,
            rewrite_sql_target: false,
            ddl_atomicity: None,
            tables_by_database: HashMap::new(),
            columns_by_database: HashMap::new(),
            has_schema_level: false,
            default_schema: None,
            open_databases: Vec::new(),
            filter_rows_by_key_equality: false,
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
    commit_calls: AtomicU32,
    rollback_calls: AtomicU32,
    execute_calls: AtomicU32,
    cancel_query_calls: AtomicU32,
    precise_cancel_query_calls: AtomicU32,
    last_query_limit: Mutex<Option<Option<u32>>>,
    open_txs: Mutex<HashSet<String>>,
    /// Regression tripwire: the driver contract no longer has `use_database`,
    /// so this can only ever be empty. Host tests assert it stays empty to
    /// prove no call path switched the session's database before a read.
    use_database_calls: Mutex<Vec<String>>,
    qualify_calls: Mutex<Vec<(Option<String>, Option<String>)>>,
    close_database_calls: Mutex<Vec<String>>,
    table_schemas_by_database: Mutex<HashMap<String, HashMap<String, TableSchema>>>,
    table_lists_by_database: Mutex<HashMap<String, Vec<TableInfo>>>,
}

impl MockDriver {
    pub fn new(db_type: impl Into<DatabaseType>, opts: MockDriverOptions) -> Arc<Self> {
        let table_schemas_by_database = opts.table_schemas_by_database.clone();
        let table_lists_by_database = opts.tables_by_database.clone();
        Arc::new(Self {
            db_type: db_type.into(),
            opts,
            session_seq: AtomicU32::new(0),
            get_columns_calls: AtomicU32::new(0),
            get_schema_calls: AtomicU32::new(0),
            query_calls: AtomicU32::new(0),
            commit_calls: AtomicU32::new(0),
            rollback_calls: AtomicU32::new(0),
            execute_calls: AtomicU32::new(0),
            cancel_query_calls: AtomicU32::new(0),
            precise_cancel_query_calls: AtomicU32::new(0),
            last_query_limit: Mutex::new(None),
            open_txs: Mutex::new(HashSet::new()),
            use_database_calls: Mutex::new(Vec::new()),
            qualify_calls: Mutex::new(Vec::new()),
            close_database_calls: Mutex::new(Vec::new()),
            table_schemas_by_database: Mutex::new(table_schemas_by_database),
            table_lists_by_database: Mutex::new(table_lists_by_database),
        })
    }

    /// Columns visible for `table` in the explicitly requested `database`,
    /// when per-database columns are configured. `Some(vec![])` means "that
    /// database has no such table" — the silent-empty case the Host must never
    /// cache. `None` means the option is not configured at all.
    fn columns_for_database(&self, database: &str, table: &str) -> Option<Vec<ColumnSchema>> {
        if self.opts.columns_by_database.is_empty() {
            return None;
        }
        Some(
            self.opts
                .columns_by_database
                .get(database)
                .and_then(|tables| tables.get(table))
                .cloned()
                .unwrap_or_default(),
        )
    }

    /// Databases passed to `use_database`, in call order. Always empty: the
    /// method was deleted from the contract, and this accessor exists so tests
    /// can assert that no host path re-introduced a session switch.
    pub fn use_database_calls(&self) -> Vec<String> {
        self.use_database_calls
            .lock()
            .ok()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// Databases passed to `close_database`, in call order.
    pub fn close_database_calls(&self) -> Vec<String> {
        self.close_database_calls
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

    pub fn set_table_schema_for_test(&self, database: &str, table: &str, schema: TableSchema) {
        if let Ok(mut schemas) = self.table_schemas_by_database.lock() {
            schemas
                .entry(database.to_string())
                .or_default()
                .insert(table.to_string(), schema);
        }
    }

    pub fn add_table_for_test(&self, database: &str, table: TableInfo) {
        if let Ok(mut tables) = self.table_lists_by_database.lock() {
            tables.entry(database.to_string()).or_default().push(table);
        }
    }

    pub fn query_calls(&self) -> u32 {
        self.query_calls.load(Ordering::Relaxed)
    }

    pub fn commit_calls(&self) -> u32 {
        self.commit_calls.load(Ordering::Relaxed)
    }

    pub fn execute_calls(&self) -> u32 {
        self.execute_calls.load(Ordering::Relaxed)
    }

    pub fn cancel_query_calls(&self) -> u32 {
        self.cancel_query_calls.load(Ordering::Relaxed)
    }

    pub fn precise_cancel_query_calls(&self) -> u32 {
        self.precise_cancel_query_calls.load(Ordering::Relaxed)
    }

    pub fn reset_columns_calls(&self) {
        self.get_columns_calls.store(0, Ordering::Relaxed);
    }

    pub fn open_transaction_count(&self) -> usize {
        self.open_txs
            .lock()
            .map(|txs| txs.len())
            .unwrap_or_default()
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
            check_constraints: vec![],
            table_options: Default::default(),
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

    fn default_host(&self) -> Option<&'static str> {
        self.opts.default_host
    }

    fn default_port(&self) -> Option<u16> {
        self.opts.default_port
    }

    fn ddl_atomicity(&self) -> DdlAtomicity {
        self.opts.ddl_atomicity.unwrap_or(DdlAtomicity::Unknown)
    }

    fn has_schema_level(&self) -> bool {
        self.opts.has_schema_level
    }

    fn default_schema(&self) -> Option<&'static str> {
        self.opts.default_schema
    }

    async fn close_database(
        &self,
        _handle: &ConnectionHandle,
        database: &str,
    ) -> Result<bool, DriverError> {
        self.close_database_calls
            .lock()
            .map(|mut g| g.push(database.to_string()))
            .map_err(|_| DriverError::QueryFailed("mock close_database lock poisoned".into()))?;
        // Schema-aware engines model a real per-database pool; others have none.
        Ok(self.opts.has_schema_level)
    }

    async fn open_databases(&self, _handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Ok(self.opts.open_databases.clone())
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
        match &self.opts.disconnect_error {
            Some(message) => Err(DriverError::ConnectionFailed(message.clone())),
            None => Ok(()),
        }
    }

    async fn get_databases(&self, _handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Ok(self.opts.databases.clone())
    }

    async fn has_complete_foreign_key_catalog_visibility(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<bool, DriverError> {
        Ok(self.opts.complete_foreign_key_catalog_visibility)
    }

    async fn get_tables(
        &self,
        _handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::AnySchema)?;
        if !self.opts.tables_by_database.is_empty() {
            return Ok(self
                .table_lists_by_database
                .lock()
                .ok()
                .and_then(|tables| tables.get(database).cloned())
                .unwrap_or_default());
        }
        Ok(self.opts.tables.clone())
    }

    async fn get_table_schema(
        &self,
        _handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::ExactSchema)?;
        self.get_schema_calls.fetch_add(1, Ordering::Relaxed);
        let per_database_schema = self
            .table_schemas_by_database
            .lock()
            .ok()
            .and_then(|schemas| {
                schemas
                    .get(database)
                    .map(|tables| tables.get(table).cloned())
            });
        if let Some(schema) = per_database_schema {
            return Ok(schema.unwrap_or_else(|| TableSchema {
                table_name: table.to_string(),
                columns: Vec::new(),
                primary_keys: Vec::new(),
                indexes: Vec::new(),
                foreign_keys: Vec::new(),
                check_constraints: Vec::new(),
                table_options: Default::default(),
            }));
        }
        if let Some(columns) = self.columns_for_database(database, table) {
            let primary_keys = columns
                .iter()
                .filter(|c| c.is_primary_key)
                .map(|c| c.name.clone())
                .collect();
            return Ok(TableSchema {
                table_name: table.to_string(),
                columns,
                primary_keys,
                indexes: Vec::new(),
                foreign_keys: Vec::new(),
                check_constraints: Vec::new(),
                table_options: Default::default(),
            });
        }
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
        database: &str,
        schema: Option<&str>,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::ExactSchema)?;
        self.get_columns_calls.fetch_add(1, Ordering::Relaxed);
        if let Some(message) = self.opts.column_errors_by_table.get(table) {
            return Err(DriverError::QueryFailed(message.clone()));
        }
        if let Some(columns) = self.columns_for_database(database, table) {
            let primary_keys = columns
                .iter()
                .filter(|c| c.is_primary_key)
                .map(|c| c.name.clone())
                .collect();
            return Ok((columns, primary_keys));
        }
        if !self.opts.columns.is_empty() {
            return Ok((self.opts.columns.clone(), self.opts.primary_keys.clone()));
        }
        let schema = self
            .get_table_schema(handle, table, database, schema)
            .await?;
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
        // Data Transfer source-structure preflight queries are catalog probes,
        // not the configured full-column-type fixture rows below. In the
        // ordinary mock case no unsupported source objects are present.
        if sql.contains("AS unsupported_object") {
            return Ok(QueryResult {
                columns: vec![ColumnInfo {
                    name: "unsupported_object".into(),
                    data_type: "text".into(),
                    nullable: false,
                }],
                rows: Vec::new(),
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
        params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        let mut result = self.query(handle, sql).await?;
        if self.opts.empty_keyset_after_cursor && sql.contains(" > ") && !params.is_empty() {
            result.rows.clear();
        }
        if self.opts.filter_rows_by_key_equality
            && !params.is_empty()
            && sql.contains(" = ")
            && !sql.contains(" > ")
        {
            result.rows.retain(|row| {
                params.iter().enumerate().all(|(index, wanted)| {
                    row.get(index)
                        .and_then(Option::as_ref)
                        .is_some_and(|cell| value_matches_param(cell, wanted))
                })
            });
        }
        Ok(result)
    }

    fn parameter_placeholder(
        &self,
        index: usize,
        _data_type: Option<&str>,
    ) -> Result<String, DriverError> {
        if self.opts.parameterized_writes {
            Ok(format!("?{index}"))
        } else {
            Err(DriverError::Unsupported(
                "parameterized migration writes are not supported".into(),
            ))
        }
    }

    async fn execute_with_params(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
        _params: &[Value],
    ) -> Result<u64, DriverError> {
        if let Some(error) = self.opts.execute_with_params_error.as_ref() {
            return Err(DriverError::QueryFailed(error.clone()));
        }
        if self.opts.parameterized_writes {
            Ok(self.opts.execute_rows_affected)
        } else {
            Err(DriverError::Unsupported(
                "parameterized migration writes are not supported".into(),
            ))
        }
    }

    async fn execute(&self, _handle: &ConnectionHandle, _sql: &str) -> Result<u64, DriverError> {
        self.execute_calls.fetch_add(1, Ordering::Relaxed);
        if let Some(error) = self.opts.execute_error.as_ref() {
            return Err(DriverError::QueryFailed(error.clone()));
        }
        Ok(self.opts.execute_rows_affected)
    }

    async fn explain(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        Ok(self.opts.explain_plan.clone())
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        self.cancel_query_calls.fetch_add(1, Ordering::Relaxed);
        if let Some(message) = &self.opts.cancel_error {
            return Err(DriverError::Unsupported(message.clone()));
        }
        Ok(())
    }

    async fn cancel_query_with_execution(
        &self,
        _handle: &ConnectionHandle,
        _execution_id: &crate::QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.precise_cancel_query_calls
            .fetch_add(1, Ordering::Relaxed);
        if let Some(message) = &self.opts.cancel_error {
            return Err(DriverError::Unsupported(message.clone()));
        }
        Ok(())
    }

    fn supports_query_execution_cancel(&self) -> bool {
        true
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
        let tx = {
            let mut txs = self.open_txs.lock().expect("mock open_txs");
            if !txs.insert(handle.id.clone()) {
                return Err(DriverError::TransactionError(
                    "A transaction is already open on this connection".into(),
                ));
            }
            TransactionHandle {
                id: format!("mock_tx_{}", handle.id),
                connection_id: handle.id.clone(),
            }
        };
        // Losing the race to a competing begin, if one is armed: publish its
        // handle into the caller's bookkeeping *before* returning, so a caller
        // that re-reads that bookkeeping after awaiting us always loses.
        if let Some(race) = &self.opts.begin_race {
            let winner = race.winner.lock().expect("mock begin_race winner").take();
            if let Some(winner) = winner {
                race.slot.lock().await.insert(handle.id.clone(), winner);
            }
        }
        Ok(tx)
    }

    async fn begin_read_snapshot(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        self.begin_transaction(handle).await
    }

    async fn commit(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        let call = self.commit_calls.fetch_add(1, Ordering::Relaxed) + 1;
        let injected_error = self.opts.commit_error_on_call == Some(call);
        if injected_error && !self.opts.commit_error_after_effect {
            return Err(DriverError::TransactionError(
                "injected commit acknowledgement loss before effect".into(),
            ));
        }
        let mut txs = self.open_txs.lock().expect("mock open_txs");
        if !txs.remove(&tx.connection_id) {
            return Err(DriverError::TransactionError(
                "Transaction not found or already ended".into(),
            ));
        }
        if injected_error {
            return Err(DriverError::TransactionError(
                "injected commit acknowledgement loss after effect".into(),
            ));
        }
        Ok(())
    }

    async fn rollback(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        let call = self.rollback_calls.fetch_add(1, Ordering::Relaxed) + 1;
        if self.opts.rollback_error_on_call == Some(call) {
            return Err(DriverError::TransactionError(
                self.opts
                    .rollback_error
                    .clone()
                    .unwrap_or_else(|| "injected rollback failure".into()),
            ));
        }
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
        definitions.extend(schema_catalog_command_definitions());
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
        if let Some(data) = self.opts.command_results.get(command) {
            return Ok(CommandResult::new(data.clone()));
        }
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
        if let Some(result) =
            try_execute_schema_catalog_command(self, handle, command, input.clone()).await?
        {
            return Ok(result);
        }
        if is_schema_object_command(command) {
            return execute_schema_object_command(self, &self.db_type, handle, command, input)
                .await;
        }
        execute_standard_sql_command(self, handle, command, input).await
    }
}

/// [`Value`] deliberately has no `PartialEq`, so binding a param in the mock is
/// a variant-by-variant comparison. A cell of another variant is a mismatch:
/// the fixture rows are literal values, not something to coerce.
fn value_matches_param(cell: &Value, wanted: &Value) -> bool {
    match (cell, wanted) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(cell), Value::Bool(wanted)) => cell == wanted,
        (Value::Integer(cell), Value::Integer(wanted)) => cell == wanted,
        (Value::Float(cell), Value::Float(wanted)) => cell == wanted,
        (Value::String(cell), Value::String(wanted)) => cell == wanted,
        (Value::Bytes(cell), Value::Bytes(wanted)) => cell == wanted,
        (Value::Timestamp(cell), Value::Timestamp(wanted)) => cell == wanted,
        (Value::Json(cell), Value::Json(wanted)) => cell == wanted,
        _ => false,
    }
}

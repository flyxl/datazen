//! Generic execution journeys: bound values, projection, rollback and cancellation.
use super::execute::{
    execute_same_family_data, execute_transfer_data_with_write_observer, map_row_values,
    ValueFormatter,
};
use super::filter::SourceFilter;
use super::model::*;
use async_trait::async_trait;
use datazen_driver_api::*;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

type Rows = Vec<Vec<Option<Value>>>;
#[derive(Default)]
struct State {
    committed: Vec<Vec<Value>>,
    pending: Vec<Vec<Value>>,
    calls: usize,
    execute_calls: usize,
    begin_calls: usize,
    schema_calls: usize,
    rollback: usize,
    metadata_refs: Vec<String>,
    source_queries: Vec<(String, Vec<Value>)>,
    identity_sync_calls: Vec<(Option<String>, String, Vec<String>)>,
    identity_insert_calls: Vec<(String, Option<String>, String, bool)>,
    discard_connection_calls: usize,
    transfer_order: Vec<&'static str>,
    write_sqls: Vec<String>,
}
struct Driver {
    rows: Rows,
    schema: TableSchema,
    state: Mutex<State>,
    fail_at: Option<usize>,
    /// Inject a lost commit acknowledgement. `Some(true)` applies pending
    /// rows before returning the error; `Some(false)` leaves them unapplied.
    commit_error_after_effect: Option<bool>,
    rollback_fails: bool,
    begin_error_on_call: Option<usize>,
    schema_error_on_call: Option<usize>,
    execute_error_on_call: Option<usize>,
    identity_sync_error: bool,
    session_identity_insert: bool,
    identity_insert_on_error: bool,
    identity_insert_off_error: bool,
    discard_connection_error: bool,
    affected_override: Option<u64>,
    include_identity_insert_clause: bool,
    stream_mode: u8,
    cancel: Option<Arc<AtomicBool>>,
}
fn unsupported<T>() -> Result<T, DriverError> {
    Err(DriverError::Unsupported("unused in test".into()))
}
#[async_trait]
impl DatabaseDriver for Driver {
    async fn cancel_query(&self, _: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
    fn driver_type(&self) -> String {
        "fixture".into()
    }
    fn parameter_placeholder(&self, _: usize, _: Option<&str>) -> Result<String, DriverError> {
        Ok("?".into())
    }
    async fn connect(&self, _: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        unsupported()
    }
    async fn disconnect(&self, _: ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
    async fn test_connection(&self, _: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        unsupported()
    }
    async fn get_databases(&self, _: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        unsupported()
    }
    async fn get_tables(
        &self,
        _: &ConnectionHandle,
        _: &str,
        _: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        unsupported()
    }
    async fn get_table_schema(
        &self,
        _: &ConnectionHandle,
        relation: &str,
        _: &str,
        schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        let mut state = self.state.lock().unwrap();
        state.schema_calls += 1;
        if self.schema_error_on_call == Some(state.schema_calls) {
            return Err(DriverError::QueryFailed(
                "injected target schema inspection failure".into(),
            ));
        }
        state.metadata_refs.push(match schema {
            Some(schema) => format!("{schema}.{relation}"),
            None => relation.to_string(),
        });
        Ok(self.schema.clone())
    }
    async fn query(&self, _: &ConnectionHandle, _: &str) -> Result<QueryResult, DriverError> {
        unsupported()
    }
    async fn query_multi(
        &self,
        _: &ConnectionHandle,
        _: &str,
        _: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        unsupported()
    }
    async fn query_with_params(
        &self,
        _: &ConnectionHandle,
        _: &str,
        _: &[Value],
    ) -> Result<QueryResult, DriverError> {
        unsupported()
    }
    async fn execute(&self, _: &ConnectionHandle, _: &str) -> Result<u64, DriverError> {
        let mut state = self.state.lock().unwrap();
        state.execute_calls += 1;
        if self.execute_error_on_call == Some(state.execute_calls) {
            return Err(DriverError::QueryFailed(
                "injected target DDL response loss".into(),
            ));
        }
        Ok(0)
    }
    async fn query_stream(
        &self,
        _: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        callback: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        assert!(!sql.contains("OFFSET"));
        assert_eq!(limit, None);
        let projection = sql
            .strip_prefix("SELECT ")
            .unwrap()
            .split(" FROM ")
            .next()
            .unwrap();
        let mut columns: Vec<ColumnInfo> = projection
            .split(", ")
            .map(|name| ColumnInfo {
                name: name.trim_matches('"').into(),
                data_type: "TEXT".into(),
                nullable: true,
            })
            .collect();
        if self.stream_mode == 1 {
            columns[0].name = "unexpected_column".into();
        }
        callback(QueryStreamEvent::StatementStart {
            index: 0,
            sql: sql.into(),
            columns,
        });
        let mut rows = self.rows.clone();
        if self.stream_mode == 4 {
            rows[0].push(None);
        }
        callback(QueryStreamEvent::Rows { index: 0, rows });
        if self.stream_mode == 2 {
            return Ok(());
        }
        callback(QueryStreamEvent::StatementEnd {
            index: 0,
            rows_affected: None,
            execution_time_ms: 0,
            truncated: self.stream_mode == 3,
        });
        if self.stream_mode == 5 {
            callback(QueryStreamEvent::StatementEnd {
                index: 0,
                rows_affected: None,
                execution_time_ms: 0,
                truncated: false,
            });
        }
        Ok(())
    }
    async fn query_stream_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
        limit: Option<u32>,
        callback: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        self.state
            .lock()
            .unwrap()
            .source_queries
            .push((sql.to_string(), params.to_vec()));
        self.query_stream(handle, sql, limit, callback).await
    }
    async fn begin_transaction(
        &self,
        _: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        let mut state = self.state.lock().unwrap();
        state.begin_calls += 1;
        if self.begin_error_on_call == Some(state.begin_calls) {
            return Err(DriverError::TransactionError(
                "injected begin transaction failure".into(),
            ));
        }
        Ok(TransactionHandle {
            id: "tx".into(),
            connection_id: "target".into(),
        })
    }
    async fn advance_transfer_identity_sequences(
        &self,
        _: &ConnectionHandle,
        schema: Option<&str>,
        table: &str,
        columns: &[String],
    ) -> Result<(), DriverError> {
        let mut state = self.state.lock().unwrap();
        state.transfer_order.push("sync");
        state.identity_sync_calls.push((
            schema.map(str::to_string),
            table.to_string(),
            columns.to_vec(),
        ));
        if self.identity_sync_error {
            return Err(DriverError::QueryFailed(
                "injected identity sequence synchronization failure".into(),
            ));
        }
        Ok(())
    }
    fn explicit_identity_insert_requires_session_toggle(&self) -> bool {
        self.session_identity_insert
    }
    async fn set_identity_insert(
        &self,
        _: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
        table: &str,
        enabled: bool,
    ) -> Result<(), DriverError> {
        let mut state = self.state.lock().unwrap();
        state.identity_insert_calls.push((
            database.to_string(),
            schema.map(str::to_string),
            table.to_string(),
            enabled,
        ));
        state.transfer_order.push(if enabled {
            "identity_on"
        } else {
            "identity_off"
        });
        let fail = if enabled {
            self.identity_insert_on_error
        } else {
            self.identity_insert_off_error
        };
        if fail {
            return Err(DriverError::QueryFailed(if enabled {
                "injected IDENTITY_INSERT ON failure".into()
            } else {
                "injected IDENTITY_INSERT OFF failure".into()
            }));
        }
        Ok(())
    }
    async fn discard_connection(&self, _: &ConnectionHandle) -> Result<(), DriverError> {
        let mut state = self.state.lock().unwrap();
        state.discard_connection_calls += 1;
        state.transfer_order.push("discard");
        if self.discard_connection_error {
            return Err(DriverError::ConnectionFailed(
                "injected transfer connection discard failure".into(),
            ));
        }
        Ok(())
    }
    fn transfer_explicit_identity_insert_clause(&self) -> Option<&'static str> {
        self.include_identity_insert_clause
            .then_some("OVERRIDING SYSTEM VALUE")
    }
    async fn execute_with_params(
        &self,
        _: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<u64, DriverError> {
        assert!(sql.contains("VALUES (?"));
        let mut state = self.state.lock().unwrap();
        state.transfer_order.push("write");
        state.write_sqls.push(sql.to_string());
        state.calls += 1;
        if self.fail_at == Some(state.calls) {
            return Err(DriverError::QueryFailed("injected write failure".into()));
        }
        state.pending.push(params.to_vec());
        if let Some(flag) = &self.cancel {
            flag.store(true, Ordering::SeqCst);
        }
        let affected = sql
            .split_once("VALUES ")
            .map(|(_, values)| values.matches('(').count() as u64)
            .unwrap_or(1);
        Ok(self.affected_override.unwrap_or(affected))
    }
    async fn commit(&self, _: TransactionHandle) -> Result<(), DriverError> {
        let mut state = self.state.lock().unwrap();
        state.transfer_order.push("commit");
        match self.commit_error_after_effect {
            Some(true) => {
                let pending = std::mem::take(&mut state.pending);
                state.committed.extend(pending);
                return Err(DriverError::TransactionError(
                    "injected lost commit acknowledgement after effect".into(),
                ));
            }
            Some(false) => {
                return Err(DriverError::TransactionError(
                    "injected lost commit acknowledgement before effect".into(),
                ));
            }
            None => {}
        }
        let pending = std::mem::take(&mut state.pending);
        state.committed.extend(pending);
        Ok(())
    }
    async fn rollback(&self, _: TransactionHandle) -> Result<(), DriverError> {
        let mut state = self.state.lock().unwrap();
        state.rollback += 1;
        state.transfer_order.push("rollback");
        if self.rollback_fails {
            return Err(DriverError::TransactionError(
                "injected rollback failure".into(),
            ));
        }
        state.pending.clear();
        Ok(())
    }
}
fn schema(names: &[&str]) -> TableSchema {
    TableSchema {
        table_name: "t".into(),
        columns: names
            .iter()
            .map(|name| ColumnSchema {
                name: (*name).into(),
                data_type: "TEXT".into(),
                nullable: true,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            })
            .collect(),
        primary_keys: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    }
}
fn mapping(source: &str, target: &str) -> ColumnMapping {
    ColumnMapping {
        source_column: source.into(),
        target_column: target.into(),
        skip: false,
        target_native_type: None,
    }
}
fn driver(rows: Rows, schema: TableSchema) -> Driver {
    Driver {
        rows,
        schema,
        state: Mutex::new(State::default()),
        fail_at: None,
        commit_error_after_effect: None,
        rollback_fails: false,
        begin_error_on_call: None,
        schema_error_on_call: None,
        execute_error_on_call: None,
        identity_sync_error: false,
        session_identity_insert: false,
        identity_insert_on_error: false,
        identity_insert_off_error: false,
        discard_connection_error: false,
        affected_override: None,
        include_identity_insert_clause: false,
        stream_mode: 0,
        cancel: None,
    }
}

struct TestSourceAdapter;
impl crate::transfer::adapter::SyncSourceAdapter for TestSourceAdapter {
    fn column_to_ir(
        &self,
        column: &ColumnSchema,
        _native_full_type: Option<&str>,
    ) -> crate::transfer::ir::IRColumn {
        crate::transfer::ir::IRColumn {
            name: column.name.clone(),
            ir_type: crate::transfer::ir::IRType::Text,
            nullable: column.nullable,
            default_expr: None,
            is_primary_key: false,
            is_auto_increment: false,
            comment: None,
        }
    }
}

struct TestTargetAdapter;
impl crate::transfer::adapter::SyncTargetAdapter for TestTargetAdapter {
    fn ir_type_to_native(&self, _: &crate::transfer::ir::IRType) -> String {
        "TEXT".into()
    }
    fn format_default(&self, _: &crate::transfer::ir::IRDefault) -> Option<String> {
        None
    }
    fn format_literal(&self, _: &Option<Value>, _: &crate::transfer::ir::IRType) -> String {
        "NULL".into()
    }
    fn transform_value(
        &self,
        value: &Option<Value>,
        _: &crate::transfer::ir::IRType,
    ) -> Option<Value> {
        value.clone()
    }
}
fn job() -> TransferJob {
    job_with_batch_size(1)
}
fn job_with_batch_size(batch_size: u32) -> TransferJob {
    TransferJob {
        source: Endpoint {
            db_session_id: "source".into(),
            database: "s".into(),
            schema: None,
        },
        target: Some(Endpoint {
            db_session_id: "target".into(),
            database: "t".into(),
            schema: None,
        }),
        sql_file_target: None,
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        tables: vec![],
        options: TransferOptions {
            batch_size,
            stop_on_error: false,
            confirmed_destructive: false,
            use_target_default_collation: false,
        },
    }
}
fn inspected(name: &str, mappings: Vec<ColumnMapping>) -> TableInspectResult {
    TableInspectResult {
        source_table: name.into(),
        target_table: name.into(),
        status: TableMappingStatus::Matched,
        create_new: false,
        enabled: true,
        column_mappings: mappings,
        source_primary_keys: vec![],
        source_columns: vec![],
        target_columns: vec![],
        source_column_types: HashMap::new(),
        target_column_types: HashMap::new(),
        incompatible_reason: None,
        source_row_count: None,
        recordset: None,
    }
}
async fn run(
    source: &Driver,
    target: &Driver,
    tables: &[TableInspectResult],
    cancel: Option<Arc<AtomicBool>>,
) -> TransferExecutionResult {
    run_with_batch_size(source, target, tables, cancel, 1).await
}
async fn run_with_batch_size(
    source: &Driver,
    target: &Driver,
    tables: &[TableInspectResult],
    cancel: Option<Arc<AtomicBool>>,
    batch_size: u32,
) -> TransferExecutionResult {
    run_with_options(source, target, tables, cancel, batch_size, false).await
}
async fn run_with_options(
    source: &Driver,
    target: &Driver,
    tables: &[TableInspectResult],
    cancel: Option<Arc<AtomicBool>>,
    batch_size: u32,
    stop_on_error: bool,
) -> TransferExecutionResult {
    let schemas = tables
        .iter()
        .map(|table| (table.source_table.clone(), source.schema.clone()))
        .collect();
    let mut transfer_job = job_with_batch_size(batch_size);
    transfer_job.options.stop_on_error = stop_on_error;
    execute_same_family_data(
        source,
        &ConnectionHandle {
            id: "source".into(),
            pool_id: "source".into(),
        },
        target,
        &ConnectionHandle {
            id: "target".into(),
            pool_id: "target".into(),
        },
        &transfer_job,
        tables,
        &schemas,
        false,
        cancel,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn batched_insert_reports_all_affected_rows_with_one_target_call() {
    let source = driver(
        vec![vec![Some(Value::Integer(1))], vec![Some(Value::Integer(2))]],
        schema(&["id"]),
    );
    let target = driver(vec![], schema(&["id"]));
    let result = run_with_batch_size(
        &source,
        &target,
        &[inspected("a", vec![mapping("id", "id")])],
        None,
        500,
    )
    .await;

    assert!(!result.partial);
    assert_eq!(result.rows_inserted, 2);
    let state = target.state.lock().unwrap();
    assert_eq!(state.calls, 1);
    assert_eq!(state.committed.len(), 1);
    assert_eq!(state.committed[0].len(), 2);
}

#[tokio::test]
async fn successful_explicit_identity_import_synchronizes_before_commit() {
    let source = driver(vec![vec![Some(Value::Integer(41))]], schema(&["id"]));
    let mut target_schema = schema(&["renamed_id"]);
    target_schema.columns[0].is_auto_increment = true;
    let target = driver(vec![], target_schema);
    let result = run(
        &source,
        &target,
        &[inspected("source_table", vec![mapping("id", "renamed_id")])],
        None,
    )
    .await;

    assert!(!result.partial);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::Committed)
    );
    let state = target.state.lock().unwrap();
    assert_eq!(
        state.identity_sync_calls,
        vec![(None, "source_table".into(), vec!["renamed_id".into()])]
    );
    assert_eq!(state.committed.len(), 1);
    assert_eq!(state.rollback, 0);
    assert_eq!(state.transfer_order, vec!["write", "sync", "commit"]);
}

#[tokio::test]
async fn explicit_identity_import_synchronizes_when_driver_reports_zero_affected_rows() {
    let source = driver(vec![vec![Some(Value::Integer(41))]], schema(&["id"]));
    let mut target_schema = schema(&["id"]);
    target_schema.columns[0].is_auto_increment = true;
    let mut target = driver(vec![], target_schema);
    target.include_identity_insert_clause = true;
    // Model a successful conflict-ignore batch: the INSERT was issued with
    // explicit IDs, but the driver reports no newly affected rows.
    target.affected_override = Some(0);
    let result = run(
        &source,
        &target,
        &[inspected("source_table", vec![mapping("id", "id")])],
        None,
    )
    .await;

    assert!(!result.partial);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::Committed)
    );
    let state = target.state.lock().unwrap();
    assert_eq!(state.identity_sync_calls.len(), 1);
    assert_eq!(state.transfer_order, vec!["write", "sync", "commit"]);
    assert!(state.write_sqls[0].contains("OVERRIDING SYSTEM VALUE"));
    assert_eq!(state.committed.len(), 1);
}

#[tokio::test]
async fn session_identity_insert_is_enabled_and_disabled_inside_the_transaction() {
    let source = driver(vec![vec![Some(Value::Integer(41))]], schema(&["id"]));
    let mut target_schema = schema(&["id"]);
    target_schema.columns[0].is_auto_increment = true;
    let mut target = driver(vec![], target_schema);
    target.session_identity_insert = true;
    let result = run(
        &source,
        &target,
        &[inspected("source_table", vec![mapping("id", "id")])],
        None,
    )
    .await;

    assert!(!result.partial);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::Committed)
    );
    let state = target.state.lock().unwrap();
    assert_eq!(
        state.identity_insert_calls,
        vec![
            ("t".into(), None, "source_table".into(), true),
            ("t".into(), None, "source_table".into(), false),
        ]
    );
    assert_eq!(
        state.transfer_order,
        vec!["identity_on", "write", "identity_off", "sync", "commit"]
    );
}

#[tokio::test]
async fn session_identity_insert_is_not_toggled_without_a_mapped_target_identity() {
    let source = driver(vec![vec![Some(Value::Integer(41))]], schema(&["id"]));
    let mut target = driver(vec![], schema(&["id"]));
    target.session_identity_insert = true;
    let result = run(
        &source,
        &target,
        &[inspected("source_table", vec![mapping("id", "id")])],
        None,
    )
    .await;

    assert!(!result.partial);
    assert!(target
        .state
        .lock()
        .unwrap()
        .identity_insert_calls
        .is_empty());
}

#[tokio::test]
async fn failed_identity_insert_on_still_attempts_off_then_rolls_back() {
    let source = driver(vec![vec![Some(Value::Integer(41))]], schema(&["id"]));
    let mut target_schema = schema(&["id"]);
    target_schema.columns[0].is_auto_increment = true;
    let mut target = driver(vec![], target_schema);
    target.session_identity_insert = true;
    target.identity_insert_on_error = true;
    let result = run(
        &source,
        &target,
        &[inspected("source_table", vec![mapping("id", "id")])],
        None,
    )
    .await;

    assert!(result.partial);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::RolledBack)
    );
    let state = target.state.lock().unwrap();
    assert_eq!(state.calls, 0);
    assert_eq!(state.rollback, 1);
    assert_eq!(state.discard_connection_calls, 0);
    assert_eq!(
        state.transfer_order,
        vec!["identity_on", "identity_off", "rollback"]
    );
}

#[tokio::test]
async fn failed_identity_insert_off_rolls_back_before_discarding_the_connection() {
    let source = driver(vec![vec![Some(Value::Integer(41))]], schema(&["id"]));
    let mut target_schema = schema(&["id"]);
    target_schema.columns[0].is_auto_increment = true;
    let mut target = driver(vec![], target_schema);
    target.session_identity_insert = true;
    target.identity_insert_off_error = true;
    let result = run(
        &source,
        &target,
        &[inspected("source_table", vec![mapping("id", "id")])],
        None,
    )
    .await;

    assert!(result.partial);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::RolledBack)
    );
    let state = target.state.lock().unwrap();
    assert_eq!(state.rollback, 1);
    assert_eq!(state.discard_connection_calls, 1);
    assert_eq!(state.committed.len(), 0);
    assert_eq!(
        state.transfer_order,
        vec![
            "identity_on",
            "write",
            "identity_off",
            "rollback",
            "discard"
        ]
    );
}

#[tokio::test]
async fn failed_identity_insert_connection_discard_marks_outcome_unknown() {
    let source = driver(vec![vec![Some(Value::Integer(41))]], schema(&["id"]));
    let mut target_schema = schema(&["id"]);
    target_schema.columns[0].is_auto_increment = true;
    let mut target = driver(vec![], target_schema);
    target.session_identity_insert = true;
    target.identity_insert_off_error = true;
    target.discard_connection_error = true;
    let result = run(
        &source,
        &target,
        &[inspected("source_table", vec![mapping("id", "id")])],
        None,
    )
    .await;

    assert!(result.partial);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::Unknown)
    );
    assert!(result.tables[0]
        .error
        .as_deref()
        .is_some_and(|error| error.contains("failed to discard target connection")));
    let state = target.state.lock().unwrap();
    assert_eq!(state.rollback, 1);
    assert_eq!(state.discard_connection_calls, 1);
    assert_eq!(state.transfer_order.last(), Some(&"discard"));
}

#[tokio::test]
async fn failed_identity_insert_off_still_discards_after_rollback_failure() {
    let source = driver(vec![vec![Some(Value::Integer(41))]], schema(&["id"]));
    let mut target_schema = schema(&["id"]);
    target_schema.columns[0].is_auto_increment = true;
    let mut target = driver(vec![], target_schema);
    target.session_identity_insert = true;
    target.identity_insert_off_error = true;
    target.rollback_fails = true;
    let result = run(
        &source,
        &target,
        &[inspected("source_table", vec![mapping("id", "id")])],
        None,
    )
    .await;

    assert!(result.partial);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::Unknown)
    );
    let state = target.state.lock().unwrap();
    assert_eq!(state.rollback, 1);
    assert_eq!(state.discard_connection_calls, 1);
    assert_eq!(
        state.transfer_order,
        vec![
            "identity_on",
            "write",
            "identity_off",
            "rollback",
            "discard"
        ]
    );
}

#[tokio::test]
async fn identity_override_clause_requires_a_mapped_target_identity_column() {
    let source = driver(vec![vec![Some(Value::Integer(41))]], schema(&["id"]));
    let target = driver(vec![], schema(&["id"]));
    let mut target = target;
    target.include_identity_insert_clause = true;
    let result = run(
        &source,
        &target,
        &[inspected("source_table", vec![mapping("id", "id")])],
        None,
    )
    .await;

    assert!(!result.partial);
    let state = target.state.lock().unwrap();
    assert!(!state.write_sqls[0].contains("OVERRIDING SYSTEM VALUE"));
}

#[tokio::test]
async fn identity_sync_failure_rolls_back_the_successful_row_batches() {
    let source = driver(vec![vec![Some(Value::Integer(41))]], schema(&["id"]));
    let mut target_schema = schema(&["id"]);
    target_schema.columns[0].is_auto_increment = true;
    let mut target = driver(vec![], target_schema);
    target.identity_sync_error = true;
    let result = run(
        &source,
        &target,
        &[inspected("source_table", vec![mapping("id", "id")])],
        None,
    )
    .await;

    assert!(result.partial);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::RolledBack)
    );
    let state = target.state.lock().unwrap();
    assert_eq!(state.identity_sync_calls.len(), 1);
    assert_eq!(state.rollback, 1);
    assert!(state.pending.is_empty());
    assert!(state.committed.is_empty());
}

#[tokio::test]
async fn failed_identity_import_never_synchronizes_sequence() {
    let source = driver(
        vec![
            vec![Some(Value::Integer(31))],
            vec![Some(Value::Integer(41))],
        ],
        schema(&["id"]),
    );
    let mut target_schema = schema(&["id"]);
    target_schema.columns[0].is_auto_increment = true;
    let mut target = driver(vec![], target_schema);
    target.fail_at = Some(1);
    let result = run(
        &source,
        &target,
        &[inspected("source_table", vec![mapping("id", "id")])],
        None,
    )
    .await;

    assert!(result.partial);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::RolledBack)
    );
    let state = target.state.lock().unwrap();
    assert!(state.identity_sync_calls.is_empty());
    assert_eq!(state.rollback, 1);
    assert!(state.committed.is_empty());
}

#[test]
fn projected_rows_keep_skips_reorder_and_subsets() {
    let schema = schema(&["id", "name", "age"]);
    for selected in [
        vec!["name", "age"],
        vec!["id", "age"],
        vec!["id", "name"],
        vec!["age", "name"],
        vec!["name"],
    ] {
        let mappings: Vec<_> = selected.iter().map(|name| mapping(name, name)).collect();
        let refs = mappings.iter().collect::<Vec<_>>();
        let row = selected
            .iter()
            .map(|name| Some(Value::String((*name).into())))
            .collect::<Vec<_>>();
        assert_eq!(
            serde_json::to_value(map_row_values(&row, &schema, &refs).unwrap()).unwrap(),
            serde_json::to_value(&row).unwrap()
        );
        assert!(map_row_values(&[], &schema, &refs).is_err());
    }
}

#[test]
fn projected_text_bytes_decode_as_utf8_while_binary_bytes_remain_bytes() {
    let mut source_schema = schema(&["tenant", "payload"]);
    source_schema.columns[0].data_type = "VARCHAR(64)".into();
    source_schema.columns[1].data_type = "BLOB".into();
    let mappings = [mapping("tenant", "tenant"), mapping("payload", "payload")];
    let refs = mappings.iter().collect::<Vec<_>>();
    let row = vec![
        Some(Value::Bytes("a-雪".as_bytes().to_vec())),
        Some(Value::Bytes(vec![0, 255])),
    ];

    let projected = map_row_values(&row, &source_schema, &refs).unwrap();
    assert!(matches!(&projected[0], Some(Value::String(value)) if value == "a-雪"));
    assert!(matches!(&projected[1], Some(Value::Bytes(value)) if value == &[0, 255]));

    let invalid = vec![
        Some(Value::Bytes(vec![0xff])),
        Some(Value::Bytes(vec![0, 255])),
    ];
    assert!(map_row_values(&invalid, &source_schema, &refs)
        .unwrap_err()
        .to_string()
        .contains("not valid UTF-8"));
}

#[tokio::test]
async fn bytes_and_strings_survive_bound_projection_and_commit() {
    let row = vec![
        Some(Value::Bytes((0..=255).collect())),
        Some(Value::String("'\\\n\0unicode雪".into())),
    ];
    let mut source_schema = schema(&["skipped", "blob", "text"]);
    source_schema.columns[1].data_type = "BLOB".into();
    let source = driver(vec![row.clone()], source_schema);
    let target = driver(vec![], schema(&["payload", "label"]));
    let result = run(
        &source,
        &target,
        &[inspected(
            "a",
            vec![mapping("blob", "payload"), mapping("text", "label")],
        )],
        None,
    )
    .await;
    assert_eq!(result.rows_inserted, 1);
    assert!(!result.partial);
    assert_eq!(
        serde_json::to_value(&target.state.lock().unwrap().committed[0]).unwrap(),
        serde_json::to_value(row.into_iter().map(Option::unwrap).collect::<Vec<_>>()).unwrap()
    );
}

#[tokio::test]
async fn recordset_scope_shares_filter_order_and_bound_parameter_order() {
    let mut source_schema = schema(&["id", "status"]);
    source_schema.columns[0].data_type = "INTEGER".into();
    let source = driver(vec![vec![Some(Value::Integer(2))]], source_schema);
    let target = driver(vec![], schema(&["id"]));
    let mut job = job();
    job.tables = vec![TableMapping {
        source_table: "a".into(),
        target_table: "a".into(),
        create_new: false,
        enabled: true,
        column_mappings: vec![mapping("id", "id")],
        ddl_override: None,
        source_filter: Some(SourceFilter(serde_json::json!({
            "filters": [{"column": "status", "operator": "eq", "value": "active"}],
            "logic": "and"
        }))),
        recordset: Some(TransferRecordset {
            order_by: Some("id".into()),
            start: Some(TransferRecordsetBound {
                value: serde_json::json!(2),
                inclusive: true,
            }),
            end: Some(TransferRecordsetBound {
                value: serde_json::json!(5),
                inclusive: false,
            }),
            tuple_range: None,
            limit: Some(2),
        }),
    }];
    let tables = vec![inspected("a", vec![mapping("id", "id")])];
    let schemas = HashMap::from([("a".into(), source.schema.clone())]);
    let result = execute_same_family_data(
        &source,
        &ConnectionHandle {
            id: "source".into(),
            pool_id: "source".into(),
        },
        &target,
        &ConnectionHandle {
            id: "target".into(),
            pool_id: "target".into(),
        },
        &job,
        &tables,
        &schemas,
        false,
        None,
    )
    .await
    .unwrap();
    assert_eq!(result.rows_inserted, 1);
    let state = source.state.lock().unwrap();
    let (sql, params) = &state.source_queries[0];
    assert!(sql.contains(r#"WHERE ("status" = ?) AND ("id" >= ?) AND ("id" < ?)"#));
    assert!(sql.contains(r#"ORDER BY "id" ASC LIMIT ?"#));
    assert!(matches!(params.as_slice(), [
        Value::String(status),
        Value::Integer(2),
        Value::Integer(5),
        Value::Integer(2)
    ] if status == "active"));
}
#[tokio::test]
async fn second_batch_failure_rolls_back_and_continues_next_table() {
    let source = driver(
        vec![vec![Some(Value::Integer(1))], vec![Some(Value::Integer(2))]],
        schema(&["id"]),
    );
    let mut target = driver(vec![], schema(&["id"]));
    target.fail_at = Some(2);
    let result = run(
        &source,
        &target,
        &[
            inspected("a", vec![mapping("id", "id")]),
            inspected("b", vec![mapping("id", "id")]),
        ],
        None,
    )
    .await;
    assert!(result.partial);
    assert_eq!(result.tables.len(), 2);
    assert!(!result.tables[0].success);
    assert_eq!(result.tables[0].rows_inserted, Some(0));
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::RolledBack)
    );
    assert!(result.tables[1].success);
    assert_eq!(result.rows_inserted, 2);
    let state = target.state.lock().unwrap();
    assert_eq!(state.rollback, 1);
    assert_eq!(state.committed.len(), 2);
}

#[tokio::test]
async fn unknown_commit_stops_later_tables_and_hides_unconfirmed_rows() {
    for applied_before_ack_loss in [false, true] {
        let source = driver(vec![vec![Some(Value::Integer(1))]], schema(&["id"]));
        let mut target = driver(vec![], schema(&["id"]));
        target.commit_error_after_effect = Some(applied_before_ack_loss);
        let result = run(
            &source,
            &target,
            &[
                inspected("a", vec![mapping("id", "id")]),
                inspected("b", vec![mapping("id", "id")]),
            ],
            None,
        )
        .await;

        assert!(result.partial);
        assert_eq!(result.rows_inserted, 0);
        assert_eq!(result.tables.len(), 2);
        assert_eq!(
            result.tables[0].outcome,
            Some(TableExecutionOutcome::Unknown)
        );
        assert_eq!(result.tables[0].rows_inserted, None);
        assert_eq!(
            result.tables[1].outcome,
            Some(TableExecutionOutcome::NotStarted)
        );
        assert_eq!(result.tables[1].rows_inserted, Some(0));
        let state = target.state.lock().unwrap();
        assert_eq!(
            state.calls, 1,
            "the later table must not reach target writes"
        );
        assert_eq!(state.committed.len(), usize::from(applied_before_ack_loss));
    }
}

#[tokio::test]
async fn debug_commit_ack_loss_seam_targets_one_successful_commit_once() {
    const ACK_LOSS_TABLE: &str = "e2e_ack_loss_target_once";

    let _guard = crate::TEST_COMMIT_ACK_LOSS_TEST_LOCK
        .lock()
        .await;
    let source = driver(vec![vec![Some(Value::Integer(1))]], schema(&["id"]));
    let target = driver(vec![], schema(&["id"]));
    super::execute::arm_test_commit_ack_loss(ACK_LOSS_TABLE).unwrap();

    let result = run(
        &source,
        &target,
        &[
            inspected("e2e_ack_loss_before", vec![mapping("id", "id")]),
            inspected(ACK_LOSS_TABLE, vec![mapping("id", "id")]),
            inspected("e2e_ack_loss_later", vec![mapping("id", "id")]),
        ],
        None,
    )
    .await;

    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::Committed)
    );
    assert_eq!(
        result.tables[1].outcome,
        Some(TableExecutionOutcome::Unknown)
    );
    assert_eq!(result.tables[1].rows_inserted, None);
    assert_eq!(
        result.tables[2].outcome,
        Some(TableExecutionOutcome::NotStarted)
    );
    assert_eq!(target.state.lock().unwrap().committed.len(), 2);
    assert!(
        !super::execute::clear_test_commit_ack_loss(),
        "the target-specific fault must be consumed once"
    );

    let repeated_target_commit = run(
        &source,
        &target,
        &[inspected(ACK_LOSS_TABLE, vec![mapping("id", "id")])],
        None,
    )
    .await;
    assert_eq!(
        repeated_target_commit.tables[0].outcome,
        Some(TableExecutionOutcome::Committed),
        "the fault must not affect another commit after its one-shot use"
    );
}

#[tokio::test]
async fn unknown_rollback_stops_later_tables_and_hides_unconfirmed_rows() {
    let source = driver(vec![vec![Some(Value::Integer(1))]], schema(&["id"]));
    let mut target = driver(vec![], schema(&["id"]));
    target.fail_at = Some(1);
    target.rollback_fails = true;
    let result = run(
        &source,
        &target,
        &[
            inspected("a", vec![mapping("id", "id")]),
            inspected("b", vec![mapping("id", "id")]),
        ],
        None,
    )
    .await;

    assert!(result.partial);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::Unknown)
    );
    assert_eq!(result.tables[0].rows_inserted, None);
    assert_eq!(
        result.tables[1].outcome,
        Some(TableExecutionOutcome::NotStarted)
    );
    assert_eq!(target.state.lock().unwrap().calls, 1);
}

#[tokio::test]
async fn confirmed_rollback_stops_when_stop_on_error_is_enabled() {
    let source = driver(vec![vec![Some(Value::Integer(1))]], schema(&["id"]));
    let mut target = driver(vec![], schema(&["id"]));
    target.fail_at = Some(1);
    let result = run_with_options(
        &source,
        &target,
        &[
            inspected("a", vec![mapping("id", "id")]),
            inspected("b", vec![mapping("id", "id")]),
        ],
        None,
        1,
        true,
    )
    .await;

    assert!(result.partial);
    assert_eq!(result.tables.len(), 1);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::RolledBack)
    );
    assert_eq!(result.tables[0].rows_inserted, Some(0));
    assert_eq!(target.state.lock().unwrap().calls, 1);
}

#[tokio::test]
async fn preflight_failure_is_not_started_and_does_not_write_that_table() {
    let source = driver(vec![vec![Some(Value::Integer(1))]], schema(&["id"]));
    let target = driver(vec![], schema(&["id"]));
    let result = run(
        &source,
        &target,
        &[
            inspected("a", vec![]),
            inspected("b", vec![mapping("id", "id")]),
        ],
        None,
    )
    .await;

    assert!(result.partial);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::NotStarted)
    );
    assert_eq!(result.tables[0].rows_inserted, Some(0));
    assert_eq!(
        result.tables[1].outcome,
        Some(TableExecutionOutcome::Committed)
    );
    assert_eq!(target.state.lock().unwrap().calls, 1);
}

#[tokio::test]
async fn later_self_overwrite_is_rejected_before_earlier_table_can_write() {
    let source = driver(vec![vec![Some(Value::Integer(1))]], schema(&["id"]));
    let target = driver(vec![], schema(&["id"]));
    let mut valid_first = inspected("valid_first", vec![mapping("id", "id")]);
    valid_first.target_table = "target_first".into();
    let tables = [
        valid_first,
        inspected("shared_relation", vec![mapping("id", "id")]),
    ];
    let schemas = tables
        .iter()
        .map(|table| (table.source_table.clone(), source.schema.clone()))
        .collect::<HashMap<_, _>>();
    let mut transfer_job = job();
    transfer_job.source.db_session_id = "target".into();
    transfer_job.source.database = "same".into();
    let target_endpoint = transfer_job.target.as_mut().unwrap();
    target_endpoint.db_session_id = "target".into();
    target_endpoint.database = "same".into();
    let formatter = ValueFormatter::SameFamily;
    let write_started = AtomicBool::new(false);

    let result = execute_transfer_data_with_write_observer(
        &source,
        &ConnectionHandle {
            id: "target".into(),
            pool_id: "target".into(),
        },
        &target,
        &ConnectionHandle {
            id: "target".into(),
            pool_id: "target".into(),
        },
        &transfer_job,
        &tables,
        &schemas,
        &formatter,
        None,
        false,
        None,
        None,
        Some(&write_started),
    )
    .await;

    assert!(matches!(
        result,
        Err(super::TransferError::Validation(message)) if message.contains("shared_relation")
    ));
    assert!(!write_started.load(Ordering::SeqCst));
    let state = target.state.lock().unwrap();
    assert_eq!(state.calls, 0, "the valid first table must not write");
    assert_eq!(state.execute_calls, 0, "no destructive DDL may run");
    assert_eq!(state.begin_calls, 0, "no transaction is needed");
}

#[tokio::test]
async fn missing_source_schema_is_not_started_and_does_not_mark_write_started() {
    let source = driver(vec![vec![Some(Value::Integer(1))]], schema(&["id"]));
    let target = driver(vec![], schema(&["id"]));
    let tables = [inspected("missing-schema", vec![mapping("id", "id")])];
    let transfer_job = job();
    let schemas = HashMap::new();
    let formatter = ValueFormatter::SameFamily;
    let write_started = AtomicBool::new(false);

    let result = execute_transfer_data_with_write_observer(
        &source,
        &ConnectionHandle {
            id: "source".into(),
            pool_id: "source".into(),
        },
        &target,
        &ConnectionHandle {
            id: "target".into(),
            pool_id: "target".into(),
        },
        &transfer_job,
        &tables,
        &schemas,
        &formatter,
        None,
        false,
        None,
        None,
        Some(&write_started),
    )
    .await
    .unwrap();

    assert_eq!(result.tables.len(), 1);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::NotStarted)
    );
    assert_eq!(result.tables[0].rows_inserted, Some(0));
    assert!(!write_started.load(Ordering::SeqCst));
    assert_eq!(target.state.lock().unwrap().calls, 0);
}

#[tokio::test]
async fn unknown_truncate_outcome_stops_later_tables_and_marks_run_started() {
    let source = driver(vec![vec![Some(Value::Integer(1))]], schema(&["id"]));
    let mut target = driver(vec![], schema(&["id"]));
    target.execute_error_on_call = Some(1);
    let tables = [
        inspected("a", vec![mapping("id", "id")]),
        inspected("b", vec![mapping("id", "id")]),
    ];
    let schemas = tables
        .iter()
        .map(|table| (table.source_table.clone(), source.schema.clone()))
        .collect::<HashMap<_, _>>();
    let mut transfer_job = job();
    transfer_job.write_mode = WriteMode::TruncateInsert;
    transfer_job.options.confirmed_destructive = true;
    let formatter = ValueFormatter::SameFamily;
    let write_started = AtomicBool::new(false);

    let result = execute_transfer_data_with_write_observer(
        &source,
        &ConnectionHandle {
            id: "source".into(),
            pool_id: "source".into(),
        },
        &target,
        &ConnectionHandle {
            id: "target".into(),
            pool_id: "target".into(),
        },
        &transfer_job,
        &tables,
        &schemas,
        &formatter,
        None,
        false,
        None,
        None,
        Some(&write_started),
    )
    .await
    .unwrap();

    assert!(result.partial);
    assert_eq!(result.tables.len(), 2);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::Unknown)
    );
    assert_eq!(result.tables[0].rows_inserted, None);
    assert_eq!(
        result.tables[1].outcome,
        Some(TableExecutionOutcome::NotStarted)
    );
    assert_eq!(target.state.lock().unwrap().execute_calls, 1);
    assert_eq!(target.state.lock().unwrap().calls, 0);
    assert!(write_started.load(Ordering::SeqCst));
}

#[tokio::test]
async fn confirmed_truncate_preamble_is_partial_when_begin_fails_and_can_continue() {
    let source = driver(vec![vec![Some(Value::Integer(1))]], schema(&["id"]));
    let mut target = driver(vec![], schema(&["id"]));
    target.begin_error_on_call = Some(1);
    let tables = [
        inspected("a", vec![mapping("id", "id")]),
        inspected("b", vec![mapping("id", "id")]),
    ];
    let schemas = tables
        .iter()
        .map(|table| (table.source_table.clone(), source.schema.clone()))
        .collect::<HashMap<_, _>>();
    let mut transfer_job = job();
    transfer_job.write_mode = WriteMode::TruncateInsert;
    transfer_job.options.confirmed_destructive = true;
    let formatter = ValueFormatter::SameFamily;

    let result = execute_transfer_data_with_write_observer(
        &source,
        &ConnectionHandle {
            id: "source".into(),
            pool_id: "source".into(),
        },
        &target,
        &ConnectionHandle {
            id: "target".into(),
            pool_id: "target".into(),
        },
        &transfer_job,
        &tables,
        &schemas,
        &formatter,
        None,
        false,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    assert!(result.partial);
    assert_eq!(result.tables.len(), 2);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::PartiallyApplied)
    );
    assert_eq!(result.tables[0].rows_inserted, Some(0));
    assert_eq!(
        result.tables[1].outcome,
        Some(TableExecutionOutcome::Committed)
    );
    let state = target.state.lock().unwrap();
    assert_eq!(
        state.execute_calls, 2,
        "both truncate statements are confirmed"
    );
    assert_eq!(state.calls, 1, "the later table's row insert still runs");
}

#[tokio::test]
async fn confirmed_truncate_preamble_is_partial_when_target_reinspection_fails() {
    let source = driver(vec![vec![Some(Value::Integer(1))]], schema(&["id"]));
    let mut target = driver(vec![], schema(&["id"]));
    target.schema_error_on_call = Some(1);
    let tables = [inspected("a", vec![mapping("id", "id")])];
    let schemas = tables
        .iter()
        .map(|table| (table.source_table.clone(), source.schema.clone()))
        .collect::<HashMap<_, _>>();
    let mut transfer_job = job();
    transfer_job.write_mode = WriteMode::TruncateInsert;
    transfer_job.options.confirmed_destructive = true;
    let formatter = ValueFormatter::SameFamily;

    let result = execute_transfer_data_with_write_observer(
        &source,
        &ConnectionHandle {
            id: "source".into(),
            pool_id: "source".into(),
        },
        &target,
        &ConnectionHandle {
            id: "target".into(),
            pool_id: "target".into(),
        },
        &transfer_job,
        &tables,
        &schemas,
        &formatter,
        None,
        false,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    assert!(result.partial);
    assert_eq!(result.tables.len(), 1);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::PartiallyApplied)
    );
    assert_eq!(result.tables[0].rows_inserted, Some(0));
    let state = target.state.lock().unwrap();
    assert_eq!(
        state.execute_calls, 1,
        "truncate succeeded before reinspection"
    );
    assert_eq!(state.begin_calls, 0, "no row transaction was started");
}

#[tokio::test]
async fn structure_and_data_created_table_reports_partial_after_confirmed_rollback() {
    let source = driver(vec![vec![Some(Value::Integer(1))]], schema(&["id"]));
    let mut target = driver(vec![], schema(&["id"]));
    target.fail_at = Some(1);
    let mut table = inspected("new_table", vec![mapping("id", "id")]);
    table.status = TableMappingStatus::CreateNew;
    let tables = [table];
    let schemas = HashMap::from([("new_table".into(), source.schema.clone())]);
    let mut transfer_job = job();
    transfer_job.mode = TransferMode::StructureAndData;
    let formatter = ValueFormatter::SameFamily;

    // The command's structure phase confirmed CREATE before entering this data phase.
    let result = execute_transfer_data_with_write_observer(
        &source,
        &ConnectionHandle {
            id: "source".into(),
            pool_id: "source".into(),
        },
        &target,
        &ConnectionHandle {
            id: "target".into(),
            pool_id: "target".into(),
        },
        &transfer_job,
        &tables,
        &schemas,
        &formatter,
        None,
        false,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    assert!(result.partial);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::PartiallyApplied)
    );
    assert_eq!(result.tables[0].rows_inserted, Some(0));
    assert_eq!(target.state.lock().unwrap().rollback, 1);
}

#[tokio::test]
async fn unknown_drop_create_preamble_stops_later_tables_before_data_writes() {
    let source = driver(vec![vec![Some(Value::Integer(1))]], schema(&["id"]));
    let mut target = driver(vec![], schema(&["id"]));
    target.execute_error_on_call = Some(1);
    let tables = [
        inspected("a", vec![mapping("id", "id")]),
        inspected("b", vec![mapping("id", "id")]),
    ];
    let schemas = tables
        .iter()
        .map(|table| (table.source_table.clone(), source.schema.clone()))
        .collect::<HashMap<_, _>>();
    let mut transfer_job = job();
    transfer_job.write_mode = WriteMode::DropCreateInsert;
    transfer_job.options.confirmed_destructive = true;
    let source_handle = ConnectionHandle {
        id: "source".into(),
        pool_id: "source".into(),
    };
    let target_handle = ConnectionHandle {
        id: "target".into(),
        pool_id: "target".into(),
    };
    let source_adapter = TestSourceAdapter;
    let target_adapter = TestTargetAdapter;
    let drop_create = super::execute::DropCreateContext {
        src_adapter: &source_adapter,
        tgt_adapter: &target_adapter,
        src_driver: &source,
        src_handle: &source_handle,
        tgt_driver: &target,
        tgt_handle: &target_handle,
        source_schemas: &schemas,
        structure_precreated: false,
    };
    let formatter = ValueFormatter::SameFamily;
    let write_started = AtomicBool::new(false);

    let result = execute_transfer_data_with_write_observer(
        &source,
        &source_handle,
        &target,
        &target_handle,
        &transfer_job,
        &tables,
        &schemas,
        &formatter,
        Some(&drop_create),
        false,
        None,
        None,
        Some(&write_started),
    )
    .await
    .unwrap();

    assert_eq!(result.tables.len(), 2);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::Unknown)
    );
    assert_eq!(result.tables[0].rows_inserted, None);
    assert_eq!(
        result.tables[1].outcome,
        Some(TableExecutionOutcome::NotStarted)
    );
    assert_eq!(target.state.lock().unwrap().execute_calls, 1);
    assert_eq!(target.state.lock().unwrap().calls, 0);
    assert!(write_started.load(Ordering::SeqCst));
}

#[tokio::test]
async fn confirmed_drop_create_preamble_is_partial_when_begin_fails_and_can_continue() {
    let source = driver(vec![vec![Some(Value::Integer(1))]], schema(&["id"]));
    let mut target = driver(vec![], schema(&["id"]));
    target.begin_error_on_call = Some(1);
    let tables = [
        inspected("a", vec![mapping("id", "id")]),
        inspected("b", vec![mapping("id", "id")]),
    ];
    let schemas = tables
        .iter()
        .map(|table| (table.source_table.clone(), source.schema.clone()))
        .collect::<HashMap<_, _>>();
    let mut transfer_job = job();
    transfer_job.write_mode = WriteMode::DropCreateInsert;
    transfer_job.options.confirmed_destructive = true;
    let source_handle = ConnectionHandle {
        id: "source".into(),
        pool_id: "source".into(),
    };
    let target_handle = ConnectionHandle {
        id: "target".into(),
        pool_id: "target".into(),
    };
    let source_adapter = TestSourceAdapter;
    let target_adapter = TestTargetAdapter;
    let drop_create = super::execute::DropCreateContext {
        src_adapter: &source_adapter,
        tgt_adapter: &target_adapter,
        src_driver: &source,
        src_handle: &source_handle,
        tgt_driver: &target,
        tgt_handle: &target_handle,
        source_schemas: &schemas,
        structure_precreated: false,
    };
    let formatter = ValueFormatter::SameFamily;

    let result = execute_transfer_data_with_write_observer(
        &source,
        &source_handle,
        &target,
        &target_handle,
        &transfer_job,
        &tables,
        &schemas,
        &formatter,
        Some(&drop_create),
        false,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    assert!(result.partial);
    assert_eq!(result.tables.len(), 2);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::PartiallyApplied)
    );
    assert_eq!(result.tables[0].rows_inserted, Some(0));
    assert_eq!(
        result.tables[1].outcome,
        Some(TableExecutionOutcome::Committed)
    );
    let state = target.state.lock().unwrap();
    assert_eq!(
        state.execute_calls, 4,
        "each table's DROP and CREATE completed"
    );
    assert_eq!(state.calls, 1, "the later table's row insert still runs");
}

#[tokio::test]
async fn cancel_after_write_reports_current_table_and_rolls_back() {
    let flag = Arc::new(AtomicBool::new(false));
    let source = driver(
        vec![vec![Some(Value::Integer(1))], vec![Some(Value::Integer(2))]],
        schema(&["id"]),
    );
    let mut target = driver(vec![], schema(&["id"]));
    target.schema.columns[0].is_auto_increment = true;
    target.session_identity_insert = true;
    target.cancel = Some(Arc::clone(&flag));
    let result = run(
        &source,
        &target,
        &[inspected("a", vec![mapping("id", "id")])],
        Some(flag),
    )
    .await;
    assert!(result.cancelled);
    assert!(result.partial);
    assert_eq!(result.tables.len(), 1);
    assert_eq!(result.rows_inserted, 0);
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::RolledBack)
    );
    assert_eq!(result.tables[0].rows_inserted, Some(0));
    let state = target.state.lock().unwrap();
    assert_eq!(state.rollback, 1);
    assert!(state.committed.is_empty());
    assert!(state.identity_sync_calls.is_empty());
    assert_eq!(
        state
            .identity_insert_calls
            .iter()
            .map(|call| call.3)
            .collect::<Vec<_>>(),
        vec![true, false]
    );
}

#[test]
fn same_named_columns_use_their_own_table_ir_and_missing_types_fail() {
    use crate::transfer::{
        adapter::SyncTargetAdapter,
        ir::{IRDefault, IRType},
    };
    struct Adapter;
    impl SyncTargetAdapter for Adapter {
        fn ir_type_to_native(&self, _: &IRType) -> String {
            "TEXT".into()
        }
        fn format_default(&self, _: &IRDefault) -> Option<String> {
            None
        }
        fn format_literal(&self, _: &Option<Value>, _: &IRType) -> String {
            panic!("bound writer must not format literals")
        }
        fn transform_value(&self, _: &Option<Value>, kind: &IRType) -> Option<Value> {
            Some(Value::String(format!("{kind:?}")))
        }
    }
    let driver = driver(vec![], schema(&["value"]));
    let types = HashMap::from([
        ("a".into(), HashMap::from([("value".into(), IRType::Int32)])),
        ("b".into(), HashMap::from([("value".into(), IRType::Text)])),
    ]);
    let formatter = super::execute::ValueFormatter::Ir {
        tgt_adapter: &Adapter,
        source_column_ir_types: &types,
    };
    let binding = mapping("value", "value");
    let row = [Some(Value::Integer(1))];
    for (table, expected) in [("a", "Int32"), ("b", "Text")] {
        let (_, params) = super::writer::bound_insert(
            &driver,
            table,
            "target",
            &[&binding],
            &driver.schema,
            &row,
            &formatter,
        )
        .unwrap();
        assert!(matches!(&params[0], Value::String(value) if value == expected));
    }
    assert!(super::writer::bound_insert(
        &driver,
        "missing",
        "target",
        &[&binding],
        &driver.schema,
        &row,
        &formatter
    )
    .is_err());
}

#[tokio::test]
async fn invalid_or_truncated_stream_never_reaches_target_writes() {
    for mode in [1, 2, 3, 4, 5] {
        let mut source = driver(vec![vec![Some(Value::Integer(1))]], schema(&["id"]));
        source.stream_mode = mode;
        let target = driver(vec![], schema(&["id"]));
        let result = run(
            &source,
            &target,
            &[inspected("a", vec![mapping("id", "id")])],
            None,
        )
        .await;
        assert!(result.partial);
        assert!(!result.tables[0].success);
        assert_eq!(target.state.lock().unwrap().calls, 0);
    }
}

#[tokio::test]
async fn source_metadata_and_bound_writer_target_use_endpoint_schema() {
    let source = driver(vec![vec![Some(Value::Integer(1))]], schema(&["id"]));
    let target = driver(vec![], schema(&["id"]));
    let mut job = job();
    job.source.schema = Some("source_scope".into());
    job.target.as_mut().unwrap().schema = Some("target_scope".into());
    let source_handle = ConnectionHandle {
        id: "source".into(),
        pool_id: "source".into(),
    };
    let target_handle = ConnectionHandle {
        id: "target".into(),
        pool_id: "target".into(),
    };
    let loaded =
        super::metadata::load_table_schema(&source, &source_handle, &job.source, "same_table")
            .await
            .unwrap();
    let tables = vec![inspected("same_table", vec![mapping("id", "id")])];
    let schemas = HashMap::from([("same_table".into(), loaded)]);
    let result = execute_same_family_data(
        &source,
        &source_handle,
        &target,
        &target_handle,
        &job,
        &tables,
        &schemas,
        false,
        None,
    )
    .await
    .unwrap();
    assert!(!result.partial);
    assert_eq!(
        source.state.lock().unwrap().metadata_refs,
        vec!["source_scope.same_table"]
    );
    assert_eq!(
        target.state.lock().unwrap().metadata_refs,
        vec!["target_scope.same_table"]
    );
}

#[tokio::test]
async fn test_tester_dotted_target_schema_fails_before_any_bound_write() {
    let source = driver(vec![vec![Some(Value::Integer(1))]], schema(&["id"]));
    let target = driver(vec![], schema(&["id"]));
    let mut transfer = job();
    transfer.target.as_mut().unwrap().schema = Some("ambiguous.schema".into());
    let tables = vec![inspected("same_table", vec![mapping("id", "id")])];
    let source_schemas = HashMap::from([("same_table".into(), source.schema.clone())]);

    let result = execute_same_family_data(
        &source,
        &ConnectionHandle {
            id: "source".into(),
            pool_id: "source".into(),
        },
        &target,
        &ConnectionHandle {
            id: "target".into(),
            pool_id: "target".into(),
        },
        &transfer,
        &tables,
        &source_schemas,
        false,
        None,
    )
    .await
    .unwrap();

    assert!(result.partial);
    assert_eq!(result.rows_inserted, 0);
    assert!(result.tables[0]
        .error
        .as_deref()
        .is_some_and(|message| message.contains("structured relation support")));
    let target_state = target.state.lock().unwrap();
    assert_eq!(target_state.calls, 0);
    assert!(target_state.metadata_refs.is_empty());
}

#[tokio::test]
async fn test_tester_same_session_same_catalog_different_schemas_can_transfer_same_table_name() {
    let source = driver(vec![vec![Some(Value::Integer(1))]], schema(&["id"]));
    let target = driver(vec![], schema(&["id"]));
    let mut transfer = job();
    transfer.source.db_session_id = "shared".into();
    transfer.target.as_mut().unwrap().db_session_id = "shared".into();
    transfer.source.database = "catalog".into();
    transfer.target.as_mut().unwrap().database = "catalog".into();
    transfer.source.schema = Some("source_schema".into());
    transfer.target.as_mut().unwrap().schema = Some("target_schema".into());
    let tables = vec![inspected("same_table", vec![mapping("id", "id")])];
    let source_schemas = HashMap::from([("same_table".into(), source.schema.clone())]);

    let result = execute_same_family_data(
        &source,
        &ConnectionHandle {
            id: "shared".into(),
            pool_id: "shared".into(),
        },
        &target,
        &ConnectionHandle {
            id: "shared".into(),
            pool_id: "shared".into(),
        },
        &transfer,
        &tables,
        &source_schemas,
        false,
        None,
    )
    .await
    .expect("different schemas are distinct relations and must not be rejected as self-overwrite");

    assert!(!result.partial);
    assert_eq!(result.rows_inserted, 1);
    assert_eq!(target.state.lock().unwrap().committed.len(), 1);
}

#[tokio::test]
async fn same_session_catalog_and_normalized_schema_rejects_before_any_write() {
    let source = driver(vec![vec![Some(Value::Integer(1))]], schema(&["id"]));
    let target = driver(vec![], schema(&["id"]));
    let mut transfer = job();
    transfer.target = Some(transfer.source.clone());
    transfer.source.schema = Some(" selected ".into());
    transfer.target.as_mut().unwrap().schema = Some("selected".into());
    let handle = ConnectionHandle {
        id: "shared".into(),
        pool_id: "shared".into(),
    };
    let tables = [inspected("same_table", vec![mapping("id", "id")])];
    let schemas = HashMap::from([("same_table".into(), source.schema.clone())]);
    let result = execute_same_family_data(
        &source, &handle, &target, &handle, &transfer, &tables, &schemas, false, None,
    )
    .await;
    assert!(
        matches!(result, Err(super::TransferError::Validation(message)) if message.contains("self-overwrite"))
    );
    let state = target.state.lock().unwrap();
    assert_eq!(state.calls, 0);
    assert!(state.metadata_refs.is_empty());
    assert!(state.committed.is_empty());
}

#[tokio::test]
async fn unknown_structure_ddl_fences_and_reports_every_unattempted_statement() {
    let mut target = driver(vec![], schema(&["id"]));
    target.execute_error_on_call = Some(1);
    let handle = ConnectionHandle {
        id: "target".into(),
        pool_id: "target".into(),
    };
    let plan = vec![
        DdlPreviewItem {
            source_table: "child".into(),
            target_table: "child_copy".into(),
            ddl: "DROP TABLE IF EXISTS child_copy".into(),
            kind: DdlPreviewKind::DropTable,
            depends_on: Vec::new(),
        },
        DdlPreviewItem {
            source_table: "parent".into(),
            target_table: "parent_copy".into(),
            ddl: "CREATE TABLE parent_copy (id INT)".into(),
            kind: DdlPreviewKind::Table,
            depends_on: Vec::new(),
        },
        DdlPreviewItem {
            source_table: "parent".into(),
            target_table: "parent_copy".into(),
            ddl: "CREATE INDEX parent_idx ON parent_copy (id)".into(),
            kind: DdlPreviewKind::Index,
            depends_on: vec!["parent".into()],
        },
    ];

    let results = super::structure::execute_database_structure_plan(
        &target,
        &handle,
        &plan,
        &HashSet::from(["child".into(), "parent".into()]),
        super::structure::DatabaseStructurePhase::Prepare,
        None,
        None,
    )
    .await;

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].outcome, Some(TableExecutionOutcome::Unknown));
    assert!(results[1..]
        .iter()
        .all(|result| result.outcome == Some(TableExecutionOutcome::NotStarted)));
    assert_eq!(target.state.lock().unwrap().execute_calls, 1);
}

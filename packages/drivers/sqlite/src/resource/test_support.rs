//! Shared test doubles for the sqlite resource provider.
//!
//! They live in their own module because the provider's contract tests span two
//! files (lifecycle and capability honesty) and both need the same fake wire.
//! Nothing here is compiled outside `#[cfg(test)]`.
//!
//! In-crate tests for the sqlite `ResourceProvider`.
//!
//! Every assertion here is about *honesty*: that the provider refuses what it
//! cannot do, releases what it took, and never reports something it did not
//! observe. The wire is a `FakeSqlite` so the tests stay inside this crate and
//! need no database file at all.

pub(crate) use std::sync::atomic::{AtomicUsize, Ordering};
pub(crate) use std::sync::{Arc, Mutex};

pub(crate) use async_trait::async_trait;

pub(crate) use datazen_driver_api::capabilities::{
    Availability, NamespaceSwitch, PreciseCancelSupport, ResetForReuse, SessionContinuity,
};
pub(crate) use datazen_driver_api::namespace::{
    NamespaceLevelKind, NamespaceShape, NamespaceTarget,
};
pub(crate) use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, CommandCall,
    DescribeResourceRequest, IdentityScope, ResourceError, ResourceHandle, ResourceProvider,
    ResourcePurpose, ResourceScope, ResultChunk, ResultSink,
};
pub(crate) use datazen_driver_api::session::{
    CancelDisposition, CloseDisposition, ContextChangeDisposition, EffectOutcome,
    ObservationConfidence, ResetDisposition, SessionContext, TransactionOptions, TransactionState,
};
pub(crate) use datazen_driver_api::{
    ColumnInfo, ConnectionConfig, ConnectionHandle, DatabaseDriver, DatabaseType, DriverError,
    MultiQueryResult, QueryExecutionId, QueryResult, ServerInfo, SslMode, StatementResult,
    TableInfo, TableSchema, TransactionHandle, Value,
};

pub(crate) use crate::resource::SqliteResourceProvider;

/// Everything the fake driver observed. Shared so a test can read it back
/// after the provider has moved on.
#[derive(Default)]
pub(crate) struct FakeWire {
    pub(crate) statements: Vec<String>,
    pub(crate) commands: Vec<(String, String)>,
    pub(crate) rows_affected: Vec<u64>,
    pub(crate) prepared: Vec<String>,
    pub(crate) cleaned: Vec<String>,
    pub(crate) connect_calls: usize,
    pub(crate) disconnect_calls: usize,
    pub(crate) begin_calls: usize,
    pub(crate) commit_calls: usize,
    pub(crate) rollback_calls: usize,
    pub(crate) session_wide_cancels: usize,
    pub(crate) targeted_cancels: usize,
    /// What `PRAGMA database_list` answers; `None` makes the read-back come back
    /// with no attached database at all.
    pub(crate) attached: Option<Vec<String>>,
    pub(crate) connect_fails: bool,
    pub(crate) disconnect_fails: bool,
    pub(crate) commit_fails: bool,
    pub(crate) rollback_fails: bool,
}

pub(crate) struct FakeSqlite {
    pub(crate) wire: Mutex<FakeWire>,
}

impl FakeSqlite {
    pub(crate) fn new() -> Arc<Self> {
        let mut wire = FakeWire::default();
        // A live read-back is the one fact sqlite's provider actually depends
        // on; a test that wants it gone clears `attached` explicitly.
        wire.attached = Some(vec!["main".to_string()]);
        Arc::new(Self {
            wire: Mutex::new(wire),
        })
    }

    pub(crate) fn with<F: FnOnce(&mut FakeWire)>(&self, apply: F) {
        let mut wire = self
            .wire
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        apply(&mut wire);
    }

    pub(crate) fn read<F: FnOnce(&FakeWire) -> R, R>(&self, read: F) -> R {
        let wire = self
            .wire
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        read(&wire)
    }
}

#[async_trait]
impl DatabaseDriver for FakeSqlite {
    fn driver_type(&self) -> DatabaseType {
        "sqlite".to_string()
    }

    async fn connect(&self, _config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        self.with(|wire| wire.connect_calls += 1);
        if self.read(|wire| wire.connect_fails) {
            return Err(DriverError::ConnectionFailed("refused".into()));
        }
        // sqlite's real `connect` sets `id == pool_id`; the fake keeps that, so a
        // test that leaned on it would not be testing a fiction.
        Ok(ConnectionHandle {
            id: "sqlite_pool_1".into(),
            pool_id: "sqlite_pool_1".into(),
        })
    }

    async fn test_connection(&self, _config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        Ok(ServerInfo {
            server_version: "fake".into(),
            server_type: "sqlite".into(),
        })
    }

    async fn disconnect(&self, _handle: ConnectionHandle) -> Result<(), DriverError> {
        self.with(|wire| wire.disconnect_calls += 1);
        if self.read(|wire| wire.disconnect_fails) {
            return Err(DriverError::ConnectionFailed("drop failed".into()));
        }
        Ok(())
    }

    async fn get_databases(&self, _handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Ok(vec!["main".into()])
    }

    async fn get_tables(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
        _schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn get_table_schema(
        &self,
        _handle: &ConnectionHandle,
        _table: &str,
        _database: &str,
        _schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        Err(DriverError::NotSupported("no schema in this fake".into()))
    }

    async fn query(
        &self,
        _handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        self.with(|wire| wire.statements.push(sql.to_string()));
        if sql == "PRAGMA database_list" {
            let attached = self.read(|wire| wire.attached.clone()).unwrap_or_default();
            return Ok(QueryResult {
                columns: vec![
                    ColumnInfo {
                        name: "seq".into(),
                        data_type: "integer".into(),
                        nullable: false,
                    },
                    ColumnInfo {
                        name: "name".into(),
                        data_type: "text".into(),
                        nullable: true,
                    },
                ],
                rows: attached
                    .into_iter()
                    .enumerate()
                    .map(|(index, name)| {
                        vec![
                            Some(Value::Integer(index as i64)),
                            Some(Value::String(name)),
                        ]
                    })
                    .collect(),
                rows_affected: None,
                execution_time_ms: 0,
            });
        }
        Ok(QueryResult {
            columns: vec![ColumnInfo {
                name: "n".into(),
                data_type: "integer".into(),
                nullable: false,
            }],
            rows: vec![vec![Some(Value::Integer(1))]],
            rows_affected: Some(1),
            execution_time_ms: 0,
        })
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        _limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        let result = self.query(handle, sql).await?;
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
        _handle: &ConnectionHandle,
        _sql: &str,
        _params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        Err(DriverError::NotSupported(
            "params are not exercised here".into(),
        ))
    }

    async fn execute(&self, _handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        self.with(|wire| {
            wire.statements.push(sql.to_string());
            wire.rows_affected.push(1);
        });
        Ok(1)
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        // The provider must never reach this; the test asserts the count.
        self.with(|wire| wire.session_wide_cancels += 1);
        Err(DriverError::Unsupported(
            "session-wide cancellation is disabled in this fake".into(),
        ))
    }

    async fn prepare_query_execution(
        &self,
        _handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.with(|wire| wire.prepared.push(execution_id.as_str().to_string()));
        Ok(())
    }

    async fn cleanup_query_execution(
        &self,
        _handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.with(|wire| wire.cleaned.push(execution_id.as_str().to_string()));
        Ok(())
    }

    async fn cancel_query_with_execution(
        &self,
        _handle: &ConnectionHandle,
        _execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        // sqlite has no interrupt primitive. A provider that reached this would
        // be asking for something the driver cannot do; the count proves it
        // does not.
        self.with(|wire| wire.targeted_cancels += 1);
        Err(DriverError::Unsupported("no interrupt".into()))
    }

    async fn begin_transaction(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        self.with(|wire| wire.begin_calls += 1);
        Ok(TransactionHandle {
            id: "sqlite_tx_1".into(),
            connection_id: "sqlite_pool_1".into(),
        })
    }

    async fn commit(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        self.with(|wire| wire.commit_calls += 1);
        if self.read(|wire| wire.commit_fails) {
            return Err(DriverError::QueryFailed("COMMIT failed".into()));
        }
        Ok(())
    }

    async fn rollback(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        self.with(|wire| wire.rollback_calls += 1);
        if self.read(|wire| wire.rollback_fails) {
            return Err(DriverError::QueryFailed("ROLLBACK failed".into()));
        }
        Ok(())
    }

    async fn execute_command(
        &self,
        _handle: &ConnectionHandle,
        command: &str,
        input: serde_json::Value,
    ) -> Result<datazen_driver_api::CommandResult, DriverError> {
        self.with(|wire| {
            wire.commands.push((command.to_string(), input.to_string()));
        });
        Ok(datazen_driver_api::CommandResult::new(
            serde_json::json!({ "echo": command }),
        ))
    }
}

/// A sink that records what the provider pushed, so backpressure is testable.
#[derive(Default)]
pub(crate) struct RecordingSink {
    pub(crate) chunks: Mutex<Vec<ResultChunk>>,
    pub(crate) completed: AtomicUsize,
}

#[async_trait]
impl ResultSink for RecordingSink {
    async fn write(&self, chunk: ResultChunk) -> Result<(), ResourceError> {
        self.chunks
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(chunk);
        Ok(())
    }

    async fn complete(&self) -> Result<(), ResourceError> {
        self.completed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn fail(&self, _reason: &str) -> Result<(), ResourceError> {
        Ok(())
    }
}

pub(crate) struct RecordingBudget {
    pub(crate) granted: Mutex<Vec<u32>>,
    pub(crate) released: Mutex<Vec<BudgetPermit>>,
    pub(crate) deny: Mutex<bool>,
}

impl RecordingBudget {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            granted: Mutex::new(Vec::new()),
            released: Mutex::new(Vec::new()),
            deny: Mutex::new(false),
        })
    }

    pub(crate) fn granted(&self) -> Vec<u32> {
        self.granted
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    pub(crate) fn released(&self) -> usize {
        self.released
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .len()
    }

    /// Make every later `acquire` be denied, so the provider can be observed
    /// refusing to open a resource it cannot afford.
    pub(crate) fn deny(&self) {
        *self
            .deny
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = true;
    }
}

#[async_trait]
impl BudgetPort for RecordingBudget {
    async fn acquire_physical_connections(
        &self,
        count: u32,
    ) -> Result<BudgetPermit, ResourceError> {
        if *self
            .deny
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
        {
            return Err(ResourceError::BudgetDenied {
                requested: count,
                reason: "the fake budget denies every request".into(),
            });
        }
        self.granted
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(count);
        Ok(BudgetPermit {
            permit_id: format!("permit-{}", self.granted().len()),
            physical_connections: count,
        })
    }

    async fn release_physical_connections(
        &self,
        permit: &BudgetPermit,
    ) -> Result<(), ResourceError> {
        self.released
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(permit.clone());
        Ok(())
    }
}

pub(crate) fn test_config() -> ConnectionConfig {
    ConnectionConfig {
        id: "cfg".into(),
        name: "sqlite".into(),
        database_type: "sqlite".into(),
        host: None,
        port: None,
        database: Some("/tmp/datazen-provider-test.db".into()),
        schema: None,
        username: None,
        password: None,
        ssl_mode: SslMode::Prefer,
        connection_timeout: 5,
        max_pool_size: 1,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    }
}

pub(crate) fn scope() -> ResourceScope {
    ResourceScope {
        purpose: ResourcePurpose::InteractiveQuery,
        max_physical_connections: 1,
        holds_open_transaction: false,
        pin_for_streaming: false,
    }
}

pub(crate) fn baseline() -> Baseline {
    Baseline::default()
}

pub(crate) fn acquire_request() -> AcquireResourceRequest {
    AcquireResourceRequest {
        connection_config: test_config(),
        target: NamespaceTarget::empty(),
        scope: scope(),
        identity_scope: IdentityScope::default(),
        baseline: baseline(),
    }
}

pub(crate) fn describe_request() -> DescribeResourceRequest {
    DescribeResourceRequest {
        connection_config: test_config(),
        identity_scope: IdentityScope::default(),
        target: NamespaceTarget::empty(),
        purpose: ResourcePurpose::InteractiveQuery,
    }
}

pub(crate) fn provider() -> (
    SqliteResourceProvider,
    Arc<FakeSqlite>,
    Arc<RecordingBudget>,
) {
    let fake = FakeSqlite::new();
    let budget = RecordingBudget::new();
    (provider_with(fake.clone()), fake, budget)
}

pub(crate) fn provider_with(fake: Arc<FakeSqlite>) -> SqliteResourceProvider {
    SqliteResourceProvider::new(fake, "sqlite")
}

pub(crate) fn budget_as_port(budget: &Arc<RecordingBudget>) -> Arc<dyn BudgetPort> {
    budget.clone()
}

pub(crate) async fn acquire(
    provider: &SqliteResourceProvider,
    budget: &Arc<RecordingBudget>,
) -> ResourceHandle {
    provider
        .acquire_resource(&acquire_request(), &budget_as_port(&budget))
        .await
        .expect("acquire should succeed for a supported target")
}

/// A handle that belongs to some other provider: same driver id, foreign
/// runtime epoch. Built by bumping the live epoch, so it is guaranteed to be
/// rejected.
pub(crate) fn forged_handle(provider: &SqliteResourceProvider) -> ResourceHandle {
    ResourceHandle::issue(
        "sqlite".to_string(),
        "someone-elses-connection".to_string(),
        provider.runtime_epoch + 1,
    )
}

/// The namespace shape sqlite advertises: a database, no catalog, no schema.
pub(crate) fn namespace_shape() -> NamespaceShape {
    crate::resource_capabilities::sqlite_namespace_shape()
}

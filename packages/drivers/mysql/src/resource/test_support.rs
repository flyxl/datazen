//! Shared test doubles for the mysql resource provider.
//!
//! They live in their own module because the provider's contract tests
//! span two files (lifecycle and capability honesty) and both need the
//! same fake wire. Nothing here is compiled outside `#[cfg(test)]`.

//! In-crate tests for the mysql `ResourceProvider`.
//!
//! Every assertion here is about *honesty*: that the provider refuses what it
//! cannot do, releases what it took, and never reports something it did not
//! observe. The wire is a `FakeMysql` so the tests stay inside this crate and
//! need no live server.

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
    ObservationConfidence, ResetDisposition, SessionObservation, TransactionOptions,
    TransactionState,
};
pub(crate) use datazen_driver_api::{
    ColumnInfo, ConnectionConfig, ConnectionHandle, DatabaseDriver, DatabaseType, DdlAtomicity,
    DriverError, MultiQueryResult, QueryExecutionId, QueryResult, ServerInfo, StatementResult,
    TableInfo, TableSchema, TransactionHandle, Value,
};

pub(crate) use crate::resource::MysqlResourceProvider;
pub(crate) use crate::resource_capabilities::{migration_operation_keys, mysql_capability_set};

/// The fake's configurable answer to `cancel_query_with_execution`.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum CancelOutcome {
    #[default]
    Ok,
    NotFound,
    Unsupported,
    SessionMismatch,
    Failure,
}

/// Everything the fake driver observed. Shared so a test can read it back
/// after the provider has moved on.
#[derive(Default)]
pub(crate) struct FakeWire {
    pub(crate) statements: Vec<String>,
    pub(crate) commands: Vec<(String, String)>,
    pub(crate) rows_affected: Vec<u64>,
    pub(crate) use_targets: Vec<String>,
    pub(crate) prepared: Vec<String>,
    pub(crate) cleaned: Vec<String>,
    pub(crate) connect_calls: usize,
    pub(crate) disconnect_calls: usize,
    pub(crate) begin_calls: usize,
    pub(crate) snapshot_calls: usize,
    pub(crate) session_wide_cancels: usize,
    /// What `SELECT DATABASE()` answers; `None` makes the read-back fail.
    pub(crate) database: Option<String>,
    pub(crate) connect_fails: bool,
    pub(crate) disconnect_fails: bool,
    pub(crate) commit_fails: bool,
    pub(crate) rollback_fails: bool,
    pub(crate) precise_cancel: bool,
    pub(crate) cancel_outcome: CancelOutcome,
}

pub(crate) struct FakeMysql {
    pub(crate) wire: Mutex<FakeWire>,
}

impl FakeMysql {
    pub(crate) fn new() -> Arc<Self> {
        let mut wire = FakeWire::default();
        // A live read-back and a driver that advertises precise cancel are
        // the two facts most mysql factories actually hold; a test that wants
        // otherwise turns them off explicitly.
        wire.database = Some("app".into());
        wire.precise_cancel = true;
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
impl DatabaseDriver for FakeMysql {
    fn driver_type(&self) -> DatabaseType {
        "mysql".to_string()
    }

    async fn connect(&self, _config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        self.with(|wire| {
            wire.connect_calls += 1;
        });
        if self.read(|wire| wire.connect_fails) {
            return Err(DriverError::ConnectionFailed("refused".into()));
        }
        Ok(ConnectionHandle {
            id: "conn-1".into(),
            pool_id: "pool-1".into(),
        })
    }

    async fn test_connection(&self, _config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        Ok(ServerInfo {
            server_version: "fake".into(),
            server_type: "mysql".into(),
        })
    }

    async fn disconnect(&self, _handle: ConnectionHandle) -> Result<(), DriverError> {
        self.with(|wire| {
            wire.disconnect_calls += 1;
        });
        if self.read(|wire| wire.disconnect_fails) {
            return Err(DriverError::ConnectionFailed("drop failed".into()));
        }
        Ok(())
    }

    async fn get_databases(&self, _handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Ok(vec!["app".into()])
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
        if sql == "SELECT DATABASE()" {
            return Ok(QueryResult {
                columns: vec![ColumnInfo {
                    name: "DATABASE()".into(),
                    data_type: "text".into(),
                    nullable: true,
                }],
                rows: vec![vec![self
                    .read(|wire| wire.database.clone())
                    .map(Value::String)]],
                rows_affected: None,
                execution_time_ms: 0,
            });
        }
        Ok(QueryResult {
            columns: vec![ColumnInfo {
                name: "n".into(),
                data_type: "bigint".into(),
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
            if let Some(database) = sql
                .strip_prefix("USE ")
                .map(|name| name.trim_matches('`').to_string())
            {
                wire.use_targets.push(database);
            }
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

    fn supports_query_execution_cancel(&self) -> bool {
        self.read(|wire| wire.precise_cancel)
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
        match self.read(|wire| wire.cancel_outcome) {
            CancelOutcome::Ok => Ok(()),
            CancelOutcome::NotFound => {
                Err(DriverError::QueryExecutionNotFound("no such run".into()))
            }
            CancelOutcome::Unsupported => Err(DriverError::Unsupported("no such target".into())),
            CancelOutcome::SessionMismatch => Err(DriverError::QueryExecutionSessionMismatch),
            CancelOutcome::Failure => Err(DriverError::QueryFailed("KILL failed".into())),
        }
    }

    async fn begin_transaction(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        self.with(|wire| wire.begin_calls += 1);
        Ok(TransactionHandle {
            id: "mysql_tx_1".into(),
            connection_id: "conn-1".into(),
        })
    }

    async fn begin_read_snapshot(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        self.with(|wire| wire.snapshot_calls += 1);
        Ok(TransactionHandle {
            id: "mysql_snapshot_1".into(),
            connection_id: "conn-1".into(),
        })
    }

    async fn commit(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        if self.read(|wire| wire.commit_fails) {
            return Err(DriverError::QueryFailed("COMMIT failed".into()));
        }
        Ok(())
    }

    async fn rollback(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
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
        name: "mysql".into(),
        database_type: "mysql".into(),
        host: Some("127.0.0.1".into()),
        port: Some(3306),
        database: Some("app".into()),
        schema: None,
        username: Some("root".into()),
        password: None,
        ssl_mode: Default::default(),
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

pub(crate) fn acquire_request() -> AcquireResourceRequest {
    AcquireResourceRequest {
        connection_config: test_config(),
        target: NamespaceTarget::empty(),
        scope: scope(),
        identity_scope: IdentityScope::default(),
        baseline: baseline(),
    }
}

pub(crate) fn baseline() -> Baseline {
    Baseline::default()
}

pub(crate) fn provider() -> (MysqlResourceProvider, Arc<FakeMysql>, Arc<RecordingBudget>) {
    let fake = FakeMysql::new();
    let budget = RecordingBudget::new();
    (provider_with(fake.clone()), fake, budget)
}

pub(crate) fn provider_with(fake: Arc<FakeMysql>) -> MysqlResourceProvider {
    MysqlResourceProvider::new(fake, "mysql", mysql_capability_set(true))
}

pub(crate) fn budget_as_port(budget: &Arc<RecordingBudget>) -> Arc<dyn BudgetPort> {
    budget.clone()
}

pub(crate) async fn acquire(
    provider: &MysqlResourceProvider,
    budget: &Arc<RecordingBudget>,
) -> ResourceHandle {
    provider
        .acquire_resource(&acquire_request(), &budget_as_port(&budget))
        .await
        .expect("acquire should succeed for a supported target")
}

//! In-memory ResourceProvider used by Host tests that exercise runtime sessions.
//!
//! Unlike `LegacyResourceAdapter`, this fixture models a provider that can
//! confirm the target held by its mock session and return statement results.
//! It still delegates connection and SQL work to the same `MockDriver` used by
//! the command-side fixtures, so successful runtime paths verify the provider
//! contract without opening a real database connection.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use datazen_driver_api::async_trait;
use datazen_driver_api::capabilities::{
    Availability, CapabilityRegistry, CapabilitySet, ContextObservation, NamespaceSwitch,
    ResetForReuse, SessionContinuity, SessionScopedHandleSupport,
    TransactionObservation as TransactionObservationCapability, TransactionSupport,
};
use datazen_driver_api::capability_domains::DataSupport;
use datazen_driver_api::namespace::{
    NamespaceLevel, NamespaceLevelKind, NamespaceShape, NamespaceTarget,
};
use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPort, CommandCall, ConnectionCostPolicy,
    DescribeResourceRequest, ResourceDescriptor, ResourceError, ResourceHandle, ResourceProvider,
    ResultChunk, ResultSink, ReusePolicy,
};
use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
use datazen_driver_api::session::{
    CancelReceipt, CloseDisposition, CompletionStatus, ContextChangeDisposition, EffectOutcome,
    ExecutionCompletion, ObservationConfidence, ResetDisposition,
    ResourceHealth as SessionResourceHealth, SessionContext, SessionHandleKind, SessionHandleRef,
    SessionObservation, SessionState, TransactionObservation, TransactionOptions, TransactionState,
};
use datazen_driver_api::{
    DatabaseDriver, QueryExecutionId, StatementResult, TransactionHandle, Value,
};

const TEST_RESOURCE_EPOCH: u64 = 1;

pub(crate) struct TestResourceProvider {
    legacy: LegacyResourceAdapter,
    driver: Arc<dyn DatabaseDriver>,
    namespace_shape: NamespaceShape,
    targets: Mutex<HashMap<String, NamespaceTarget>>,
    transactions: Mutex<HashMap<String, TransactionHandle>>,
}

impl TestResourceProvider {
    pub(crate) fn new(driver: Arc<dyn DatabaseDriver>) -> Self {
        let namespace_shape = NamespaceShape {
            levels: vec![
                NamespaceLevel::present(NamespaceLevelKind::Database, false),
                NamespaceLevel::absent(NamespaceLevelKind::Catalog),
                NamespaceLevel::present(NamespaceLevelKind::Schema, false),
            ],
            ..NamespaceShape::default()
        };
        let mut capabilities = CapabilitySet::default();
        capabilities.stateful_session = Availability::Supported;
        capabilities.namespace_switch = NamespaceSwitch::Unsupported;
        capabilities.context_observation = ContextObservation::Full;
        capabilities.transaction_observation = TransactionObservationCapability::Full;
        capabilities.session_scoped_handles = SessionScopedHandleSupport::Supported;
        capabilities.reset_for_reuse = ResetForReuse::Unsupported;
        capabilities.data = DataSupport::BufferedReadWrite;
        capabilities.transactions = TransactionSupport {
            max_open_transactions: Some(1),
            ..TransactionSupport::default()
        };
        let legacy = LegacyResourceAdapter::new(
            Arc::clone(&driver),
            "postgres",
            "test",
            TEST_RESOURCE_EPOCH,
            namespace_shape.clone(),
            capabilities,
        );

        Self {
            legacy,
            driver,
            namespace_shape,
            targets: Mutex::new(HashMap::new()),
            transactions: Mutex::new(HashMap::new()),
        }
    }

    fn connection(
        &self,
        handle: &ResourceHandle,
    ) -> Result<datazen_driver_api::ConnectionHandle, ResourceError> {
        self.legacy.connection_handle(handle)
    }

    fn target(&self, handle: &ResourceHandle) -> Result<NamespaceTarget, ResourceError> {
        self.connection(handle)?;
        lock(&self.targets)
            .get(handle.resource_key())
            .cloned()
            .ok_or_else(|| invalid_resource(handle, "observe_session"))
    }

    fn transaction_observation(&self, handle: &ResourceHandle) -> TransactionObservation {
        lock(&self.transactions)
            .get(handle.resource_key())
            .map(|transaction| TransactionObservation::begun(transaction.id.clone(), 1))
            .unwrap_or(TransactionObservation {
                state: TransactionState::Idle,
                transaction_id: None,
                effect: None,
                revision: 0,
            })
    }

    fn context(&self, handle: &ResourceHandle) -> Result<SessionContext, ResourceError> {
        let namespace = self.target(handle)?;
        let transaction_state = self.transaction_observation(handle).state;
        Ok(SessionContext {
            search_path: namespace.schema.iter().cloned().collect(),
            namespace,
            transaction_state,
            autocommit: Some(transaction_state == TransactionState::Idle),
            confidence: ObservationConfidence::Confirmed,
            ..SessionContext::unobserved()
        })
    }

    async fn execute_sql(
        &self,
        handle: &ResourceHandle,
        call: &CommandCall,
        sink: &dyn ResultSink,
    ) -> Result<Vec<StatementResult>, ResourceError> {
        let connection = self.connection(handle)?;
        // Catalog/schema commands have no SQL input. Dispatch them before
        // validating the SQL-only command payload, just as the runtime's
        // production command adapter does.
        if !matches!(call.command.as_str(), "query" | "query_stream" | "execute") {
            let result = self
                .driver
                .execute_command(&connection, &call.command, call.input.clone())
                .await?;
            sink.write_output(serde_json::json!({
                "kind": "commandResult",
                "data": result.data,
            }))
            .await?;
            sink.complete().await?;
            return Ok(Vec::new());
        }
        let sql = call
            .input
            .get("sql")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| ResourceError::SinkRejected {
                reason: "SQL input unavailable in test resource provider".into(),
            })?;

        let statements = match call.command.as_str() {
            "query" | "query_stream" => {
                self.driver
                    .query_multi(&connection, sql, None)
                    .await?
                    .results
            }
            "execute" => {
                let rows_affected = if let Some(values) = call
                    .input
                    .get("boundParameters")
                    .or_else(|| call.input.get("params"))
                    .and_then(serde_json::Value::as_array)
                {
                    let values = values
                        .iter()
                        .cloned()
                        .map(serde_json::from_value::<Value>)
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_| ResourceError::SinkRejected {
                            reason: "invalid bound parameters in test resource provider".into(),
                        })?;
                    self.driver
                        .execute_with_params(&connection, sql, &values)
                        .await?
                } else {
                    self.driver.execute(&connection, sql).await?
                };
                vec![StatementResult {
                    sql: sql.to_string(),
                    columns: Vec::new(),
                    rows: Vec::new(),
                    rows_affected: Some(rows_affected),
                    execution_time_ms: 0,
                    truncated: false,
                }]
            }
            _ => {
                return Err(ResourceError::SinkRejected {
                    reason: "unexpected SQL command in test resource provider".into(),
                })
            }
        };

        for (statement_index, statement) in statements.iter().enumerate() {
            sink.write(ResultChunk {
                statement_index,
                sql: statement.sql.clone(),
                rows: statement.rows.clone(),
                rows_affected: statement.rows_affected,
            })
            .await?;
        }
        sink.complete().await?;
        Ok(statements)
    }
}

#[async_trait]
impl ResourceProvider for TestResourceProvider {
    fn provider_id(&self) -> &str {
        self.legacy.provider_id()
    }

    fn capabilities(&self) -> &CapabilityRegistry {
        self.legacy.capabilities()
    }

    fn namespace_shape(&self) -> &NamespaceShape {
        &self.namespace_shape
    }

    fn connection_handle(
        &self,
        handle: &ResourceHandle,
    ) -> Result<datazen_driver_api::ConnectionHandle, ResourceError> {
        self.connection(handle)
    }

    async fn describe_resource(
        &self,
        request: &DescribeResourceRequest,
    ) -> Result<ResourceDescriptor, ResourceError> {
        self.namespace_shape.canonicalize(&request.target)?;
        let mut descriptor = self.legacy.describe_resource(request).await?;
        descriptor.session_continuity = SessionContinuity::Fixed;
        descriptor.reuse_policy = ReusePolicy::SingleUse;
        descriptor.connection_cost_policy = ConnectionCostPolicy::DeclaredConservative {
            declared_cost: 1,
            hard_cap: Some(1),
        };
        descriptor.namespace_shape = self.namespace_shape.clone();
        Ok(descriptor)
    }

    async fn acquire_resource(
        &self,
        request: &AcquireResourceRequest,
        budget: &Arc<dyn BudgetPort>,
    ) -> Result<ResourceHandle, ResourceError> {
        self.namespace_shape.canonicalize(&request.target)?;
        let handle = self.legacy.acquire_resource(request, budget).await?;
        lock(&self.targets).insert(handle.resource_key().to_string(), request.target.clone());
        Ok(handle)
    }

    async fn execute_on_resource(
        &self,
        handle: &ResourceHandle,
        _execution_id: &QueryExecutionId,
        call: &CommandCall,
        sink: &dyn ResultSink,
    ) -> Result<ExecutionCompletion, ResourceError> {
        let context = self.context(handle)?;
        let statements = self.execute_sql(handle, call, sink).await?;
        let context_after = self.context(handle)?;
        Ok(ExecutionCompletion {
            completion_status: CompletionStatus::Succeeded,
            effect_outcome: EffectOutcome::Completed,
            statement_results: statements,
            context_before: context,
            context_after,
            transaction_observation: self.transaction_observation(handle),
            session_handles: self
                .transaction_observation(handle)
                .transaction_id
                .map(|id| {
                    vec![SessionHandleRef::new(
                        id,
                        SessionHandleKind::Transaction,
                        handle.resource_key(),
                        handle.runtime_epoch(),
                    )]
                })
                .unwrap_or_default(),
            protocol_drained: true,
            resource_health: SessionResourceHealth::Healthy,
        })
    }

    async fn observe_session(
        &self,
        handle: &ResourceHandle,
    ) -> Result<SessionObservation, ResourceError> {
        let context = self.context(handle)?;
        let transaction = self.transaction_observation(handle);
        let handles = transaction
            .transaction_id
            .as_ref()
            .map(|id| {
                vec![SessionHandleRef::new(
                    id,
                    SessionHandleKind::Transaction,
                    handle.resource_key(),
                    handle.runtime_epoch(),
                )]
            })
            .unwrap_or_default();
        Ok(SessionObservation {
            state: SessionState::Ready,
            context,
            transaction,
            handles,
            protocol_drained: true,
            resource_health: SessionResourceHealth::Healthy,
            context_revision: 1,
        })
    }

    async fn change_context(
        &self,
        handle: &ResourceHandle,
        _desired: &NamespaceTarget,
    ) -> Result<ContextChangeDisposition, ResourceError> {
        self.connection(handle)?;
        Ok(ContextChangeDisposition::Unsupported)
    }

    async fn begin_transaction(
        &self,
        handle: &ResourceHandle,
        _options: &TransactionOptions,
    ) -> Result<TransactionObservation, ResourceError> {
        let connection = self.connection(handle)?;
        let transaction = self.driver.begin_transaction(&connection).await?;
        let observation = TransactionObservation::begun(transaction.id.clone(), 1);
        lock(&self.transactions).insert(handle.resource_key().to_string(), transaction);
        Ok(observation)
    }

    async fn commit_transaction(
        &self,
        handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        let transaction = lock(&self.transactions)
            .remove(handle.resource_key())
            .ok_or_else(|| invalid_resource(handle, "commit_transaction"))?;
        self.driver.commit(transaction).await?;
        Ok(TransactionObservation::finished(
            TransactionState::Idle,
            EffectOutcome::Completed,
            2,
        ))
    }

    async fn rollback_transaction(
        &self,
        handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        let transaction = lock(&self.transactions)
            .remove(handle.resource_key())
            .ok_or_else(|| invalid_resource(handle, "rollback_transaction"))?;
        self.driver.rollback(transaction).await?;
        Ok(TransactionObservation::finished(
            TransactionState::Idle,
            EffectOutcome::RolledBack,
            2,
        ))
    }

    async fn request_cancel(
        &self,
        handle: &ResourceHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<CancelReceipt, ResourceError> {
        self.legacy.request_cancel(handle, execution_id).await
    }

    async fn reset_resource(
        &self,
        handle: &ResourceHandle,
        baseline: &Baseline,
    ) -> Result<ResetDisposition, ResourceError> {
        self.legacy.reset_resource(handle, baseline).await
    }

    async fn close_resource(
        &self,
        handle: &ResourceHandle,
    ) -> Result<CloseDisposition, ResourceError> {
        let transaction = { lock(&self.transactions).remove(handle.resource_key()) };
        if let Some(transaction) = transaction {
            self.driver.rollback(transaction).await?;
        }
        let result = self.legacy.close_resource(handle).await?;
        lock(&self.targets).remove(handle.resource_key());
        Ok(result)
    }

    async fn close_handle(
        &self,
        handle: &ResourceHandle,
        session_handle: &SessionHandleRef,
    ) -> Result<(), ResourceError> {
        self.connection(handle)?;
        if !session_handle.belongs_to(handle.resource_key(), handle.runtime_epoch()) {
            return Err(ResourceError::InvalidResourceState {
                resource_key: handle.resource_key().to_string(),
                state: "session handle belongs to another resource".into(),
                operation: "close_handle".into(),
            });
        }
        if session_handle.closed {
            return Ok(());
        }
        if session_handle.kind != SessionHandleKind::Transaction {
            return Err(ResourceError::unsupported(
                self.provider_id(),
                "close_handle",
                "the test provider issues transaction handles only",
            ));
        }
        let transaction = lock(&self.transactions)
            .remove(handle.resource_key())
            .filter(|transaction| transaction.id == session_handle.handle_id)
            .ok_or_else(|| invalid_resource(handle, "close_handle"))?;
        self.driver.rollback(transaction).await?;
        Ok(())
    }
}

fn invalid_resource(handle: &ResourceHandle, operation: &str) -> ResourceError {
    ResourceError::InvalidResourceState {
        resource_key: handle.resource_key().to_string(),
        state: "not registered".into(),
        operation: operation.to_string(),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

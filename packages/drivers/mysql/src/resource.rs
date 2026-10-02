//! The real [`ResourceProvider`] for the mysql driver family.
//!
//! `lib.rs` hands out one memoized provider per factory id (`mysql`,
//! `mariadb`, `doris`, `starrocks`, `manticore`, `ob_oracle`), each bound to
//! the very driver `create()` returns. Every one of them carries its own
//! `provider_id`, its own `runtime_epoch` and its own `CapabilityRegistry`, so
//! a capability granted to one id is never inherited by another — while a
//! handle issued by the same id stays valid across any number of lookups,
//! which is exactly what the memoization buys.
//!
//! Two design rules shape everything below.
//!
//! 1. **The provider only ever speaks the wire protocol.** It works through
//!    `Arc<dyn DatabaseDriver>`, never through a downcast. Four of the six
//!    factories wrap the driver in `ReuseDriver`, whose inner driver has no
//!    accessor, so a downcast-based design could not serve them at all.
//! 2. **Nothing is reported that was not observed.** A capability cell the
//!    driver cannot back up stays `Unsupported`/`Unknown`, and an observation
//!    the provider did not read off the wire is returned `unobserved()`
//!    rather than filled in from the acquisition request. See
//!    `docs/architecture/platform/driver-capability-migration.md` §2.1 and §6.1.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use datazen_driver_api::capabilities::{
    CapabilityRegistry, CapabilitySet, CapabilitySnapshot, SessionContinuity,
};
use datazen_driver_api::namespace::{NamespaceShape, NamespaceTarget, TargetRequirements};
use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, CommandCall, ConnectionCostPolicy,
    DescribeResourceRequest, InitializationRequirement, ResourceDescriptor, ResourceError,
    ResourceHandle, ResourceProvider, ResultChunk, ResultSink, ReusePolicy,
};
use datazen_driver_api::session::{
    CancelDisposition, CancelReceipt, CloseDisposition, CompletionStatus, ContextChangeDisposition,
    EffectOutcome, ExecutionCompletion, ExecutionState, ObservationConfidence, ResetDisposition,
    ResourceHealth, SessionContext, SessionObservation, SessionState, TransactionObservation,
    TransactionOptions, TransactionState,
};
use datazen_driver_api::{
    ConnectionHandle, DatabaseDriver, DriverError, QueryExecutionId, SqlTarget, TransactionHandle,
    Value, PROTOCOL_VERSION,
};

use crate::mysql::MysqlDriver;
use crate::resource_capabilities::mysql_namespace_shape;

/// One acquired resource: the live driver connection plus the budget permit
/// that was taken for it. The permit is released exactly once, by
/// [`MysqlResourceProvider::close_resource`].
struct LiveResource {
    connection: ConnectionHandle,
    permit: BudgetPermit,
    budget: Arc<dyn BudgetPort>,
    /// The namespace this resource is currently pointed at, as last read back
    /// or last confirmed by `change_context`. Never a wish: it is written only
    /// from a server read-back.
    target: NamespaceTarget,
}

/// A transaction this provider opened. The driver pins the physical connection
/// (`mysql.rs::begin_transaction`), so the provider has to know about it:
/// while one is open, `change_context` must not pretend an in-place switch
/// happened.
struct OpenTransaction {
    handle: TransactionHandle,
    /// Opened through `begin_read_snapshot`, i.e. a `READ ONLY` repeatable-read
    /// snapshot rather than a plain transaction.
    snapshot: bool,
}

pub(crate) struct MysqlResourceProvider {
    driver: Arc<dyn DatabaseDriver>,
    driver_id: String,
    runtime_epoch: u64,
    namespace_shape: NamespaceShape,
    capabilities: CapabilityRegistry,
    live: Mutex<BTreeMap<String, LiveResource>>,
    transactions: Mutex<BTreeMap<String, OpenTransaction>>,
    revision: AtomicU64,
}

/// Per-instance runtime epoch. `ResourceHandle::runtime_epoch` is a `u64`,
/// so a monotonically increasing counter is the honest identifier: every
/// provider instance takes a different one and therefore rejects every handle
/// minted by another.
static RUNTIME_EPOCH_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Poison-tolerant lock. A panicking driver call must not turn every later
/// provider call into a panic.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `TransactionHandle` is neither `Clone` nor `Copy` — the driver takes it by
/// value. Re-mint an equivalent one instead of losing the bookkeeping.
fn copy_transaction(handle: &TransactionHandle) -> TransactionHandle {
    TransactionHandle {
        id: handle.id.clone(),
        connection_id: handle.connection_id.clone(),
    }
}

impl MysqlResourceProvider {
    /// Build a provider bound to one driver instance. The `runtime_epoch` is
    /// per instance, so a handle issued by one family id is rejected by every
    /// other id. `lib.rs` memoizes exactly one instance per factory, which is
    /// what lets a handle outlive the lookup that produced it.
    pub(crate) fn new(
        driver: Arc<dyn DatabaseDriver>,
        driver_id: &str,
        declared: CapabilitySet,
    ) -> Self {
        let snapshot =
            CapabilitySnapshot::new(driver_id, env!("CARGO_PKG_VERSION"), PROTOCOL_VERSION, 0);
        let mut capabilities = CapabilityRegistry::new(driver_id.to_string(), snapshot);
        capabilities.capabilities = declared;
        Self {
            driver,
            driver_id: driver_id.to_string(),
            runtime_epoch: RUNTIME_EPOCH_SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1,
            namespace_shape: mysql_namespace_shape(),
            capabilities,
            live: Mutex::new(BTreeMap::new()),
            transactions: Mutex::new(BTreeMap::new()),
            revision: AtomicU64::new(0),
        }
    }

    fn next_revision(&self) -> u64 {
        self.revision.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Validate the handle's ownership and runtime epoch, then look up the live
    /// connection. Every public method starts here.
    fn connection_of(
        &self,
        handle: &ResourceHandle,
        operation: &str,
    ) -> Result<ConnectionHandle, ResourceError> {
        handle.check(&self.driver_id, self.runtime_epoch)?;
        lock(&self.live)
            .get(handle.resource_key())
            .map(|resource| resource.connection.clone())
            .ok_or_else(|| ResourceError::InvalidResourceState {
                resource_key: handle.resource_key().to_string(),
                state: "resource is not held by this provider".to_string(),
                operation: operation.to_string(),
            })
    }

    /// The namespace this resource is currently pointed at, or an explicit
    /// error when the handle is not ours.
    fn target_of(&self, handle: &ResourceHandle) -> Result<NamespaceTarget, ResourceError> {
        handle.check(&self.driver_id, self.runtime_epoch)?;
        lock(&self.live)
            .get(handle.resource_key())
            .map(|resource| resource.target.clone())
            .ok_or_else(|| ResourceError::InvalidResourceState {
                resource_key: handle.resource_key().to_string(),
                state: "resource is not held by this provider".to_string(),
                operation: "target".to_string(),
            })
    }

    fn store_target(&self, handle: &ResourceHandle, target: NamespaceTarget) {
        if let Some(resource) = lock(&self.live).get_mut(handle.resource_key()) {
            resource.target = target;
        }
    }

    fn open_transaction_of(&self, handle: &ResourceHandle) -> Option<OpenTransaction> {
        lock(&self.transactions)
            .get(handle.resource_key())
            .map(|transaction| OpenTransaction {
                handle: copy_transaction(&transaction.handle),
                snapshot: transaction.snapshot,
            })
    }

    fn transaction_observation(&self, handle: &ResourceHandle) -> TransactionObservation {
        match self.open_transaction_of(handle) {
            Some(transaction) => TransactionObservation {
                state: TransactionState::Active,
                transaction_id: Some(transaction.handle.id.clone()),
                effect: None,
                revision: 0,
            },
            // No record means no claim. The physical connection comes from a
            // pool, so the provider cannot rule out a transaction somebody
            // else left open on it; `Idle` would be an invented observation.
            None => TransactionObservation {
                state: TransactionState::Unknown,
                transaction_id: None,
                effect: None,
                revision: 0,
            },
        }
    }

    /// Take the open transaction off the resource, or report that there is
    /// none. A caller must never "commit" or "roll back" nothing successfully.
    fn take_transaction(
        &self,
        handle: &ResourceHandle,
        operation: &str,
    ) -> Result<OpenTransaction, ResourceError> {
        lock(&self.transactions)
            .remove(handle.resource_key())
            .ok_or_else(|| ResourceError::InvalidResourceState {
                resource_key: handle.resource_key().to_string(),
                state: "no transaction is open on this resource".to_string(),
                operation: operation.to_string(),
            })
    }

    /// Reject a target the namespace shape cannot express before anything is
    /// connected or issued.
    fn validate_target(&self, target: &NamespaceTarget) -> Result<(), ResourceError> {
        self.namespace_shape
            .canonicalize(target)
            .and_then(|canonical| {
                self.namespace_shape
                    .validate_requirements(&canonical.requested, &TargetRequirements::default())
            })
            .map(|_| ())
    }

    /// `read DATABASE()` on the wire. This is the only accepted evidence of the
    /// current namespace; a remembered target is never treated as a read-back.
    async fn read_back_database(
        &self,
        connection: &ConnectionHandle,
    ) -> Result<Option<String>, ResourceError> {
        let result = self
            .driver
            .query_at(connection, "SELECT DATABASE()", SqlTarget::new(None, None))
            .await
            .map_err(ResourceError::Driver)?;
        Ok(result
            .rows
            .iter()
            .find_map(|row| row.first().and_then(first_string)))
    }

    /// Effect of a completed statement: with no transaction open the effects
    /// are durable, with one open they exist but a rollback can still discard
    /// them, which is `PartiallyApplied` and not `Completed`.
    fn effect_outcome(&self, handle: &ResourceHandle) -> EffectOutcome {
        if self.open_transaction_of(handle).is_some() {
            EffectOutcome::PartiallyApplied
        } else {
            EffectOutcome::Completed
        }
    }

    /// A completion that reports the driver command's own dispatch result.
    fn completion(
        &self,
        handle: &ResourceHandle,
        statement_results: Vec<datazen_driver_api::StatementResult>,
        effect_outcome: EffectOutcome,
    ) -> ExecutionCompletion {
        ExecutionCompletion {
            completion_status: CompletionStatus::Succeeded,
            effect_outcome,
            statement_results,
            // Not read on the hot path: `observe_session` is the method that
            // pays for a read-back. Reporting a remembered target here would be
            // an invented observation.
            context_before: SessionContext::unobserved(),
            context_after: SessionContext::unobserved(),
            transaction_observation: self.transaction_observation(handle),
            // `session_scoped_handles` is `Unsupported`: the provider issues
            // none, so it must not report any.
            session_handles: Vec::new(),
            protocol_drained: true,
            // The command round-tripped on this very connection, which is
            // evidence of liveness.
            resource_health: ResourceHealth::Healthy,
        }
    }
}

/// Pull the first string out of a `Value`, whether it arrived as a text column
/// or a typed one.
fn first_string(cell: &Option<Value>) -> Option<String> {
    let value = cell.as_ref()?;
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Bytes(bytes) => String::from_utf8(bytes.clone()).ok(),
        _ => None,
    }
}

/// Parse `{"sql": ..., "limit": ...}` out of a standard SQL command input.
/// `sql_input_with_target` is private to `driver-api`, so each provider parses
/// the standard envelope itself instead of duplicating a shared helper there.
fn sql_of(driver_id: &str, input: &serde_json::Value) -> Result<String, ResourceError> {
    input
        .get("sql")
        .and_then(|value| value.as_str())
        .map(|sql| sql.to_string())
        .filter(|sql| !sql.trim().is_empty())
        .ok_or_else(|| {
            ResourceError::unsupported(
                driver_id,
                "execute_on_resource",
                "command input requires a non-empty \"sql\" field",
            )
        })
}

fn limit_of(input: &serde_json::Value) -> Option<u32> {
    input
        .get("limit")
        .and_then(|value| value.as_u64())
        .map(|limit| limit.min(u32::MAX as u64) as u32)
}

#[async_trait::async_trait]
impl ResourceProvider for MysqlResourceProvider {
    fn provider_id(&self) -> &str {
        &self.driver_id
    }

    fn capabilities(&self) -> &CapabilityRegistry {
        &self.capabilities
    }

    fn namespace_shape(&self) -> &NamespaceShape {
        &self.namespace_shape
    }

    async fn describe_resource(
        &self,
        request: &DescribeResourceRequest,
    ) -> Result<ResourceDescriptor, ResourceError> {
        self.validate_target(&request.target)?;
        Ok(ResourceDescriptor {
            provider_id: self.driver_id.clone(),
            resource_key: request.connection_config.id.clone(),
            namespace_shape: self.namespace_shape.clone(),
            session_continuity: SessionContinuity::Leased,
            reuse_policy: ReusePolicy::Unknown,
            initialization_requirements: Vec::<InitializationRequirement>::new(),
            connection_cost_policy: ConnectionCostPolicy::DeclaredConservative {
                declared_cost: 1,
                hard_cap: None,
            },
        })
    }

    async fn acquire_resource(
        &self,
        request: &AcquireResourceRequest,
        budget: &Arc<dyn BudgetPort>,
    ) -> Result<ResourceHandle, ResourceError> {
        self.validate_target(&request.target)?;
        let permit = budget
            .acquire_physical_connections(request.scope.max_physical_connections.max(1))
            .await?;
        let connection = match self.driver.connect(&request.connection_config).await {
            Ok(connection) => connection,
            Err(error) => {
                // The permit was taken before the connect; a failed connect must
                // give it back, exactly once.
                let _ = budget.release_physical_connections(&permit).await;
                return Err(ResourceError::Driver(error));
            }
        };
        let handle = ResourceHandle::issue(
            self.driver_id.clone(),
            connection.id.clone(),
            self.runtime_epoch,
        );
        let target = request
            .target
            .database
            .as_deref()
            .map(|database| NamespaceTarget::empty().with_database(database.to_string()))
            .unwrap_or_else(NamespaceTarget::empty);
        lock(&self.live).insert(
            handle.resource_key().to_string(),
            LiveResource {
                connection,
                permit,
                budget: Arc::clone(budget),
                target,
            },
        );
        Ok(handle)
    }

    async fn execute_on_resource(
        &self,
        handle: &ResourceHandle,
        execution_id: &QueryExecutionId,
        call: &CommandCall,
        sink: &dyn ResultSink,
    ) -> Result<ExecutionCompletion, ResourceError> {
        let connection = self.connection_of(handle, "execute_on_resource")?;
        let target = self.target_of(handle)?;
        let sql_target = SqlTarget::new(target.database.as_deref(), target.schema.as_deref());

        // Register this execution id with the driver so `request_cancel` can
        // address exactly this run and never fall back to a session-wide kill.
        self.driver
            .prepare_query_execution(&connection, execution_id)
            .await
            .map_err(ResourceError::Driver)?;

        let outcome = match call.command.as_str() {
            "query" => {
                let sql = sql_of(&self.driver_id, &call.input)?;
                let multi = self
                    .driver
                    .query_multi_at(&connection, &sql, limit_of(&call.input), sql_target)
                    .await
                    .map_err(ResourceError::Driver)?;
                for (statement_index, statement) in multi.results.iter().enumerate() {
                    // Awaiting the write is the backpressure signal: a slow
                    // sink throttles the producer instead of buffering.
                    sink.write(ResultChunk {
                        statement_index,
                        sql: statement.sql.clone(),
                        rows: statement.rows.clone(),
                        rows_affected: statement.rows_affected,
                    })
                    .await?;
                }
                let results = multi.results;
                self.completion(handle, results, self.effect_outcome(handle))
            }
            "execute" => {
                let sql = sql_of(&self.driver_id, &call.input)?;
                let rows_affected = self
                    .driver
                    .execute_at(&connection, &sql, sql_target)
                    .await
                    .map_err(ResourceError::Driver)?;
                sink.write(ResultChunk {
                    statement_index: 0,
                    sql: sql.clone(),
                    rows: Vec::new(),
                    rows_affected: Some(rows_affected),
                })
                .await?;
                self.completion(handle, Vec::new(), self.effect_outcome(handle))
            }
            other => {
                // Anything else is a driver command: the driver owns its own
                // dispatch (admin commands, DDL helpers, ...). The result is
                // carried losslessly as one JSON row.
                let result = self
                    .driver
                    .execute_command(&connection, other, call.input.clone())
                    .await
                    .map_err(ResourceError::Driver)?;
                sink.write(ResultChunk {
                    statement_index: 0,
                    sql: other.to_string(),
                    rows: vec![vec![Some(Value::Json(result.data))]],
                    rows_affected: None,
                })
                .await?;
                self.completion(handle, Vec::new(), self.effect_outcome(handle))
            }
        };

        // Deregistering is best effort: a cleanup failure must not turn a
        // completed execution into an error the caller sees as a query failure.
        if let Err(_error) = self
            .driver
            .cleanup_query_execution(&connection, execution_id)
            .await
        {
            tracing::debug!(
                driver = %self.driver_id,
                command = %call.command,
                "execution cleanup did not complete"
            );
        }

        sink.complete().await?;
        Ok(outcome)
    }

    async fn observe_session(
        &self,
        handle: &ResourceHandle,
    ) -> Result<SessionObservation, ResourceError> {
        let connection = self.connection_of(handle, "observe_session")?;
        let revision = self.next_revision();
        let transaction = self.transaction_observation(handle);
        let transaction_state = transaction.state;

        // Real read-back: the only accepted evidence of the current namespace.
        let (namespace, observed) = match self.read_back_database(&connection).await? {
            Some(database) => (NamespaceTarget::empty().with_database(database), true),
            None => (NamespaceTarget::empty(), false),
        };
        if observed {
            self.store_target(handle, namespace.clone());
        }

        Ok(SessionObservation {
            state: SessionState::Ready,
            context: SessionContext {
                namespace,
                // MySQL has no `search_path`, and the provider never queries
                // the authenticated identity, so both stay empty rather than
                // being guessed.
                search_path: Vec::new(),
                effective_identity: None,
                transaction_state,
                // `None` is "the driver cannot tell", which is the truth: the
                // provider never issues `SELECT @@autocommit`.
                autocommit: None,
                // `Partial`, matching the declared `context_observation`: the
                // namespace came off the wire but the transaction state is the
                // provider's own bookkeeping, so the context as a whole is not
                // fully confirmed.
                confidence: if observed {
                    ObservationConfidence::Partial
                } else {
                    ObservationConfidence::Unknown
                },
            },
            transaction,
            handles: Vec::new(),
            protocol_drained: true,
            resource_health: ResourceHealth::Healthy,
            context_revision: revision,
        })
    }

    async fn change_context(
        &self,
        handle: &ResourceHandle,
        desired: &NamespaceTarget,
    ) -> Result<ContextChangeDisposition, ResourceError> {
        let connection = self.connection_of(handle, "change_context")?;
        self.validate_target(desired)?;
        let database = match desired.database.as_deref() {
            Some(database) if !database.trim().is_empty() => database.trim().to_string(),
            // No database in the request: there is nothing this driver could
            // switch to. `unsupported` is the honest answer, not a fake success.
            _ => return Ok(ContextChangeDisposition::Unsupported),
        };

        if self.open_transaction_of(handle).is_some() {
            // The driver pinned a physical connection for the transaction
            // (`mysql.rs::begin_transaction`) and that connection does not
            // re-apply the pool's active database. Switching now would report
            // a context the open transaction is not running in.
            return Ok(ContextChangeDisposition::RequiresReplacement);
        }

        let use_sql =
            MysqlDriver::build_use_database_sql(&database).map_err(ResourceError::Driver)?;
        self.driver
            .execute_at(&connection, &use_sql, SqlTarget::new(None, None))
            .await
            .map_err(ResourceError::Driver)?;

        // Confirm on the wire. Anything other than an exact match is not a
        // confirmed switch, whatever the `USE` returned.
        match self.read_back_database(&connection).await? {
            Some(observed) if observed == database => {
                self.store_target(handle, NamespaceTarget::empty().with_database(database));
                Ok(ContextChangeDisposition::Confirmed)
            }
            _ => Ok(ContextChangeDisposition::RequiresReplacement),
        }
    }

    async fn begin_transaction(
        &self,
        handle: &ResourceHandle,
        options: &TransactionOptions,
    ) -> Result<TransactionObservation, ResourceError> {
        let connection = self.connection_of(handle, "begin_transaction")?;
        if let Some(level) = options.isolation_level.as_deref() {
            if !self
                .capabilities
                .capabilities
                .transactions
                .isolation_levels
                .iter()
                .any(|declared| declared.eq_ignore_ascii_case(level.trim()))
            {
                return Err(ResourceError::unsupported(
                    &self.driver_id,
                    "begin_transaction",
                    format!("isolation level `{level}` is not declared by this provider"),
                ));
            }
        }
        if self.open_transaction_of(handle).is_some() {
            return Err(ResourceError::InvalidResourceState {
                resource_key: handle.resource_key().to_string(),
                state: "a transaction is already open on this resource".to_string(),
                operation: "begin_transaction".to_string(),
            });
        }
        // A read-only request maps onto the driver's real
        // `START TRANSACTION WITH CONSISTENT SNAPSHOT, READ ONLY`; anything
        // else is a plain transaction. The two are different wire statements,
        // so the distinction is preserved instead of being flattened.
        let snapshot = options.read_only == Some(true);
        let transaction = if snapshot {
            self.driver.begin_read_snapshot(&connection).await
        } else {
            self.driver.begin_transaction(&connection).await
        }
        .map_err(ResourceError::Driver)?;
        let revision = self.next_revision();
        lock(&self.transactions).insert(
            handle.resource_key().to_string(),
            OpenTransaction {
                handle: copy_transaction(&transaction),
                snapshot,
            },
        );
        tracing::debug!(
            driver = %self.driver_id,
            snapshot,
            "transaction opened on a pinned connection"
        );
        Ok(TransactionObservation::begun(transaction.id, revision))
    }

    async fn commit_transaction(
        &self,
        handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        // Ownership and epoch validation only: the driver resolves the pinned
        // connection itself, out of the `TransactionHandle`.
        self.connection_of(handle, "commit_transaction")?;
        let transaction = self.take_transaction(handle, "commit_transaction")?;
        let revision = self.next_revision();
        match self.driver.commit(transaction.handle).await {
            Ok(()) => Ok(TransactionObservation::finished(
                TransactionState::Idle,
                EffectOutcome::Completed,
                revision,
            )),
            // A failed COMMIT leaves the outcome genuinely unknown. Reporting
            // `Unknown` is the supported way to say so; claiming `RolledBack`
            // or `Completed` here would be a fabrication.
            Err(_error) => Ok(TransactionObservation::finished(
                TransactionState::Unknown,
                EffectOutcome::Unknown,
                revision,
            )),
        }
    }

    async fn rollback_transaction(
        &self,
        handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        // Ownership and epoch validation only: the driver resolves the pinned
        // connection itself, out of the `TransactionHandle`.
        self.connection_of(handle, "rollback_transaction")?;
        let transaction = self.take_transaction(handle, "rollback_transaction")?;
        let revision = self.next_revision();
        match self.driver.rollback(transaction.handle).await {
            Ok(()) => Ok(TransactionObservation::finished(
                TransactionState::Idle,
                EffectOutcome::RolledBack,
                revision,
            )),
            Err(_error) => Ok(TransactionObservation::finished(
                TransactionState::Unknown,
                EffectOutcome::Unknown,
                revision,
            )),
        }
    }

    async fn request_cancel(
        &self,
        handle: &ResourceHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<CancelReceipt, ResourceError> {
        let connection = self.connection_of(handle, "request_cancel")?;
        let receipt = |disposition: CancelDisposition| CancelReceipt {
            execution_id: execution_id.clone(),
            disposition,
            state: None,
        };
        if !self.driver.supports_query_execution_cancel() {
            return Ok(receipt(CancelDisposition::Unsupported));
        }
        match self
            .driver
            .cancel_query_with_execution(&connection, execution_id)
            .await
        {
            Ok(()) => Ok(CancelReceipt {
                execution_id: execution_id.clone(),
                disposition: CancelDisposition::Requested,
                state: Some(ExecutionState::CancelRequested),
            }),
            // Not addressable by this driver: an explicit refusal, never a
            // session-wide kill and never a silent success.
            Err(DriverError::Unsupported(_))
            | Err(DriverError::NotSupported(_))
            | Err(DriverError::QueryExecutionSessionMismatch) => {
                Ok(receipt(CancelDisposition::Unsupported))
            }
            Err(DriverError::QueryExecutionNotFound(_)) => {
                Ok(receipt(CancelDisposition::NotRegistered))
            }
            Err(error) => Err(ResourceError::Driver(error)),
        }
    }

    async fn reset_resource(
        &self,
        handle: &ResourceHandle,
        _baseline: &Baseline,
    ) -> Result<ResetDisposition, ResourceError> {
        let _ = self.connection_of(handle, "reset_resource")?;
        // `reset_for_reuse` is `Unsupported`: the provider has no verified
        // path back to a baseline, and a pool of size one is not a reason to
        // claim one (migration doc §6.1). The caller must re-acquire.
        Ok(ResetDisposition::Discard)
    }

    async fn close_resource(
        &self,
        handle: &ResourceHandle,
    ) -> Result<CloseDisposition, ResourceError> {
        handle.check(&self.driver_id, self.runtime_epoch)?;
        // Drop the bookkeeping first so a repeated close cannot double-release
        // the budget permit or reconnect a live resource.
        let resource = lock(&self.live).remove(handle.resource_key());
        let transaction = lock(&self.transactions).remove(handle.resource_key());
        let Some(resource) = resource else {
            // Already closed. `check` above still rejects a foreign or stale
            // handle; a handle of ours that is simply gone is not an error,
            // and it must not charge the budget a second time.
            return Ok(CloseDisposition::Closed);
        };

        let outcome = self.driver.disconnect(resource.connection.clone()).await;
        match outcome {
            Ok(()) => {
                resource
                    .budget
                    .release_physical_connections(&resource.permit)
                    .await?;
                if transaction.is_some() {
                    // The permit is genuinely released above: `disconnect`
                    // closed the physical connection, so the charge is over.
                    // What is *not* clean is the caller's transaction —
                    // `disconnect` rolled the pinned connection back without
                    // the caller asking — so the close is not confirmed.
                    tracing::warn!(
                        driver = %self.driver_id,
                        "closed a resource that still had an open transaction; the driver rolled it back"
                    );
                    return Ok(CloseDisposition::CloseUnconfirmed);
                }
                Ok(CloseDisposition::Closed)
            }
            Err(error) => {
                // The connection may still be alive, so the permit is not
                // released: the budget stays charged until the leak is settled.
                tracing::warn!(
                    driver = %self.driver_id,
                    "disconnect failed during close; the budget permit is retained"
                );
                Err(ResourceError::Driver(error))
            }
        }
    }
}

#[cfg(test)]
#[path = "resource_capability_tests.rs"]
mod capability_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
#[path = "resource_tests.rs"]
mod tests;

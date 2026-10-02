//! The real [`ResourceProvider`] for the mongodb driver.
//!
//! `lib.rs` hands out one provider instance per `resource_provider()` call, so
//! each instance carries its own `runtime_epoch` and its own
//! `CapabilityRegistry` and rejects every handle minted by another.
//!
//! Three design rules shape everything below.
//!
//! 1. **The provider only ever speaks the driver trait.** It works through
//!    `Arc<dyn DatabaseDriver>` and never downcasts, so a wrapped driver keeps
//!    working exactly as it does through the registry.
//! 2. **A MongoDB client is not a session.** `MongodbDriver::connect` stores a
//!    `Client` — MongoDB's own stateless connection pool — and every statement
//!    names its database explicitly. So the descriptor reports
//!    `SessionContinuity::Leased`, `stateful_session` stays `Unsupported`, and
//!    `change_context` refuses outright instead of pretending it moved
//!    something. See `driver-capability-migration.md` §2.1 事实二 and §6.1.
//! 3. **Nothing is reported that was not observed.** There is no session
//!    context to read back, no transaction this driver opened, and no cursor
//!    handle to interrupt, so `observe_session` reports `Unknown` rather than a
//!    guess and `request_cancel` refuses rather than returning a receipt that
//!    claims work nobody did.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use datazen_driver_api::capabilities::{CapabilityRegistry, CapabilitySnapshot, SessionContinuity};
use datazen_driver_api::namespace::{NamespaceShape, NamespaceTarget, TargetRequirements};
use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, CommandCall, ConnectionCostPolicy,
    DescribeResourceRequest, InitializationRequirement, ResourceDescriptor, ResourceError,
    ResourceHandle, ResourceProvider, ResultChunk, ResultSink, ReusePolicy,
};
use datazen_driver_api::session::{
    CancelDisposition, CancelReceipt, CloseDisposition, CompletionStatus, ContextChangeDisposition,
    EffectOutcome, ExecutionCompletion, ResetDisposition, ResourceHealth, SessionContext,
    SessionObservation, TransactionObservation, TransactionOptions, TransactionState,
};
use datazen_driver_api::{
    ConnectionHandle, DatabaseDriver, QueryExecutionId, SqlTarget, Value, PROTOCOL_VERSION,
};

use crate::resource_capabilities::{mongodb_capability_set, mongodb_namespace_shape};

/// One acquired resource: the live driver connection plus the budget permit
/// that was taken for it. The permit is released exactly once, by
/// [`MongodbResourceProvider::close_resource`].
struct LiveResource {
    connection: ConnectionHandle,
    permit: BudgetPermit,
    budget: Arc<dyn BudgetPort>,
}

pub(crate) struct MongodbResourceProvider {
    driver: Arc<dyn DatabaseDriver>,
    driver_id: String,
    runtime_epoch: u64,
    namespace_shape: NamespaceShape,
    capabilities: CapabilityRegistry,
    live: Mutex<BTreeMap<String, LiveResource>>,
    revision: AtomicU64,
}

/// Per-instance runtime epoch. `ResourceHandle::runtime_epoch` is a `u64`, so a
/// monotonically increasing counter is the honest identifier: every provider
/// instance takes a different one and therefore rejects every handle minted by
/// another.
static RUNTIME_EPOCH_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Poison-tolerant lock. A panicking driver call must not turn every later
/// provider call into a panic.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl MongodbResourceProvider {
    /// Build a provider bound to one driver instance. The `runtime_epoch` is
    /// fresh per call, so a handle issued by one factory instance is rejected
    /// by every other.
    pub(crate) fn new(driver: Arc<dyn DatabaseDriver>, driver_id: &str) -> Self {
        let snapshot =
            CapabilitySnapshot::new(driver_id, env!("CARGO_PKG_VERSION"), PROTOCOL_VERSION, 0);
        let mut capabilities = CapabilityRegistry::new(driver_id.to_string(), snapshot);
        capabilities.capabilities = mongodb_capability_set();
        Self {
            driver,
            driver_id: driver_id.to_string(),
            runtime_epoch: RUNTIME_EPOCH_SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1,
            namespace_shape: mongodb_namespace_shape(),
            capabilities,
            live: Mutex::new(BTreeMap::new()),
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

    /// The refusal every transaction entry point returns.
    ///
    /// A MongoDB deployment only offers multi-document transactions inside a
    /// server-side session this driver never opens, so `transactions` declares
    /// no isolation level and no savepoints. Returning this error says the
    /// capability is missing; opening something and calling it a transaction
    /// would be worse.
    fn no_transactions(&self, operation: &str) -> ResourceError {
        ResourceError::unsupported(
            &self.driver_id,
            operation,
            "mongodb only offers multi-document transactions inside a server-side session this driver does not open",
        )
    }

    /// The provider opened no transaction and tracks none, so every
    /// observation is `Unknown`. There is no ambient transaction to inherit
    /// from a client either: a MongoDB client holds none.
    fn transaction_observation(&self) -> TransactionObservation {
        TransactionObservation {
            state: TransactionState::Unknown,
            transaction_id: None,
            effect: None,
            revision: 0,
        }
    }

    /// A completion that reports the driver command's own dispatch result.
    ///
    /// The effect outcome is `Unknown`, not `Completed`: a MongoDB write is
    /// acknowledged by the primary (or by a configured write concern), but the
    /// provider did not open a transaction that could still roll it back and it
    /// holds no read-back that would prove durability. §2.1 declares
    /// `context_observation` and `transaction_observation` `Unsupported`, and
    /// this field follows the same rule — report only what was observed.
    fn completion(
        &self,
        statement_results: Vec<datazen_driver_api::StatementResult>,
    ) -> ExecutionCompletion {
        ExecutionCompletion {
            completion_status: CompletionStatus::Succeeded,
            effect_outcome: EffectOutcome::Unknown,
            statement_results,
            // Not read on the hot path: `observe_session` is the method that
            // would pay for a read-back, and there is nothing there to read.
            context_before: SessionContext::unobserved(),
            context_after: SessionContext::unobserved(),
            transaction_observation: self.transaction_observation(),
            // `session_scoped_handles` is `Unsupported`: the provider issues
            // none, so it must not report any.
            session_handles: Vec::new(),
            protocol_drained: true,
            // The command round-tripped on this very client, which is evidence
            // of liveness — and nothing more is claimed.
            resource_health: ResourceHealth::Healthy,
        }
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
                "command input requires a non-empty \"sql\" field holding a MongoDB JSON command",
            )
        })
}

fn limit_of(input: &serde_json::Value) -> Option<u32> {
    input
        .get("limit")
        .and_then(|value| value.as_u64())
        .map(|limit| limit.min(u32::MAX as u64) as u32)
}

/// The database the JSON command is aimed at.
///
/// MongoDB commands carry their own `"database"` field (`mongodb.rs::query`
/// and `::execute` read exactly this), so the statement names its own
/// namespace and the provider never has to remember one.
fn database_of(input: &serde_json::Value) -> Option<String> {
    input
        .get("database")
        .and_then(|value| value.as_str())
        .map(|database| database.to_string())
        .filter(|database| !database.trim().is_empty())
}

#[async_trait::async_trait]
impl ResourceProvider for MongodbResourceProvider {
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
            // Not `Fixed`: `connect` stores a `Client`, and a client that owns
            // MongoDB's own connection pool is not a session the host may pin
            // state onto (§6.1 / CM-19).
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
        lock(&self.live).insert(
            handle.resource_key().to_string(),
            LiveResource {
                connection,
                permit,
                budget: Arc::clone(budget),
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
        let database = database_of(&call.input);
        let sql_target = SqlTarget::new(database.as_deref(), None);

        // Register this execution id so `request_cancel` can tell "not
        // addressable" apart from "never registered". mongodb refuses both, but
        // it refuses them for different reasons and the receipt says which.
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
                self.completion(multi.results)
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
                self.completion(Vec::new())
            }
            other => {
                // Anything else is a driver command: the driver owns its own
                // dispatch (schema object commands, migration helpers, ...). The
                // result is carried losslessly as one JSON row.
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
                self.completion(Vec::new())
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
        // Ownership and epoch are still validated: a foreign handle must not be
        // able to read anything out of this method, not even a null answer.
        self.connection_of(handle, "observe_session")?;
        // A MongoDB client reports no current database, no autocommit flag and
        // no search path — every one of those lives on the *statement*, not on
        // the connection. `context_observation` is `Unsupported`, so the honest
        // answer is `unobservable()`: `Unknown` state, `Unknown` confidence, no
        // invented namespace. The revision still advances so a caller can tell
        // this observation from a remembered one.
        let mut observation = SessionObservation::unobservable();
        // `unobservable()` already answers `Unsupported` for the transaction and
        // `Unknown` for the context. Only the revision is this provider's to
        // advance; every other field stays at the honest default.
        observation.context_revision = self.next_revision();
        Ok(observation)
    }

    async fn change_context(
        &self,
        handle: &ResourceHandle,
        _desired: &NamespaceTarget,
    ) -> Result<ContextChangeDisposition, ResourceError> {
        // Ownership and epoch are still validated: a foreign handle must not be
        // able to read anything out of this method.
        self.connection_of(handle, "change_context")?;
        // The database is named per statement by the command itself
        // (`mongodb.rs::resolve_database` takes the explicit argument and never
        // mutates pool state), so there is no session state to switch. `InPlace`
        // would be a lie and `RequiresReplacement` would promise a switch that
        // cannot happen: this is a plain refusal.
        Ok(ContextChangeDisposition::Unsupported)
    }

    async fn begin_transaction(
        &self,
        handle: &ResourceHandle,
        _options: &TransactionOptions,
    ) -> Result<TransactionObservation, ResourceError> {
        // Validate the handle first so the refusal is about the capability, not
        // about the caller's ownership of the resource.
        self.connection_of(handle, "begin_transaction")?;
        Err(self.no_transactions("begin_transaction"))
    }

    async fn commit_transaction(
        &self,
        handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        self.connection_of(handle, "commit_transaction")?;
        // Never a successful no-op: nothing was ever begun, so there is nothing
        // to commit, and reporting `Idle`/`Completed` would claim an outcome no
        // server produced.
        Err(self.no_transactions("commit_transaction"))
    }

    async fn rollback_transaction(
        &self,
        handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        self.connection_of(handle, "rollback_transaction")?;
        Err(self.no_transactions("rollback_transaction"))
    }

    async fn request_cancel(
        &self,
        handle: &ResourceHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<CancelReceipt, ResourceError> {
        let _ = self.connection_of(handle, "request_cancel")?;
        // `precise_cancel` is `Unsupported` and the driver holds no cursor
        // handle (`MongodbDriver::cancel_query` now answers
        // `DriverError::Unsupported` rather than the `Ok(())` §7.1 calls out as
        // the last broken fail-closed default), so there is nothing to ask.
        // This is the explicit refusal the contract asks for — never a
        // session-wide kill and never a silent `Ok` that implies a cancellation
        // nobody performed.
        Ok(CancelReceipt {
            execution_id: execution_id.clone(),
            disposition: CancelDisposition::Unsupported,
            state: None,
        })
    }

    async fn reset_resource(
        &self,
        handle: &ResourceHandle,
        _baseline: &Baseline,
    ) -> Result<ResetDisposition, ResourceError> {
        let _ = self.connection_of(handle, "reset_resource")?;
        // `reset_for_reuse` is `Unsupported`: the provider has no verified path
        // back to a baseline, and "the client holds no state" is an argument
        // about why nothing needs resetting — not evidence that the caller got
        // its initialisation back (§6.1 / CM-19). The caller must re-acquire.
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
        let Some(resource) = resource else {
            // Already closed. `check` above still rejects a foreign or stale
            // handle; a handle of ours that is simply gone is not an error, and
            // it must not charge the budget a second time.
            return Ok(CloseDisposition::Closed);
        };

        match self.driver.disconnect(resource.connection.clone()).await {
            Ok(()) => {
                // The permit is released exactly once, after the client is
                // really gone — never claimed as recovered when the close
                // failed.
                resource
                    .budget
                    .release_physical_connections(&resource.permit)
                    .await?;
                // No transaction was ever opened through this provider, so a
                // close is clean by construction.
                Ok(CloseDisposition::Closed)
            }
            Err(error) => {
                // The client may still be alive, so the permit is not released:
                // the budget stays charged until the leak is settled.
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

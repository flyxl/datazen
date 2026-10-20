//! The PostgreSQL resource provider's bookkeeping.
//!
//! This is a real implementation, not a shell. Every contract method — all
//! fifteen, in [`contract`] — is bound to this crate's own connection and
//! execution machinery:
//!
//! | contract method | what actually runs |
//! |---|---|
//! | [`ResourceProvider::describe_resource`] | pure math over the request + [`postgres_namespace_shape`](super::capabilities::postgres_namespace_shape) |
//! | [`ResourceProvider::acquire_resource`] | `BudgetPort::acquire_physical_connections`, then `PostgresDriver::connect_impl` (`connection.rs:381`), which opens the pool **and** its control pool (`connection.rs:424`) |
//! | [`ResourceProvider::execute_on_resource`] | `prepare_query_execution_impl` → `PostgresDriver::execute_command_impl` (`catalog.rs:378`) → the rows are decoded and streamed into the sink |
//! | [`ResourceProvider::observe_session`] | one real round trip on the pinned or pooled backend (`observation.rs`) |
//! | begin / commit / rollback | `begin_transaction_impl` / `commit_impl` / `rollback_impl` (`execution.rs:915` / `:980` / `:997`), which really pin a `PoolConnection` |
//! | [`ResourceProvider::request_cancel`] | `cancel_query_with_execution_impl` (`execution.rs:369`): execution-addressed, `pg_cancel_backend` on a *separate* control pool |
//! | [`ResourceProvider::close_resource`] | `disconnect_impl` (`connection.rs:444`) plus exactly one budget release |
//! | [`ResourceProvider::close_handle`] | `rollback_impl` on the *named* pinned transaction (`take_named_transaction` first proves the ref is the one open) |
//!
//! The four rules this provider never breaks are stated where they are
//! enforced: ownership on [`PostgresResourceProvider::resource_of`], explicit
//! refusal in [`PostgresResourceProvider::validate_acquisition`], truthful
//! observation in [`observation`] and [`PostgresResourceProvider::context_of`],
//! and one-and-only-one budget release in
//! [`PostgresResourceProvider::close_resource`]. Their test-visible summary
//! lives with the methods in [`contract`].

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use datazen_driver_api::capabilities::CapabilityRegistry;
use datazen_driver_api::namespace::NamespaceShape;
use datazen_driver_api::resource::{
    AcquireResourceRequest, BudgetPermit, BudgetPort, CommandCall, ConnectionCostPolicy,
    ResourceError, ResourceHandle, ResultSink,
};
use datazen_driver_api::session::{
    CompletionStatus, EffectOutcome, ExecutionCompletion, ResourceHealth, SessionContext,
    SessionHandleKind, SessionHandleRef, TransactionObservation, TransactionState,
};
use datazen_driver_api::{ConnectionHandle, DriverError};
use sqlx::PgPool;

use crate::postgres::PostgresDriver;

use super::capabilities::{
    postgres_capability_registry, postgres_namespace_shape, POSTGRES_PROVIDER_ID,
};
use super::observation;
use super::payload::{decode_command_result, StatementOutput};
use super::registry::{LiveResource, OpenTransaction, ResourceRegistry};

/// Each provider instance takes the next epoch. A handle issued by an older
/// instance of the same driver then fails the epoch check, instead of silently
/// addressing whatever connection happens to occupy the same key today.
static PROVIDER_EPOCH: AtomicU64 = AtomicU64::new(1);

fn next_runtime_epoch() -> u64 {
    PROVIDER_EPOCH.fetch_add(1, Ordering::SeqCst) + 1
}

/// PostgreSQL's implementation of the resource contract.
pub struct PostgresResourceProvider {
    driver: Arc<PostgresDriver>,
    capabilities: CapabilityRegistry,
    namespace_shape: NamespaceShape,
    runtime_epoch: u64,
    resources: Mutex<ResourceRegistry>,
    close_gate: tokio::sync::Mutex<()>,
}

impl PostgresResourceProvider {
    /// Bind a provider to a driver instance. The driver *is* the provider's
    /// real connection surface, so the `Arc` is shared rather than re-created:
    /// the host and the provider must see the same pools.
    pub fn new(driver: Arc<PostgresDriver>) -> Self {
        Self {
            driver,
            capabilities: postgres_capability_registry(),
            namespace_shape: postgres_namespace_shape(),
            runtime_epoch: next_runtime_epoch(),
            resources: Mutex::new(ResourceRegistry::default()),
            close_gate: tokio::sync::Mutex::new(()),
        }
    }

    /// The driver whose resources this provider hands out.
    pub fn driver(&self) -> &Arc<PostgresDriver> {
        &self.driver
    }

    /// The epoch stamped into every handle this provider issues.
    pub fn runtime_epoch(&self) -> u64 {
        self.runtime_epoch
    }

    /// Poison-tolerant lock. A panic in another task must not wedge every
    /// later resource call, and what this mutex guards is plain bookkeeping.
    /// The guard is always dropped before an `.await` — no lock is ever held
    /// across a suspension point.
    fn lock(&self) -> MutexGuard<'_, ResourceRegistry> {
        self.resources
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Ownership + epoch, then the resource record. Every public method starts
    /// here, so no operation can act on a handle it does not own.
    fn resource_of(
        &self,
        handle: &ResourceHandle,
        operation: &str,
    ) -> Result<LiveResource, ResourceError> {
        handle.check(POSTGRES_PROVIDER_ID, self.runtime_epoch)?;
        self.lock()
            .live
            .get(handle.resource_key())
            .filter(|resource| !resource.close_started)
            .map(LiveResource::snapshot)
            .ok_or_else(|| ResourceError::InvalidResourceState {
                resource_key: handle.resource_key().to_string(),
                operation: operation.to_string(),
                state: "this provider instance holds no open resource with this key: it was \
                        never acquired here, or it was already closed"
                    .to_string(),
            })
    }

    /// Take the open transaction out of the record, or refuse.
    fn take_transaction(
        &self,
        handle: &ResourceHandle,
        operation: &str,
    ) -> Result<OpenTransaction, ResourceError> {
        handle.check(POSTGRES_PROVIDER_ID, self.runtime_epoch)?;
        let key = handle.resource_key().to_string();
        let mut registry = self.lock();
        let resource = registry
            .live
            .get_mut(&key)
            .filter(|resource| !resource.close_started)
            .ok_or_else(|| ResourceError::InvalidResourceState {
                resource_key: key.clone(),
                operation: operation.to_string(),
                state: "no resource with this key is open on this provider".to_string(),
            })?;
        resource
            .transaction
            .take()
            .ok_or_else(|| ResourceError::TransactionResolutionRequired {
                operation: operation.to_string(),
            })
    }

    /// Take out the open transaction **only if it is the one named**.
    ///
    /// [`Self::take_transaction`] answers "is a transaction open?"; this one
    /// answers "is *this* handle's transaction open?", which is the question
    /// [`ResourceProvider::close_handle`] has to ask. The distinction matters:
    /// a host that replays a stale ref, or races a `commit_transaction`, must
    /// be told which of the two happened — not quietly handed whichever
    /// transaction happens to be open now.
    fn take_named_transaction(
        &self,
        handle: &ResourceHandle,
        handle_id: &str,
    ) -> Result<OpenTransaction, ResourceError> {
        handle.check(POSTGRES_PROVIDER_ID, self.runtime_epoch)?;
        let key = handle.resource_key().to_string();
        let mut registry = self.lock();
        let resource = registry
            .live
            .get_mut(&key)
            .filter(|resource| !resource.close_started)
            .ok_or_else(|| ResourceError::InvalidResourceState {
                resource_key: key.clone(),
                operation: "close_handle".to_string(),
                state: "no resource with this key is open on this provider".to_string(),
            })?;

        let open = resource
            .transaction
            .as_ref()
            .map(|transaction| transaction.id.clone());
        match open {
            Some(id) if id == handle_id => {
                resource
                    .transaction
                    .take()
                    .ok_or_else(|| ResourceError::InvalidResourceState {
                        resource_key: key.clone(),
                        operation: "close_handle".to_string(),
                        state: "the named session handle left the open transaction while it was \
                                being closed"
                            .to_string(),
                    })
            }
            Some(other) => Err(ResourceError::InvalidResourceState {
                resource_key: key.clone(),
                operation: "close_handle".to_string(),
                state: format!(
                    "session handle '{handle_id}' is not the transaction open on this resource; \
                     the open one is '{other}'"
                ),
            }),
            None => Err(ResourceError::InvalidResourceState {
                resource_key: key,
                operation: "close_handle".to_string(),
                state: format!(
                    "no transaction is open on this resource, so session handle '{handle_id}' \
                     names nothing this provider holds"
                ),
            }),
        }
    }

    /// Advance the resource's revision under the lock.
    fn bump_revision(&self, handle: &ResourceHandle) -> u64 {
        let mut registry = self.lock();
        match registry.live.get_mut(handle.resource_key()) {
            Some(resource) => {
                resource.revision += 1;
                resource.revision
            }
            // The record vanished between the check and here (a concurrent
            // close): report 0 rather than inventing a revision.
            None => 0,
        }
    }

    /// The pool the driver opened for this session. Absent means the session
    /// is gone, which is reported — never papered over with a fake context.
    async fn pool_of(
        &self,
        connection: &ConnectionHandle,
        operation: &str,
    ) -> Result<PgPool, ResourceError> {
        self.driver
            .pools
            .read()
            .await
            .get(&connection.pool_id)
            .cloned()
            .ok_or_else(|| ResourceError::InvalidResourceState {
                resource_key: connection.id.clone(),
                operation: operation.to_string(),
                state: "the driver holds no connection pool for this session".to_string(),
            })
    }

    /// The live context, read from the server.
    ///
    /// With an open transaction the read runs on the pinned connection and is
    /// [`observation::pinned_context`]. Without one, the backend comes from the
    /// pool and only pool-constant facts are reported
    /// ([`observation::leased_context`]): the caller still learns the session's
    /// database and principal, but not a context it could mistake for
    /// confirmed.
    async fn context_of(&self, resource: &LiveResource) -> Result<SessionContext, ResourceError> {
        if self
            .driver
            .fixed_sessions
            .read()
            .await
            .contains_key(&resource.connection.id)
        {
            return self
                .driver
                .read_fixed_session(&resource.connection)
                .await
                .map_err(ResourceError::Driver);
        }
        if let Some(transaction) = resource.transaction.as_ref() {
            let facts = observation::read_pinned_session(
                &self.driver.transactions,
                &transaction.connection_id,
            )
            .await?;
            return Ok(observation::pinned_context(&facts));
        }
        let pool = self
            .pool_of(&resource.connection, "read_session_context")
            .await?;
        let facts = observation::read_pooled_session(&pool).await?;
        Ok(observation::leased_context(&facts))
    }

    /// The transaction observation that matches [`Self::context_of`].
    fn transaction_observation_of(&self, resource: &LiveResource) -> TransactionObservation {
        match resource.transaction.as_ref() {
            Some(transaction) => {
                TransactionObservation::begun(transaction.id.clone(), resource.revision)
            }
            // `Unknown`, not `Unsupported`: the driver *has* a transaction API
            // (and declares `transaction_observation = Full`); this session just
            // has no transaction this provider opened.
            None => TransactionObservation {
                state: TransactionState::Unknown,
                transaction_id: None,
                effect: None,
                revision: resource.revision,
            },
        }
    }

    /// Reconcile a transaction started/ended by user SQL from server evidence.
    /// The handle names a cleanup action on this retained physical resource.
    fn reconcile_transaction(
        &self,
        handle: &ResourceHandle,
        state: TransactionState,
    ) -> Result<LiveResource, ResourceError> {
        let mut registry = self.lock();
        let resource = registry
            .live
            .get_mut(handle.resource_key())
            .ok_or_else(|| ResourceError::InvalidResourceState {
                resource_key: handle.resource_key().into(),
                operation: "reconcile_transaction".into(),
                state: "SessionLost".into(),
            })?;
        match state {
            TransactionState::Idle => resource.transaction = None,
            TransactionState::Active | TransactionState::Aborted
                if resource.transaction.is_none() =>
            {
                resource.transaction = Some(OpenTransaction {
                    id: format!("pg_sql_tx_{}", uuid::Uuid::new_v4()),
                    connection_id: resource.connection.id.clone(),
                });
            }
            _ => {}
        }
        Ok(resource.snapshot())
    }

    /// The handles this provider issued for the live resource. Never a handle
    /// created by some other call path.
    fn session_handles_of(
        &self,
        handle: &ResourceHandle,
        resource: &LiveResource,
    ) -> Vec<SessionHandleRef> {
        match resource.transaction.as_ref() {
            Some(transaction) => vec![SessionHandleRef::new(
                transaction.id.clone(),
                SessionHandleKind::Transaction,
                handle.resource_key().to_string(),
                self.runtime_epoch,
            )],
            None => Vec::new(),
        }
    }

    /// Everything an acquire must check, done before any budget is charged and
    /// before any socket is opened. Split out so the ordering is verifiable on
    /// its own: a refused request must never cost the caller a connection.
    pub(super) fn validate_acquisition(
        &self,
        request: &AcquireResourceRequest,
    ) -> Result<u32, ResourceError> {
        // The namespace shape is the contract: PostgreSQL has a database level
        // and a schema level and *no* catalog level, so a catalog target is
        // refused by name.
        self.namespace_shape.canonicalize(&request.target)?;

        if !request.baseline.initialization_requirements.is_empty() {
            return Err(ResourceError::OperationNotSupported {
                driver: POSTGRES_PROVIDER_ID.to_string(),
                operation: "acquire_resource".to_string(),
                reason: format!(
                    "this provider declares no initialization requirements (describe_resource \
                     returns an empty list) and replaying the caller's {} requirement(s) has no \
                     verified implementation. Acquire an empty baseline, or close and \
                     re-acquire.",
                    request.baseline.initialization_requirements.len()
                ),
            });
        }

        // The cost policy is the single source of truth for the charge, and one
        // helper answers it for both the description and this charge. If it ever
        // stops being a bounded pool, acquisition is refused rather than guessed at.
        let fixed = contract::fixed_purpose(request.scope.purpose)
            || request.scope.pin_for_streaming
            || request.scope.holds_open_transaction;
        let cost = contract::resource_connection_cost(
            &request.connection_config,
            request.scope.purpose,
            fixed,
        );
        let ConnectionCostPolicy::PoolBounded {
            max_physical_connections: required,
        } = cost
        else {
            return Err(ResourceError::OperationNotSupported {
                driver: POSTGRES_PROVIDER_ID.to_string(),
                operation: "acquire_resource".to_string(),
                reason: "the declared connection cost policy is no longer a bounded pool, so \
                         the charge for this resource cannot be determined"
                    .to_string(),
            });
        };

        if request.scope.max_physical_connections < required {
            return Err(ResourceError::BudgetDenied {
                requested: required,
                reason: format!(
                    "the scope allows {} physical connection(s); this resource needs {} for its \
                     data pool (the cancel-only control connection is driver-internal and not \
                     charged)",
                    request.scope.max_physical_connections, required
                ),
            });
        }

        Ok(required)
    }

    /// Register an open resource and issue its handle. Split from
    /// `acquire_resource` so the accounting can be tested without a server.
    fn register_resource(
        &self,
        connection: ConnectionHandle,
        permit: BudgetPermit,
        budget: Arc<dyn BudgetPort>,
    ) -> ResourceHandle {
        let resource_key = connection.id.clone();
        let mut registry = self.lock();
        registry.live.insert(
            resource_key.clone(),
            LiveResource {
                connection,
                close_started: false,
                permit,
                budget,
                transaction: None,
                revision: 1,
            },
        );
        ResourceHandle::issue(POSTGRES_PROVIDER_ID, resource_key, self.runtime_epoch)
    }

    /// Test-only seeding. `#[cfg(test)]`, so it is invisible to the integration
    /// tests in `tests/`: those must go through the real acquisition path.
    #[cfg(test)]
    pub(crate) fn register_resource_for_test(
        &self,
        connection: ConnectionHandle,
        permit: BudgetPermit,
        budget: Arc<dyn BudgetPort>,
    ) -> ResourceHandle {
        self.register_resource(connection, permit, budget)
    }

    /// Commit or roll back, including the honesty rule for an unconfirmed
    /// outcome.
    async fn finish_transaction(
        &self,
        handle: &ResourceHandle,
        operation: &str,
        commit: bool,
    ) -> Result<TransactionObservation, ResourceError> {
        let resource = self.resource_of(handle, operation)?;
        let transaction = match self.take_transaction(handle, operation) {
            Ok(transaction) => transaction,
            Err(error)
                if self
                    .driver
                    .fixed_sessions
                    .read()
                    .await
                    .contains_key(&resource.connection.id) =>
            {
                let _ = error;
                OpenTransaction {
                    id: "fixed_sql_transaction".into(),
                    connection_id: resource.connection.id.clone(),
                }
            }
            Err(error) => return Err(error),
        };
        let revision = self.bump_revision(handle);

        let aborted = commit
            && self
                .driver
                .read_fixed_session(&resource.connection)
                .await
                .map(|context| context.transaction_state == TransactionState::Aborted)
                .unwrap_or(false);
        let outcome = if commit {
            self.driver.commit_impl(transaction.to_handle()).await
        } else {
            self.driver.rollback_impl(transaction.to_handle()).await
        };

        match outcome {
            Ok(()) => Ok(TransactionObservation::finished(
                TransactionState::Idle,
                if aborted {
                    EffectOutcome::RolledBack
                } else {
                    EffectOutcome::Completed
                },
                revision,
            )),
            Err(error) => {
                // `commit_impl` takes the pinned connection out of the driver's
                // registry *before* it sends COMMIT, so a failure here leaves
                // the outcome genuinely unobservable. Reporting `Completed`
                // would invent one; `Unknown` is the only truthful disposition.
                tracing::warn!(
                    driver = POSTGRES_PROVIDER_ID,
                    resource_key = %handle.resource_key(),
                    error = %error,
                    "transaction end could not be confirmed; reported as effect unknown"
                );
                Ok(TransactionObservation::finished(
                    TransactionState::Unknown,
                    EffectOutcome::Unknown,
                    revision,
                ))
            }
        }
    }

    /// Run one Driver Command on the live resource and hand what came back to
    /// the sink.
    ///
    /// SQL rows use incremental delivery; catalog/admin payloads use typed
    /// output. Fixed sessions only admit commands with a same-connection path.
    async fn run_command(
        &self,
        handle: &ResourceHandle,
        resource: &LiveResource,
        call: &CommandCall,
        execution_id: &datazen_driver_api::QueryExecutionId,
        sink: &dyn ResultSink,
    ) -> Result<ExecutionCompletion, ResourceError> {
        if self
            .driver
            .fixed_sessions
            .read()
            .await
            .contains_key(&resource.connection.id)
            && !matches!(
                call.command.as_str(),
                "query"
                    | "query_stream"
                    | "execute"
                    | "explain"
                    | "get_table_schema"
                    | "backup_dump"
            )
        {
            return Err(ResourceError::unsupported(
                POSTGRES_PROVIDER_ID,
                "execute_on_resource",
                "this catalog/admin command does not execute on a retained session",
            ));
        }
        if let Some(database) = call
            .input
            .get("database")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            if !self
                .driver
                .is_active_database(&resource.connection, database)
                .await
            {
                return Err(ResourceError::unsupported(
                    POSTGRES_PROVIDER_ID,
                    "execute_on_resource",
                    "the target database requires a separately budgeted resource",
                ));
            }
        }
        // The pre-execution state is a snapshot, deliberately: what the caller
        // sees as "before" is what was true when the statement was sent.
        let context_before = self.context_of(resource).await?;
        let statement_results = if call.command == "backup_dump" {
            self.driver
                .retained_backup(&resource.connection, execution_id, &call.input, sink)
                .await?;
            Vec::new()
        } else if matches!(call.command.as_str(), "query" | "query_stream") {
            let sql = call.input["sql"].as_str().ok_or_else(|| {
                ResourceError::Driver(DriverError::InvalidConfig("sql is required".into()))
            })?;
            let target = datazen_driver_api::SqlTarget::new(
                call.input
                    .get("database")
                    .and_then(serde_json::Value::as_str),
                call.input.get("schema").and_then(serde_json::Value::as_str),
            );
            self.driver
                .ensure_transaction_reaches(&resource.connection, target)
                .await?;
            let sql = datazen_driver_api::DatabaseDriver::qualified_sql(&*self.driver, sql, target);
            let limit = call
                .input
                .get("limit")
                .and_then(serde_json::Value::as_u64)
                .map(|n| n.min(u32::MAX as u64) as u32);
            let parameters: Option<Vec<datazen_driver_api::Value>> = call
                .input
                .get("boundParameters")
                .map(|value| serde_json::from_value(value.clone()))
                .transpose()
                .map_err(|_| DriverError::InvalidConfig("invalid bound parameters".into()))?;
            self.driver
                .stream_resource_query(
                    &resource.connection,
                    execution_id,
                    &sql,
                    limit,
                    parameters.as_deref(),
                    sink,
                )
                .await?
        } else {
            let result = self
                .driver
                .execute_command_impl(&resource.connection, &call.command, call.input.clone())
                .await
                .map_err(ResourceError::Driver)?;
            if call.command == "execute" {
                let sql = call
                    .input
                    .get("sql")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        ResourceError::Driver(DriverError::InvalidConfig(
                            "execute command requires string input 'sql'".into(),
                        ))
                    })?;
                let (statements, outputs) = decode_command_result(&result.data, sql)?;
                for output in outputs {
                    match output {
                        StatementOutput::Rows(chunk) => sink.write(chunk).await?,
                        StatementOutput::Summary(summary) => {
                            sink.write_statement_summary(summary).await?
                        }
                    }
                }
                statements
            } else {
                sink.write_output(
                    serde_json::json!({ "kind": "commandResult", "data": result.data }),
                )
                .await?;
                Vec::new()
            }
        };
        let context_after = self.context_of(resource).await?;
        let current = if self
            .driver
            .fixed_sessions
            .read()
            .await
            .contains_key(&resource.connection.id)
        {
            self.reconcile_transaction(handle, context_after.transaction_state)?
        } else {
            resource.snapshot()
        };
        let mut transaction_observation = self.transaction_observation_of(&current);
        transaction_observation.state = context_after.transaction_state;

        let session_handles = match current.transaction {
            Some(transaction) => vec![SessionHandleRef::new(
                transaction.id,
                SessionHandleKind::Transaction,
                handle.resource_key().to_string(),
                self.runtime_epoch,
            )],
            None => Vec::new(),
        };

        sink.complete().await?;
        Ok(ExecutionCompletion {
            completion_status: CompletionStatus::Succeeded,
            effect_outcome: EffectOutcome::Completed,
            statement_results,
            context_before,
            context_after,
            transaction_observation,
            session_handles,
            // Every SQLx result was consumed before completion; no row buffer
            // is duplicated in statement_results.
            protocol_drained: true,
            // The command returned, which is the evidence for `Healthy`; a
            // failure above is an `Err`, never a completion.
            resource_health: ResourceHealth::Healthy,
        })
    }
}

/// The fifteen [`ResourceProvider`] methods. A child module so it sees the
/// provider's private bookkeeping without widening any visibility.
mod contract;

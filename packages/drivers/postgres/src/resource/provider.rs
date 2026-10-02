//! The PostgreSQL resource provider's bookkeeping.
//!
//! This is a real implementation, not a shell. Every contract method — all
//! fourteen, in [`contract`] — is bound to this crate's own connection and
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
    postgres_capability_registry, postgres_connection_cost, postgres_namespace_shape,
    POSTGRES_PROVIDER_ID,
};
use super::observation;
use super::payload::decode_command_result;
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
        let resource =
            registry
                .live
                .get_mut(&key)
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
        match resource.transaction.as_ref() {
            Some(transaction) => {
                let facts = observation::read_pinned_session(
                    &self.driver.transactions,
                    &transaction.connection_id,
                )
                .await?;
                Ok(observation::pinned_context(&facts))
            }
            None => {
                let pool = self
                    .pool_of(&resource.connection, "read_session_context")
                    .await?;
                let facts = observation::read_pooled_session(&pool).await?;
                Ok(observation::leased_context(&facts))
            }
        }
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
    fn validate_acquisition(&self, request: &AcquireResourceRequest) -> Result<u32, ResourceError> {
        // The namespace shape is the contract: PostgreSQL has a database level
        // and a schema level and *no* catalog level, so a catalog target is
        // refused by name.
        self.namespace_shape.canonicalize(&request.target)?;

        if request.scope.pin_for_streaming {
            return Err(ResourceError::OperationNotSupported {
                driver: POSTGRES_PROVIDER_ID.to_string(),
                operation: "acquire_resource".to_string(),
                reason: "pin_for_streaming needs a fixed, session-bound connection. This \
                         provider declares statefulSession = unsupported, because a sqlx pool \
                         of any size hands out a different backend per statement."
                    .to_string(),
            });
        }

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

        // The cost policy is the single source of truth for the charge. If it
        // ever stops being a bounded pool, acquisition is refused rather than
        // guessed at.
        let ConnectionCostPolicy::PoolBounded {
            max_physical_connections: required,
        } = postgres_connection_cost(&request.connection_config)
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
                    "the scope allows {} physical connection(s); this resource needs {}: a pool \
                     of {} plus the control pool used for cancellation",
                    request.scope.max_physical_connections,
                    required,
                    request.connection_config.effective_max_pool_size()
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
        registry.closed.remove(&resource_key);
        registry.live.insert(
            resource_key.clone(),
            LiveResource {
                connection,
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

    /// How many tombstones this instance is holding. `#[cfg(test)]`, because the
    /// size of `closed` is not something a caller may ask about: it exists
    /// purely so a confirmed close can be told apart from a key never held.
    ///
    /// Read by `tests_behaviour` to pin what `closed` actually does — see
    /// `the_tombstone_set_only_grows_and_nothing_reclaims_it` there.
    #[cfg(test)]
    pub(crate) fn tombstone_count_for_test(&self) -> usize {
        self.lock().closed.len()
    }

    /// Commit or roll back, including the honesty rule for an unconfirmed
    /// outcome.
    async fn finish_transaction(
        &self,
        handle: &ResourceHandle,
        operation: &str,
        commit: bool,
    ) -> Result<TransactionObservation, ResourceError> {
        let transaction = self.take_transaction(handle, operation)?;
        let revision = self.bump_revision(handle);

        let outcome = if commit {
            self.driver.commit_impl(transaction.to_handle()).await
        } else {
            self.driver.rollback_impl(transaction.to_handle()).await
        };

        match outcome {
            Ok(()) => Ok(TransactionObservation::finished(
                TransactionState::Idle,
                EffectOutcome::Completed,
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
    /// The accepted command set is deliberately narrow: only results the sink
    /// channel can carry truthfully. A catalog or admin command returns a
    /// payload a `ResultChunk` has no way to express, and reporting that as
    /// "no rows, succeeded" is exactly the shell behaviour the contract
    /// forbids, so it is refused with `OperationNotSupported`.
    async fn run_command(
        &self,
        handle: &ResourceHandle,
        resource: &LiveResource,
        call: &CommandCall,
        sink: &dyn ResultSink,
    ) -> Result<ExecutionCompletion, ResourceError> {
        // The pre-execution state is a snapshot, deliberately: what the caller
        // sees as "before" is what was true when the statement was sent.
        let context_before = self.context_of(resource).await?;
        let transaction = resource.transaction.clone();
        let transaction_observation = self.transaction_observation_of(resource);

        let result = self
            .driver
            .execute_command_impl(&resource.connection, &call.command, call.input.clone())
            .await;

        let result = match result {
            Ok(result) => result,
            Err(error) => {
                // The sink learns the execution ended before the error returns
                // to the caller.
                let reason = format!("driver command '{}' failed: {error}", call.command);
                let _ = sink.fail(&reason).await;
                return Err(ResourceError::Driver(DriverError::QueryFailed(reason)));
            }
        };

        let (statement_results, chunks) = decode_command_result(&result.data)?;
        for chunk in chunks {
            // A sink that rejects a chunk has already been handed rows, so the
            // statements did run; `SinkRejected` reports the delivery failure
            // without pretending the work did not happen.
            sink.write(chunk).await?;
        }
        sink.complete().await?;

        let context_after = self.context_of(resource).await?;

        let session_handles = match transaction {
            Some(transaction) => vec![SessionHandleRef::new(
                transaction.id,
                SessionHandleKind::Transaction,
                handle.resource_key().to_string(),
                self.runtime_epoch,
            )],
            None => Vec::new(),
        };

        Ok(ExecutionCompletion {
            completion_status: CompletionStatus::Succeeded,
            effect_outcome: EffectOutcome::Completed,
            statement_results,
            context_before,
            context_after,
            transaction_observation,
            session_handles,
            // Every row was buffered and handed to the sink; the connection
            // will not be read again for this execution.
            protocol_drained: true,
            // The command returned, which is the evidence for `Healthy`; a
            // failure above is an `Err`, never a completion.
            resource_health: ResourceHealth::Healthy,
        })
    }
}

/// The fourteen [`ResourceProvider`] methods. A child module so it sees the
/// provider's private bookkeeping without widening any visibility.
mod contract;

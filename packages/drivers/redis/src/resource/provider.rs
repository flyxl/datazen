//! The Redis resource provider's bookkeeping.
//!
//! This is a real implementation, not a shell. Every contract method — all
//! fourteen, in [`contract`] — is bound to this crate's own connection and
//! execution machinery:
//!
//! | contract method | what actually runs |
//! |---|---|
//! | [`ResourceProvider::describe_resource`] | pure math over the request + [`redis_namespace_shape`](super::capabilities::redis_namespace_shape) |
//! | [`ResourceProvider::acquire_resource`] | `BudgetPort::acquire_physical_connections`, then `DatabaseDriver::connect` (`database.rs:67`), which opens one `RedisLiveConn`, then a real `SELECT` on it |
//! | [`ResourceProvider::execute_on_resource`] | `DatabaseDriver::execute_command` (`database.rs:310`) → `execute_redis_command`; the payload is decoded and streamed into the sink |
//! | [`ResourceProvider::observe_session`] | one real `INFO server` round trip on the resource's own connection (`observation.rs`) |
//! | [`ResourceProvider::change_context`] | a real `SELECT n` on the *same* live connection (`driver/session.rs:9-18`) |
//! | begin / commit / rollback | refused — Redis has no all-or-nothing transaction |
//! | [`ResourceProvider::request_cancel`] | refused — Redis cancels a connection, not an execution |
//! | [`ResourceProvider::close_resource`] | `DatabaseDriver::disconnect` (`database.rs:82`) plus exactly one budget release |
//!
//! The four rules this provider never breaks are stated where they are
//! enforced: ownership on [`RedisResourceProvider::resource_of`], explicit
//! refusal in [`RedisResourceProvider::validate_acquisition`], truthful
//! observation in [`observation`] and [`RedisResourceProvider::context_of`],
//! and one-and-only-one budget release in
//! [`RedisResourceProvider::close_resource`]. Their test-visible summary
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
    TransactionObservation, TransactionState,
};
use datazen_driver_api::{ConnectionHandle, DatabaseDriver, DriverError};

use crate::driver::RedisDriver;

use super::capabilities::{
    redis_capability_registry, redis_connection_cost, redis_namespace_shape, REDIS_PROVIDER_ID,
};
use super::observation;
use super::payload::decode_command_result;
use super::registry::{LiveResource, ResourceRegistry};

/// Each provider instance takes the next epoch. A handle issued by an older
/// instance of the same driver then fails the epoch check, instead of silently
/// addressing whatever connection happens to occupy the same key today.
static PROVIDER_EPOCH: AtomicU64 = AtomicU64::new(1);

fn next_runtime_epoch() -> u64 {
    PROVIDER_EPOCH.fetch_add(1, Ordering::SeqCst) + 1
}

/// Redis's implementation of the resource contract.
pub struct RedisResourceProvider {
    driver: Arc<RedisDriver>,
    capabilities: CapabilityRegistry,
    namespace_shape: NamespaceShape,
    runtime_epoch: u64,
    resources: Mutex<ResourceRegistry>,
}

impl RedisResourceProvider {
    /// Bind a provider to a driver instance. The driver *is* the provider's
    /// real connection surface, so the `Arc` is shared rather than re-created:
    /// the host and the provider must see the same connections. `connect` writes
    /// into `RedisDriver::connections` and every later command is looked up by
    /// `handle.pool_id`, so a second `RedisDriver` would be a driver with no
    /// connections at all.
    pub fn new(driver: Arc<RedisDriver>) -> Self {
        Self {
            driver,
            capabilities: redis_capability_registry(),
            namespace_shape: redis_namespace_shape(),
            runtime_epoch: next_runtime_epoch(),
            resources: Mutex::new(ResourceRegistry::default()),
        }
    }

    /// The driver whose resources this provider hands out. `pub(crate)` rather
    /// than public: a caller outside this crate has no business reaching past
    /// the contract to the connection registry behind it.
    ///
    /// `#[cfg(test)]` because no production path reads it — every contract
    /// method already has `self.driver` in hand. Leaving it in the non-test
    /// build is what turns it into a dead-method warning.
    #[cfg(test)]
    pub(crate) fn driver(&self) -> &Arc<RedisDriver> {
        &self.driver
    }

    /// The epoch stamped into every handle this provider issues. Tests read it
    /// to prove the provider is memoized: a second instance would take the
    /// next epoch and invalidate every live handle. Test-only for the same
    /// reason as `driver` above.
    #[cfg(test)]
    pub(crate) fn runtime_epoch(&self) -> u64 {
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
        handle.check(REDIS_PROVIDER_ID, self.runtime_epoch)?;
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

    /// Record the logical database a live session is now attached to.
    ///
    /// This is *provider bookkeeping, not an observation*. The Redis server
    /// cannot be asked which database a connection is on, so the only honest
    /// way to keep the number is to write down what this provider itself
    /// selected — and `change_context` only calls this after `SELECT` has
    /// returned `+OK`. It is never surfaced as an observed namespace.
    fn set_db_index(&self, handle: &ResourceHandle, db_index: u32) -> Result<(), ResourceError> {
        let mut registry = self.lock();
        let resource = registry
            .live
            .get_mut(handle.resource_key())
            .ok_or_else(|| ResourceError::InvalidResourceState {
                resource_key: handle.resource_key().to_string(),
                operation: "change_context".to_string(),
                state: "no resource with this key is open on this provider".to_string(),
            })?;
        resource.db_index = db_index;
        Ok(())
    }

    /// The context of a live Redis session. Always the same honest partial —
    /// see [`observation`] for why the namespace is empty and the confidence is
    /// [`ObservationConfidence::Partial`] rather than `Confirmed`.
    async fn context_of(&self, resource: &LiveResource) -> Result<SessionContext, ResourceError> {
        // The round trip is what makes this worth asking for: a session that
        // cannot answer `INFO server` is not `Ready` and not `Healthy`, and
        // `observe_session` says so with an error rather than a default.
        observation::probe_liveness(&self.driver, &resource.connection).await?;
        Ok(observation::redis_context())
    }

    /// The transaction observation for a Redis session.
    ///
    /// `Unsupported`, not `Unknown`: unlike PostgreSQL — which has a real
    /// transaction API and merely has none open on this session — Redis has no
    /// all-or-nothing unit at all. `MULTI` queues commands for later `EXEC`;
    /// it is a batching mechanism, not a transaction, and reporting it as one
    /// would be the exact over-claim this provider exists to avoid.
    fn transaction_observation_of(&self, resource: &LiveResource) -> TransactionObservation {
        TransactionObservation {
            state: TransactionState::Unsupported,
            transaction_id: None,
            effect: None,
            revision: resource.revision,
        }
    }

    /// Everything an acquire must check, done before any budget is charged and
    /// before any socket is opened. Split out so the ordering is verifiable on
    /// its own: a refused request must never cost the caller a connection.
    ///
    /// Returns the logical database index to select, so the caller does not
    /// re-derive it after the charge has been made.
    fn validate_acquisition(&self, request: &AcquireResourceRequest) -> Result<u32, ResourceError> {
        // The namespace shape is the contract: Redis has a database level and
        // neither a catalog nor a schema level, so a catalog or schema target is
        // refused by name.
        let target = self.namespace_shape.canonicalize(&request.target)?;

        if request.scope.holds_open_transaction {
            return Err(ResourceError::OperationNotSupported {
                driver: REDIS_PROVIDER_ID.to_string(),
                operation: "acquire_resource".to_string(),
                reason: "this resource cannot hold an open transaction. Redis has no \
                         all-or-nothing unit: MULTI queues commands for a later EXEC, it does \
                         not roll back on failure. Acquire a resource for a scope that does \
                         not hold a transaction."
                    .to_string(),
            });
        }

        if !request.baseline.initialization_requirements.is_empty() {
            return Err(ResourceError::OperationNotSupported {
                driver: REDIS_PROVIDER_ID.to_string(),
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

        // An absent target means database 0, which is the Redis protocol's own
        // default for a fresh connection — not a guess about what the caller
        // wanted. A present target arrives in the canonical path (already
        // alias-resolved by `canonicalize`) and is parsed by the driver's own
        // parser, so "db3" and "3" mean the same thing here as they do
        // everywhere else in this crate, and anything else is refused before a
        // socket opens.
        let db_index = match target.path.first().map(String::as_str) {
            None => 0,
            Some(name) => RedisDriver::parse_db_name(name).map_err(ResourceError::Driver)?,
        };

        // The cost policy is the single source of truth for the charge. If it
        // ever stops being a bounded pool, acquisition is refused rather than
        // guessed at.
        let ConnectionCostPolicy::PoolBounded {
            max_physical_connections: required,
        } = redis_connection_cost()
        else {
            return Err(ResourceError::OperationNotSupported {
                driver: REDIS_PROVIDER_ID.to_string(),
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
                    "the scope allows {} physical connection(s); this resource needs {}: \
                     DatabaseDriver::connect (database.rs:67) opens exactly one RedisLiveConn \
                     per handle, and a Redis session cannot be moved off it",
                    request.scope.max_physical_connections, required,
                ),
            });
        }

        Ok(db_index)
    }

    /// Open the physical connection for an acquire and select the target
    /// database on it. Split from `acquire_resource` so the permit's fate on
    /// failure has exactly one place to be decided.
    ///
    /// On any failure the connection is torn down before the error is returned,
    /// so a caller that sees `Err` never leaves a live socket behind — which is
    /// the same discipline `acquire_resource` applies to the budget permit.
    async fn open_session(
        &self,
        config: &datazen_driver_api::ConnectionConfig,
        db_index: u32,
    ) -> Result<ConnectionHandle, ResourceError> {
        let connection = self
            .driver
            .connect(config)
            .await
            .map_err(ResourceError::Driver)?;

        if let Err(error) = self.select_db_on(&connection, db_index).await {
            tracing::error!(
                driver = REDIS_PROVIDER_ID,
                resource_key = %connection.id,
                error = %error,
                "the acquired Redis session could not be attached to the requested database; \
                 tearing the connection down rather than leaving it on the wrong database"
            );
            // Best effort: the close may itself fail, and there is nothing to
            // escalate to — the caller is about to receive an `Err` either way.
            let _ = self.driver.disconnect(connection.clone()).await;
            return Err(error);
        }

        Ok(connection)
    }

    /// Issue `SELECT db_index` on the resource's own live connection.
    ///
    /// This is the one place namespace switching happens, and it happens *in
    /// place* — the same `RedisLiveConn` the session has been using, dispatched
    /// through `with_redis_conn!` so standalone, cluster and sentinel all take
    /// the identical path. A successful `SELECT` is the proof the switch
    /// happened: Redis answers `+OK` or an out-of-range error, so there is no
    /// state in which the call could succeed and the session stayed put.
    pub(crate) async fn select_db_on(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
    ) -> Result<(), ResourceError> {
        let mut conns = self.driver.connections.write().await;
        let rc = RedisDriver::get_conn(&mut conns, handle).map_err(ResourceError::Driver)?;
        RedisDriver::select_db(&mut rc.live, db_index)
            .await
            .map_err(|error| {
                ResourceError::Driver(DriverError::QueryFailed(format!(
                    "attaching the session to logical database {db_index} failed: {error}"
                )))
            })
    }

    /// Register an open resource and issue its handle. Split from
    /// `acquire_resource` so the accounting can be tested without a server.
    fn register_resource(
        &self,
        connection: ConnectionHandle,
        permit: BudgetPermit,
        budget: Arc<dyn BudgetPort>,
        db_index: u32,
    ) -> ResourceHandle {
        let resource_key = connection.id.clone();
        let mut registry = self.lock();
        registry.live.insert(
            resource_key.clone(),
            LiveResource {
                connection,
                permit,
                budget,
                db_index,
                revision: 1,
            },
        );
        ResourceHandle::issue(REDIS_PROVIDER_ID, resource_key, self.runtime_epoch)
    }

    /// Test-only seeding. `#[cfg(test)]`, so it is invisible to the integration
    /// tests in `tests/`: those must go through the real acquisition path.
    #[cfg(test)]
    pub(crate) fn register_resource_for_test(
        &self,
        connection: ConnectionHandle,
        permit: BudgetPermit,
        budget: Arc<dyn BudgetPort>,
        db_index: u32,
    ) -> ResourceHandle {
        self.register_resource(connection, permit, budget, db_index)
    }

    /// Run one Driver Command on the live resource and hand what came back to
    /// the sink.
    ///
    /// The accepted command set is deliberately narrow: only results the sink
    /// channel can carry truthfully. Redis's command surface answers with
    /// free-form JSON — key listings, parsed `INFO` blocks — which a
    /// `ResultChunk` has no way to express, and reporting that as "no rows,
    /// succeeded" is exactly the shell behaviour the contract forbids, so it is
    /// refused with `OperationNotSupported`.
    async fn run_command(
        &self,
        resource: &LiveResource,
        call: &CommandCall,
        sink: &dyn ResultSink,
    ) -> Result<ExecutionCompletion, ResourceError> {
        let result = self
            .driver
            .execute_command(&resource.connection, &call.command, call.input.clone())
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

        // A refused payload leaves the sink unfinished, and that is correct: the
        // `?` below returns before `sink.complete()`, so the caller learns the
        // execution ended without having been handed a fabricated empty result.
        let (statement_results, chunks) = decode_command_result(&result.data)?;
        for chunk in chunks {
            // A sink that rejects a chunk has already been handed rows, so the
            // statements did run; `SinkRejected` reports the delivery failure
            // without pretending the work did not happen.
            sink.write(chunk).await?;
        }
        sink.complete().await?;

        // The context is the same honest partial on both sides of the command:
        // Redis cannot confirm a namespace, so before and after would carry the
        // same empty value. The command's own round trip is the liveness
        // evidence, so — unlike PostgreSQL, which issues two more reads per
        // execution to say the same thing — no extra `INFO server` is paid for
        // here.
        Ok(ExecutionCompletion {
            completion_status: CompletionStatus::Succeeded,
            effect_outcome: EffectOutcome::Completed,
            statement_results,
            context_before: observation::redis_context(),
            context_after: observation::redis_context(),
            transaction_observation: self.transaction_observation_of(resource),
            // This provider issues no transaction or cursor handles, and must
            // never report a handle it did not register.
            session_handles: Vec::new(),
            // Every row was buffered and handed to the sink; the connection
            // will not be read again for this execution.
            protocol_drained: true,
            // The command returned, which is the evidence for `Healthy`; a
            // failure above is an `Err`, never a completion.
            resource_health: ResourceHealth::Healthy,
        })
    }

    /// Take the resource out of `live` for a close, applying the idempotency
    /// rule.
    ///
    /// The record is removed *before* the socket is touched, so no concurrent
    /// call can operate on a resource whose close is in flight. `Ok(None)`
    /// means "already closed": the handle carries that fact, its budget was
    /// released by the call that closed it, and releasing a second time is the
    /// bug this guards.
    ///
    /// Ownership is checked here even though this is the one method that does
    /// not go through `resource_of`. Without it, a handle minted by some other
    /// provider would be accepted whenever its `resource_key` happened to
    /// collide with one of ours — and it would be accepted as "already closed"
    /// rather than rejected, which is the worst shape: a silent `Ok` for
    /// someone else's resource.
    fn take_for_close(
        &self,
        handle: &ResourceHandle,
        operation: &str,
    ) -> Result<Option<LiveResource>, ResourceError> {
        handle.check(REDIS_PROVIDER_ID, self.runtime_epoch)?;
        let mut registry = self.lock();
        match registry.live.remove(handle.resource_key()) {
            Some(resource) => Ok(Some(resource)),
            None if handle.is_closed() => Ok(None),
            None => Err(ResourceError::InvalidResourceState {
                resource_key: handle.resource_key().to_string(),
                operation: operation.to_string(),
                state: "this provider instance holds no open resource with this key: it was \
                        never acquired here, a previous close of it was not confirmed, or the \
                        handle belongs to another provider instance"
                    .to_string(),
            }),
        }
    }
}

/// The fourteen [`ResourceProvider`] methods. A child module so it sees the
/// provider's private bookkeeping without widening any visibility.
mod contract;

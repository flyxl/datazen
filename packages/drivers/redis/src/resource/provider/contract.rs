//! The fourteen contract methods, and nothing else.
//!
//! This file is the *declaration surface*: every method of
//! [`ResourceProvider`] exactly as Redis answers it. The bookkeeping they
//! lean on — handle ownership, budget accounting, the database the session is
//! attached to, the session read — lives in [`super`], which also holds the
//! module doc that maps each contract method to the driver call that really
//! runs behind it.
//!
//! The rules restated for the reader who only opens this file:
//!
//! * **A handle is checked on every call** ([`ResourceHandle::check`]). A handle
//!   from another provider, or from an older instance of this one, is an error
//!   — never a silent no-op.
//! * **An unsupported request is an error.** Everything this provider does not
//!   declare (a transaction, a per-execution cancel, snapshots, baseline
//!   replay) is refused with an explicit [`ResourceError`] *before* any budget
//!   is charged and *before* any command is sent.
//! * **No observation is invented.** [`ResourceProvider::observe_session`] reads
//!   the server once; a field that read cannot confirm stays `Unknown` /
//!   `Partial`, which is why the observed namespace is empty.
//! * **The budget is released exactly once**, and is never claimed as recovered
//!   when a close could not be confirmed.

use std::sync::Arc;

use async_trait::async_trait;
use datazen_driver_api::capabilities::{CapabilityRegistry, SessionContinuity};
use datazen_driver_api::namespace::{NamespaceShape, NamespaceTarget};
use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPort, CommandCall, DescribeResourceRequest,
    ResourceDescriptor, ResourceError, ResourceHandle, ResourceProvider, ResultSink, ReusePolicy,
};
use datazen_driver_api::session::{
    CancelReceipt, CloseDisposition, ContextChangeDisposition, ExecutionCompletion,
    ResetDisposition, SessionObservation, TransactionObservation, TransactionOptions,
};
use datazen_driver_api::{DatabaseDriver, QueryExecutionId};

use super::super::capabilities::{redis_connection_cost, REDIS_PROVIDER_ID};
use super::super::observation;
use super::RedisResourceProvider;

use crate::driver::RedisDriver;

#[async_trait]
impl ResourceProvider for RedisResourceProvider {
    fn provider_id(&self) -> &str {
        REDIS_PROVIDER_ID
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
        // Describing is free and must stay free, so an invalid target gets the
        // same answer here as it does at acquire time.
        self.namespace_shape.canonicalize(&request.target)?;

        Ok(ResourceDescriptor {
            provider_id: REDIS_PROVIDER_ID.to_string(),
            resource_key: format!("redis_session:config:{}", request.connection_config.id),
            // This is the one declaration PostgreSQL cannot make and Redis can,
            // and the evidence is structural rather than empirical:
            // `RedisDriver::connect` (`database.rs:67`) inserts exactly **one**
            // `RedisConn` into the registry, and every later call resolves it
            // by `handle.pool_id` (`driver/mod.rs:127`). There is no pool, so
            // there is nothing to hand out but the same wire connection.
            session_continuity: SessionContinuity::Fixed,
            reuse_policy: ReusePolicy::SingleUse,
            // No initialization protocol is proven for this driver, so none is
            // declared. `acquire_resource` refuses a non-empty baseline rather
            // than silently skipping it.
            initialization_requirements: Vec::new(),
            connection_cost_policy: redis_connection_cost(),
            namespace_shape: self.namespace_shape.clone(),
        })
    }

    async fn acquire_resource(
        &self,
        request: &AcquireResourceRequest,
        budget: &Arc<dyn BudgetPort>,
    ) -> Result<ResourceHandle, ResourceError> {
        // The database index is resolved *before* the charge, so a request this
        // provider cannot serve never costs the caller a permit.
        let db_index = self.validate_acquisition(request)?;

        let permit = budget.acquire_physical_connections(1).await?;

        let connection = match self
            .open_session(&request.connection_config, db_index)
            .await
        {
            Ok(connection) => connection,
            Err(error) => {
                // The connect failed, so the charge is released here and nowhere
                // else: the caller receives no handle it could close with.
                if let Err(release_error) = budget.release_physical_connections(&permit).await {
                    tracing::error!(
                        driver = REDIS_PROVIDER_ID,
                        error = %release_error,
                        "failed to release the physical-connection budget after a failed connect"
                    );
                }
                return Err(error);
            }
        };

        Ok(self.register_resource(connection, permit, budget.clone(), db_index))
    }

    async fn execute_on_resource(
        &self,
        handle: &ResourceHandle,
        _execution_id: &QueryExecutionId,
        call: &CommandCall,
        sink: &dyn ResultSink,
    ) -> Result<ExecutionCompletion, ResourceError> {
        let resource = self.resource_of(handle, "execute_on_resource")?;

        // `_execution_id` is deliberately unused, and that is a declaration
        // rather than an omission. PostgreSQL registers the execution id for the
        // duration of the call so that a concurrent `request_cancel` addresses
        // this execution; this provider has no cancel set at all, because
        // `request_cancel` is refused below and `preciseCancel` stays at its
        // fail-closed default. Registering an id nothing can consume would be
        // bookkeeping for a capability this driver does not have.
        self.run_command(&resource, call, sink).await
    }

    async fn observe_session(
        &self,
        handle: &ResourceHandle,
    ) -> Result<SessionObservation, ResourceError> {
        let resource = self.resource_of(handle, "observe_session")?;
        let context = self.context_of(&resource).await?;
        let revision = self.bump_revision(handle);
        // The observation was taken before the revision moved, so restate it
        // under the new revision: a caller comparing two observations must see
        // them as different.
        let mut transaction = self.transaction_observation_of(&resource);
        transaction.revision = revision;

        Ok(SessionObservation {
            // The session just answered a round trip and is not executing
            // anything this provider started. The wording of that claim lives
            // with the code that is allowed to make it.
            state: observation::live_session_state(),
            transaction,
            context,
            // This provider issues no session-scoped sub-resources: a command
            // is one round trip on this same connection, and a `SELECT` changes
            // the connection rather than creating a handle.
            handles: Vec::new(),
            protocol_drained: true,
            // The observation succeeded, which is the evidence for `Healthy`.
            resource_health: observation::live_resource_health(),
            context_revision: revision,
        })
    }

    async fn change_context(
        &self,
        handle: &ResourceHandle,
        desired: &NamespaceTarget,
    ) -> Result<ContextChangeDisposition, ResourceError> {
        let resource = self.resource_of(handle, "change_context")?;
        // Validate the target first: a namespace Redis does not have is refused
        // before the connection is touched, so the session is never left on a
        // half-applied namespace.
        let canonical = self.namespace_shape.canonicalize(desired)?;

        // `SELECT <n>` is the real, in-place namespace switch. Redis answers `+OK`
        // on success and an out-of-range error otherwise, so a successful call
        // is the proof — there is no separate read-back to trust.
        let db_index = match canonical.path.first().map(String::as_str) {
            None => 0,
            Some(name) => RedisDriver::parse_db_name(name).map_err(|_| {
                ResourceError::NamespaceTargetRejected {
                    reason: format!(
                        "this session is attached to logical database {}, and {name:?} is not a \
                         Redis logical database name. Use \"db3\" or \"3\".",
                        resource.db_index
                    ),
                }
            })?,
        };

        self.select_db_on(&resource.connection, db_index).await?;
        self.set_db_index(handle, db_index)?;

        // `Confirmed`, not `RequiresReplacement`: the same socket stayed open and
        // the `SELECT` really moved it. Nothing here reconnects.
        Ok(ContextChangeDisposition::Confirmed)
    }

    async fn begin_transaction(
        &self,
        handle: &ResourceHandle,
        options: &TransactionOptions,
    ) -> Result<TransactionObservation, ResourceError> {
        self.resource_of(handle, "begin_transaction")?;

        // Redis `MULTI` queues commands for a later `EXEC` and `DISCARD` throws
        // them away. That is batching, not a transaction: there is no
        // all-or-nothing boundary, nothing to roll back to a consistent point,
        // and `options.defers_commit` has no meaning without one. Accepting the
        // call and returning a transaction object would be the worst outcome —
        // the caller would build rollback logic on a guarantee that does not
        // exist.
        Err(ResourceError::OperationNotSupported {
            driver: REDIS_PROVIDER_ID.to_string(),
            operation: "begin_transaction".to_string(),
            reason: match options.isolation_level.as_deref() {
                // `TransactionSupport::isolation_levels` is declared empty, so a
                // named level is quoted back rather than dropped.
                Some(level) => format!(
                    "Redis has no all-or-nothing transaction, so isolation level {level:?} cannot \
                     be honoured. MULTI/EXEC batches commands for a later EXEC and DISCARD drops \
                     them; there is no rollback."
                ),
                None => "Redis has no all-or-nothing transaction. MULTI/EXEC batches commands for \
                         a later EXEC and DISCARD drops them; there is no rollback."
                    .to_string(),
            },
        })
    }

    async fn commit_transaction(
        &self,
        handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        self.resource_of(handle, "commit_transaction")?;

        // Refused rather than `Ok`: `EXEC` is a batch flush, not a commit, and
        // reporting it as one would tell the caller its writes are durable when
        // nothing was guaranteed in the first place.
        Err(ResourceError::OperationNotSupported {
            driver: REDIS_PROVIDER_ID.to_string(),
            operation: "commit_transaction".to_string(),
            reason: "no transaction was opened, because Redis MULTI/EXEC is a command batch \
                     rather than a transaction. There is nothing to commit and nothing to roll \
                     back."
                .to_string(),
        })
    }

    async fn rollback_transaction(
        &self,
        handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        self.resource_of(handle, "rollback_transaction")?;

        // `DISCARD` would abandon queued commands, not undo applied ones. It is
        // not reachable through this contract because no queue is ever opened
        // here, and reaching for it would undo a caller's illusion rather than
        // their data.
        Err(ResourceError::OperationNotSupported {
            driver: REDIS_PROVIDER_ID.to_string(),
            operation: "rollback_transaction".to_string(),
            reason: "Redis cannot undo applied commands. DISCARD abandons a queued MULTI batch; \
                     it does not roll back writes that already happened, so it is not a \
                     rollback and is not offered as one."
                .to_string(),
        })
    }

    async fn request_cancel(
        &self,
        handle: &ResourceHandle,
        _execution_id: &QueryExecutionId,
    ) -> Result<CancelReceipt, ResourceError> {
        self.resource_of(handle, "request_cancel")?;

        // `CLIENT KILL` kills a *connection*, which would destroy the session
        // this resource exists to provide — the session-wide cancel the trait
        // explicitly forbids. `preciseCancel` is therefore left at its
        // fail-closed `Unknown` default, and the refusal happens here rather
        // than returning a receipt that implies an execution was tracked.
        Err(ResourceError::OperationNotSupported {
            driver: REDIS_PROVIDER_ID.to_string(),
            operation: "request_cancel".to_string(),
            reason: "Redis has no per-execution cancel. CLIENT KILL terminates the whole \
                     connection, which would destroy this resource's session rather than stop \
                     one execution."
                .to_string(),
        })
    }

    async fn reset_resource(
        &self,
        handle: &ResourceHandle,
        baseline: &Baseline,
    ) -> Result<ResetDisposition, ResourceError> {
        self.resource_of(handle, "reset_resource")?;

        // A baseline this provider cannot replay is refused outright rather than
        // acknowledged and skipped. `FLUSHDB` is the opposite of a reset — it
        // destroys the caller's data — so it is never reached from here.
        if !baseline.initialization_requirements.is_empty() {
            return Err(ResourceError::OperationNotSupported {
                driver: REDIS_PROVIDER_ID.to_string(),
                operation: "reset_resource".to_string(),
                reason: format!(
                    "this provider declares no initialization requirements (describe_resource \
                     returns an empty list) and replaying the caller's {} requirement(s) has no \
                     verified implementation. The connection is left exactly as it is; FLUSHDB is \
                     destructive and is never run as a reset.",
                    baseline.initialization_requirements.len()
                ),
            });
        }

        // `Discard`, not `Clean`: nothing was replayed and nothing was verified
        // clean. The handle stays valid and the connection stays open — the
        // session context (the selected logical database) is deliberately *not*
        // restored, because restoring it is itself a `SELECT` this contract did
        // not ask for and did not confirm.
        Ok(ResetDisposition::Discard)
    }

    async fn close_resource(
        &self,
        handle: &ResourceHandle,
    ) -> Result<CloseDisposition, ResourceError> {
        let resource_key = handle.resource_key();

        // Taken out of `live` before the socket is touched, so nothing can
        // operate on a resource whose close is in flight.
        let Some(resource) = self.take_for_close(handle, "close_resource")? else {
            // Already closed by an earlier call: the permit went with that call,
            // so the budget is already released and a second release is exactly
            // the bug `is_closed` exists to prevent.
            return Ok(CloseDisposition::Closed);
        };

        // `RedisDriver::disconnect` (`database.rs:82`) discards the registry
        // remove and contains no other statement that can fail, so it cannot
        // return `Err` at all — not merely "cannot fail after that point". The
        // `Err` arm below is therefore dead, and it is dead for a different
        // reason than `postgres`'s twin: that one throws away a genuinely
        // fallible `ROLLBACK`, this one never had a fallible call to begin with.
        // `RedisResourceProvider::new` takes a concrete `Arc<RedisDriver>`, so
        // there is also no way to substitute a driver that does fail, which is
        // what keeps that arm untested rather than merely unreached.
        match self.driver.disconnect(resource.connection.clone()).await {
            Ok(()) => {
                handle.mark_closed();
                // The connection is really gone, so the charge is really
                // recoverable. Released here and nowhere else.
                resource
                    .budget
                    .release_physical_connections(&resource.permit)
                    .await?;
                tracing::debug!(
                    driver = REDIS_PROVIDER_ID,
                    resource_key,
                    "released the single physical connection charged to this resource"
                );
                Ok(CloseDisposition::Closed)
            }
            Err(error) => {
                // The connection may or may not be gone. The budget is **not**
                // released and the handle is not marked closed, so the leak is
                // visible instead of being reported as a clean close.
                tracing::warn!(
                    driver = REDIS_PROVIDER_ID,
                    resource_key,
                    error = %error,
                    "close could not be confirmed; the physical-connection budget is not \
                     released and a retry is refused"
                );
                Ok(CloseDisposition::CloseUnconfirmed)
            }
        }
    }
}

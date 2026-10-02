//! The thirteen contract methods, and nothing else.
//!
//! This file is the *declaration surface*: every method of
//! [`ResourceProvider`] exactly as postgres answers it. The bookkeeping they
//! lean on — handle ownership, budget accounting, transaction pinning, the
//! session read — lives in [`super`], which also holds the module doc that maps
//! each contract method to the driver call that really runs behind it.
//!
//! The rules restated for the reader who only opens this file:
//!
//! * **A handle is checked on every call** ([`ResourceHandle::check`]). A handle
//!   from another provider, or from an older instance of this one, is an error
//!   — never a silent no-op.
//! * **An unsupported request is an error.** Everything this provider does not
//!   declare (a fixed session, snapshots, an in-place namespace switch, an
//!   isolation level, baseline replay, a pinned streaming connection) is
//!   refused with an explicit [`ResourceError`] *before* any budget is charged
//!   and *before* any socket is touched.
//! * **No observation is invented.** [`ResourceProvider::observe_session`] reads
//!   the server; a field the read cannot confirm stays `Unknown` / `Partial`.
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
    CancelDisposition, CancelReceipt, CloseDisposition, ContextChangeDisposition,
    ExecutionCompletion, ResetDisposition, ResourceHealth, SessionObservation, SessionState,
    TransactionObservation, TransactionOptions,
};
use datazen_driver_api::{DriverError, QueryExecutionId};

use super::super::capabilities::{postgres_connection_cost, POSTGRES_PROVIDER_ID};
use super::super::registry::OpenTransaction;
use super::PostgresResourceProvider;

#[async_trait]
impl ResourceProvider for PostgresResourceProvider {
    fn provider_id(&self) -> &str {
        POSTGRES_PROVIDER_ID
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
            provider_id: POSTGRES_PROVIDER_ID.to_string(),
            resource_key: format!("pg_pool:config:{}", request.connection_config.id),
            // A pool lends a backend per statement. That is not a fixed
            // session, and claiming `Fixed` here is what would let a caller
            // pin assumptions the driver cannot keep.
            session_continuity: SessionContinuity::Leased,
            reuse_policy: ReusePolicy::SingleUse,
            // No initialization protocol is proven for this driver, so none is
            // declared. `acquire_resource` refuses a non-empty baseline rather
            // than silently skipping it.
            initialization_requirements: Vec::new(),
            connection_cost_policy: postgres_connection_cost(&request.connection_config),
            namespace_shape: self.namespace_shape.clone(),
        })
    }

    async fn acquire_resource(
        &self,
        request: &AcquireResourceRequest,
        budget: &Arc<dyn BudgetPort>,
    ) -> Result<ResourceHandle, ResourceError> {
        let physical_connections = self.validate_acquisition(request)?;

        let permit = budget
            .acquire_physical_connections(physical_connections)
            .await?;

        let connection = match self.driver.connect_impl(&request.connection_config).await {
            Ok(connection) => connection,
            Err(error) => {
                // The connect failed, so the charge is released here and nowhere
                // else: the caller receives no handle it could close with.
                if let Err(release_error) = budget.release_physical_connections(&permit).await {
                    tracing::error!(
                        driver = POSTGRES_PROVIDER_ID,
                        error = %release_error,
                        "failed to release the physical-connection budget after a failed connect"
                    );
                }
                return Err(ResourceError::Driver(error));
            }
        };

        Ok(self.register_resource(connection, permit, budget.clone()))
    }

    async fn execute_on_resource(
        &self,
        handle: &ResourceHandle,
        execution_id: &QueryExecutionId,
        call: &CommandCall,
        sink: &dyn ResultSink,
    ) -> Result<ExecutionCompletion, ResourceError> {
        let resource = self.resource_of(handle, "execute_on_resource")?;

        // Register the execution id against *this* session for the duration of
        // the call, so a `request_cancel` that arrives while the statement runs
        // addresses this execution and no other. It is removed again below, so
        // the driver's registry cannot grow without bound.
        self.driver
            .prepare_query_execution_impl(&resource.connection, execution_id)
            .await
            .map_err(ResourceError::Driver)?;

        let outcome = self.run_command(handle, &resource, call, sink).await;
        // Either way the registration goes away: after this call the execution
        // is no longer running, and a later cancel must report that honestly
        // instead of replaying a stale registration.
        let _ = self
            .driver
            .finish_query_execution(&resource.connection, execution_id)
            .await;

        outcome
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
            // anything this provider started.
            state: SessionState::Ready,
            transaction,
            context,
            handles: self.session_handles_of(handle, &resource),
            protocol_drained: true,
            // The observation succeeded, which is the evidence for `Healthy`.
            resource_health: ResourceHealth::Healthy,
            context_revision: revision,
        })
    }

    async fn change_context(
        &self,
        handle: &ResourceHandle,
        desired: &NamespaceTarget,
    ) -> Result<ContextChangeDisposition, ResourceError> {
        self.resource_of(handle, "change_context")?;
        // Validate the target first: a namespace PostgreSQL does not have is
        // refused even though the disposition would have been the same.
        self.namespace_shape.canonicalize(desired)?;

        // PostgreSQL cannot attach a live session to another database. The
        // driver reaches another database by routing the statement to a
        // *different* pool, i.e. a different resource with its own handle — so
        // the honest disposition is "acquire a replacement". No SQL is issued
        // here and nothing is silently reconnected.
        Ok(ContextChangeDisposition::RequiresReplacement)
    }

    async fn begin_transaction(
        &self,
        handle: &ResourceHandle,
        options: &TransactionOptions,
    ) -> Result<TransactionObservation, ResourceError> {
        let resource = self.resource_of(handle, "begin_transaction")?;

        // `TransactionSupport::isolation_levels` is declared empty, so a level
        // is refused rather than dropped: `begin_transaction_impl` sends a bare
        // `BEGIN`, which would ignore the request and report a transaction the
        // caller believes runs at a different level.
        if let Some(level) = options.isolation_level.as_deref() {
            if !level.trim().is_empty() {
                return Err(ResourceError::OperationNotSupported {
                    driver: POSTGRES_PROVIDER_ID.to_string(),
                    operation: "begin_transaction".to_string(),
                    reason: format!(
                        "isolation level '{level}' is refused: this provider declares \
                         TransactionSupport::isolation_levels as empty because \
                         begin_transaction_impl sends a bare BEGIN. No level is honoured by \
                         accident."
                    ),
                });
            }
        }

        // The same rule for read-only: a bare `BEGIN` is read-write, so
        // accepting the option would report a read-only transaction that is not
        // one.
        if options.read_only == Some(true) {
            return Err(ResourceError::OperationNotSupported {
                driver: POSTGRES_PROVIDER_ID.to_string(),
                operation: "begin_transaction".to_string(),
                reason: "read_only = true is refused: begin_transaction_impl sends a bare BEGIN. \
                         A repeatable-read read-only snapshot is an undeclared capability \
                         (snapshots = unsupported)."
                    .to_string(),
            });
        }

        if resource.transaction.is_some() {
            return Err(ResourceError::TransactionResolutionRequired {
                operation: "begin_transaction".to_string(),
            });
        }

        let transaction = self
            .driver
            .begin_transaction_impl(&resource.connection)
            .await
            .map_err(ResourceError::Driver)?;

        let opened = OpenTransaction {
            id: transaction.id,
            connection_id: transaction.connection_id,
        };
        let revision = self.bump_revision(handle);
        if let Some(live) = self.lock().live.get_mut(handle.resource_key()) {
            live.transaction = Some(opened.clone());
        }

        Ok(TransactionObservation::begun(opened.id, revision))
    }

    async fn commit_transaction(
        &self,
        handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        self.finish_transaction(handle, "commit_transaction", true)
            .await
    }

    async fn rollback_transaction(
        &self,
        handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        self.finish_transaction(handle, "rollback_transaction", false)
            .await
    }

    async fn request_cancel(
        &self,
        handle: &ResourceHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<CancelReceipt, ResourceError> {
        let resource = self.resource_of(handle, "request_cancel")?;

        // Execution-addressed, never session-wide: the driver looks the id up in
        // its own registry, refuses a session mismatch, and delivers
        // `pg_cancel_backend` through a separate control pool so the cancel does
        // not need a connection from the busy pool. The legacy session-wide
        // `cancel_query` is never called.
        let disposition = match self
            .driver
            .cancel_query_with_execution_impl(&resource.connection, execution_id)
            .await
        {
            Ok(()) => CancelDisposition::Requested,
            Err(DriverError::QueryExecutionNotFound(_))
            | Err(DriverError::QueryExecutionSessionMismatch) => CancelDisposition::NotRegistered,
            Err(error) => return Err(ResourceError::Driver(error)),
        };

        Ok(CancelReceipt {
            execution_id: execution_id.clone(),
            disposition,
            // The driver does not publish an execution's state after a cancel
            // *request*, and a request is not a terminal event. `None` is the
            // honest value; a plausible `Cancelled` here would be a lie.
            state: None,
        })
    }

    async fn reset_resource(
        &self,
        handle: &ResourceHandle,
        _baseline: &Baseline,
    ) -> Result<ResetDisposition, ResourceError> {
        self.resource_of(handle, "reset_resource")?;
        // The baseline is not replayed: this driver has no proven
        // initialization protocol (see `describe_resource`). `Discard` is the
        // truthful disposition — the resource is not claimed to be back at a
        // baseline it never had. The handle stays valid; close and re-acquire is
        // the way to a fresh session.
        Ok(ResetDisposition::Discard)
    }

    async fn close_resource(
        &self,
        handle: &ResourceHandle,
    ) -> Result<CloseDisposition, ResourceError> {
        handle.check(POSTGRES_PROVIDER_ID, self.runtime_epoch)?;

        let resource = {
            let mut registry = self.lock();
            match registry.live.remove(handle.resource_key()) {
                Some(resource) => resource,
                // A confirmed close is idempotent, and its budget was already
                // released — releasing twice would be the bug.
                None if registry.closed.contains(handle.resource_key()) => {
                    return Ok(CloseDisposition::Closed);
                }
                None => {
                    return Err(ResourceError::InvalidResourceState {
                        resource_key: handle.resource_key().to_string(),
                        operation: "close_resource".to_string(),
                        state: "this provider instance never held this resource, and no \
                                confirmed close is recorded for it"
                            .to_string(),
                    });
                }
            }
        };

        match self.driver.disconnect_impl(resource.connection).await {
            Ok(()) => {
                self.lock().closed.insert(handle.resource_key().to_string());
                // The connections are really gone, so the charge is really
                // recoverable. Released here and nowhere else.
                resource
                    .budget
                    .release_physical_connections(&resource.permit)
                    .await?;
                Ok(CloseDisposition::Closed)
            }
            Err(error) => {
                // The pools may or may not be gone. The budget is **not**
                // released and the resource is not recorded as closed, so the
                // leak is visible instead of being reported as a clean close.
                tracing::warn!(
                    driver = POSTGRES_PROVIDER_ID,
                    resource_key = %handle.resource_key(),
                    error = %error,
                    "close could not be confirmed; the physical-connection budget is not \
                     released and a retry is refused"
                );
                Ok(CloseDisposition::CloseUnconfirmed)
            }
        }
    }
}

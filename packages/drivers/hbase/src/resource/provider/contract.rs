//! The fourteen [`ResourceProvider`] methods, one by one.
//!
//! A child module so it reaches the provider's private bookkeeping without
//! widening any visibility.
//!
//! The rules restated, because each one is load-bearing here:
//!
//! 1. **The handle is checked on every call.** A handle from another provider,
//!    or from another instance of this one, is refused before anything else
//!    happens.
//! 2. **An unsupported request is an explicit error, returned before the
//!    budget is charged and before the wire is touched.** Where the contract has
//!    a dedicated "not supported" value (`change_context`, `request_cancel`) it
//!    is returned as a value; where it does not (`begin`/`commit`/`rollback`)
//!    it is an `Err`. Neither path is a silent `Ok(())`.
//! 3. **No observation is invented.** Context, transaction and handles stay at
//!    their honest constants; the one cell a probe can earn — resource health —
//!    is derived from a measured status, never defaulted.
//! 4. **The budget is released exactly once**, at confirmed close or after a
//!    failed connect.

use std::sync::Arc;

use async_trait::async_trait;
use datazen_driver_api::capabilities::CapabilityRegistry;
use datazen_driver_api::namespace::{NamespaceShape, NamespaceTarget};
use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPort, CommandCall, DescribeResourceRequest,
    ResourceDescriptor, ResourceError, ResourceHandle, ResourceProvider, ResultSink,
};
use datazen_driver_api::session::{
    CancelDisposition, CancelReceipt, CloseDisposition, CompletionStatus, ContextChangeDisposition,
    EffectOutcome, ExecutionCompletion, ResetDisposition, ResourceHealth, SessionContext,
    SessionObservation, TransactionObservation, TransactionOptions, TransactionState,
};
use datazen_driver_api::{DatabaseDriver, DriverError, QueryExecutionId};

use super::super::capabilities::HBASE_PROVIDER_ID;
use super::super::observation::observed_session;
use super::super::payload::decode_command_result;
use super::super::probe::probe_liveness;
use super::HBaseResourceProvider;

/// Why this driver has no transaction, quoted wherever a caller asks for one.
///
/// Not a shrug: the Stargate REST API exposes no transaction endpoint, and
/// `HBaseDriver::execute` refuses writes before they reach the wire, so a
/// transaction here would cover nothing and could not be observed. HBase
/// itself does have transactions — they are simply not reachable through this
/// surface, which is why the refusal names the surface.
const NO_TRANSACTIONS: &str = "the Stargate REST API exposes no transaction endpoint, and this \
                                driver refuses writes outright (HBaseDriver::execute), so there \
                                is no transaction on this resource to begin, commit or roll back";

/// The transaction observation reported for a resource that has none.
///
/// `Unsupported` with no id and no effect — as opposed to `Idle`, which would
/// claim a transaction was observed and found to be nothing.
fn no_transaction(revision: u64) -> TransactionObservation {
    TransactionObservation {
        state: TransactionState::Unsupported,
        transaction_id: None,
        effect: None,
        revision,
    }
}

#[async_trait]
impl ResourceProvider for HBaseResourceProvider {
    fn provider_id(&self) -> &str {
        HBASE_PROVIDER_ID
    }

    fn capabilities(&self) -> &CapabilityRegistry {
        &self.capabilities
    }

    fn namespace_shape(&self) -> &NamespaceShape {
        &self.namespace_shape
    }

    /// What this resource would be, without becoming one.
    ///
    /// No connection is opened and no budget is charged: describing must be
    /// free. The target is canonicalized first, so a caller asking for a
    /// namespace level this driver does not have is told so here — where it can
    /// still do something about it.
    async fn describe_resource(
        &self,
        request: &DescribeResourceRequest,
    ) -> Result<ResourceDescriptor, ResourceError> {
        self.descriptor(request)
    }

    /// Open one resource, or explain why it cannot be opened.
    ///
    /// Order matters and is deliberate: validate, then charge, then connect. A
    /// rejected request leaves the caller charged nothing and nothing opened.
    async fn acquire_resource(
        &self,
        request: &AcquireResourceRequest,
        budget: &Arc<dyn BudgetPort>,
    ) -> Result<ResourceHandle, ResourceError> {
        let (handle, _live) = self.acquire(request, budget).await?;
        Ok(handle)
    }

    /// Run one command on this resource and stream its rows into the sink.
    async fn execute_on_resource(
        &self,
        handle: &ResourceHandle,
        execution_id: &QueryExecutionId,
        call: &CommandCall,
        sink: &dyn ResultSink,
    ) -> Result<ExecutionCompletion, ResourceError> {
        let resource = self.resource_of(handle, "execute_on_resource")?;
        tracing::debug!(
            execution_id = %execution_id.as_str(),
            command = %call.command,
            resource_key = %handle.resource_key(),
            "hbase: executing on resource"
        );
        self.run_command(&resource, call, sink).await
    }

    /// Report what one cheap round trip proves about this resource.
    ///
    /// Deliberately narrow: the probe can establish liveness and nothing else,
    /// so context, transaction and handles stay at their honest constants. See
    /// [`observed_session`](super::super::observation::observed_session) for
    /// why each one is what it is.
    async fn observe_session(
        &self,
        handle: &ResourceHandle,
    ) -> Result<SessionObservation, ResourceError> {
        let resource = self.resource_of(handle, "observe_session")?;
        let liveness = probe_liveness(self.driver().as_ref(), &resource.connection).await;
        let revision = self.bump_revision(handle)?;
        Ok(observed_session(liveness, revision))
    }

    /// Refuse a namespace change outright.
    ///
    /// Not `RequiresReplacement`: there is no other namespace to replace *with*.
    /// One Stargate endpoint has one namespace and it is pinned by the base URL
    /// the connection was built from.
    async fn change_context(
        &self,
        handle: &ResourceHandle,
        desired: &NamespaceTarget,
    ) -> Result<ContextChangeDisposition, ResourceError> {
        self.resource_of(handle, "change_context")?;
        // Canonicalized first: a target naming a level this driver does not have
        // is refused as invalid, which is more informative than reporting the
        // switch as unsupported.
        self.canonical_target(desired)?;
        Ok(ContextChangeDisposition::Unsupported)
    }

    async fn begin_transaction(
        &self,
        handle: &ResourceHandle,
        _options: &TransactionOptions,
    ) -> Result<TransactionObservation, ResourceError> {
        self.resource_of(handle, "begin_transaction")?;
        Err(ResourceError::unsupported(
            HBASE_PROVIDER_ID,
            "begin_transaction",
            NO_TRANSACTIONS,
        ))
    }

    async fn commit_transaction(
        &self,
        handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        self.resource_of(handle, "commit_transaction")?;
        Err(ResourceError::unsupported(
            HBASE_PROVIDER_ID,
            "commit_transaction",
            NO_TRANSACTIONS,
        ))
    }

    async fn rollback_transaction(
        &self,
        handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        self.resource_of(handle, "rollback_transaction")?;
        Err(ResourceError::unsupported(
            HBASE_PROVIDER_ID,
            "rollback_transaction",
            NO_TRANSACTIONS,
        ))
    }

    /// Report that no execution on this resource can be cancelled.
    ///
    /// `CancelDisposition::Unsupported` with **no** fallback: the driver's own
    /// `cancel_query` is `Ok(())` — a no-op that would produce a receipt
    /// implying a cancel was requested. The contract forbids exactly that, so
    /// the honest answer is a disposition that says nothing happened.
    async fn request_cancel(
        &self,
        handle: &ResourceHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<CancelReceipt, ResourceError> {
        self.resource_of(handle, "request_cancel")?;
        Ok(CancelReceipt {
            execution_id: execution_id.clone(),
            disposition: CancelDisposition::Unsupported,
            state: None,
        })
    }

    /// Report that this resource cannot be handed out again.
    ///
    /// `Discard`, not `Clean`: nothing was replayed, so there is no evidence the
    /// resource is back to a known state and the caller must not reuse it.
    ///
    /// The resource itself is deliberately *left in the registry*: it still owns
    /// a budget permit, and dropping the record here would strand that charge
    /// with no way to release it. It is unusable by contract, not forgotten —
    /// `close_resource` still releases it exactly once.
    async fn reset_resource(
        &self,
        handle: &ResourceHandle,
        _baseline: &Baseline,
    ) -> Result<ResetDisposition, ResourceError> {
        self.resource_of(handle, "reset_resource")?;
        Ok(ResetDisposition::Discard)
    }

    /// Close this resource, releasing its budget exactly once.
    async fn close_resource(
        &self,
        handle: &ResourceHandle,
    ) -> Result<CloseDisposition, ResourceError> {
        self.close(handle).await
    }
}

/// Everything one execution needs after the sink has been fed.
///
/// Split out of the trait method so the pre-execution state and the
/// post-execution state can be captured honestly on either side of the command.
impl HBaseResourceProvider {
    async fn run_command(
        &self,
        resource: &super::super::registry::LiveResource,
        call: &CommandCall,
        sink: &dyn ResultSink,
    ) -> Result<ExecutionCompletion, ResourceError> {
        // Unobserved on both sides, and deliberately *not* the acquisition
        // target: this driver has no readable context, so echoing the caller's
        // own input back would be the provider agreeing with itself.
        let context_before = SessionContext::unobserved();

        let result = self
            .driver()
            .execute_command(&resource.connection, &call.command, call.input.clone())
            .await;

        let result = match result {
            Ok(result) => result,
            Err(error) => {
                // The sink learns the execution ended before the error reaches
                // the caller, so a consumer is never left waiting on rows that
                // will not come.
                let reason = format!("driver command '{}' failed: {error}", call.command);
                let _ = sink.fail(&reason).await;
                return Err(ResourceError::Driver(DriverError::QueryFailed(reason)));
            }
        };

        let (statement_results, chunks) = decode_command_result(&result.data)?;
        for chunk in chunks {
            // A sink that rejects a chunk has already been handed rows, so the
            // statement did run; `SinkRejected` reports the delivery failure
            // without pretending the work never happened.
            sink.write(chunk).await?;
        }
        sink.complete().await?;

        Ok(ExecutionCompletion {
            completion_status: CompletionStatus::Succeeded,
            effect_outcome: EffectOutcome::Completed,
            statement_results,
            context_before,
            context_after: SessionContext::unobserved(),
            transaction_observation: no_transaction(resource.revision),
            // No cursor, no prepared statement and no scanner: this driver
            // issues none. A scan's scanner is read to exhaustion and deleted
            // inside the command that created it.
            session_handles: Vec::new(),
            // Every row was buffered and handed to the sink; the client will not
            // be read again for this execution.
            protocol_drained: true,
            // The command returned, which is the evidence for `Healthy` — a
            // command that failed is an `Err` above, never a completion.
            resource_health: ResourceHealth::Healthy,
        })
    }
}
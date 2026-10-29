//! Synthetic provider for host wiring only; no live database assertions.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use datazen_driver_api::capabilities::{
    Availability, CapabilityRegistry, CapabilitySet, SessionContinuity,
};
use datazen_driver_api::namespace::{NamespaceShape, NamespaceTarget};
use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPort, CommandCall, ConnectionCostPolicy,
    DescribeResourceRequest, ResourceDescriptor, ResourceError, ResourceHandle, ResourceProvider,
    ResultSink, ReusePolicy,
};
use datazen_driver_api::session::{
    CancelReceipt, CloseDisposition, ContextChangeDisposition, ExecutionCompletion,
    ResetDisposition, ResourceHealth, SessionHandleRef, SessionObservation, TransactionObservation,
    TransactionOptions,
};
use datazen_driver_api::{DriverError, QueryExecutionId};

pub(super) struct CountingProvider {
    driver_id: String,
    capabilities: CapabilityRegistry,
    shape: NamespaceShape,
    acquires: AtomicU64,
    closes: AtomicU64,
    resets: AtomicU64,
    pub calls: Mutex<Vec<(String, String)>>,
    permits: Mutex<
        Vec<(
            Arc<dyn BudgetPort>,
            datazen_driver_api::resource::BudgetPermit,
        )>,
    >,
}

impl CountingProvider {
    pub(super) fn new(driver_id: &str) -> Arc<Self> {
        Arc::new(Self {
            driver_id: driver_id.to_owned(),
            capabilities: wiring_capabilities(driver_id),
            shape: shape(),
            acquires: AtomicU64::new(0),
            closes: AtomicU64::new(0),
            resets: AtomicU64::new(0),
            calls: Mutex::new(vec![]),
            permits: Mutex::new(vec![]),
        })
    }

    pub(super) fn closes(&self) -> u64 {
        self.closes.load(Ordering::SeqCst)
    }

    pub(super) fn acquires(&self) -> u64 {
        self.acquires.load(Ordering::SeqCst)
    }
}

fn wiring_capabilities(driver_id: &str) -> CapabilityRegistry {
    let mut set = CapabilitySet::default();
    set.stateful_session = Availability::Supported;
    let mut registry = CapabilityRegistry::new(
        driver_id.to_owned(),
        datazen_driver_api::capabilities::CapabilitySnapshot::new(
            driver_id,
            "test",
            datazen_driver_api::PROTOCOL_VERSION,
            0,
        ),
    );
    registry.capabilities = set;
    registry.snapshot.confirmed.insert(
        datazen_driver_api::restore_sql::CAPABILITY_KEY.into(),
        datazen_driver_api::restore_sql::IMPLICIT_COMMIT.into(),
    );
    registry
}

#[async_trait]
impl ResourceProvider for CountingProvider {
    fn provider_id(&self) -> &str {
        &self.driver_id
    }

    fn capabilities(&self) -> &CapabilityRegistry {
        &self.capabilities
    }

    fn namespace_shape(&self) -> &NamespaceShape {
        &self.shape
    }

    fn connection_handle(
        &self,
        _handle: &ResourceHandle,
    ) -> Result<datazen_driver_api::ConnectionHandle, ResourceError> {
        Err(ResourceError::unsupported(
            self.provider_id(),
            "connection_handle",
            "fixture has no native driver connection",
        ))
    }

    async fn describe_resource(
        &self,
        request: &DescribeResourceRequest,
    ) -> Result<ResourceDescriptor, ResourceError> {
        Ok(ResourceDescriptor {
            provider_id: self.driver_id.clone(),
            resource_key: format!("{}:key", request.connection_config.id),
            session_continuity: SessionContinuity::Fixed,
            reuse_policy: ReusePolicy::Unknown,
            initialization_requirements: Vec::new(),
            connection_cost_policy: ConnectionCostPolicy::DeclaredConservative {
                declared_cost: 1,
                hard_cap: None,
            },
            namespace_shape: shape(),
        })
    }

    async fn acquire_resource(
        &self,
        request: &AcquireResourceRequest,
        budget: &Arc<dyn BudgetPort>,
    ) -> Result<ResourceHandle, ResourceError> {
        let permit = budget.acquire_physical_connections(1).await?;
        let seq = self.acquires.fetch_add(1, Ordering::SeqCst);
        let handle = ResourceHandle::issue(
            self.driver_id.clone(),
            format!("{}#{}", request.connection_config.id, seq),
            1,
        );
        self.permits.lock().unwrap().push((budget.clone(), permit));
        Ok(handle)
    }

    async fn execute_on_resource(
        &self,
        handle: &ResourceHandle,
        execution_id: &QueryExecutionId,
        call: &CommandCall,
        sink: &dyn ResultSink,
    ) -> Result<ExecutionCompletion, ResourceError> {
        self.calls.lock().unwrap().push((
            execution_id.as_str().into(),
            call.input["sql"].as_str().unwrap().into(),
        ));
        sink.complete().await?;
        let observed = self.observe_session(handle).await?;
        let mut completion = ExecutionCompletion::not_started();
        completion.completion_status = datazen_driver_api::session::CompletionStatus::Succeeded;
        completion.effect_outcome = datazen_driver_api::session::EffectOutcome::Completed;
        completion.context_before = observed.context.clone();
        completion.context_after = observed.context;
        completion.transaction_observation = observed.transaction;
        completion.protocol_drained = true;
        completion.resource_health = ResourceHealth::Healthy;
        Ok(completion)
    }

    async fn observe_session(
        &self,
        _handle: &ResourceHandle,
    ) -> Result<SessionObservation, ResourceError> {
        {
            use datazen_driver_api::session::*;
            let mut observed = SessionObservation::unobservable();
            observed.state = SessionState::Ready;
            observed.context.namespace = NamespaceTarget::empty().with_database("app");
            observed.context.transaction_state = TransactionState::Idle;
            observed.context.autocommit = Some(true);
            observed.context.confidence = ObservationConfidence::Confirmed;
            observed.transaction.state = TransactionState::Idle;
            observed.protocol_drained = true;
            observed.resource_health = ResourceHealth::Healthy;
            Ok(observed)
        }
    }

    async fn change_context(
        &self,
        _handle: &ResourceHandle,
        _desired: &NamespaceTarget,
    ) -> Result<ContextChangeDisposition, ResourceError> {
        Err(ResourceError::Driver(DriverError::Unsupported(
            "not exercised".into(),
        )))
    }

    async fn begin_transaction(
        &self,
        _handle: &ResourceHandle,
        _options: &TransactionOptions,
    ) -> Result<TransactionObservation, ResourceError> {
        Err(ResourceError::Driver(DriverError::Unsupported(
            "not exercised".into(),
        )))
    }

    async fn commit_transaction(
        &self,
        _handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        Err(ResourceError::Driver(DriverError::Unsupported(
            "not exercised".into(),
        )))
    }

    async fn rollback_transaction(
        &self,
        _handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        Err(ResourceError::Driver(DriverError::Unsupported(
            "not exercised".into(),
        )))
    }

    async fn request_cancel(
        &self,
        _handle: &ResourceHandle,
        _execution_id: &QueryExecutionId,
    ) -> Result<CancelReceipt, ResourceError> {
        Err(ResourceError::Driver(DriverError::Unsupported(
            "not exercised".into(),
        )))
    }

    async fn reset_resource(
        &self,
        _handle: &ResourceHandle,
        _baseline: &Baseline,
    ) -> Result<ResetDisposition, ResourceError> {
        self.resets.fetch_add(1, Ordering::SeqCst);
        Ok(ResetDisposition::Discard)
    }

    async fn close_resource(
        &self,
        _handle: &ResourceHandle,
    ) -> Result<CloseDisposition, ResourceError> {
        let permits = std::mem::take(&mut *self.permits.lock().unwrap());
        for (budget, permit) in permits {
            budget.release_physical_connections(&permit).await?;
        }
        self.closes.fetch_add(1, Ordering::SeqCst);
        Ok(CloseDisposition::Closed)
    }

    async fn close_handle(
        &self,
        _handle: &ResourceHandle,
        session_handle: &SessionHandleRef,
    ) -> Result<(), ResourceError> {
        Err(ResourceError::unsupported(
            self.driver_id.as_str(),
            "close_handle",
            format!(
                "synthetic provider issues no session handles: {}",
                session_handle.handle_id
            ),
        ))
    }
}

fn shape() -> NamespaceShape {
    use datazen_driver_api::namespace::{NamespaceLevel, NamespaceLevelKind};
    NamespaceShape {
        levels: vec![NamespaceLevel::present(NamespaceLevelKind::Database, true)],
        ..Default::default()
    }
}

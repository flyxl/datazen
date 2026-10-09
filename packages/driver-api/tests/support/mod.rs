//! Shared fakes for the out-of-tree contract test: the resource contract must
//! be implementable **from outside the crate**, using only its public API.
//!
//! Nothing here can reach a private item of `datazen-driver-api`. If a provider
//! contract needed a `pub(crate)` hook, a sealed type or a private
//! constructor, `resource_contract.rs` would stop compiling — which is exactly
//! the guarantee Delivery 1 is supposed to make (plan line 92:
//! 「opaque resource、ResourceProvider、固定执行、状态观察、reset、close 和精确
//! cancel 契约」).
//!
//! The fake provider below is deliberately written the way an out-of-tree
//! driver would write it, and it refuses nothing by accident: every refusal is
//! an explicit value, because criterion 3 forbids a missing capability
//! answering with a no-op success.

pub use std::sync::atomic::{AtomicUsize, Ordering};
pub use std::sync::Arc;

pub use datazen_driver_api::capabilities::{
    Availability, CapabilityRegistry, CapabilitySet, CapabilitySnapshot, ContextObservation,
    PreciseCancelSupport, ResetForReuse, SessionContinuity, SnapshotSupport,
    TransactionObservation as TransactionObservationCap,
};
pub use datazen_driver_api::namespace::{
    NamespaceLevel, NamespaceLevelKind, NamespaceShape, NamespaceTarget,
};
pub use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, CommandCall, ConnectionCostPolicy,
    DescribeResourceRequest, ResourceDescriptor, ResourceError, ResourceHandle, ResourceProvider,
    ResourceScope, ResultChunk, ResultSink,
};
pub use datazen_driver_api::session::{
    CancelDisposition, CancelReceipt, CloseDisposition,
    CloseDisposition as SessionCloseDisposition, CompletionStatus, ContextChangeDisposition,
    EffectOutcome, ExecutionCompletion, ExecutionState, ObservationConfidence, ResetDisposition,
    ResourceHealth, SessionContext, SessionHandleKind, SessionHandleRef, SessionObservation,
    SessionState, TransactionObservation, TransactionOptions, TransactionState,
};
pub use datazen_driver_api::{
    require_resource_provider, ConnectionConfig, DatabaseDriverFactory, DdlAtomicity,
    QueryExecutionId,
};

// ---------------------------------------------------------------------------
// A budget an out-of-tree provider can charge against.
// ---------------------------------------------------------------------------

pub struct AllowanceBudget {
    pub outstanding: AtomicUsize,
    pub issued: AtomicUsize,
    pub released: AtomicUsize,
    pub ceiling: u32,
}

impl AllowanceBudget {
    pub fn new(ceiling: u32) -> Self {
        Self {
            outstanding: AtomicUsize::new(0),
            issued: AtomicUsize::new(0),
            released: AtomicUsize::new(0),
            ceiling,
        }
    }
}

#[datazen_driver_api::async_trait]
impl BudgetPort for AllowanceBudget {
    async fn acquire_physical_connections(
        &self,
        requested: u32,
    ) -> Result<BudgetPermit, ResourceError> {
        if requested > self.ceiling {
            return Err(ResourceError::BudgetDenied {
                requested,
                reason: "test ceiling".into(),
            });
        }
        let seq = self.issued.fetch_add(1, Ordering::SeqCst);
        self.outstanding.fetch_add(1, Ordering::SeqCst);
        Ok(BudgetPermit {
            permit_id: format!("permit-{seq}"),
            physical_connections: requested,
        })
    }

    async fn release_physical_connections(&self, _: &BudgetPermit) -> Result<(), ResourceError> {
        self.released.fetch_add(1, Ordering::SeqCst);
        self.outstanding.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// A sink that keeps every chunk, so streaming is observable from outside.
// ---------------------------------------------------------------------------

pub struct CollectingSink {
    pub chunks: std::sync::Mutex<Vec<ResultChunk>>,
    pub completed: AtomicUsize,
    pub failed: AtomicUsize,
}

impl CollectingSink {
    pub fn new() -> Self {
        Self {
            chunks: std::sync::Mutex::new(Vec::new()),
            completed: AtomicUsize::new(0),
            failed: AtomicUsize::new(0),
        }
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.lock().map(|chunks| chunks.len()).unwrap_or(0)
    }
}

#[datazen_driver_api::async_trait]
impl ResultSink for CollectingSink {
    async fn write(&self, chunk: ResultChunk) -> Result<(), ResourceError> {
        // A poisoned lock is a real defect, but it must not be swallowed by a
        // silent success either: report it as a sink rejection.
        let mut chunks = self
            .chunks
            .lock()
            .map_err(|_| ResourceError::SinkRejected {
                reason: "collector lock poisoned".into(),
            })?;
        chunks.push(chunk);
        Ok(())
    }

    async fn complete(&self) -> Result<(), ResourceError> {
        self.completed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn fail(&self, _: &str) -> Result<(), ResourceError> {
        self.failed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The out-of-tree provider.
// ---------------------------------------------------------------------------

/// What this fake provider is honestly able to do. Everything else answers
/// `Unsupported` by name.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ProviderProfile {
    pub stateful: bool,
    pub observes_context: bool,
    pub precise_cancel: bool,
    pub verified_reset: bool,
}

pub struct FakeProvider {
    pub epoch: u64,
    pub profile: ProviderProfile,
    pub shape: NamespaceShape,
    pub capabilities: CapabilityRegistry,
    pub open: std::sync::Mutex<std::collections::BTreeMap<String, BudgetPermit>>,
    pub cancelled: std::sync::Mutex<std::collections::BTreeSet<String>>,
    pub seen: std::sync::Mutex<std::collections::BTreeSet<String>>,
    pub disconnects: AtomicUsize,
}

impl FakeProvider {
    pub fn new(epoch: u64, profile: ProviderProfile) -> Self {
        let mut shape = NamespaceShape::default();
        shape.levels = vec![
            NamespaceLevel {
                kind: NamespaceLevelKind::Database,
                exists: true,
                required: true,
            },
            NamespaceLevel {
                kind: NamespaceLevelKind::Schema,
                exists: true,
                required: false,
            },
        ];

        let mut capabilities = CapabilitySet::default();
        capabilities.stateful_session = Availability::Supported;
        capabilities.context_observation = ContextObservation::Full;
        capabilities.transaction_observation = TransactionObservationCap::Full;
        capabilities.session_scoped_handles =
            datazen_driver_api::capabilities::SessionScopedHandleSupport::Supported;
        capabilities.snapshots = SnapshotSupport::Coordinated;
        capabilities.precise_cancel = PreciseCancelSupport::Supported;
        capabilities.reset_for_reuse = ResetForReuse::Verified;
        capabilities.transactions.savepoints = Availability::Supported;
        capabilities
            .ddl_atomicity
            .by_operation
            .insert("create_table".to_string(), DdlAtomicity::Transactional);

        let snapshot = CapabilitySnapshot::new(
            "fake-provider",
            "1.2.3",
            datazen_driver_api::PROTOCOL_VERSION,
            9,
        );
        Self {
            epoch,
            profile,
            shape,
            capabilities: CapabilityRegistry::new("fake-provider", snapshot),
            open: std::sync::Mutex::new(Default::default()),
            cancelled: std::sync::Mutex::new(Default::default()),
            seen: std::sync::Mutex::new(Default::default()),
            disconnects: AtomicUsize::new(0),
        }
    }

    pub fn stateful() -> Self {
        Self::new(
            1,
            ProviderProfile {
                stateful: true,
                observes_context: true,
                precise_cancel: true,
                verified_reset: true,
            },
        )
    }

    pub fn minimal() -> Self {
        Self::new(
            1,
            ProviderProfile {
                stateful: false,
                observes_context: false,
                precise_cancel: false,
                verified_reset: false,
            },
        )
    }

    pub fn require(&self, what: &str) -> Result<(), ResourceError> {
        if self.profile.stateful {
            Ok(())
        } else {
            Err(ResourceError::unsupported(
                "fake-provider",
                what,
                "not implemented by this fake",
            ))
        }
    }
}

impl std::fmt::Debug for FakeProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeProvider")
            .field("epoch", &self.epoch)
            .finish()
    }
}

pub fn config() -> ConnectionConfig {
    serde_json::from_value(serde_json::json!({
        "id": "cfg-1",
        "name": "fake",
        "databaseType": "fake",
    }))
    .expect("a minimal config deserializes")
}

#[datazen_driver_api::async_trait]
impl ResourceProvider for FakeProvider {
    fn provider_id(&self) -> &str {
        "fake-provider"
    }

    fn capabilities(&self) -> &CapabilityRegistry {
        &self.capabilities
    }

    fn namespace_shape(&self) -> &NamespaceShape {
        &self.shape
    }

    async fn describe_resource(
        &self,
        request: &DescribeResourceRequest,
    ) -> Result<ResourceDescriptor, ResourceError> {
        let canonical = self.shape.canonicalize(&request.target)?;
        self.shape
            .validate_requirements(&request.target, &Default::default())?;
        Ok(ResourceDescriptor {
            provider_id: self.provider_id().to_string(),
            resource_key: format!("fake#{}", canonical.path.join("/")),
            session_continuity: if self.profile.stateful {
                SessionContinuity::Fixed
            } else {
                SessionContinuity::Leased
            },
            reuse_policy: if self.profile.verified_reset {
                datazen_driver_api::resource::ReusePolicy::ReusableAfterReset
            } else {
                datazen_driver_api::resource::ReusePolicy::Unknown
            },
            initialization_requirements: Vec::new(),
            connection_cost_policy: ConnectionCostPolicy::PoolBounded {
                max_physical_connections: 1,
            },
            namespace_shape: self.shape.clone(),
        })
    }

    async fn acquire_resource(
        &self,
        request: &AcquireResourceRequest,
        budget: &Arc<dyn BudgetPort>,
    ) -> Result<ResourceHandle, ResourceError> {
        let canonical = self.shape.canonicalize(&request.target)?;
        self.shape
            .validate_requirements(&request.target, &Default::default())?;
        let permit = budget
            .acquire_physical_connections(request.scope.max_physical_connections.max(1))
            .await?;
        let handle = ResourceHandle::issue(
            self.provider_id().to_string(),
            canonical.path.join("/"),
            self.epoch,
        );
        self.open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(handle.resource_key().to_string(), permit);
        Ok(handle)
    }

    async fn execute_on_resource(
        &self,
        handle: &ResourceHandle,
        execution_id: &QueryExecutionId,
        call: &CommandCall,
        sink: &dyn ResultSink,
    ) -> Result<ExecutionCompletion, ResourceError> {
        self.require("execute_on_resource")?;
        handle.check(self.provider_id(), self.epoch)?;
        self.seen
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(execution_id.as_str().to_string());

        sink.write(ResultChunk {
            statement_index: 0,
            sql: call.command.clone(),
            rows: vec![vec![Some(datazen_driver_api::Value::String(
                execution_id.as_str().to_string(),
            ))]],
            rows_affected: Some(1),
        })
        .await?;
        sink.complete().await?;

        // This profile declares full context observation, so it is allowed to
        // say *confirmed*. A provider that cannot read the session leaves
        // `confidence` alone and `matches_target` refuses to agree.
        let context = SessionContext {
            namespace: NamespaceTarget::empty().with_database("app"),
            confidence: ObservationConfidence::Confirmed,
            ..SessionContext::unobserved()
        };
        Ok(ExecutionCompletion {
            completion_status: CompletionStatus::Succeeded,
            effect_outcome: EffectOutcome::Completed,
            statement_results: Vec::new(),
            context_before: SessionContext::unobserved(),
            context_after: context.clone(),
            transaction_observation: TransactionObservation {
                state: TransactionState::Idle,
                transaction_id: None,
                effect: None,
                revision: 3,
            },
            session_handles: vec![SessionHandleRef {
                handle_id: "seq-1".to_string(),
                kind: SessionHandleKind::ServerPrepared,
                resource_id: handle.resource_key().to_string(),
                runtime_epoch: self.epoch,
                closed: false,
            }],
            protocol_drained: true,
            resource_health: ResourceHealth::Healthy,
        })
    }

    async fn observe_session(
        &self,
        handle: &ResourceHandle,
    ) -> Result<SessionObservation, ResourceError> {
        self.require("observe_session")?;
        handle.check(self.provider_id(), self.epoch)?;
        Ok(SessionObservation {
            state: SessionState::Ready,
            context: SessionContext {
                namespace: NamespaceTarget::empty().with_database("app"),
                confidence: ObservationConfidence::Confirmed,
                ..SessionContext::unobserved()
            },
            transaction: TransactionObservation {
                state: TransactionState::Idle,
                transaction_id: None,
                effect: None,
                revision: 7,
            },
            handles: Vec::new(),
            protocol_drained: true,
            resource_health: ResourceHealth::Healthy,
            context_revision: 11,
        })
    }

    async fn change_context(
        &self,
        handle: &ResourceHandle,
        desired: &NamespaceTarget,
    ) -> Result<ContextChangeDisposition, ResourceError> {
        self.require("change_context")?;
        handle.check(self.provider_id(), self.epoch)?;
        if desired.database.as_deref() == Some("app") {
            Ok(ContextChangeDisposition::Confirmed)
        } else {
            Ok(ContextChangeDisposition::RequiresReplacement)
        }
    }

    async fn begin_transaction(
        &self,
        handle: &ResourceHandle,
        options: &TransactionOptions,
    ) -> Result<TransactionObservation, ResourceError> {
        self.require("begin_transaction")?;
        handle.check(self.provider_id(), self.epoch)?;
        Ok(TransactionObservation::begun(
            format!(
                "txn-{}",
                options.isolation_level.as_deref().unwrap_or("default")
            ),
            1,
        ))
    }

    async fn commit_transaction(
        &self,
        handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        self.require("commit_transaction")?;
        handle.check(self.provider_id(), self.epoch)?;
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
        self.require("rollback_transaction")?;
        handle.check(self.provider_id(), self.epoch)?;
        Ok(TransactionObservation::finished(
            TransactionState::Aborted,
            EffectOutcome::RolledBack,
            2,
        ))
    }

    async fn request_cancel(
        &self,
        handle: &ResourceHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<CancelReceipt, ResourceError> {
        handle.check(self.provider_id(), self.epoch)?;
        let mut cancelled = self
            .cancelled
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !self.profile.precise_cancel {
            // The whole point: no session-wide fallback, and no `Ok(())`.
            return Ok(CancelReceipt {
                execution_id: execution_id.clone(),
                disposition: CancelDisposition::Unsupported,
                state: None,
            });
        }
        let known = self
            .seen
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(execution_id.as_str());
        if !known {
            // Never ran on this resource: refusing to answer is the whole point,
            // because a session-wide cancel would have hit somebody else's work.
            return Ok(CancelReceipt {
                execution_id: execution_id.clone(),
                disposition: CancelDisposition::NotRegistered,
                state: None,
            });
        }
        if !cancelled.insert(execution_id.as_str().to_string()) {
            return Ok(CancelReceipt {
                execution_id: execution_id.clone(),
                disposition: CancelDisposition::AlreadyFinished,
                state: Some(ExecutionState::Succeeded),
            });
        }
        Ok(CancelReceipt {
            execution_id: execution_id.clone(),
            disposition: CancelDisposition::Requested,
            state: Some(ExecutionState::CancelRequested),
        })
    }

    async fn reset_resource(
        &self,
        handle: &ResourceHandle,
        _: &Baseline,
    ) -> Result<ResetDisposition, ResourceError> {
        self.require("reset_resource")?;
        handle.check(self.provider_id(), self.epoch)?;
        Ok(if self.profile.verified_reset {
            ResetDisposition::Clean
        } else {
            ResetDisposition::Discard
        })
    }

    async fn close_resource(
        &self,
        handle: &ResourceHandle,
    ) -> Result<CloseDisposition, ResourceError> {
        handle.check(self.provider_id(), self.epoch)?;
        let removed = self
            .open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(handle.resource_key());
        match removed {
            Some(_) => {
                self.disconnects.fetch_add(1, Ordering::SeqCst);
                Ok(CloseDisposition::Closed)
            }
            None => Ok(CloseDisposition::Closed),
        }
    }
}

// ---------------------------------------------------------------------------
// A factory that advertises the provider through the public extension point.
// ---------------------------------------------------------------------------

pub struct AdvertisingFactory {
    pub provider: Arc<FakeProvider>,
}

impl DatabaseDriverFactory for AdvertisingFactory {
    fn create(&self) -> Arc<dyn datazen_driver_api::DatabaseDriver> {
        unimplemented!("this factory only exists to hand out a provider")
    }

    fn driver_id(&self) -> &'static str {
        "fake"
    }

    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(Arc::clone(&self.provider) as Arc<dyn ResourceProvider>)
    }
}

/// A factory that says nothing at all about resources.
pub struct SilentFactory;

impl DatabaseDriverFactory for SilentFactory {
    fn create(&self) -> Arc<dyn datazen_driver_api::DatabaseDriver> {
        unimplemented!("this factory only exists to prove the accessor rejects it")
    }

    fn driver_id(&self) -> &'static str {
        "silent"
    }
}

/// Widen the concrete budget to the trait object the contract demands, so the
/// test calls the provider exactly the way an out-of-tree caller would.
pub fn port(budget: &Arc<AllowanceBudget>) -> Arc<dyn BudgetPort> {
    budget.clone()
}

pub fn request(purpose: datazen_driver_api::resource::ResourcePurpose) -> DescribeResourceRequest {
    DescribeResourceRequest {
        connection_config: config(),
        identity_scope: Default::default(),
        target: NamespaceTarget::empty().with_database("app"),
        purpose,
    }
}

pub fn acquire_request() -> AcquireResourceRequest {
    AcquireResourceRequest {
        connection_config: config(),
        target: NamespaceTarget::empty().with_database("app"),
        scope: ResourceScope {
            purpose: datazen_driver_api::resource::ResourcePurpose::InteractiveQuery,
            max_physical_connections: 1,
            holds_open_transaction: false,
            pin_for_streaming: false,
        },
        identity_scope: Default::default(),
        baseline: Baseline::default(),
    }
}

//! The vector resource provider: state, validation, and where the answers live.
//!
//! `VectorDriver` had no legacy resource adapter to wrap, so this provider was
//! written by hand against the contract. This file holds the parts every
//! contract method shares — the epoch, the live-resource registry, acquisition
//! validation, budget bookkeeping — and [`contract`] holds the fourteen
//! implementations themselves.
//!
//! ## What each contract method actually runs
//!
//! | Contract method | Driver code behind it |
//! |---|---|
//! | `provider_id` | the constant in [`capabilities`] |
//! | `capabilities` | [`vector_capability_registry`] plus the evidence table |
//! | `namespace_shape` | [`vector_namespace_shape`] — empty levels, so any supplied target level is *rejected* by `canonicalize` rather than ignored |
//! | `describe_resource` | `vector_connection_cost` and the same canonicalization check `acquire_resource` runs; no connection is opened |
//! | `acquire_resource` | [`validate_acquisition`], then `BudgetPort::acquire_physical_connections(1)`, then `VectorDriver::connect` (`src/vector.rs:167`) — the permit is released here on failure and nowhere else |
//! | `execute_on_resource` | `VectorDriver::execute_command` → `execute_standard_sql_command` → `VectorDriver::query_multi_at` (`src/vector.rs`), decoded by [`decode_command_result`](super::payload::decode_command_result) and pushed into the sink |
//! | `observe_session` | [`VectorDriver::probe_liveness`](crate::vector::VectorDriver::probe_liveness), mapped by [`observed_session`](super::observation::observed_session) |
//! | `change_context` | canonicalization, then `Unsupported` — an instance has exactly one namespace, fixed by its base URL |
//! | `begin_transaction` | `Err` — no transaction endpoint, and the driver refuses writes |
//! | `commit_transaction` | `Err` — same fact, reported for the commit |
//! | `rollback_transaction` | `Err` — same fact, reported for the rollback |
//! | `request_cancel` | `CancelDisposition::Unsupported`; `VectorDriver::cancel_query` is a no-op `Ok(())` and the contract forbids it as a fallback |
//! | `reset_resource` | `ResetDisposition::Discard` — nothing replayed, nothing proven clean |
//! | `close_resource` | `VectorDriver::disconnect` (`src/vector.rs:185`), then the budget released exactly once |
//!
//! ## The three rules this file keeps
//!
//! 1. **The handle is checked on every call**, and a key this provider does not
//!    hold is an error, not a shrug — see [`Self::resource_of`].
//! 2. **An unsupported request is refused before the budget is charged and
//!    before the wire is touched.** A caller that asked for something this
//!    driver cannot do pays nothing and has changed nothing.
//! 3. **The budget is released exactly once per acquire** — at confirmed close,
//!    or immediately after a failed connect. Never both, never neither.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use datazen_driver_api::capabilities::CapabilityRegistry;
use datazen_driver_api::namespace::{NamespaceShape, NamespaceTarget};
use datazen_driver_api::resource::{
    AcquireResourceRequest, BudgetPort, ConnectionCostPolicy, DescribeResourceRequest,
    ResourceDescriptor, ResourceError, ResourceHandle, ReusePolicy,
};
use datazen_driver_api::session::CloseDisposition;
use datazen_driver_api::DatabaseDriver;

use super::capabilities::{
    vector_capability_registry, vector_connection_cost, vector_namespace_shape, VECTOR_PROVIDER_ID,
    VECTOR_SESSION_CONTINUITY,
};
use super::registry::{LiveResource, ResourceRegistry};
use crate::vector::VectorDriver;

/// Monotonic across every provider instance in this process, so a handle minted
/// by one provider can never be accepted by another even if both are called
/// "vector".
static PROVIDER_EPOCH: AtomicU64 = AtomicU64::new(1);

fn next_runtime_epoch() -> u64 {
    PROVIDER_EPOCH.fetch_add(1, Ordering::Relaxed)
}

/// The Qdrant [`ResourceProvider`](datazen_driver_api::resource::ResourceProvider).
pub struct VectorResourceProvider {
    driver: Arc<VectorDriver>,
    capabilities: CapabilityRegistry,
    namespace_shape: NamespaceShape,
    /// Minted per instance and embedded in every handle this instance issues.
    runtime_epoch: u64,
    resources: Mutex<ResourceRegistry>,
}

impl VectorResourceProvider {
    pub fn new(driver: Arc<VectorDriver>) -> Self {
        Self {
            driver,
            capabilities: vector_capability_registry(),
            namespace_shape: vector_namespace_shape(),
            runtime_epoch: next_runtime_epoch(),
            resources: Mutex::new(ResourceRegistry::default()),
        }
    }

    pub fn driver(&self) -> &Arc<VectorDriver> {
        &self.driver
    }

    pub fn runtime_epoch(&self) -> u64 {
        self.runtime_epoch
    }

    /// The registry lock.
    ///
    /// A *synchronous* mutex on purpose: every critical section below is plain
    /// bookkeeping with no `.await` inside it, so there is never a guard held
    /// across a suspension point, and a resource call can never be interleaved
    /// into the middle of one.
    ///
    /// Poison-tolerant for the same reason: a panic in another task must not
    /// wedge every later resource call, and a half-applied bookkeeping change
    /// shows up as a key that is present or absent — which the next call checks
    /// anyway.
    fn lock(&self) -> MutexGuard<'_, ResourceRegistry> {
        self.resources
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Resolve a handle to the state this provider holds for it.
    ///
    /// Three distinct failures, all as `Err` — none of them is "carry on with
    /// what I happen to have":
    ///
    /// * the handle is not this provider's, or carries another provider's epoch
    ///   → `check` rejects it;
    /// * it is ours but the key is not live → `InvalidResourceState`, saying
    ///   plainly that the resource was never acquired here or is already closed.
    fn resource_of(
        &self,
        handle: &ResourceHandle,
        operation: &str,
    ) -> Result<LiveResource, ResourceError> {
        handle.check(VECTOR_PROVIDER_ID, self.runtime_epoch)?;
        let registry = self.lock();
        let live = registry.live.get(handle.resource_key()).ok_or_else(|| {
            ResourceError::InvalidResourceState {
                resource_key: handle.resource_key().to_string(),
                operation: operation.to_string(),
                state: format!(
                    "this provider instance holds no open resource with this key: it was never \
                     acquired here, or it was already closed"
                ),
            }
        })?;
        Ok(live.snapshot())
    }

    /// The next revision for a handle, so two look-alike observations differ.
    fn bump_revision(&self, handle: &ResourceHandle) -> Result<u64, ResourceError> {
        let mut registry = self.lock();
        let revision = match registry.live.get_mut(handle.resource_key()) {
            Some(live) => {
                live.revision += 1;
                live.revision
            }
            None => {
                return Err(ResourceError::InvalidResourceState {
                    resource_key: handle.resource_key().to_string(),
                    operation: "observe_session".to_string(),
                    state: "the resource was not open when its revision was advanced".to_string(),
                })
            }
        };
        Ok(revision)
    }

    /// The driver's `ConnectionHandle` for one live resource, so a test can put
    /// the driver's pool into the state a crashed client would.
    ///
    /// The registry entry is deliberately *not* removed: the point of the test
    /// is that a provider which still believes it holds a resource, while the
    /// driver no longer has the connection, must report `Gone` rather than
    /// fabricate a live reading.
    #[cfg(test)]
    pub(super) fn connection_of_for_test(
        &self,
        handle: &ResourceHandle,
    ) -> Option<datazen_driver_api::ConnectionHandle> {
        self.lock()
            .live
            .get(handle.resource_key())
            .map(|live| live.connection.clone())
    }

    /// Everything an acquire must satisfy — checked **before** the budget is
    /// charged and before any connection is opened.
    ///
    /// Each refusal names the driver's own reason, so a caller can tell a wrong
    /// request from a broken driver:
    ///
    /// * a target naming any namespace level → `canonicalize` rejects it,
    ///   because Qdrant has no database, catalog or schema level;
    /// * a baseline with requirements → this driver issues none and could not
    ///   replay them (it speaks no SQL), so a "replay this before reuse"
    ///   promise cannot be kept;
    /// * a cost policy other than the one this provider declares → refusing is
    ///   better than silently substituting a cheaper estimate;
    /// * a scope that allows fewer connections than the resource needs →
    ///   `BudgetDenied`;
    /// * `holds_open_transaction` / `pin_for_streaming` → there is no
    ///   transaction to hold and no handle to pin.
    fn validate_acquisition(request: &AcquireResourceRequest) -> Result<u32, ResourceError> {
        vector_namespace_shape().canonicalize(&request.target)?;

        if !request.baseline.initialization_requirements.is_empty() {
            return Err(ResourceError::OperationNotSupported {
                driver: VECTOR_PROVIDER_ID.to_string(),
                operation: "acquire_resource".to_string(),
                reason: format!(
                    "this driver declares {} initialization requirement(s) and can execute none of \
                     them (it speaks the Qdrant REST API, not SQL), so a baseline that must be \
                     replayed before reuse cannot be honoured",
                    request.baseline.initialization_requirements.len()
                ),
            });
        }

        let required = match vector_connection_cost(&request.connection_config) {
            ConnectionCostPolicy::PoolBounded {
                max_physical_connections,
            } => max_physical_connections,
            other => {
                return Err(ResourceError::OperationNotSupported {
                    driver: VECTOR_PROVIDER_ID.to_string(),
                    operation: "acquire_resource".to_string(),
                    reason: format!(
                    "this provider declares PoolBounded only; a cost policy of {other:?} cannot \
                         be honoured without guessing what one acquire actually creates"
                ),
                })
            }
        };

        if request.scope.max_physical_connections < required {
            return Err(ResourceError::BudgetDenied {
                requested: required,
                reason: format!(
                    "the scope allows {} physical connection(s) but this resource needs {required}",
                    request.scope.max_physical_connections
                ),
            });
        }

        if request.scope.holds_open_transaction {
            return Err(ResourceError::unsupported(
                VECTOR_PROVIDER_ID,
                "acquire_resource",
                "the scope requires an open transaction, and this resource has no transaction to \
                 hold: the Qdrant REST API exposes none and this driver refuses writes outright",
            ));
        }

        if request.scope.pin_for_streaming {
            return Err(ResourceError::unsupported(
                VECTOR_PROVIDER_ID,
                "acquire_resource",
                "the scope requires pinning a server-side handle for streaming, and this driver \
                 issues none: query results are materialised in full before the first chunk is \
                 produced",
            ));
        }

        Ok(required)
    }

    /// Charge the budget, then open the resource, releasing the permit again if
    /// the open fails.
    ///
    /// The two failure paths are deliberately distinct: a *budget* refusal
    /// returns the refusal untouched, while a *connection* failure releases
    /// what it just took. Either way the caller ends up charged zero.
    pub(super) async fn acquire(
        &self,
        request: &AcquireResourceRequest,
        budget: &Arc<dyn BudgetPort>,
    ) -> Result<(ResourceHandle, LiveResource), ResourceError> {
        let required = Self::validate_acquisition(request)?;

        let permit = budget.acquire_physical_connections(required).await?;
        let connection = match self.driver.connect(&request.connection_config).await {
            Ok(connection) => connection,
            Err(error) => {
                // Nothing was opened, so nothing may stay charged.
                budget.release_physical_connections(&permit).await?;
                return Err(error.into());
            }
        };

        let live = LiveResource {
            connection,
            permit: permit.clone(),
            budget: budget.clone(),
            revision: 0,
        };
        let handle = self
            .register_resource(&request.connection_config.id, live.clone())
            .await;
        Ok((handle, live))
    }

    /// Add a freshly opened resource to the registry and mint its handle.
    async fn register_resource(&self, config_id: &str, live: LiveResource) -> ResourceHandle {
        let resource_key = Self::resource_key_for(config_id);
        let handle = ResourceHandle::issue(VECTOR_PROVIDER_ID, &resource_key, self.runtime_epoch);
        self.lock().live.insert(resource_key, live);
        handle
    }

    /// The key one resource is stored under.
    ///
    /// Derived from the connection config id, never from credentials: two
    /// resources for the same config share a key only if the same provider
    /// opened them, and the epoch — not the key — is what keeps two providers
    /// apart.
    fn resource_key_for(config_id: &str) -> String {
        format!("vector_resource:{config_id}")
    }

    /// Close a resource and release its permit exactly once.
    ///
    /// `Ok` here means the driver's client is gone; only then is the charge
    /// released. A refused close returns `CloseUnconfirmed` **without**
    /// releasing, because releasing would under-report a resource that is still
    /// holding the budget.
    pub(super) async fn close(
        &self,
        handle: &ResourceHandle,
    ) -> Result<CloseDisposition, ResourceError> {
        handle.check(VECTOR_PROVIDER_ID, self.runtime_epoch)?;
        let resource_key = handle.resource_key().to_string();

        let live = {
            let mut registry = self.lock();
            match registry.live.remove(&resource_key) {
                Some(live) => live,
                None => {
                    // A confirmed close is idempotent; anything else is not.
                    if registry.closed.contains(&resource_key) {
                        return Ok(CloseDisposition::Closed);
                    }
                    return Err(ResourceError::InvalidResourceState {
                        resource_key,
                        operation: "close_resource".to_string(),
                        state:
                            "this provider instance holds no open resource with this key: it was \
                                never acquired here, or it was already closed"
                                .to_string(),
                    });
                }
            }
        };

        if let Err(error) = self.driver.disconnect(live.connection.clone()).await {
            // Put it back: the client is still in the driver's pool, so the
            // resource is still open and still charged.
            self.lock().live.insert(resource_key.clone(), live);
            tracing::warn!(
                resource_key = %resource_key,
                error = %error,
                "vector: close was not confirmed; the resource stays open and stays charged"
            );
            return Ok(CloseDisposition::CloseUnconfirmed);
        }

        live.budget
            .release_physical_connections(&live.permit)
            .await?;
        self.lock().closed.insert(resource_key);
        Ok(CloseDisposition::Closed)
    }

    /// The descriptor handed back by `describe_resource`.
    ///
    /// Built without opening a connection: describing a resource must not cost
    /// one. The continuity is `Leased` on purpose — one HTTP client is not a
    /// fixed session, and `SessionContinuity::is_fixed()` exists precisely so a
    /// pool of size one cannot masquerade as one.
    pub(super) fn descriptor(
        &self,
        request: &DescribeResourceRequest,
    ) -> Result<ResourceDescriptor, ResourceError> {
        vector_namespace_shape().canonicalize(&request.target)?;
        Ok(ResourceDescriptor {
            provider_id: VECTOR_PROVIDER_ID.to_string(),
            resource_key: Self::resource_key_for(&request.connection_config.id),
            session_continuity: VECTOR_SESSION_CONTINUITY,
            reuse_policy: ReusePolicy::SingleUse,
            initialization_requirements: Vec::new(),
            connection_cost_policy: vector_connection_cost(&request.connection_config),
            namespace_shape: vector_namespace_shape(),
        })
    }

    /// The namespace target a caller asked for, checked against this shape.
    pub(super) fn canonical_target(&self, target: &NamespaceTarget) -> Result<(), ResourceError> {
        self.namespace_shape.canonicalize(target).map(|_| ())
    }
}

mod contract;

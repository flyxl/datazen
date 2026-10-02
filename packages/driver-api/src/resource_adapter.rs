//! Legacy [`DatabaseDriver`] → [`ResourceProvider`] bridge (P2, delivery 2).
//!
//! Source of truth: plan line 95 — 「现有 Command 和迁移接口保持领域语义；
//! 资源获得方式通过 adapter 演进」, plus
//! `driver-capability-migration.md` §6.1 / §6.2 / §7.1.
//!
//! # What this adapter is for
//!
//! It lets a driver that has **not** been migrated yet be reached through the
//! resource contract, so the host can move to acquire-by-resource without
//! forcing all 15 drivers to migrate in one commit. The *Command* semantics
//! are untouched: `execute_on_resource` forwards the caller's `(command,
//! input)` pair to the driver's own `execute_command`. What changes is only
//! **how the connection was obtained**.
//!
//! # What it deliberately refuses to do
//!
//! Each of these is a no-op success the migration document forbids, so the
//! adapter turns them into explicit refusals instead:
//!
//! | Contract call | Legacy reality | Adapter answer |
//! |---|---|---|
//! | `observe_session` | no observation API | `SessionObservation::unobservable()` — every field unknown, **never** back-filled with the acquisition target |
//! | `change_context` | switching means reconnecting | `ContextChangeDisposition::Unsupported` — never a silent reconnect |
//! | `reset_resource` | no verified baseline path | `ResetDisposition::Discard` — never `Clean` |
//! | `commit` / `rollback_transaction` | resolved inside driver commands | `ResourceError::OperationNotSupported` |
//! | `request_cancel` | legacy `cancel_query` is **session-wide** and returns `Ok(())` in 13/15 drivers | `CancelDisposition::Unsupported` unless the driver itself declares precise execution cancel |
//!
//! The `cancel_query` point is the important one: `Ok(())` from 13 drivers is
//! precisely the 「新增能力缺失不会 no-op 成功」 failure the plan calls out, so
//! this adapter never calls it. The only accepted path is
//! `DatabaseDriver::cancel_query_with_execution`, gated on the driver's own
//! `supports_query_execution_cancel()` declaration.
//!
//! # Session continuity
//!
//! `session_continuity` is never declared `Fixed`. A pool of size one is not a
//! fixed session (`driver-capability-migration.md` §6.1) and `DatabaseDriver`
//! exposes no way to prove otherwise, so the adapter reports
//! `SessionContinuity::Unknown`, which `is_fixed()` treats exactly like
//! `Leased` (`CM-19`).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::async_trait;
use crate::capabilities::{
    CapabilityRegistry, CapabilitySet, CapabilitySnapshot, PreciseCancelSupport, SessionContinuity,
};
use crate::namespace::{NamespaceShape, NamespaceTarget, TargetRequirements};
use crate::resource::{
    AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, CommandCall, ConnectionCostPolicy,
    DescribeResourceRequest, ResourceDescriptor, ResourceError, ResourceHandle, ResourceProvider,
    ResultSink, ReusePolicy,
};
use crate::session::{
    CancelDisposition, CancelReceipt, CloseDisposition, CompletionStatus, ContextChangeDisposition,
    EffectOutcome, ExecutionCompletion, ExecutionState, ResetDisposition, ResourceHealth,
    SessionContext, SessionHandleRef, SessionObservation, TransactionObservation,
    TransactionOptions, TransactionState,
};
use crate::{ConnectionHandle, DatabaseDriver, DriverError, QueryExecutionId};

/// Everything the adapter tracks for one acquired resource.
struct LiveResource {
    /// The driver's own handle, kept verbatim so the adapter never has to
    /// reconstruct a `pool_id` it was not given.
    connection: ConnectionHandle,
    permit: BudgetPermit,
    /// Retained from `acquire_resource` so `close_resource` can release the
    /// charge exactly once.
    budget: Arc<dyn BudgetPort>,
}

/// A [`ResourceProvider`] view over an existing, unmigrated [`DatabaseDriver`].
pub struct LegacyResourceAdapter {
    driver: Arc<dyn DatabaseDriver>,
    driver_id: String,
    runtime_epoch: u64,
    namespace_shape: NamespaceShape,
    capabilities: CapabilityRegistry,
    live: Mutex<BTreeMap<String, LiveResource>>,
}

impl std::fmt::Debug for LegacyResourceAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LegacyResourceAdapter")
            .field("driver_id", &self.driver_id)
            .field("runtime_epoch", &self.runtime_epoch)
            .field("open_resources", &self.open_resource_count())
            .finish()
    }
}

/// Small helper so the production path never needs a bare `unwrap()`.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Capability revision the adapter stamps on a snapshot that carries evidence.
///
/// [`LegacyResourceAdapter::new`] starts at `0`, the sentinel meaning "this
/// provider had no capability module yet". A provider that opts into
/// [`LegacyResourceAdapter::with_evidence`] moves to `1`: recording evidence
/// is itself a change in what the snapshot means, and a cached revision-`0`
/// snapshot must not be mistaken for one carrying evidence.
pub const ADAPTER_EVIDENCE_REVISION: u64 = 1;

impl LegacyResourceAdapter {
    /// Wrap a driver. `runtime_epoch` is the host's current epoch; every handle
    /// this adapter issues is bound to it, and a handle minted under an older
    /// epoch is rejected on use.
    ///
    /// `capabilities` is the driver's own declaration, and it is the caller's
    /// job to make it honest: every cell it leaves alone keeps its
    /// non-supporting default, so `CapabilityRegistry::require_*` rejects that
    /// capability by name. A driver that has not studied the contract passes
    /// [`CapabilitySet::default`] and declares nothing.
    ///
    /// [`Self::new`] then overwrites exactly one of those cells —
    /// `precise_cancel` — because that is the one the adapter can settle on
    /// the driver's behalf: it is `Supported` only when the driver itself says
    /// it implements the exact execution-handle protocol, and `Unknown`
    /// otherwise. A caller cannot lift that cell by declaring it, which is the
    /// point: a declaration is a claim, and this is the one claim in the set
    /// the adapter refuses to take on faith.
    pub fn new(
        driver: Arc<dyn DatabaseDriver>,
        driver_id: impl Into<String>,
        driver_version: impl Into<String>,
        runtime_epoch: u64,
        namespace_shape: NamespaceShape,
        capabilities: CapabilitySet,
    ) -> Self {
        let driver_id = driver_id.into();
        let mut registry = CapabilityRegistry::new(
            driver_id.clone(),
            CapabilitySnapshot::new(
                driver_id.clone(),
                driver_version,
                crate::PROTOCOL_VERSION,
                0,
            ),
        );
        registry.capabilities = capabilities;
        registry.capabilities.precise_cancel = if driver.supports_query_execution_cancel() {
            PreciseCancelSupport::Supported
        } else {
            PreciseCancelSupport::Unknown
        };
        Self {
            driver,
            driver_id,
            runtime_epoch,
            namespace_shape,
            capabilities: registry,
            live: Mutex::new(BTreeMap::new()),
        }
    }

    /// Record the evidence behind this adapter's declaration.
    ///
    /// [`Self::new`] builds its snapshot at revision `0` — "no capability
    /// module yet" — so this builder moves the snapshot to
    /// [`ADAPTER_EVIDENCE_REVISION`] and merges the evidence in the same step.
    /// Fixing the revision up here rather than leaving it to the caller is
    /// what lets the method be infallible: the check in
    /// [`CapabilitySnapshot::with_evidence`] that rejects evidence at revision
    /// `0` cannot be reached from here, because the revision was just
    /// corrected.
    ///
    /// `new` keeps its six-argument shape, so this is purely additive: a
    /// driver that calls nothing keeps revision `0` and an empty evidence
    /// table, and [`Self::evidence_gaps`] reports all twelve cells missing.
    pub fn with_evidence<K, V, I>(self, evidence: I) -> Self
    where
        K: Into<String>,
        V: Into<String>,
        I: IntoIterator<Item = (K, V)>,
    {
        let LegacyResourceAdapter {
            driver,
            driver_id,
            runtime_epoch,
            namespace_shape,
            mut capabilities,
            live,
        } = self;
        if capabilities.snapshot.capability_revision == 0 {
            capabilities.snapshot.capability_revision = ADAPTER_EVIDENCE_REVISION;
        }
        capabilities.snapshot = capabilities.snapshot.merge_evidence(evidence);
        Self {
            driver,
            driver_id,
            runtime_epoch,
            namespace_shape,
            capabilities,
            live,
        }
    }

    /// The declared cells this adapter carries no evidence for.
    ///
    /// Delegated to [`CapabilityRegistry::evidence_gaps`], which does not need
    /// the adapter's cooperation to answer.
    pub fn evidence_gaps(&self) -> Vec<&'static str> {
        self.capabilities.evidence_gaps()
    }

    /// The wrapped driver, for callers that still need the legacy API.
    pub fn driver(&self) -> &Arc<dyn DatabaseDriver> {
        &self.driver
    }

    /// How many resources this adapter currently holds open.
    pub fn open_resource_count(&self) -> usize {
        lock(&self.live).len()
    }

    fn unsupported(&self, operation: &str, reason: &str) -> ResourceError {
        ResourceError::unsupported(self.driver_id.clone(), operation, reason)
    }

    /// Ownership + epoch check, then the driver's own connection handle.
    fn connection_of(
        &self,
        handle: &ResourceHandle,
        operation: &str,
    ) -> Result<ConnectionHandle, ResourceError> {
        handle.check(&self.driver_id, self.runtime_epoch)?;
        let resource_key = handle.resource_key().to_string();
        lock(&self.live)
            .get(&resource_key)
            .map(|live| live.connection.clone())
            .ok_or_else(|| ResourceError::InvalidResourceState {
                resource_key,
                state: "not open".to_string(),
                operation: operation.to_string(),
            })
    }

    /// Validate the target against the namespace shape before any wire work
    /// (`connection-management.md` §4.3): canonicalize — which also rejects a
    /// value for a level this database does not have and merges alias chains —
    /// then apply the per-operation requirements.
    fn validate_target(&self, target: &NamespaceTarget) -> Result<(), ResourceError> {
        let canonical = self.namespace_shape.canonicalize(target)?;
        self.namespace_shape
            .validate_requirements(target, &TargetRequirements::default())?;
        tracing::debug!(
            driver = %self.driver_id,
            requested = ?target,
            canonical = ?canonical.path,
            "legacy adapter canonicalized a namespace target"
        );
        Ok(())
    }

    /// The descriptor an unmigrated driver can honestly produce: no fixed
    /// session, no confirmed reuse, an empty initialization baseline, and a
    /// conservative per-resource connection charge.
    fn descriptor(&self, request: &DescribeResourceRequest) -> ResourceDescriptor {
        ResourceDescriptor {
            provider_id: self.driver_id.clone(),
            resource_key: format!("{}#{:?}", self.driver_id, request.purpose),
            // Never `Fixed`: `DatabaseDriver` cannot prove a session-scoped
            // physical connection (`driver-capability-migration.md` §6.1).
            session_continuity: SessionContinuity::Unknown,
            // No verified reset-to-baseline, so a resource must be re-acquired
            // rather than reused (`CM-19`).
            reuse_policy: ReusePolicy::Unknown,
            initialization_requirements: Vec::new(),
            // The real internal pool size of an unmigrated driver is not
            // observable from here. One connection per acquired resource is the
            // declared, conservative figure; `acquire_resource` charges the
            // `ResourceScope` number against the real budget.
            connection_cost_policy: ConnectionCostPolicy::DeclaredConservative {
                declared_cost: 1,
                hard_cap: None,
            },
            namespace_shape: self.namespace_shape.clone(),
        }
    }
}

#[async_trait]
impl ResourceProvider for LegacyResourceAdapter {
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
        Ok(self.descriptor(request))
    }

    async fn acquire_resource(
        &self,
        request: &AcquireResourceRequest,
        budget: &Arc<dyn BudgetPort>,
    ) -> Result<ResourceHandle, ResourceError> {
        self.validate_target(&request.target)?;
        // Budget first: never open a physical connection the caller cannot pay
        // for (`connection-management.md` §9.3).
        let requested = request.scope.max_physical_connections.max(1);
        let permit = budget.acquire_physical_connections(requested).await?;

        let connection = match self.driver.connect(&request.connection_config).await {
            Ok(connection) => connection,
            Err(err) => {
                // Nothing opened, so the charge goes straight back. A failure
                // to release it is real and must not be swallowed.
                if let Err(release) = budget.release_physical_connections(&permit).await {
                    tracing::warn!(
                        driver = %self.driver_id,
                        error = %release,
                        "budget release failed after a failed connect"
                    );
                }
                return Err(ResourceError::Driver(err));
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

    /// Delegates to the driver's own `execute_command` with the caller's exact
    /// `(command, input)` pair, so domain semantics stay in the driver.
    ///
    /// The completion is truthful but coarse: a legacy `CommandResult` is one
    /// opaque JSON payload with no per-statement observation, no context
    /// read-back and no protocol-drain signal, so those stay unknown/empty
    /// rather than being reconstructed. Callers that need the payload itself
    /// keep using `DatabaseDriver::execute_command`, which is unchanged.
    async fn execute_on_resource(
        &self,
        handle: &ResourceHandle,
        _execution_id: &QueryExecutionId,
        call: &CommandCall,
        _sink: &dyn ResultSink,
    ) -> Result<ExecutionCompletion, ResourceError> {
        let connection = self.connection_of(handle, "execute_on_resource")?;
        self.driver
            .execute_command(&connection, &call.command, call.input.clone())
            .await
            .map(|_| ExecutionCompletion {
                completion_status: CompletionStatus::Succeeded,
                effect_outcome: EffectOutcome::Completed,
                statement_results: Vec::new(),
                context_before: SessionContext::unobserved(),
                context_after: SessionContext::unobserved(),
                transaction_observation: TransactionObservation {
                    state: TransactionState::Unknown,
                    transaction_id: None,
                    effect: None,
                    revision: 0,
                },
                // This path registers no session-scoped handle, so the
                // truthful list is empty (`connection-management.md` §6.5).
                session_handles: Vec::<SessionHandleRef>::new(),
                protocol_drained: true,
                resource_health: ResourceHealth::Unknown,
            })
            .map_err(ResourceError::Driver)
    }

    /// The legacy driver has no read-back API. Returning the acquisition target
    /// here would be a fabrication, so every field stays unknown
    /// (`connection-management.md` §5.1, §7.2).
    async fn observe_session(
        &self,
        handle: &ResourceHandle,
    ) -> Result<SessionObservation, ResourceError> {
        self.connection_of(handle, "observe_session")?;
        Ok(SessionObservation::unobservable())
    }

    /// A legacy driver changes namespace by reconnecting, which would silently
    /// invalidate the caller's execution context. Refuse instead.
    async fn change_context(
        &self,
        handle: &ResourceHandle,
        _desired: &NamespaceTarget,
    ) -> Result<ContextChangeDisposition, ResourceError> {
        self.connection_of(handle, "change_context")?;
        Ok(ContextChangeDisposition::Unsupported)
    }

    async fn begin_transaction(
        &self,
        handle: &ResourceHandle,
        _options: &TransactionOptions,
    ) -> Result<TransactionObservation, ResourceError> {
        let connection = self.connection_of(handle, "begin_transaction")?;
        let transaction = self.driver.begin_transaction(&connection).await?;
        Ok(TransactionObservation::begun(transaction.id, 0))
    }

    /// A legacy driver resolves transactions inside its own commands, so the
    /// adapter cannot claim to know whether the effects landed. Refuse rather
    /// than report `Unknown` as `RolledBack` (`connection-management.md` §7.6).
    async fn commit_transaction(
        &self,
        handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        self.connection_of(handle, "commit_transaction")?;
        Err(self.unsupported(
            "commit_transaction",
            "legacy drivers resolve transactions inside their own commands; the outcome cannot be read back",
        ))
    }

    async fn rollback_transaction(
        &self,
        handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError> {
        self.connection_of(handle, "rollback_transaction")?;
        Err(self.unsupported(
            "rollback_transaction",
            "legacy drivers resolve transactions inside their own commands; the outcome cannot be read back",
        ))
    }

    /// Precise cancel only.
    ///
    /// Three of the four `CancelDisposition`s are reachable here:
    /// `Unsupported` when the driver cannot address a single execution,
    /// `NotRegistered` when the driver reports the execution is not in its
    /// registry, and `Requested` when the driver accepts. `AlreadyFinished` is
    /// left to a provider that tracks execution state — the adapter does not,
    /// and states `None` in the receipt rather than inventing one.
    async fn request_cancel(
        &self,
        handle: &ResourceHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<CancelReceipt, ResourceError> {
        let connection = self.connection_of(handle, "request_cancel")?;
        let receipt = |disposition: CancelDisposition| CancelReceipt {
            execution_id: execution_id.clone(),
            disposition,
            state: None,
        };

        if !self.driver.supports_query_execution_cancel() {
            return Ok(receipt(CancelDisposition::Unsupported));
        }
        match self
            .driver
            .cancel_query_with_execution(&connection, execution_id)
            .await
        {
            Ok(()) => Ok(CancelReceipt {
                execution_id: execution_id.clone(),
                disposition: CancelDisposition::Requested,
                state: Some(ExecutionState::CancelRequested),
            }),
            Err(
                DriverError::Unsupported(_)
                | DriverError::NotSupported(_)
                | DriverError::QueryExecutionSessionMismatch,
            ) => Ok(receipt(CancelDisposition::Unsupported)),
            Err(DriverError::QueryExecutionNotFound(_)) => {
                Ok(receipt(CancelDisposition::NotRegistered))
            }
            Err(err) => Err(ResourceError::Driver(err)),
        }
    }

    /// Always [`ResetDisposition::Discard`]. The legacy path has no verified
    /// baseline return, and `Clean` would hand out a resource whose state
    /// nobody has checked (`connection-management.md` §6.2).
    async fn reset_resource(
        &self,
        handle: &ResourceHandle,
        _baseline: &Baseline,
    ) -> Result<ResetDisposition, ResourceError> {
        self.connection_of(handle, "reset_resource")?;
        Ok(ResetDisposition::Discard)
    }

    /// Idempotent close. The resource is taken out of the map *before* the
    /// driver is asked to disconnect, so a repeated close cannot release the
    /// same budget twice. An unconfirmed close reports `CloseUnconfirmed` and
    /// does **not** claim the budget was recovered
    /// (`connection-management.md` §7.5).
    ///
    /// # Why this flow is not shared with the driver-owned providers
    ///
    /// Five sites implement "take, disconnect, release on success only":
    /// this one, `postgres/src/resource/provider/contract.rs`,
    /// `redis/src/resource/provider/contract.rs`, and
    /// `sqlite/src/resource.rs` / `mysql/src/resource.rs`. Measured, they are
    /// three contracts wearing one shape, and only `postgres` and `redis` are
    /// the same one:
    ///
    /// - `postgres` / `redis`: `disconnect` errors become
    ///   `Ok(CloseUnconfirmed)`, the budget stays charged, the handle is not
    ///   marked closed, and `handle.is_closed()` lets a later call tell "already
    ///   closed" (`Ok(Closed)`) from "never held"
    ///   (`Err(InvalidResourceState)`).
    ///
    ///   That "not marked closed" half is currently **unreachable and therefore
    ///   untestable**, and the next person to touch it needs to know before
    ///   they write a test for it:
    ///
    ///   1. `postgres`'s `disconnect_impl` (`src/connection.rs`) ends in
    ///      `Ok(())` — every `remove` / `retain` it performs is infallible and
    ///      there is no `?` in it — so it cannot return `Err`, so the
    ///      `CloseUnconfirmed` arm of `close_resource` is dead. `redis` is the
    ///      same shape. Nothing in either crate produces this disposition.
    ///   2. So "mark closed only on a *confirmed* close" has no test, in this
    ///      repo, that can fail. Moving `handle.mark_closed()` onto the
    ///      unconfirmed arm — the tempting one-line edit — fails **silently**:
    ///      the handle would then claim to be closed after a close that was not
    ///      confirmed, the retry would answer `Ok(Closed)`, the budget would
    ///      never be released, and the entire suite would stay green. Measured:
    ///      that mutation turns `cargo test -p datazen-driver-postgres` EXIT=0
    ///      with 194 passed and 0 failed.
    ///   3. So if a fault-injectable disconnect seam is ever introduced — which
    ///      is what it would take to reach that arm — the branch **must** come
    ///      with a test in the same change. A green suite means nothing about
    ///      this arm today, so treating green as the permission to move the
    ///      `mark_closed()` call is the specific mistake to avoid.
    /// - This adapter: identical budget discipline, but it keeps **no** record of
    ///   confirmed closes at all, so a missing key is always `Ok(Closed)` and the
    ///   never-held case is simply not expressible.
    /// - `sqlite` / `mysql`: `disconnect` errors become
    ///   `Err(ResourceError::Driver(_))`, not `CloseUnconfirmed`, and a
    ///   successful disconnect can still return `CloseUnconfirmed` when the
    ///   caller left a transaction open — a third outcome the two above never
    ///   produce.
    ///
    /// What would be left to share is roughly a dozen lines: one `check`, one
    /// `remove`, and one `release_physical_connections` behind a match. Getting
    /// them behind a generic in driver-api means passing the absent-key policy,
    /// the disconnect-error mapping, the mark-closed step, the take step and
    /// the disconnect itself as parameters — more surface than the code it
    /// replaces, with the budget-release invariant (the part actually worth
    /// sharing) still written out once per site. So the flow stays duplicated
    /// and this comment carries the reasoning instead of a helper that hides
    /// which contract each site chose.
    ///
    /// What *is* shared is the rule itself, and it is asserted in each
    /// driver's own tests.
    ///
    /// `redis` already extracted its part as a private `take_for_close`
    /// (`redis/src/resource/provider.rs`). That is not a counterexample,
    /// but both obvious readings of it are wrong. It deduplicates nothing:
    /// it has exactly one caller (`provider/contract.rs:339`), and it was
    /// pulled out to give two invariants a name — remove from `live` before
    /// touching the socket, and a `None` backed by `handle.is_closed()` means
    /// idempotent, never a second release. The same code inline would be
    /// equally correct and just less likely to be read as a rule.
    ///
    /// And it is not parameter-free: it takes `handle` and `operation` from
    /// the caller, `operation` being the literal `"close_resource"` at the one
    /// call site. What it has no parameters for is *policy* — the absent-key
    /// behaviour is hardcoded in its body, so there is only one thing to
    /// express. Cross-crate, policy joins those as parameters, which is what
    /// makes such a helper mostly parameters.
    ///
    /// Revisit when a fourth site appears with exactly the `postgres`/`redis`
    /// policy — that is, when one policy has a second place to serve. Not
    /// merely when someone has extracted one somewhere.
    async fn close_resource(
        &self,
        handle: &ResourceHandle,
    ) -> Result<CloseDisposition, ResourceError> {
        handle.check(&self.driver_id, self.runtime_epoch)?;
        let taken = {
            let mut live = lock(&self.live);
            live.remove(handle.resource_key())
        };
        let Some(live) = taken else {
            // Already closed: idempotent, and no second budget release.
            return Ok(CloseDisposition::Closed);
        };

        if let Err(err) = self.driver.disconnect(live.connection).await {
            tracing::warn!(
                driver = %self.driver_id,
                resource_key = %handle.resource_key(),
                error = %err,
                "legacy adapter close unconfirmed; budget is not reported as recovered"
            );
            return Ok(CloseDisposition::CloseUnconfirmed);
        }

        // Only now is the resource actually gone, so the charge is released —
        // once, and only once.
        live.budget
            .release_physical_connections(&live.permit)
            .await
            .map_err(|err| {
                tracing::error!(
                    driver = %self.driver_id,
                    resource_key = %handle.resource_key(),
                    error = %err,
                    "resource closed but its budget charge could not be released"
                );
                err
            })?;
        Ok(CloseDisposition::Closed)
    }
}

#[cfg(test)]
#[path = "resource_adapter_tests.rs"]
mod tests;

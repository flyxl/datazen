//! Opaque resource contract (P2).
//!
//! Source of truth: `docs/architecture/platform/connection-management.md` §4
//! (DTOs and target validation order), §5.1 (the nine operation rows, which the
//! [`ResourceProvider`] trait below spells out as 11 async methods plus 3
//! required accessors), §5.2 (capabilities) and
//! `docs/architecture/platform/driver-capability-migration.md`
//! §6 (degraded legacy paths, retirement preconditions).
//!
//! # Why the handle is opaque
//!
//! A [`ResourceHandle`] names a physical connection owned by one provider. It
//! is deliberately:
//!
//! * field-private — callers cannot build one out of strings they happen to
//!   know;
//! * **not** `Deserialize` — reading one back would re-introduce forgery;
//! * provider-validated — every operation checks both `provider_id` and
//!   `runtime_epoch`, so a handle from a previous incarnation of the same
//!   resource key is refused instead of silently reusing a dead connection.
//!
//! Handles are valid inside the provider/worker only and must never be handed
//! to a client.
//!
//! # Why `execute_on_resource` takes a handle
//!
//! The whole point of the contract is that an execution runs on *the* resource
//! the caller asked for. A provider that quietly reaches back into a pool and
//! grabs an arbitrary connection breaks session continuity, which is exactly
//! the defect CM-19 asserts against.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::async_trait;
use crate::capabilities::{CapabilityRegistry, SessionContinuity};
use crate::namespace::{NamespaceLevelKind, NamespaceShape, NamespaceTarget};
use crate::session::{
    CancelReceipt, CloseDisposition, ContextChangeDisposition, ExecutionCompletion,
    ResetDisposition, SessionHandleKind, SessionObservation, TransactionObservation,
    TransactionOptions,
};
use crate::{ConnectionConfig, DriverError, QueryExecutionId};

/// Opaque, provider-owned reference to one physical resource.
///
/// See the module docs for why this is not deserializable.
///
/// # Identity vs. closed state
///
/// `provider_id` + `resource_key` + `runtime_epoch` say *which* resource this
/// is, and that is the whole of the handle's identity: equality and hashing
/// deliberately ignore `closed`, so closing a resource does not stop its
/// handle from equalling the handle it was cloned from. A handle and its
/// clones name one thing; that thing having been closed is a fact *about* it,
/// not a change of identity, and a value that participates in equality while
/// mutating is a trap for any future map keyed on it.
///
/// `closed` is the replacement for the per-provider `BTreeSet` of confirmed
/// close keys that used to live inside each driver. That set had to be
/// unbounded to stay exact — answering "was key K ever closed?" at an
/// arbitrary later time means remembering every key ever issued, which is the
/// same size as the set of keys — so every provider leaked one entry per
/// confirmed close for the life of the process. Carrying the fact on the
/// handle bounds it by the handles the caller still holds, and keeps the
/// answer exact with no window and no eviction: `is_closed()` is true forever
/// once [`Self::mark_closed`] has run.
#[derive(Debug)]
pub struct ResourceHandle {
    provider_id: String,
    resource_key: String,
    runtime_epoch: u64,
    closed: Arc<AtomicBool>,
}

impl PartialEq for ResourceHandle {
    fn eq(&self, other: &Self) -> bool {
        // Identity only — see the type docs. `closed` is excluded on purpose.
        self.provider_id == other.provider_id
            && self.resource_key == other.resource_key
            && self.runtime_epoch == other.runtime_epoch
    }
}

impl Eq for ResourceHandle {}

/// Hand-written because the derived one would be wrong in a way that only
/// shows up later: cloning an `Arc` shares the flag, and that is exactly what
/// a clone of a handle should mean. Closing through the copy marks the
/// original closed too, so a caller cannot end up with two views of one
/// resource that disagree about whether it is closed.
impl Clone for ResourceHandle {
    fn clone(&self) -> Self {
        Self {
            provider_id: self.provider_id.clone(),
            resource_key: self.resource_key.clone(),
            runtime_epoch: self.runtime_epoch,
            closed: Arc::clone(&self.closed),
        }
    }
}

impl std::hash::Hash for ResourceHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // The same three fields `PartialEq` compares, in the same order. A
        // handle that hashes equal to another must compare equal to it.
        self.provider_id.hash(state);
        self.resource_key.hash(state);
        self.runtime_epoch.hash(state);
    }
}

impl ResourceHandle {
    /// Mint a handle. Providers call this from `acquire_resource`; it is the
    /// only way one comes into existence.
    pub fn issue(
        provider_id: impl Into<String>,
        resource_key: impl Into<String>,
        runtime_epoch: u64,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            resource_key: resource_key.into(),
            runtime_epoch,
            closed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    pub fn resource_key(&self) -> &str {
        &self.resource_key
    }

    pub fn runtime_epoch(&self) -> u64 {
        self.runtime_epoch
    }

    /// Record that this resource's close was **confirmed** — the driver's
    /// disconnect returned, so the budget charge really was released.
    ///
    /// Only call this after a successful disconnect. Calling it on an
    /// unconfirmed close would claim a recovery that never happened and make a
    /// later retry silently succeed while the permit stays held.
    ///
    /// The flag is behind an `Arc`, so every clone of this handle observes it.
    /// That is the point: the caller may hand a copy to the next layer and
    /// close through any one of them.
    pub fn mark_closed(&self) {
        // Release, not Relaxed: a reader that sees the flag set must also see
        // everything the provider did before marking it (the budget release,
        // the removal from the registry). The flag publishes that, so it is
        // synchronising, not a standalone boolean.
        self.closed.store(true, Ordering::Release);
    }

    /// Whether this resource was already closed and confirmed.
    ///
    /// A provider's `close_resource` reads this first: true means "already
    /// closed, return `Ok(CloseDisposition::Closed)` and release nothing",
    /// which is what makes a repeated close idempotent for the life of the
    /// process with nothing retained on the provider's side.
    ///
    /// Acquire, to pair with [`Self::mark_closed`].
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// The ownership + epoch check every provider operation performs before
    /// touching the wire.
    pub fn check(&self, provider_id: &str, runtime_epoch: u64) -> Result<(), ResourceError> {
        if self.provider_id != provider_id {
            return Err(ResourceError::ResourceOwnershipMismatch {
                expected: provider_id.to_string(),
                actual: self.provider_id.clone(),
            });
        }
        if self.runtime_epoch != runtime_epoch {
            return Err(ResourceError::StaleRuntimeEpoch {
                expected: runtime_epoch,
                actual: self.runtime_epoch,
            });
        }
        Ok(())
    }
}

/// Serialization is one-way on purpose: a handle may be logged or embedded in
/// a receipt, but never reconstructed from data.
impl Serialize for ResourceHandle {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("ResourceHandle", 3)?;
        state.serialize_field("providerId", &self.provider_id)?;
        state.serialize_field("resourceKey", &self.resource_key)?;
        state.serialize_field("runtimeEpoch", &self.runtime_epoch)?;
        state.end()
    }
}

/// Whether an acquired resource may be handed out again after a reset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReusePolicy {
    SingleUse,
    ReusableAfterReset,
    Unknown,
}

impl Default for ReusePolicy {
    fn default() -> Self {
        Self::Unknown
    }
}

/// One statement the provider must run to bring a resource to its
/// initialization baseline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializationRequirement {
    pub id: String,
    pub sql: String,
    pub mandatory: bool,
    /// True when the statement is safe to re-run during `reset_resource`.
    pub idempotent: bool,
}

/// How the resource is charged against the connection budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionCostPolicy {
    /// The provider owns a real pool and constrains it to this size.
    PoolBounded { max_physical_connections: u32 },
    /// A third-party SDK whose real connection cost is not observable. The
    /// declared figure is a conservative upper bound the budget must honour.
    DeclaredConservative {
        declared_cost: u32,
        hard_cap: Option<u32>,
    },
    /// The provider will not run under strict budget mode at all.
    RefusedUnderStrictBudget,
}

/// The description a provider returns before anything is acquired.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDescriptor {
    pub provider_id: String,
    pub resource_key: String,
    pub session_continuity: SessionContinuity,
    pub reuse_policy: ReusePolicy,
    pub initialization_requirements: Vec<InitializationRequirement>,
    pub connection_cost_policy: ConnectionCostPolicy,
    pub namespace_shape: NamespaceShape,
}

impl ResourceDescriptor {
    /// True when the provider can promise one physical session for the whole
    /// resource lifetime. Defaults to false, so a pool of size one cannot
    /// accidentally pass for a fixed session.
    pub fn is_fixed_session(&self) -> bool {
        self.session_continuity.is_fixed()
    }
}

/// Who is asking for a resource and what it is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResourcePurpose {
    InteractiveQuery,
    MetadataInspection,
    SchemaMutation,
    ExportJob,
    MigrationJob,
    SynchronizationJob,
    TransferJob,
    AgentSession,
}

/// The identity a resource will be established under.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityScope {
    /// The authenticated principal, when the caller knows it.
    pub principal: Option<String>,
    pub allow_privileged: bool,
}

/// How long and how wide a resource may be held.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceScope {
    pub purpose: ResourcePurpose,
    pub max_physical_connections: u32,
    pub holds_open_transaction: bool,
    pub pin_for_streaming: bool,
}

/// Input of `describeResource`: validated config, identity scope, target and
/// purpose.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribeResourceRequest {
    pub connection_config: ConnectionConfig,
    pub identity_scope: IdentityScope,
    pub target: NamespaceTarget,
    pub purpose: ResourcePurpose,
}

/// The initialization baseline a reset returns to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Baseline {
    pub initialization_requirements: Vec<InitializationRequirement>,
}

impl Baseline {
    pub fn new(initialization_requirements: Vec<InitializationRequirement>) -> Self {
        Self {
            initialization_requirements,
        }
    }
}

/// Input of `acquireResource`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcquireResourceRequest {
    pub connection_config: ConnectionConfig,
    pub target: NamespaceTarget,
    pub scope: ResourceScope,
    pub identity_scope: IdentityScope,
    pub baseline: Baseline,
}

/// One Driver Command invocation, routed through the resource contract.
///
/// `command` and `input` are the same pair `DatabaseDriver::execute_command`
/// takes: the adapter path does not re-spell a driver's domain semantics, it
/// only changes *how the connection was obtained*.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandCall {
    pub command: String,
    pub input: serde_json::Value,
}

impl CommandCall {
    pub fn new(command: impl Into<String>, input: serde_json::Value) -> Self {
        Self {
            command: command.into(),
            input,
        }
    }
}

/// An opaque budget permit covering N physical connections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetPermit {
    pub permit_id: String,
    pub physical_connections: u32,
}

/// The connection budget a provider charges against.
///
/// Budgeting is per *physical* connection, not per resource: returning an
/// acquired connection to an idle pool does not release the budget
/// (`connection-management.md` §9.3).
#[async_trait]
pub trait BudgetPort: Send + Sync {
    async fn acquire_physical_connections(
        &self,
        requested: u32,
    ) -> Result<BudgetPermit, ResourceError>;

    async fn release_physical_connections(
        &self,
        permit: &BudgetPermit,
    ) -> Result<(), ResourceError>;
}

/// One batch of result rows produced by an execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultChunk {
    pub statement_index: usize,
    pub sql: String,
    pub rows: Vec<Vec<Option<crate::Value>>>,
    pub rows_affected: Option<u64>,
}

/// Where an execution writes its rows.
///
/// Awaiting [`ResultSink::write`] *is* the backpressure signal: a slow sink
/// slows the driver. The trait deliberately exposes no channel or runtime
/// type — a provider must not leak its own async runtime into this API
/// (`connection-management.md` §5.1).
#[async_trait]
pub trait ResultSink: Send + Sync {
    async fn write(&self, chunk: ResultChunk) -> Result<(), ResourceError>;
    async fn complete(&self) -> Result<(), ResourceError>;
    async fn fail(&self, reason: &str) -> Result<(), ResourceError>;
}

/// A handle the driver issued while an execution ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuedHandle {
    pub handle_id: String,
    pub kind: SessionHandleKind,
}

/// Failures specific to the resource contract.
///
/// Every variant names a situation where the honest answer is "I cannot do
/// that"; none of them has a success-with-no-effect counterpart.
#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    #[error("driver {driver} rejected operation {operation}: {reason}")]
    OperationNotSupported {
        driver: String,
        operation: String,
        reason: String,
    },
    #[error("resource handle belongs to provider {actual}, not {expected}")]
    ResourceOwnershipMismatch { expected: String, actual: String },
    #[error("resource handle runtime epoch {actual} is stale; provider is at {expected}")]
    StaleRuntimeEpoch { expected: u64, actual: u64 },
    #[error("resource {resource_key} is {state} and cannot accept {operation}")]
    InvalidResourceState {
        resource_key: String,
        operation: String,
        state: String,
    },
    #[error("namespace target rejected: {reason}")]
    NamespaceTargetRejected { reason: String },
    #[error("namespace level {kind:?} does not exist on this database")]
    NonexistentNamespaceLevel { kind: NamespaceLevelKind },
    #[error("namespace level {kind:?} is required but was not provided")]
    MissingRequiredNamespaceLevel { kind: NamespaceLevelKind },
    #[error("namespace level {kind:?} is forbidden for this operation")]
    ForbiddenNamespaceLevel { kind: NamespaceLevelKind },
    #[error("alias `{alias}` resolves in a cycle and cannot be merged")]
    AliasConflict { alias: String },
    #[error("transaction state must be resolved before {operation}")]
    TransactionResolutionRequired { operation: String },
    #[error("execution {execution_id} is not in this resource's cancel set")]
    CancelTargetNotRegistered { execution_id: String },
    #[error("connection budget denied {requested} physical connections: {reason}")]
    BudgetDenied { requested: u32, reason: String },
    #[error("result sink rejected a write: {reason}")]
    SinkRejected { reason: String },
    #[error("driver error: {0}")]
    Driver(#[from] DriverError),
}

impl ResourceError {
    pub fn unsupported(
        driver: impl Into<String>,
        operation: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::OperationNotSupported {
            driver: driver.into(),
            operation: operation.into(),
            reason: reason.into(),
        }
    }

    pub fn invalid_namespace(reason: impl Into<String>) -> Self {
        Self::NamespaceTargetRejected {
            reason: reason.into(),
        }
    }
}

/// The resource operations of `connection-management.md` §5.1.
///
/// §5.1 lists **nine operation rows**; this trait spells them out as **11 async
/// methods** plus **3 required accessors** (`provider_id`, `capabilities`,
/// `namespace_shape`) — **14 required methods** in total. The row/method gap
/// is deliberate: §5.1 counts `begin/commit/rollback` as one row, while the
/// trait needs three separately implemented methods so a driver can support a
/// commit without inventing a rollback.
///
/// All 14 methods are **required**, with no default bodies. A provider that
/// cannot honour one must say so in its own code, where the failure is
/// visible, instead of inheriting a body that quietly does nothing. This is
/// what makes "新增能力缺失不会 no-op 成功" structural rather than a convention.
///
/// The "no default bodies" rule is not a comment that can be ignored —
/// `resource_no_default_bodies_tests.rs` fails the build if any method in this
/// trait gains a body, so `impl ResourceProvider for X {}` cannot compile by
/// accident.
///
/// A provider must additionally:
///
/// * validate ownership and epoch on every call ([`ResourceHandle::check`]);
/// * never fall back to a session-wide cancel from `request_cancel`;
/// * never return a fabricated observation — an unreadable field stays
///   `Unknown`;
/// * release budget exactly once per acquire.
#[async_trait]
pub trait ResourceProvider: Send + Sync {
    /// Stable identity of this provider. Used as the handle's owner.
    fn provider_id(&self) -> &str;

    /// Declared capabilities. The registry's `require_*` methods are how a
    /// caller turns an undeclared capability into an explicit error.
    fn capabilities(&self) -> &CapabilityRegistry;

    /// The namespace shape every target is validated against.
    fn namespace_shape(&self) -> &NamespaceShape;

    /// Describe what acquiring would give, without acquiring.
    async fn describe_resource(
        &self,
        request: &DescribeResourceRequest,
    ) -> Result<ResourceDescriptor, ResourceError>;

    /// Acquire a resource, charging the budget for its physical connections.
    ///
    /// The budget arrives as an `Arc` so the provider can keep it until the
    /// matching `close_resource` and release the charge **exactly once**,
    /// which is not possible when the two calls are separate.
    async fn acquire_resource(
        &self,
        request: &AcquireResourceRequest,
        budget: &Arc<dyn BudgetPort>,
    ) -> Result<ResourceHandle, ResourceError>;

    /// Execute one Command on *this* resource. The provider must not reach
    /// back into a pool for a different connection.
    async fn execute_on_resource(
        &self,
        handle: &ResourceHandle,
        execution_id: &QueryExecutionId,
        call: &CommandCall,
        sink: &dyn ResultSink,
    ) -> Result<ExecutionCompletion, ResourceError>;

    /// Read the session back from the same physical resource.
    async fn observe_session(
        &self,
        handle: &ResourceHandle,
    ) -> Result<SessionObservation, ResourceError>;

    /// Move the session to another namespace, or say why not. Must not mutate
    /// host configuration and must not silently reconnect.
    async fn change_context(
        &self,
        handle: &ResourceHandle,
        desired: &NamespaceTarget,
    ) -> Result<ContextChangeDisposition, ResourceError>;

    /// Open a transaction on this resource.
    async fn begin_transaction(
        &self,
        handle: &ResourceHandle,
        options: &TransactionOptions,
    ) -> Result<TransactionObservation, ResourceError>;

    /// Commit the resource's transaction. An unconfirmed commit is reported as
    /// `EffectOutcome::Unknown`, never as `Completed`.
    async fn commit_transaction(
        &self,
        handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError>;

    /// Roll back the resource's transaction, with the same honesty rule.
    async fn rollback_transaction(
        &self,
        handle: &ResourceHandle,
    ) -> Result<TransactionObservation, ResourceError>;

    /// Request cancellation of exactly one execution. A driver that cannot
    /// address one execution returns
    /// [`crate::session::CancelDisposition::Unsupported`]; it must not call a
    /// session-wide cancel instead.
    async fn request_cancel(
        &self,
        handle: &ResourceHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<CancelReceipt, ResourceError>;

    /// Return the resource to its initialization baseline, covering the
    /// protocol layer only. Already-registered handles are out of scope.
    async fn reset_resource(
        &self,
        handle: &ResourceHandle,
        baseline: &Baseline,
    ) -> Result<ResetDisposition, ResourceError>;

    /// Close the resource. Idempotent, and budget is released exactly once —
    /// never claimed as recovered when the close is unconfirmed.
    async fn close_resource(
        &self,
        handle: &ResourceHandle,
    ) -> Result<CloseDisposition, ResourceError>;
}

#[cfg(test)]
#[path = "resource_tests.rs"]
mod tests;

/// Enforces "no default bodies on `ResourceProvider`" in CI rather than in
/// review. Lives beside the trait because it reads `resource.rs` as text.
#[cfg(test)]
#[path = "resource_no_default_bodies_tests.rs"]
mod no_default_bodies_tests;

/// The `closed` flag, asserted at the type's own level rather than only through
/// a driver. Inline here because `resource_tests.rs` is a separate file; these
/// are tests of this struct's own semantics, not of the contract around it.
#[cfg(test)]
mod closed_flag_tests {
    use super::ResourceHandle;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn handle() -> ResourceHandle {
        ResourceHandle::issue("postgres", "conn-1", 7)
    }

    fn hash_of(handle: &ResourceHandle) -> u64 {
        let mut hasher = DefaultHasher::new();
        handle.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn a_fresh_handle_is_not_closed() {
        assert!(
            !handle().is_closed(),
            "issue() must mint a handle that has never been closed"
        );
    }

    #[test]
    fn marking_one_clone_marks_every_clone() {
        let original = handle();
        let copy = original.clone();

        copy.mark_closed();

        assert!(
            original.is_closed(),
            "a clone shares the flag: closing through a copy must be visible on \
             the handle the caller kept, or two views of one resource could \
             disagree about whether it is closed"
        );
    }

    #[test]
    fn closing_does_not_change_a_handle_s_identity() {
        let original = handle();
        let before_hash = hash_of(&original);
        let equal_before = original.clone();

        original.mark_closed();

        assert_eq!(
            original, equal_before,
            "identity is the provider, key and epoch — closing a resource does \
             not stop its handle equalling the handle it was cloned from"
        );
        assert_eq!(
            hash_of(&original),
            before_hash,
            "if the flag were hashed, two equal handles could hash differently, \
             which breaks any map keyed on them"
        );
    }

    #[test]
    fn closing_one_resource_does_not_make_it_equal_to_another() {
        let closed = handle();
        closed.mark_closed();
        let other = ResourceHandle::issue("postgres", "conn-2", 7);

        assert_ne!(
            closed, other,
            "identity must still discriminate: dropping the key or the epoch \
             from equality would make every handle compare equal once any of \
             them is closed"
        );
    }

    #[test]
    fn the_receipt_shape_does_not_grow_a_closed_field() {
        let handle = handle();
        handle.mark_closed();

        let value = serde_json::to_value(&handle).expect("handle serializes");
        let object = value.as_object().expect("handle serializes as an object");
        assert_eq!(
            object.len(),
            3,
            "a handle is embedded in receipts; the closed flag is runtime state \
             and adding a field would change every receipt that carries one"
        );
        assert!(!object.contains_key("closed"));
    }
}

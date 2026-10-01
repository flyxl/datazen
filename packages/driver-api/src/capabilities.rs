//! Capability declarations for the opaque resource contract (P2).
//!
//! Source of truth: `docs/architecture/platform/connection-management.md` §5.2
//! (capability enums) and §4 (`CapabilitySnapshot` DTO), plus
//! `docs/architecture/platform/driver-capability-migration.md` §5.3
//! (breaking-change table) and §6.1 (fail-closed rules).
//!
//! # Fail-closed design
//!
//! Every enum in this module has a [`Default`] that is the *non*-supporting
//! value (`Unknown` / `Unsupported`). A driver that does not declare a
//! capability therefore lands on a value that disables the dependent
//! behaviour instead of enabling it. This is what keeps "新增能力缺失不会
//! no-op 成功" true at the type level rather than by convention.
//!
//! Two invariants are encoded here and must not be relaxed:
//!
//! * `SessionContinuity::Fixed` may only be declared when the driver really
//!   owns a session-scoped physical connection. A pool of size one is **not**
//!   evidence of a fixed session (`driver-capability-migration.md` §6.1).
//! * An `Unsupported` / `Unknown` capability must be turned into an explicit
//!   error by [`CapabilityRegistry::require_*`]; it must never be mapped onto a
//!   successful no-op.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::DdlAtomicity;
use crate::{MIN_PROTOCOL_VERSION, PROTOCOL_VERSION};

/// Coarse "does the driver support this at all" answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Availability {
    Supported,
    Unsupported,
    Unknown,
}

impl Availability {
    /// `Unsupported` and `Unknown` are both fail-closed: they gate features off.
    pub fn enables_feature(self) -> bool {
        matches!(self, Self::Supported)
    }
}

impl Default for Availability {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Whether the driver can move a live session to another namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NamespaceSwitch {
    /// The same session changes namespace without a new connection.
    InPlace,
    /// The switch is possible but only by replacing the underlying resource.
    RequiresReplacement,
    Unsupported,
    Unknown,
}

impl NamespaceSwitch {
    /// `Unknown` must not enable in-place switching.
    pub fn switches_in_place(self) -> bool {
        matches!(self, Self::InPlace)
    }
}

impl Default for NamespaceSwitch {
    fn default() -> Self {
        Self::Unknown
    }
}

/// How completely a driver can report the observable session context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContextObservation {
    Full,
    Partial,
    Unsupported,
}

impl ContextObservation {
    /// Only `Full` yields a confirmed snapshot. `Partial` must surface
    /// `ObservationConfidence::Partial`, `Unsupported` yields nothing at all.
    pub fn confirms_context(self) -> bool {
        matches!(self, Self::Full)
    }
}

impl Default for ContextObservation {
    fn default() -> Self {
        Self::Unsupported
    }
}

/// How completely a driver can report transaction state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransactionObservation {
    Full,
    Partial,
    Unsupported,
}

impl Default for TransactionObservation {
    fn default() -> Self {
        Self::Unsupported
    }
}

/// Whether the driver can hand out session-scoped handles (cursor, prepared
/// statement, transaction) that stay valid for the resource's lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionScopedHandleSupport {
    Supported,
    Unsupported,
    Unknown,
}

impl SessionScopedHandleSupport {
    /// `Unknown` gates handles off; a handle list is only trustworthy when the
    /// driver has actually said it can issue one.
    pub fn enables_feature(self) -> bool {
        matches!(self, Self::Supported)
    }
}

impl Default for SessionScopedHandleSupport {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Whether `resetResource` has been verified to return a resource to its
/// initialization baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResetForReuse {
    Verified,
    Unsupported,
}

impl Default for ResetForReuse {
    fn default() -> Self {
        Self::Unsupported
    }
}

/// Whether `requestCancel` can address a single execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PreciseCancelSupport {
    Supported,
    Unsupported,
    Unknown,
}

impl PreciseCancelSupport {
    /// Only `Supported` allows a cancel request to be forwarded. `Unknown` is
    /// treated as "not in the cancel set", never as "cancel succeeded".
    pub fn accepts_precise_cancel(self) -> bool {
        matches!(self, Self::Supported)
    }
}

impl Default for PreciseCancelSupport {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Consistency guarantees offered for read snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotSupport {
    PerTable,
    PerDatabase,
    Coordinated,
    Unsupported,
}

impl SnapshotSupport {
    /// Only `Unsupported` gates snapshots off; every other value is a real,
    /// named guarantee rather than a yes/no guess.
    pub fn enables_feature(self) -> bool {
        !matches!(self, Self::Unsupported)
    }
}

impl Default for SnapshotSupport {
    fn default() -> Self {
        Self::Unsupported
    }
}

/// Transaction feature surface beyond "a transaction can be started".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionSupport {
    /// Isolation level identifiers the driver accepts, spelled the way the
    /// driver spells them. Empty means "cannot confirm any level".
    pub isolation_levels: Vec<String>,
    /// Whether savepoints inside an open transaction are usable.
    pub savepoints: Availability,
    /// Hard limit on concurrently open transactions, when the driver has one.
    pub max_open_transactions: Option<u32>,
}

impl Default for TransactionSupport {
    fn default() -> Self {
        Self {
            isolation_levels: Vec::new(),
            savepoints: Availability::Unknown,
            max_open_transactions: None,
        }
    }
}

/// Per-operation DDL atomicity, keyed by the migration operation name.
///
/// Mirrors `DatabaseDriver::ddl_atomicity`, which only expresses a single
/// driver-wide answer; a resource contract caller needs the per-operation
/// answer before deciding whether to wrap a planned change set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DdlAtomicitySupport {
    pub by_operation: BTreeMap<String, DdlAtomicity>,
}

impl DdlAtomicitySupport {
    /// Look up one operation; an unknown operation is `Unknown`, never
    /// `Transactional`.
    pub fn atomicity_for(&self, operation: &str) -> DdlAtomicity {
        self.by_operation
            .get(operation)
            .copied()
            .unwrap_or(DdlAtomicity::Unknown)
    }
}

/// Whether a resource keeps one physical session or hands out pooled leases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionContinuity {
    /// One session-scoped physical connection bound to the resource.
    Fixed,
    /// A pooled lease; successive operations may run on different connections.
    Leased,
    /// The driver cannot confirm which of the two it is.
    Unknown,
}

impl SessionContinuity {
    /// Only `Fixed` supports session-scoped guarantees. `Unknown` is treated
    /// exactly like `Leased`; a pool of size one is not a fixed session
    /// (`driver-capability-migration.md` §6.1).
    pub fn is_fixed(self) -> bool {
        matches!(self, Self::Fixed)
    }
}

impl Default for SessionContinuity {
    fn default() -> Self {
        Self::Unknown
    }
}

/// The full capability surface a `ResourceProvider` declares.
///
/// Every field defaults to its non-supporting value, so
/// [`CapabilitySet::default`] is the correct declaration for a provider that
/// has not been migrated yet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySet {
    pub stateful_session: Availability,
    pub namespace_switch: NamespaceSwitch,
    pub context_observation: ContextObservation,
    pub transaction_observation: TransactionObservation,
    pub session_scoped_handles: SessionScopedHandleSupport,
    pub reset_for_reuse: ResetForReuse,
    pub precise_cancel: PreciseCancelSupport,
    pub snapshots: SnapshotSupport,
    pub transactions: TransactionSupport,
    pub ddl_atomicity: DdlAtomicitySupport,
}

impl CapabilitySet {
    /// True when at least one non-default capability was declared. Used by the
    /// contract tests to prove the defaults really are inert.
    pub fn declares_anything(&self) -> bool {
        *self != Self::default()
    }
}

/// Driver identity plus the confirmed capability evidence that was captured
/// alongside it (`connection-management.md` §4, `CapabilitySnapshot`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySnapshot {
    pub driver_id: String,
    pub driver_version: String,
    pub protocol_version: u32,
    pub capability_revision: u64,
    /// Free-form record of what was actually confirmed. Empty is a legitimate
    /// value: it means nothing has been confirmed yet.
    pub confirmed: BTreeMap<String, String>,
}

impl CapabilitySnapshot {
    pub fn new(
        driver_id: impl Into<String>,
        driver_version: impl Into<String>,
        protocol_version: u32,
        capability_revision: u64,
    ) -> Self {
        Self {
            driver_id: driver_id.into(),
            driver_version: driver_version.into(),
            protocol_version,
            capability_revision,
            confirmed: BTreeMap::new(),
        }
    }
}

/// A declared capability was used where the provider cannot honour it.
///
/// Every construction site of this error is a place where the alternative
/// would have been a silent no-op success.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("capability `{capability}` is not available: {provider_id}")]
pub struct CapabilityError {
    pub capability: &'static str,
    pub provider_id: String,
}

/// A resource provider's declared capability surface plus its identity.
///
/// The `require_*` methods are the contract's rejection surface: callers that
/// depend on a capability ask for it and receive an explicit error when the
/// provider did not declare it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityRegistry {
    pub provider_id: String,
    pub snapshot: CapabilitySnapshot,
    pub capabilities: CapabilitySet,
}

impl CapabilityRegistry {
    pub fn new(provider_id: impl Into<String>, snapshot: CapabilitySnapshot) -> Self {
        Self {
            provider_id: provider_id.into(),
            snapshot,
            capabilities: CapabilitySet::default(),
        }
    }

    fn reject(&self, capability: &'static str) -> CapabilityError {
        CapabilityError {
            capability,
            provider_id: self.provider_id.clone(),
        }
    }

    /// Require one session-scoped physical connection for the resource.
    pub fn require_stateful_session(&self) -> Result<(), CapabilityError> {
        if self.capabilities.stateful_session.enables_feature() {
            Ok(())
        } else {
            Err(self.reject("statefulSession"))
        }
    }

    /// Require confirmed, complete session-context observation.
    pub fn require_context_observation(&self) -> Result<(), CapabilityError> {
        if self.capabilities.context_observation.confirms_context() {
            Ok(())
        } else {
            Err(self.reject("contextObservation"))
        }
    }

    /// Require at least partial transaction-state observation.
    pub fn require_transaction_observation(&self) -> Result<(), CapabilityError> {
        match self.capabilities.transaction_observation {
            TransactionObservation::Full | TransactionObservation::Partial => Ok(()),
            TransactionObservation::Unsupported => Err(self.reject("transactionObservation")),
        }
    }

    /// Require session-scoped handles to be issuable.
    pub fn require_session_scoped_handles(&self) -> Result<(), CapabilityError> {
        if self.capabilities.session_scoped_handles.enables_feature() {
            Ok(())
        } else {
            Err(self.reject("sessionScopedHandles"))
        }
    }

    /// Require a verified reset-to-baseline path.
    pub fn require_reset_for_reuse(&self) -> Result<(), CapabilityError> {
        match self.capabilities.reset_for_reuse {
            ResetForReuse::Verified => Ok(()),
            ResetForReuse::Unsupported => Err(self.reject("resetForReuse")),
        }
    }

    /// Require precise, single-execution cancellation.
    ///
    /// A provider that cannot address one execution must not fall back to a
    /// session-wide cancel; this method is where that is refused.
    pub fn require_precise_cancel(&self) -> Result<(), CapabilityError> {
        if self.capabilities.precise_cancel.accepts_precise_cancel() {
            Ok(())
        } else {
            Err(self.reject("preciseCancel"))
        }
    }

    /// Require a coordinated snapshot.
    pub fn require_snapshots(&self) -> Result<(), CapabilityError> {
        match self.capabilities.snapshots {
            SnapshotSupport::PerTable
            | SnapshotSupport::PerDatabase
            | SnapshotSupport::Coordinated => Ok(()),
            SnapshotSupport::Unsupported => Err(self.reject("snapshots")),
        }
    }

    /// Require the resource to stay pinned to one session while a context
    /// switch is requested.
    pub fn require_in_place_namespace_switch(&self) -> Result<(), CapabilityError> {
        if self.capabilities.namespace_switch.switches_in_place() {
            Ok(())
        } else {
            Err(self.reject("namespaceSwitch"))
        }
    }

    /// Snapshot the registry into the DTO the execution receipt carries.
    pub fn snapshot(&self) -> CapabilitySnapshot {
        self.snapshot.clone()
    }
}

/// Where a factory's declared protocol version sits relative to this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProtocolCompatibility {
    /// Within `[MIN_PROTOCOL_VERSION, PROTOCOL_VERSION]`.
    Current,
    /// Below `MIN_PROTOCOL_VERSION`: must be rejected, never degraded.
    BelowMinimum,
    /// Above `PROTOCOL_VERSION`: not understood by this host.
    AheadOfHost,
}

impl ProtocolCompatibility {
    pub fn is_usable(self) -> bool {
        matches!(self, Self::Current)
    }
}

/// A protocol gate decision that is not `Current`.
///
/// The host gate itself (`src-tauri/src/db/registry.rs`) lives outside
/// `packages/driver-api`; this error only states that the driver is outside the
/// window this crate can honour. It exists so that no code path can quietly
/// lower `MIN_PROTOCOL_VERSION` or drop capabilities to make an
/// incompatible driver load
/// (`driver-capability-migration.md` §5.4).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolGateError {
    #[error(
        "driver protocol version {declared} is below the minimum compatible version {minimum}"
    )]
    BelowMinimum { declared: u32, minimum: u32 },
    #[error("driver protocol version {declared} is newer than host protocol version {host}")]
    AheadOfHost { declared: u32, host: u32 },
}

/// Classify a declared `protocol_version()` against this crate's window.
///
/// [`check_protocol_compatibility`] is the fail-closed wrapper; this function
/// exposes the raw classification so a host gate can apply its own decision
/// for [`ProtocolCompatibility::AheadOfHost`] without this crate pre-empting
/// that choice (`driver-capability-migration.md` §5.4 item H).
pub fn classify_protocol_version(declared: u32) -> ProtocolCompatibility {
    if declared < MIN_PROTOCOL_VERSION {
        ProtocolCompatibility::BelowMinimum
    } else if declared > PROTOCOL_VERSION {
        ProtocolCompatibility::AheadOfHost
    } else {
        ProtocolCompatibility::Current
    }
}

/// Fail-closed protocol gate: anything outside the supported window is an
/// error. A caller that wants the two out-of-window cases handled differently
/// must use [`classify_protocol_version`] and make that choice explicit.
pub fn check_protocol_compatibility(
    declared: u32,
) -> Result<ProtocolCompatibility, ProtocolGateError> {
    match classify_protocol_version(declared) {
        ProtocolCompatibility::Current => Ok(ProtocolCompatibility::Current),
        ProtocolCompatibility::BelowMinimum => Err(ProtocolGateError::BelowMinimum {
            declared,
            minimum: MIN_PROTOCOL_VERSION,
        }),
        ProtocolCompatibility::AheadOfHost => Err(ProtocolGateError::AheadOfHost {
            declared,
            host: PROTOCOL_VERSION,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_with(mutate: impl FnOnce(&mut CapabilityRegistry)) -> CapabilityRegistry {
        let mut registry = CapabilityRegistry::new(
            "test-provider",
            CapabilitySnapshot::new("test-driver", "0.0.0", PROTOCOL_VERSION, 1),
        );
        mutate(&mut registry);
        registry
    }

    #[test]
    fn default_capability_set_declares_nothing() {
        let set = CapabilitySet::default();
        assert!(!set.declares_anything());
        assert!(!set.stateful_session.enables_feature());
        assert!(!set.namespace_switch.switches_in_place());
        assert!(!set.context_observation.confirms_context());
        assert!(!set.precise_cancel.accepts_precise_cancel());
        assert!(!set.snapshots.enables_feature());
    }

    #[test]
    fn every_requirement_rejects_on_the_default_registry() {
        let registry = registry_with(|_| {});
        assert!(registry.require_stateful_session().is_err());
        assert!(registry.require_context_observation().is_err());
        assert!(registry.require_transaction_observation().is_err());
        assert!(registry.require_session_scoped_handles().is_err());
        assert!(registry.require_reset_for_reuse().is_err());
        assert!(registry.require_precise_cancel().is_err());
        assert!(registry.require_snapshots().is_err());
        assert!(registry.require_in_place_namespace_switch().is_err());
    }

    #[test]
    fn rejection_error_names_the_capability_and_provider() {
        let registry = registry_with(|_| {});
        let err = registry.require_precise_cancel().unwrap_err();
        assert_eq!(err.capability, "preciseCancel");
        assert_eq!(err.provider_id, "test-provider");
    }

    #[test]
    fn unknown_is_not_the_same_as_supported() {
        let registry = registry_with(|registry| {
            // `Unknown` is what an unmigrated driver reports.
            registry.capabilities.stateful_session = Availability::Unknown;
        });
        assert!(registry.require_stateful_session().is_err());
    }

    #[test]
    fn requirements_pass_once_the_capability_is_declared() {
        let registry = registry_with(|registry| {
            registry.capabilities.stateful_session = Availability::Supported;
            registry.capabilities.context_observation = ContextObservation::Full;
            registry.capabilities.transaction_observation = TransactionObservation::Full;
            registry.capabilities.session_scoped_handles = SessionScopedHandleSupport::Supported;
            registry.capabilities.reset_for_reuse = ResetForReuse::Verified;
            registry.capabilities.precise_cancel = PreciseCancelSupport::Supported;
            registry.capabilities.snapshots = SnapshotSupport::Coordinated;
            registry.capabilities.namespace_switch = NamespaceSwitch::InPlace;
        });
        assert!(registry.require_stateful_session().is_ok());
        assert!(registry.require_context_observation().is_ok());
        assert!(registry.require_transaction_observation().is_ok());
        assert!(registry.require_session_scoped_handles().is_ok());
        assert!(registry.require_reset_for_reuse().is_ok());
        assert!(registry.require_precise_cancel().is_ok());
        assert!(registry.require_snapshots().is_ok());
        assert!(registry.require_in_place_namespace_switch().is_ok());
    }

    #[test]
    fn partial_context_observation_does_not_confirm() {
        let registry = registry_with(|registry| {
            registry.capabilities.context_observation = ContextObservation::Partial;
        });
        assert!(registry.require_context_observation().is_err());
    }

    #[test]
    fn requires_replacement_does_not_switch_in_place() {
        let registry = registry_with(|registry| {
            registry.capabilities.namespace_switch = NamespaceSwitch::RequiresReplacement;
        });
        assert!(registry.require_in_place_namespace_switch().is_err());
    }

    #[test]
    fn session_continuity_unknown_is_not_a_fixed_session() {
        assert!(!SessionContinuity::default().is_fixed());
        assert!(!SessionContinuity::Leased.is_fixed());
        assert!(SessionContinuity::Fixed.is_fixed());
    }

    #[test]
    fn ddl_atomicity_lookup_defaults_to_unknown() {
        let mut support = DdlAtomicitySupport::default();
        assert_eq!(support.atomicity_for("createTable"), DdlAtomicity::Unknown);
        support
            .by_operation
            .insert("createTable".to_string(), DdlAtomicity::Transactional);
        assert_eq!(
            support.atomicity_for("createTable"),
            DdlAtomicity::Transactional
        );
        assert_eq!(support.atomicity_for("dropTable"), DdlAtomicity::Unknown);
    }

    #[test]
    fn protocol_gate_accepts_the_supported_window() {
        assert_eq!(
            check_protocol_compatibility(PROTOCOL_VERSION),
            Ok(ProtocolCompatibility::Current)
        );
        assert_eq!(
            check_protocol_compatibility(MIN_PROTOCOL_VERSION),
            Ok(ProtocolCompatibility::Current)
        );
    }

    #[test]
    fn protocol_gate_rejects_below_minimum_without_degrading() {
        let err = check_protocol_compatibility(MIN_PROTOCOL_VERSION.saturating_sub(1))
            .expect_err("below-minimum driver must be rejected");
        assert_eq!(
            err,
            ProtocolGateError::BelowMinimum {
                declared: MIN_PROTOCOL_VERSION - 1,
                minimum: MIN_PROTOCOL_VERSION,
            }
        );
        assert_eq!(
            classify_protocol_version(MIN_PROTOCOL_VERSION - 1),
            ProtocolCompatibility::BelowMinimum
        );
    }

    #[test]
    fn protocol_gate_rejects_ahead_of_host_without_degrading() {
        let ahead = PROTOCOL_VERSION + 1;
        let err = check_protocol_compatibility(ahead).expect_err("newer driver must be surfaced");
        assert_eq!(
            err,
            ProtocolGateError::AheadOfHost {
                declared: ahead,
                host: PROTOCOL_VERSION,
            }
        );
        assert_eq!(
            classify_protocol_version(ahead),
            ProtocolCompatibility::AheadOfHost
        );
    }

    #[test]
    fn capability_snapshot_defaults_to_no_confirmed_evidence() {
        let snapshot = CapabilitySnapshot::new("driver", "1.2.3", PROTOCOL_VERSION, 7);
        assert!(snapshot.confirmed.is_empty());
        assert_eq!(snapshot.capability_revision, 7);
    }
}

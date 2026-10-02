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

use crate::capability_domains::{BackupSupport, DataSupport};
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
    /// The `data` domain: row read / row write / streaming results
    /// (`platform-development-plan.md:93`).
    pub data: DataSupport,
    /// The `backup` domain: producing an artifact / consuming one back
    /// (`platform-development-plan.md:93`).
    pub backup: BackupSupport,
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

/// Evidence was attached to a snapshot that declares revision `0`.
///
/// `0` is reserved for "this provider had no capability module yet" — the
/// sentinel postgres's own revision comment names. A snapshot that carries
/// evidence at revision `0` says two contradictory things at once: the
/// evidence exists, and the declaration that would produce it does not. A
/// cached snapshot could then not be told apart from a current one, which is
/// the single thing `capability_revision` exists to prevent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "provider `{provider_id}` recorded evidence at capability revision {capability_revision}: \
     revision 0 means no capability module existed yet, so the evidence cannot be \
     attributed to any declaration"
)]
pub struct EvidenceRevisionError {
    pub provider_id: String,
    pub capability_revision: u64,
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

    /// Attach recorded evidence. `new` keeps its four-argument shape so every
    /// existing construction site is untouched; the evidence channel is
    /// opt-in through this builder.
    ///
    /// Evidence keys are the cell names of [`CapabilitySet`] in its serialized
    /// (`camelCase`) spelling, which is what [`Self::evidence_gaps`] matches
    /// on. Keys outside the twelve are kept as-is.
    ///
    /// # Errors
    ///
    /// [`EvidenceRevisionError`] when evidence is supplied and
    /// `capability_revision` is `0`. The rejection is deliberate: without it
    /// a driver could record evidence under a revision that claims no
    /// capability module exists, and nothing would catch the contradiction.
    pub fn with_evidence<K, V, I>(self, evidence: I) -> Result<Self, EvidenceRevisionError>
    where
        K: Into<String>,
        V: Into<String>,
        I: IntoIterator<Item = (K, V)>,
    {
        let incoming: BTreeMap<String, String> = evidence
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();
        if self.capability_revision == 0 && !incoming.is_empty() {
            return Err(EvidenceRevisionError {
                provider_id: self.driver_id.clone(),
                capability_revision: self.capability_revision,
            });
        }
        Ok(self.merge_evidence(incoming))
    }

    /// Merge evidence without consulting the revision invariant.
    ///
    /// `pub(crate)` because it is only sound for a caller that has already
    /// established a non-zero revision — `LegacyResourceAdapter` fixes its own
    /// revision and then merges through here. The public gate that a driver
    /// has to pass is [`Self::with_evidence`].
    pub(crate) fn merge_evidence<K, V, I>(self, evidence: I) -> Self
    where
        K: Into<String>,
        V: Into<String>,
        I: IntoIterator<Item = (K, V)>,
    {
        let mut confirmed = self.confirmed;
        confirmed.extend(
            evidence
                .into_iter()
                .map(|(key, value)| (key.into(), value.into())),
        );
        Self { confirmed, ..self }
    }

    /// The declared cells that carry no evidence, in declaration order.
    ///
    /// This is deliberately computed from `self.confirmed` rather than from
    /// anything the driver passes in, so a driver cannot answer on its own
    /// behalf: recording nothing is observable as "all twelve are missing"
    /// without the driver's cooperation, which is what separates *declares
    /// nothing here* from *declares nothing there*.
    ///
    /// The twelve names below are the **only** copy of the cell list in the
    /// codebase. Adding a field to [`CapabilitySet`] means adding it here; a
    /// second list would be free to drift from this one, and a drifted list
    /// reports either a phantom gap or a silent miss.
    ///
    /// Keys in `confirmed` that are not cells are left alone and are not
    /// reported: `confirmed` is an evidence table, not a cell table, so
    /// evidence beyond the twelve is strictly more information rather than an
    /// error.
    pub fn evidence_gaps(&self) -> Vec<&'static str> {
        const CAPABILITY_CELLS: [&str; 12] = [
            "statefulSession",
            "namespaceSwitch",
            "contextObservation",
            "transactionObservation",
            "sessionScopedHandles",
            "resetForReuse",
            "preciseCancel",
            "snapshots",
            "transactions",
            "ddlAtomicity",
            "data",
            "backup",
        ];
        CAPABILITY_CELLS
            .iter()
            .copied()
            .filter(|cell| !self.confirmed.contains_key(*cell))
            .collect()
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

    /// Delegate to [`CapabilitySnapshot::evidence_gaps`], which owns the cell
    /// list. Callers usually hold a registry rather than a bare snapshot.
    pub fn evidence_gaps(&self) -> Vec<&'static str> {
        self.snapshot.evidence_gaps()
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

    /// Require rows to be readable at all.
    pub fn require_row_read(&self) -> Result<(), CapabilityError> {
        if self.capabilities.data.enables_row_read() {
            Ok(())
        } else {
            Err(self.reject("data.rowRead"))
        }
    }

    /// Require rows to be writable, not merely readable.
    ///
    /// A read-only resource satisfies [`CapabilityRegistry::require_row_read`]
    /// and fails here; that is the whole point of asking separately.
    pub fn require_row_write(&self) -> Result<(), CapabilityError> {
        if self.capabilities.data.enables_row_write() {
            Ok(())
        } else {
            Err(self.reject("data.rowWrite"))
        }
    }

    /// Require results to arrive incrementally rather than fully buffered.
    pub fn require_streaming_results(&self) -> Result<(), CapabilityError> {
        if self.capabilities.data.enables_streaming_results() {
            Ok(())
        } else {
            Err(self.reject("data.streamingResults"))
        }
    }

    /// Require the provider to produce a backup artifact.
    pub fn require_backup_artifact(&self) -> Result<(), CapabilityError> {
        if self.capabilities.backup.enables_artifact() {
            Ok(())
        } else {
            Err(self.reject("backup.artifact"))
        }
    }

    /// Require the provider to consume a backup artifact back into a resource.
    pub fn require_restore_from_artifact(&self) -> Result<(), CapabilityError> {
        if self.capabilities.backup.enables_restore() {
            Ok(())
        } else {
            Err(self.reject("backup.restore"))
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
    fn undeclared_data_and_backup_domains_reject_every_request() {
        let registry = registry_with(|_| {});
        for result in [
            registry.require_row_read(),
            registry.require_row_write(),
            registry.require_streaming_results(),
            registry.require_backup_artifact(),
            registry.require_restore_from_artifact(),
        ] {
            let error = result.expect_err("an undeclared domain must not succeed");
            assert!(!error.capability.is_empty());
        }
    }

    #[test]
    fn a_degraded_domain_is_accepted_only_on_the_axes_it_keeps() {
        let read_only = registry_with(|registry| {
            registry.capabilities.data = DataSupport::StreamingReadOnly;
        });
        assert!(read_only.require_row_read().is_ok());
        assert!(read_only.require_streaming_results().is_ok());
        assert!(read_only.require_row_write().is_err());

        let artifact_only = registry_with(|registry| {
            registry.capabilities.backup = BackupSupport::ArtifactOnly;
        });
        assert!(artifact_only.require_backup_artifact().is_ok());
        assert!(artifact_only.require_restore_from_artifact().is_err());
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

    #[test]
    fn evidence_gaps_covers_exactly_the_declared_cells() {
        // Drift guard. The gap list is the only copy of the cell names in the
        // codebase, and a `CapabilitySet` that grows a field would otherwise
        // leave that field permanently un-reachable: not missing enough to
        // report, not present enough to check. Comparing against the
        // serialized shape catches that here rather than in a driver's review.
        let snapshot = CapabilitySnapshot::new("driver", "1.2.3", PROTOCOL_VERSION, 7);
        let declared: Vec<String> = serde_json::to_value(CapabilitySet::default())
            .expect("CapabilitySet is a plain data struct")
            .as_object()
            .expect("CapabilitySet serializes to a JSON object")
            .keys()
            .cloned()
            .collect();

        // Nothing recorded -> every declared field must show up as a gap.
        // Compared as a set: the gap list is in declaration order, while
        // `serde_json::Value`'s object is key-sorted, so the two orders are
        // independent and only membership is the invariant.
        let mut gaps = snapshot.evidence_gaps();
        let mut declared_sorted = declared.clone();
        gaps.sort_unstable();
        declared_sorted.sort();
        assert_eq!(gaps, declared_sorted);
        assert_eq!(gaps.len(), 12);

        // Record every cell under its serialized name -> no gaps left.
        let filled = snapshot
            .with_evidence(
                declared
                    .iter()
                    .map(|cell| (cell.clone(), "synthetic rationale".to_string())),
            )
            .expect("revision 7 is allowed to carry evidence");
        assert!(filled.evidence_gaps().is_empty());
    }

    #[test]
    fn extra_evidence_keys_are_neither_gaps_nor_errors() {
        // `confirmed` is an evidence table, not a cell table. A driver that
        // records more than the twelve is not wrong, and the extra key must not
        // leak into the gap list as a phantom cell.
        let snapshot = CapabilitySnapshot::new("driver", "1.2.3", PROTOCOL_VERSION, 7)
            .with_evidence([
                (
                    "connectionCostPolicy",
                    "pool sizing is charged per namespace",
                ),
                ("ddlAtomicity", "PostgreSQL runs DDL in a transaction"),
            ])
            .expect("revision 7 is allowed to carry evidence");

        let gaps = snapshot.evidence_gaps();
        assert!(!gaps.contains(&"connectionCostPolicy"));
        assert!(!gaps.contains(&"ddlAtomicity"));
        assert_eq!(gaps.len(), 11);
    }
}

//! Session, transaction and execution observation contracts (P2).
//!
//! Source of truth: `docs/architecture/platform/connection-management.md` §4
//! (DTOs), §5.1 (observation / transaction / cancel / reset / close
//! dispositions), §6.5 (session-scoped handle registration) and §7.6
//! (`cancelExecution`).
//!
//! # The fabrication ban
//!
//! The one rule that shapes this whole module: **an unknown observation stays
//! unknown**. A provider that cannot read the session context must return
//! [`ObservationConfidence::Unknown`] with [`NamespaceTarget::empty`], and it
//! must not backfill the acquisition target, the last requested target, or
//! anything else the provider happens to know (`connection-management.md`
//! §5.1, §7.2). [`SessionContext::unobserved`] is the constructor that makes
//! that the path of least resistance.

use serde::{Deserialize, Serialize};

use crate::namespace::NamespaceTarget;
use crate::{QueryExecutionId, StatementResult};

/// How much the driver was actually able to confirm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObservationConfidence {
    Confirmed,
    Partial,
    Unknown,
}

impl ObservationConfidence {
    /// Only `Confirmed` may be presented to a user as a fact.
    pub fn is_confirmed(self) -> bool {
        matches!(self, Self::Confirmed)
    }
}

impl Default for ObservationConfidence {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Lifecycle of one namespace level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NamespaceLevelKind {
    Database,
    Catalog,
    Schema,
}

/// Transaction state as read back from the same physical resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransactionState {
    /// No transaction is open. Named `Idle` rather than `None` so it cannot be
    /// confused with "field absent".
    Idle,
    Active,
    Aborted,
    /// There may be a transaction and the driver cannot tell. Treated
    /// conservatively: a close must not assume a rollback happened.
    Unknown,
    /// The driver cannot report transaction state at all.
    Unsupported,
}

impl TransactionState {
    /// True only when the driver affirmatively reports no open transaction.
    pub fn is_confirmed_idle(self) -> bool {
        matches!(self, Self::Idle)
    }

    /// True when a close must resolve the transaction before proceeding.
    pub fn requires_resolution(self) -> bool {
        matches!(self, Self::Active | Self::Aborted | Self::Unknown)
    }
}

impl Default for TransactionState {
    fn default() -> Self {
        Self::Unsupported
    }
}

/// What actually happened to the effects of an execution or a transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EffectOutcome {
    NotStarted,
    Completed,
    RolledBack,
    PartiallyApplied,
    /// Effects happened but the driver cannot say which.
    Unknown,
}

impl EffectOutcome {
    /// `Unknown` must never be reported as `Completed` (plan line 99).
    pub fn is_certain(self) -> bool {
        !matches!(self, Self::Unknown | Self::NotStarted)
    }
}

impl Default for EffectOutcome {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Lifecycle of one execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionState {
    Queued,
    Running,
    /// A precise cancel has been delivered and accepted, but the execution has
    /// not reached a terminal state yet.
    CancelRequested,
    Succeeded,
    Failed,
    Cancelled,
}

impl ExecutionState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

/// Machine-readable reason an execution did not succeed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionErrorCode {
    SqlError,
    ProtocolError,
    Cancelled,
    Timeout,
    ResourceLost,
    PipelineAborted,
    HostRejected,
}

/// Terminal result of one statement inside an execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompletionStatus {
    Succeeded,
    Failed {
        code: ExecutionErrorCode,
    },
    /// The execution reached a terminal state as a result of a cancel. Only a
    /// terminal event may set this; a cancel *request* must not.
    Cancelled {
        code: ExecutionErrorCode,
    },
}

impl CompletionStatus {
    pub fn is_success(self) -> bool {
        matches!(self, Self::Succeeded)
    }
}

/// Resource liveness as observed on the resource itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResourceHealth {
    Healthy,
    Degraded,
    /// The physical resource is gone; every handle on it is dead.
    Lost,
    Unknown,
}

impl Default for ResourceHealth {
    fn default() -> Self {
        Self::Unknown
    }
}

/// The session context as read back from the physical resource.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContext {
    pub namespace: NamespaceTarget,
    /// Effective `search_path`, when the driver can report it.
    pub search_path: Vec<String>,
    /// The identity the server actually authenticated, when reportable.
    pub effective_identity: Option<String>,
    pub transaction_state: TransactionState,
    /// `None` means "the driver cannot tell", which is different from `false`.
    pub autocommit: Option<bool>,
    pub confidence: ObservationConfidence,
}

impl SessionContext {
    /// A context in which nothing was observed. This is what a provider must
    /// return when it cannot read the session state; it deliberately carries no
    /// namespace so it can never be mistaken for the acquisition target.
    pub fn unobserved() -> Self {
        Self {
            namespace: NamespaceTarget::empty(),
            search_path: Vec::new(),
            effective_identity: None,
            transaction_state: TransactionState::Unknown,
            autocommit: None,
            confidence: ObservationConfidence::Unknown,
        }
    }

    /// True when this observation agrees with `target`.
    ///
    /// An unobserved context never agrees with anything, which is how
    /// "don't fake a confirmation with `initialTarget`" becomes checkable.
    pub fn matches_target(&self, target: &NamespaceTarget) -> bool {
        self.confidence.is_confirmed() && self.namespace == *target
    }
}

/// A resource-scoped handle the driver handed out and is tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionHandleKind {
    Transaction,
    Cursor,
    ServerPrepared,
}

/// Reference to a session-scoped handle.
///
/// Handles are registered before their execution reaches a terminal state and
/// are never persisted (`connection-management.md` §6.5). `runtime_epoch` lets
/// the provider reject a handle that belongs to a previous incarnation of the
/// same resource id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHandleRef {
    pub handle_id: String,
    pub kind: SessionHandleKind,
    pub resource_id: String,
    pub runtime_epoch: u64,
    pub closed: bool,
}

impl SessionHandleRef {
    pub fn new(
        handle_id: impl Into<String>,
        kind: SessionHandleKind,
        resource_id: impl Into<String>,
        runtime_epoch: u64,
    ) -> Self {
        Self {
            handle_id: handle_id.into(),
            kind,
            resource_id: resource_id.into(),
            runtime_epoch,
            closed: false,
        }
    }

    /// True when the handle belongs to `resource_id` at `epoch`.
    pub fn belongs_to(&self, resource_id: &str, epoch: u64) -> bool {
        self.resource_id == resource_id && self.runtime_epoch == epoch
    }
}

/// Transaction state plus the effect of the operation that produced it.
///
/// `effect` is `None` for `begin` (nothing has been applied yet) and carries
/// [`EffectOutcome::Unknown`] whenever the result of a commit or rollback
/// could not be confirmed — never a borrowed `Completed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionObservation {
    pub state: TransactionState,
    pub transaction_id: Option<String>,
    pub effect: Option<EffectOutcome>,
    pub revision: u64,
}

impl TransactionObservation {
    /// A begin observation with no effect yet.
    pub fn begun(transaction_id: impl Into<String>, revision: u64) -> Self {
        Self {
            state: TransactionState::Active,
            transaction_id: Some(transaction_id.into()),
            effect: None,
            revision,
        }
    }

    /// A terminal transaction observation. Passing [`EffectOutcome::Unknown`]
    /// is the supported way to say "the commit outcome is not knowable".
    pub fn finished(state: TransactionState, effect: EffectOutcome, revision: u64) -> Self {
        Self {
            state,
            transaction_id: None,
            effect: Some(effect),
            revision,
        }
    }

    /// True when the provider asserts the transaction is committed.
    pub fn committed(&self) -> bool {
        self.effect == Some(EffectOutcome::Completed) && self.state == TransactionState::Idle
    }
}

/// Options a caller may request when opening a transaction.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionOptions {
    /// Spelled the way the driver spells it; validated against
    /// `CapabilitySet::transactions.isolation_levels`.
    pub isolation_level: Option<String>,
    pub read_only: Option<bool>,
    /// When true the provider must not roll back on error, so the caller keeps
    /// ownership of the decision.
    pub defers_commit: bool,
}

/// Result of `observeSession`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionObservation {
    pub state: SessionState,
    pub context: SessionContext,
    pub transaction: TransactionObservation,
    /// Truthful handle list — empty when the driver issued none. A driver must
    /// never report handles it did not register (`connection-management.md`
    /// §6.5).
    pub handles: Vec<SessionHandleRef>,
    pub protocol_drained: bool,
    pub resource_health: ResourceHealth,
    pub context_revision: u64,
}

impl SessionObservation {
    /// The observation returned by a provider that can observe nothing. Every
    /// field is `Unknown`/empty and no target is invented.
    pub fn unobservable() -> Self {
        Self {
            state: SessionState::Unknown,
            context: SessionContext::unobserved(),
            transaction: TransactionObservation {
                state: TransactionState::Unsupported,
                transaction_id: None,
                effect: None,
                revision: 0,
            },
            handles: Vec::new(),
            protocol_drained: false,
            resource_health: ResourceHealth::Unknown,
            context_revision: 0,
        }
    }
}

/// Lifecycle of the session bound to a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionState {
    New,
    Opening,
    Ready,
    Executing,
    Reconfiguring,
    Closing,
    Closed,
    Lost,
    /// The driver cannot place the session in the state machine.
    Unknown,
}

impl Default for SessionState {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Outcome of `changeContext`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContextChangeDisposition {
    /// The same physical session now serves the new target.
    Confirmed,
    /// The switch is possible but needs a different resource; the caller must
    /// acquire one. The current resource is left untouched.
    RequiresReplacement,
    /// The driver cannot switch namespace at all.
    Unsupported,
}

impl ContextChangeDisposition {
    /// True only when the change is confirmed on the *same* resource.
    pub fn changed_in_place(self) -> bool {
        matches!(self, Self::Confirmed)
    }
}

/// Outcome of `requestCancel`.
///
/// The three outcomes a caller must be able to tell apart — plus the
/// fail-closed case — are separate variants. There is deliberately no
/// catch-all success: a cancel that was not addressed to a registered
/// execution can never come back as `Requested`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CancelDisposition {
    /// 已取消 — the driver accepted a precise cancel for this execution. This
    /// is a *request*, not a result: the execution only becomes `Cancelled`
    /// when a terminal event says so.
    Requested,
    /// 不在取消集合 — the execution id is not in this resource's cancel set
    /// (never registered, belongs to another resource, or a stale epoch).
    NotRegistered,
    /// 已完成不可取消 — the execution already reached a terminal state. The
    /// cancel handle is not reactivated.
    AlreadyFinished,
    /// The driver cannot cancel one execution. Must not fall back to a
    /// session-wide cancel.
    Unsupported,
}

impl CancelDisposition {
    /// True only for a cancel that was actually accepted.
    pub fn is_accepted(self) -> bool {
        matches!(self, Self::Requested)
    }
}

/// Receipt returned by `requestCancel`
/// (`connection-management.md` §7.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelReceipt {
    pub execution_id: QueryExecutionId,
    pub disposition: CancelDisposition,
    /// `Some(CancelRequested)` when accepted; the execution's unchanged state
    /// otherwise. `None` means the provider does not track this execution's
    /// state — it must never substitute a plausible one
    /// (`connection-management.md` §7.6).
    pub state: Option<ExecutionState>,
}

/// Outcome of `resetResource`. There is no third value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResetDisposition {
    /// The resource is back at its initialization baseline and may be reused.
    Clean,
    /// Reset could not be proven; the resource must not be handed out again
    /// without a full re-acquire.
    Discard,
}

/// Outcome of `closeResource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloseDisposition {
    /// The physical resource is gone. Budget release may be accounted.
    Closed,
    /// The close was requested but not confirmed. Budget must **not** be
    /// reported as recovered; the resource goes to verification/quarantine.
    CloseUnconfirmed,
}

/// Everything an execution produced, read back from the resource that ran it.
///
/// No `PartialEq`: `StatementResult` does not derive it, and a comparison of
/// two completions is not a contract anybody should depend on.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionCompletion {
    pub completion_status: CompletionStatus,
    pub effect_outcome: EffectOutcome,
    pub statement_results: Vec<StatementResult>,
    pub context_before: SessionContext,
    pub context_after: SessionContext,
    pub transaction_observation: TransactionObservation,
    /// Truthful handle list, empty when none were issued.
    pub session_handles: Vec<SessionHandleRef>,
    /// True once the protocol layer has finished draining, so the connection
    /// is not read again.
    pub protocol_drained: bool,
    pub resource_health: ResourceHealth,
}

impl ExecutionCompletion {
    /// A completion for an execution that never started. Used by providers
    /// that reject work before touching the wire. Every observation stays
    /// unknown: nothing was read, so nothing is reported.
    pub fn not_started() -> Self {
        Self {
            completion_status: CompletionStatus::Failed {
                code: ExecutionErrorCode::HostRejected,
            },
            effect_outcome: EffectOutcome::NotStarted,
            statement_results: Vec::new(),
            context_before: SessionContext::unobserved(),
            context_after: SessionContext::unobserved(),
            transaction_observation: TransactionObservation {
                state: TransactionState::Unknown,
                transaction_id: None,
                effect: Some(EffectOutcome::NotStarted),
                revision: 0,
            },
            session_handles: Vec::new(),
            protocol_drained: true,
            resource_health: ResourceHealth::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::NamespaceTarget;

    fn confirmed_context(database: &str) -> SessionContext {
        let mut namespace = NamespaceTarget::empty();
        namespace.database = Some(database.to_string());
        SessionContext {
            namespace,
            search_path: vec!["public".to_string()],
            effective_identity: Some("app".to_string()),
            transaction_state: TransactionState::Idle,
            autocommit: Some(true),
            confidence: ObservationConfidence::Confirmed,
        }
    }

    #[test]
    fn unobserved_context_does_not_match_any_target() {
        let context = SessionContext::unobserved();
        let mut target = NamespaceTarget::empty();
        target.database = Some("main".to_string());
        assert!(!context.matches_target(&target));
        assert_eq!(context.confidence, ObservationConfidence::Unknown);
        assert!(context.namespace.database.is_none());
    }

    #[test]
    fn partial_observation_is_not_a_confirmation() {
        let mut context = confirmed_context("main");
        context.confidence = ObservationConfidence::Partial;
        let mut target = NamespaceTarget::empty();
        target.database = Some("main".to_string());
        assert!(!context.matches_target(&target));
    }

    #[test]
    fn confirmed_context_matches_only_the_observed_target() {
        let mut target = NamespaceTarget::empty();
        target.database = Some("main".to_string());
        assert!(confirmed_context("main").matches_target(&target));

        let mut other = NamespaceTarget::empty();
        other.database = Some("other".to_string());
        assert!(!confirmed_context("main").matches_target(&other));
    }

    #[test]
    fn unknown_transaction_state_requires_resolution() {
        assert!(TransactionState::Unknown.requires_resolution());
        assert!(TransactionState::Active.requires_resolution());
        assert!(!TransactionState::Idle.requires_resolution());
        assert!(!TransactionState::Unsupported.requires_resolution());
    }

    #[test]
    fn commit_uncertainty_is_reported_as_unknown_not_completed() {
        let observation =
            TransactionObservation::finished(TransactionState::Unknown, EffectOutcome::Unknown, 3);
        assert!(!observation.committed());
        assert!(!observation.effect.expect("effect present").is_certain());
    }

    #[test]
    fn only_confirmed_commit_reports_committed() {
        let observation =
            TransactionObservation::finished(TransactionState::Idle, EffectOutcome::Completed, 4);
        assert!(observation.committed());
    }

    #[test]
    fn begun_transaction_has_no_effect_yet() {
        let observation = TransactionObservation::begun("tx-1", 1);
        assert_eq!(observation.state, TransactionState::Active);
        assert!(observation.effect.is_none());
        assert!(!observation.committed());
    }

    #[test]
    fn cancel_dispositions_are_distinguishable() {
        assert!(CancelDisposition::Requested.is_accepted());
        assert!(!CancelDisposition::NotRegistered.is_accepted());
        assert!(!CancelDisposition::AlreadyFinished.is_accepted());
        assert!(!CancelDisposition::Unsupported.is_accepted());

        let all = [
            CancelDisposition::Requested,
            CancelDisposition::NotRegistered,
            CancelDisposition::AlreadyFinished,
            CancelDisposition::Unsupported,
        ];
        let mut wire: Vec<String> = all
            .iter()
            .map(|d| serde_json::to_string(d).expect("disposition serializes"))
            .collect();
        wire.sort();
        wire.dedup();
        assert_eq!(
            wire,
            vec![
                "\"alreadyFinished\"",
                "\"notRegistered\"",
                "\"requested\"",
                "\"unsupported\""
            ],
            "each disposition is its own wire value, not a catch-all success"
        );
    }

    #[test]
    fn cancel_receipt_defaults_to_cancel_requested_when_accepted() {
        let receipt = CancelReceipt {
            execution_id: QueryExecutionId::new("exec-1"),
            disposition: CancelDisposition::Requested,
            state: Some(ExecutionState::CancelRequested),
        };
        assert!(receipt.disposition.is_accepted());
        assert!(!receipt.state.is_some_and(ExecutionState::is_terminal));
    }

    #[test]
    fn a_rejected_cancel_reports_no_state_rather_than_an_invented_one() {
        for disposition in [
            CancelDisposition::NotRegistered,
            CancelDisposition::AlreadyFinished,
            CancelDisposition::Unsupported,
        ] {
            let receipt = CancelReceipt {
                execution_id: QueryExecutionId::new("exec-2"),
                disposition,
                state: None,
            };
            let json = serde_json::to_value(&receipt).expect("receipt serializes");
            assert!(json["state"].is_null(), "unknown state stays null");
        }
    }

    #[test]
    fn terminal_execution_state_is_distinguishable_from_cancel_requested() {
        assert!(!ExecutionState::CancelRequested.is_terminal());
        assert!(!ExecutionState::Running.is_terminal());
        assert!(ExecutionState::Cancelled.is_terminal());
        assert!(ExecutionState::Succeeded.is_terminal());
        assert!(ExecutionState::Failed.is_terminal());
    }

    #[test]
    fn reset_has_only_clean_or_discard() {
        let _clean: ResetDisposition = serde_json::from_str("\"clean\"").expect("clean parses");
        let _discard: ResetDisposition =
            serde_json::from_str("\"discard\"").expect("discard parses");
        // No third value serializes into the enum.
        assert!(serde_json::from_str::<ResetDisposition>("\"success\"").is_err());
        assert!(serde_json::from_str::<ResetDisposition>("\"ok\"").is_err());
    }

    #[test]
    fn close_unconfirmed_is_not_closed() {
        assert_ne!(CloseDisposition::Closed, CloseDisposition::CloseUnconfirmed);
    }

    #[test]
    fn context_change_requires_replacement_is_not_in_place() {
        assert!(ContextChangeDisposition::Confirmed.changed_in_place());
        assert!(!ContextChangeDisposition::RequiresReplacement.changed_in_place());
        assert!(!ContextChangeDisposition::Unsupported.changed_in_place());
    }

    #[test]
    fn handle_ownership_checks_resource_and_epoch() {
        let handle = SessionHandleRef::new("cursor-1", SessionHandleKind::Cursor, "res-1", 9);
        assert!(handle.belongs_to("res-1", 9));
        assert!(!handle.belongs_to("res-1", 10));
        assert!(!handle.belongs_to("res-2", 9));
    }

    #[test]
    fn unobservable_observation_invents_nothing() {
        let observation = SessionObservation::unobservable();
        assert_eq!(observation.state, SessionState::Unknown);
        assert_eq!(observation.resource_health, ResourceHealth::Unknown);
        assert_eq!(observation.transaction.state, TransactionState::Unsupported);
        assert!(observation.handles.is_empty());
        assert!(!observation.protocol_drained);
        assert_eq!(
            observation.context.confidence,
            ObservationConfidence::Unknown
        );
        assert!(observation.context.namespace.database.is_none());
        assert!(observation.context.autocommit.is_none());
    }

    #[test]
    fn not_started_completion_reports_no_effects() {
        let completion = ExecutionCompletion::not_started();
        assert_eq!(completion.effect_outcome, EffectOutcome::NotStarted);
        assert!(!completion.completion_status.is_success());
        assert!(completion.statement_results.is_empty());
        assert!(completion.session_handles.is_empty());
        assert_eq!(
            completion.context_before.confidence,
            ObservationConfidence::Unknown
        );
        assert_eq!(
            completion.context_after.confidence,
            ObservationConfidence::Unknown
        );
    }
}

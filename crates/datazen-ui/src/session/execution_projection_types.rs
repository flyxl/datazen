//! DTO mirrors, error/effect contracts and counter helpers for the
//! `ExecutionProjection` port. Split out of `execution_projection.rs` to keep
//! each source file under the 800-line ceiling (same shape as T083's types
//! module). All removal stages are recorded per item: the consumer is the
//! query-panel result view, which has not landed yet.

use std::cmp::Ordering;

use serde_json::Value;

use crate::session::counters::{counter_value, Counter, CounterError, CounterInput};
// Re-export the reused DTOs so one `use ...execution_projection::*` serves the
// tests and the future result view (they stay T083's types, not duplicates).
pub(crate) use crate::session::counters::SessionHandle;
pub(crate) use crate::session::editor_session_controller::{
    ExecutionErrorCode, ExecutionReceipt, ExecutionState, ExecutionTarget, NamespaceTarget,
    SessionView,
};

/// Fixed read boundaries mirrored from `targetCommandPhase.ts` (only the two
/// the projection itself emits). Recorded for host observability.
#[allow(dead_code)] // diagnostics; remove with the host observer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReadPhase {
    ExecutionView,
    ArtifactRead,
}

/// The projection's own error projection. React throws plain `Error`s; the
/// three fixed messages are kept verbatim via [`ProjectionError::message`]
/// and host failures ride in [`ProjectionError::Host`].
#[allow(dead_code)] // error surface; remove with the query-panel result view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ProjectionError {
    /// "Artifact read returned a different chunk"
    ArtifactIdentityMismatch,
    /// "Execution identity mismatch"
    ExecutionIdentityMismatch,
    /// "Artifact publication metadata unavailable"
    PublicationMetadataUnavailable,
    /// A wire counter the React `counterValue` would refuse.
    Counter(CounterError),
    /// Any host-side rejection (e.g. the test's `disconnected`).
    Host(String),
}

impl ProjectionError {
    pub(crate) fn message(&self) -> String {
        match self {
            Self::ArtifactIdentityMismatch => "Artifact read returned a different chunk".into(),
            Self::ExecutionIdentityMismatch => "Execution identity mismatch".into(),
            Self::PublicationMetadataUnavailable => "Artifact publication metadata unavailable".into(),
            Self::Counter(e) => e.as_str().to_owned(),
            Self::Host(message) => message.clone(),
        }
    }
}

// --- Minimal backend-client DTO mirrors -------------------------------------

/// `EffectOutcome` — only the values an execution view can report.
#[allow(dead_code)] // DTO mirror; remove with the query-panel result view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EffectOutcome {
    NotStarted,
    Completed,
    RolledBack,
    PartiallyApplied,
    Unknown,
}

/// `ResultCompleteness` from the backend-client contract.
#[allow(dead_code)] // DTO mirror; remove with the query-panel result view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResultCompleteness {
    Pending,
    Complete,
    Truncated,
}

/// `StatementResultSource.writableMapping`.
#[allow(dead_code)] // DTO mirror; remove with the query-panel result view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WritableMapping {
    Verified,
    ReadOnly,
}

/// `RuntimeResultBinding` — the writable binding validated by the projection.
#[allow(dead_code)] // DTO mirror; remove with the query-panel result view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RuntimeResultBinding {
    pub handle: SessionHandle,
    pub resource_binding_id: String,
    pub execution_id: String,
}

/// Minimal `ResultProvenance`: the projection never reads it, but a caller
/// reads `requestedTarget.namespace.database` off the published view.
#[allow(dead_code)] // DTO mirror; remove with the query-panel result view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResultProvenance {
    pub requested_target: ExecutionTarget,
}

/// The full `ExecutionView` the projection publishes (T083 only carried
/// `execution_id` + `error_code`).
#[allow(dead_code)] // DTO mirror; remove with the query-panel result view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExecutionView {
    pub execution_id: String,
    pub state: ExecutionState,
    pub effect_outcome: EffectOutcome,
    pub provenance: Option<ResultProvenance>,
    pub artifact_ids: Vec<String>,
    pub result_completeness: ResultCompleteness,
    pub truncation_reason: Option<String>,
    pub error_code: Option<ExecutionErrorCode>,
    pub runtime_binding: Option<RuntimeResultBinding>,
}

/// Minimal `StatementResultSource`: context/relation are carried by the wire
/// DTO but never read by this projection, so they are intentionally omitted.
#[allow(dead_code)] // DTO mirror; remove with the query-panel result view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StatementResultSource {
    pub execution_id: String,
    pub statement_index: serde_json::Number,
    pub writable_mapping: WritableMapping,
}

/// `ArtifactChunk` — wire counters stay decimal strings (`Counter`), bytes are
/// raw result bytes (never a path or credential).
#[allow(dead_code)] // DTO mirror; remove with the query-panel result view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ArtifactChunk {
    pub artifact_id: String,
    pub chunk_index: Counter,
    pub total_chunks: Option<Counter>,
    pub published_chunk_count: Option<Counter>,
    pub published_byte_length: Option<Counter>,
    pub offset: Counter,
    pub bytes: Vec<u8>,
    pub result_completeness: ResultCompleteness,
}

/// One `ConnectionEvent` payload; mirrors the four React variants.
#[allow(dead_code)] // DTO mirror; remove with the query-panel result view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ConnectionEvent {
    SessionChanged { session: SessionView },
    ExecutionChanged { execution: ExecutionView },
    ResultChunk { artifact_id: String, chunk_index: Counter, source: StatementResultSource },
    StreamResetRequired { reason: String },
}

/// `EventEnvelope<ConnectionEvent>`.
#[allow(dead_code)] // DTO mirror; remove with the query-panel result view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EventEnvelope {
    pub stream_id: String,
    pub sequence: Counter,
    pub runtime_epoch: String,
    pub execution_id: Option<String>,
    pub job_id: Option<String>,
    pub session_handle: Option<SessionHandle>,
    pub context_revision: Option<Counter>,
    pub payload: ConnectionEvent,
}

/// A `resultChunk` after `published()` decoding. `payload` is the parsed JSON
/// (`Null` for an opaque/binary chunk); `source` may be adopted from an
/// embedded `source` object.
#[allow(dead_code)] // DTO mirror; remove with the query-panel result view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PublishedResultChunk {
    pub chunk: ArtifactChunk,
    pub source: Option<StatementResultSource>,
    pub payload: Value,
}

// --- Immediate-mode I/O ------------------------------------------------------

/// Work the host must perform. The host answers with the matching `on_*` feed.
#[allow(dead_code)] // emitted work; remove when the result view wires the port.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ProjectionEffect {
    GetExecution { call_id: u64, execution_id: String },
    ReadArtifact { call_id: u64, artifact_id: String, chunk_index: Counter },
    SubscribeEvents { call_id: u64, stream_id: String, after_sequence: Option<Counter> },
    CloseStream { call_id: u64, stream_call_id: u64 },
}

/// One delivery from the async iterator, supplied by the host.
#[allow(dead_code)] // host feed; remove when the result view wires the port.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum StreamItem {
    Event(EventEnvelope),
    Done,
    Error(ProjectionError),
}

// --- Counter / terminal helpers ---------------------------------------------

/// Canonical decimal form of a wire counter (the React `bigint`).
pub(crate) fn canonical(value: &str) -> Result<String, ProjectionError> {
    counter_value(CounterInput::Text(value.to_owned()))
        .map(|v| v.as_str().to_owned())
        .map_err(ProjectionError::Counter)
}

pub(crate) fn compare(a: &str, b: &str) -> Result<Ordering, ProjectionError> {
    // Compare the `BigCounter` values, whose `Ord` is numeric (length first,
    // then digits) — comparing their decimal strings would order "10" before
    // "2" and break the chunk sort / gap dedupe.
    let a = counter_value(CounterInput::Text(a.to_owned())).map_err(ProjectionError::Counter)?;
    let b = counter_value(CounterInput::Text(b.to_owned())).map_err(ProjectionError::Counter)?;
    Ok(a.cmp(&b))
}

/// Arbitrary-precision decimal increment (`+1n`) without a bigint crate.
pub(crate) fn increment(decimal: &str) -> String {
    let mut digits = decimal.as_bytes().to_vec();
    let mut carry = 1u8;
    for byte in digits.iter_mut().rev() {
        if carry == 0 {
            break;
        }
        let sum = *byte - b'0' + carry;
        if sum == 10 {
            *byte = b'0';
        } else {
            *byte = b'0' + sum;
            carry = 0;
        }
    }
    if carry != 0 {
        digits.insert(0, b'1');
    }
    // Strip leading zeros the way `BigInt` normalises.
    let start = digits.iter().position(|b| *b != b'0').unwrap_or(digits.len() - 1);
    String::from_utf8_lossy(&digits[start..]).into_owned()
}

pub(crate) fn is_terminal(state: ExecutionState) -> bool {
    matches!(
        state,
        ExecutionState::Succeeded | ExecutionState::Failed | ExecutionState::Cancelled
    )
}

pub(crate) fn with_binding_null(mut view: ExecutionView) -> ExecutionView {
    view.runtime_binding = None;
    view
}

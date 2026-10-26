//! Execution domain: session/execution state machines, targets, ownership,
//! and the artifact lifecycle vocabulary (§5.1 / §5.3, D3).
//!
//! Moved here in D3 (2026-10-26) from `datazen-platform-api` (`dto/session.rs`,
//! `dto/execution.rs`, `dto/artifact.rs`, `target.rs` 的执行侧); platform-api
//! re-exports the same types until its retirement in D8. Layering: domain
//! depends on its own types, `shared` values, and the connection-side
//! namespace vocabulary — never on application, hosts, or the legacy crates.

pub mod artifact;
pub mod execution;
pub mod session;
pub mod target;

pub use artifact::{
    AbortReason, ArtifactChunk, ArtifactLifecycle, ArtifactMetadata, ArtifactProvenance,
    ArtifactSummary,
};
pub use execution::{
    CancelOutcome, CapabilitySnapshot, EffectOutcome, ExecutionErrorCode, ExecutionState,
    ResultCompleteness, ResultProvenance, StatementResultSource, WritableMapping,
};
pub use session::{
    AttachmentState, CancelDisposition, ContextConfidence, ResourceRelease, SessionContext,
    SessionHandle, SessionState, TransactionState,
};
pub use target::{ExecutionTarget, ObjectTarget};

/// 执行侧拥有的 shared ID（§5.1：唯一定义在 `shared::id`，领域再导出同一类型）。
pub use crate::shared::id::{DbSessionId, ExecutionId, ResourceId, RuntimeEpoch, StreamId};

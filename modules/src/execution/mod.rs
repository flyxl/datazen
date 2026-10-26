//! Execution vertical slice: session/execution state machines, targets,
//! artifacts, and the service-boundary DTOs (§5.1 / §5.3, D3).
//!
//! D3 vocabulary slice (2026-10-26): the execution-side vocabulary moved from
//! `datazen-platform-api`; the mechanisms (session actors, leases, queues,
//! waiting, reclamation, budget ledger, driver registry) and the application
//! orchestration (`RuntimeConnectionUseCases` split) follow in later D3
//! slices. platform-api re-exports every type below until D8.

pub mod application;
pub mod domain;

pub use application::{
    ArchivedEnvelope, BackupOutput, CancelReceipt, CloseReceipt, ConnectionEvent,
    ConnectionEventEnvelope, ContextChangeReceipt, EventEnvelope, EventSequence,
    ExecutionReceipt, ExecutionView, IdempotentOperation, OpenSessionReceipt, ResultChunk,
    ResultChunkEvent, RuntimeResultBinding, SessionView, SubmissionToken,
};
pub use domain::{
    AbortReason, ArtifactChunk, ArtifactLifecycle, ArtifactMetadata, ArtifactProvenance,
    ArtifactSummary, AttachmentState, CancelDisposition, CancelOutcome, CapabilitySnapshot,
    ContextConfidence, DbSessionId, EffectOutcome, ExecutionErrorCode, ExecutionId,
    ExecutionState, ExecutionTarget, ObjectTarget, ResourceId, ResourceRelease, ResultCompleteness,
    ResultProvenance, RuntimeEpoch, SessionContext, SessionHandle, SessionState, StatementResultSource,
    StreamId, TransactionState, WritableMapping,
};

/// 应用层请求 DTO（`CommandCall` / `OpenSessionRequest` / …）经
/// [`application::requests`] 提供；再导出便于宿主一站式导入。
pub use application::requests;

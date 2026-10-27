//! Execution vertical slice: session/execution state machines, targets,
//! artifacts, and the service-boundary DTOs (§5.1 / §5.3, D3).
//!
//! D3 vocabulary slice (2026-10-26): the execution-side vocabulary moved from
//! `datazen-modules::platform_api`; the mechanisms (session actors, leases, queues,
//! waiting, reclamation, budget ledger, driver registry) and the application
//! orchestration (`RuntimeConnectionUseCases` split) follow in later D3
//! slices. platform-api re-exports every type below until D8.

pub mod application;
pub mod domain;
/// 私有机制（过渡期公开供旧包再导出；见模块头说明）。
pub mod runtime;
pub mod target;

pub use application::{
    endpoint_scheme, EventSink, EventSubscription, ExecutionIdentity, ExecutionPolicy,
    ExecutionSettingsReader, ExecutionTargetResolver, IdentityResolver, NetworkProvider, RoutePlan,
    SubmissionPresentation, SubmissionTokenIssuer, TargetResolution, TunnelBinding, TunnelSpec,
    VerifiedSubmission,
};
pub use application::{
    ArchivedEnvelope, BackupOutput, CancelReceipt, CloseReceipt, ConnectionEvent,
    ConnectionEventEnvelope, ContextChangeReceipt, EventEnvelope, EventSequence, ExecutionReceipt,
    ExecutionView, IdempotentOperation, OpenSessionReceipt, ResultChunk, ResultChunkEvent,
    RuntimeResultBinding, SessionView, SubmissionToken,
};
pub use domain::{
    has_active_execution, owner_matches_client, owner_matches_job, provenance_fingerprint,
    AbortReason, ArtifactChunk, ArtifactLifecycle, ArtifactMetadata, ArtifactProvenance,
    ArtifactRepository, ArtifactSpec, ArtifactSummary, ArtifactWriter, AttachmentState, ByteRange,
    CancelDisposition, CancelOutcome, CanonicalNamespace, CanonicalNamespaceId, CanonicalTarget,
    CapabilitySnapshot, ChunkIndex, ChunkPayload, CloseDisposition, ContextConfidence, DbSessionId,
    EffectOutcome, ExecutionErrorCode, ExecutionId, ExecutionState, ExecutionTarget, ExportReceipt,
    ExportSink, InvalidationReason, ObjectTarget, ReplacementCommit, ReplacementOperation,
    ReplacementOutcome, ResourceId, ResourceRelease, ResultCompleteness, ResultProvenance,
    RevokeReason, RuntimeEpoch, SessionContext, SessionDirectory, SessionHandle, SessionOwner,
    SessionState, StatementResultSource, StreamId, TransactionState, WritableMapping,
    ARTIFACT_BYTE_QUOTA,
};

/// 应用层请求 DTO（`CommandCall` / `OpenSessionRequest` / …）经
/// [`application::requests`] 提供；再导出便于宿主一站式导入。
pub use application::requests;

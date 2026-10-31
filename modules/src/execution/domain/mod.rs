//! Execution domain: session/execution state machines, targets, ownership,
//! and the artifact lifecycle vocabulary (§5.1 / §5.3, D3).
//!
//! Moved here in D3 (2026-10-26) from `datazen::platform_api` (`dto/session.rs`,
//! `dto/execution.rs`, `dto/artifact.rs`, `target.rs` 的执行侧); platform-api
//! re-exports the same types until its retirement in D8. Layering: domain
//! depends on its own types, `shared` values, and the connection-side
//! namespace vocabulary — never on application, hosts, or the legacy crates.

pub mod artifact;
pub mod artifact_repository;
pub mod execution;
pub mod permission;
pub mod session;
pub mod session_directory;
pub mod sql_guard;
pub mod target;

pub use artifact::{
    AbortReason, ArtifactChunk, ArtifactLifecycle, ArtifactMetadata, ArtifactProvenance,
    ArtifactSummary,
};
pub use artifact_repository::{
    provenance_fingerprint, ArtifactRepository, ArtifactSpec, ArtifactWriter, ByteRange,
    ChunkIndex, ChunkPayload, ExportReceipt, ExportSink, RevokeReason, ARTIFACT_BYTE_QUOTA,
};
pub use execution::{
    CancelOutcome, CapabilitySnapshot, EffectOutcome, ExecutionErrorCode, ExecutionState,
    ResultCompleteness, ResultProvenance, StatementResultSource, WritableMapping,
};
// 权限模式词汇（M1 前置刀自 mcp::domain 归位）：它是持久化在应用设置里的
// wire 词汇，被 workflow（resource_execution 的 permission_mode）、ai
// （db_tools 的 SQL 分级）、table_browsing（目标注入策略）与 MCP server
// 共同消费——依赖方向上它是执行分级词汇，不是 MCP 专属。
pub use permission::{
    check_sql_allowed, check_tool_call, is_resource_listed, is_tool_listed,
    query_history_content_allowed, McpPermissionMode,
};
pub use session::{
    AttachmentState, CancelDisposition, ContextConfidence, ResourceRelease, SessionContext,
    SessionHandle, SessionState, TransactionState,
};
pub use session_directory::{
    has_active_execution, owner_matches_client, owner_matches_job, CloseDisposition,
    InvalidationReason, ReplacementCommit, ReplacementOperation, ReplacementOutcome,
    SessionDirectory, SessionOwner,
};
pub use target::{
    CanonicalNamespace, CanonicalNamespaceId, CanonicalTarget, ExecutionTarget, LayerRequirement,
    ObjectTarget, TargetCanonicalIdRules, TargetCanonicalizationError, TargetCaseFolding,
    TargetCaseRules, TargetNamespacePathSegment, TargetNamespaceShape, TargetRequirements,
};

/// 执行侧拥有的 shared ID（§5.1：唯一定义在 `shared::id`，领域再导出同一类型）。
pub use crate::shared::id::{DbSessionId, ExecutionId, ResourceId, RuntimeEpoch, StreamId};

// SQL Guard 纯规则（原 datazen-kernel::sql_guard，D3 归位 execution::domain）。
pub use sql_guard as guard;

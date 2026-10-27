//! 连接运行时（`connection`）。
//!
//! - [`types`]：newtype、目标、归属与池键指纹。
//! - [`session`]：会话状态机与句柄登记投影。
//! - [`execution`]：执行终态、结果与 sink。
//! - [`capability`]：能力快照与默认值。
//! - [`error`]：三套互不交织的拒绝面 —— `ApiError`（请求面）、`RuntimeError`（会话面）、
//!   `ProviderError`（provider 面）。
//! - [`port`]：资源级端口 —— fake provider 实现的就是这一层。
//! - `testing`：假资源夹具，仅在 `cfg(test)` 或 `feature = "test-harness"` 下存在。
//!
//! 依赖方向单向向下：`types` ← `session`/`execution`/`capability` ← `port` ← `testing`。

pub mod capability;
pub mod error;
pub mod execution;
pub mod port;
pub mod session;
pub mod types;

pub use error::{ApiError, ProviderError, RuntimeError};
pub use execution::{
    EffectOutcome, ExecutionErrorCode, ExecutionReceipt, ExecutionState, ResultCompleteness,
    ResultSink, SessionCommand, SinkWrite, StatementResult, StatementResultSource,
    TruncationReason, TruncationRecord,
};
pub use session::{
    AttachmentState, CloseMode, CommandCall, ContextConfidence, ExecuteAtTargetRequest,
    ExecuteInSessionRequest, HandleKind, SessionContext, SessionHandle, SessionHandleRef,
    SessionState, SessionView, TransactionState,
};
pub use types::{
    ConfigRevision, ConnectionId, Counter, DbSessionId, ExecutionId, ExecutionTarget, HandleId,
    JobId, LeaseId, NamespaceInput, NamespaceTarget, ObjectTarget, OrganizationId, OwnerRef,
    PoolKeyFingerprint, PoolKeyInputs, PrincipalId, ResourceId, StreamId, TargetNamespaceLayer,
    TargetNamespaceShape, Timestamp, WorkerId,
};

#[cfg(any(test, feature = "test-harness"))]
pub mod testing;

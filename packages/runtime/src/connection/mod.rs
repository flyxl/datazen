//! 连接运行时（`connection`）。
//!
//! - [`types`]：newtype、目标、归属与池键指纹（connection-management.md §4 的标识半部）。
//! - [`session`]：会话状态机与句柄登记投影（§4 的会话半部、§6.5）。
//! - [`execution`]：执行终态、结果与 sink（§4 的执行半部、§7、§13）。
//! - [`capability`]：能力快照与默认值（connection-management.md §5.2）。
//! - [`error`]：请求拒绝面 `ApiError` 与 provider 面 `ProviderError`（§13 的两个独立错误命名空间）。
//! - [`port`]：资源级端口 —— fake provider 实现的就是这一层（§5.1）。
//! - `testing`：假资源夹具，仅在 `cfg(test)` 或 `feature = "test-harness"` 下存在
//!   （fake-runtime-fixtures.md §2）。
//!
//! 依赖方向单向向下：`types` ← `session`/`execution`/`capability` ← `port` ← `testing`。

pub mod capability;
pub mod error;
pub mod execution;
pub mod port;
pub mod session;
pub mod types;

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

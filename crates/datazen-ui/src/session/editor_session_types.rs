//! DTO mirrors, emitted work and the host port for the editor-session state
//! machine. Split out of `editor_session_controller.rs` to keep each source
//! file under the 800-line ceiling.

use crate::session::counters::Counter;

/// `NamespaceTarget` — the database/catalog/schema resolution of a session.
#[allow(dead_code)] // DTO mirror; consumed by the host effect executor. Remove when a view drives the controller.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct NamespaceTarget {
    pub database: Option<String>,
    pub catalog: Option<String>,
    pub schema: Option<String>,
    pub path: Vec<String>,
}

/// `ObjectTarget` — a single addressable object inside a namespace.
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ObjectTarget {
    pub kind: String,
    pub name: String,
    pub signature: Option<String>,
}

/// `ExecutionTarget` — what an execution runs against (no `backendId`).
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionTarget {
    pub connection_id: String,
    pub namespace: NamespaceTarget,
    pub object: Option<ObjectTarget>,
}

/// `OwnerRef` — editor owners own a pane-local `editorSessionId`.
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnerRef {
    Editor { client_instance_id: String, editor_session_id: String },
    Job { job_id: String, stage_id: String },
    WorkflowBlock { job_id: String, block_id: String },
    ClientSession { client_instance_id: String, purpose: String },
}

impl OwnerRef {
    /// React `owner.kind === 'editor' && ... && owner.editorSessionId === ...`.
    pub fn same_editor_session(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (
                Self::Editor { editor_session_id: a, .. },
                Self::Editor { editor_session_id: b, .. }
            ) if a == b
        )
    }
}

/// `SessionState`. `refuses_reuse` is the `['lost','closed','closing']` gate
/// in `ensureSession`; `is_lost_or_closed` is the `openNewSession` gate.
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    New,
    Opening,
    Ready,
    Executing,
    Reconfiguring,
    Closing,
    Closed,
    Lost,
}

impl SessionState {
    pub fn refuses_reuse(self) -> bool {
        matches!(self, Self::Lost | Self::Closed | Self::Closing)
    }
    pub fn is_lost_or_closed(self) -> bool {
        matches!(self, Self::Lost | Self::Closed)
    }
    pub fn from_close(state: CloseState) -> Self {
        match state {
            CloseState::Closing => Self::Closing,
            CloseState::Closed => Self::Closed,
            CloseState::Lost => Self::Lost,
        }
    }
}

/// `ContextConfidence` of an observed session context.
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextConfidence {
    Confirmed,
    Partial,
    Unknown,
}

/// `TransactionState` of an observed session context.
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionState {
    None,
    Active,
    Aborted,
    Unknown,
    Unsupported,
}

/// `SessionContext` — the context an operation actually ran in.
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionContext {
    pub namespace: NamespaceTarget,
    pub search_path: Option<Vec<String>>,
    pub effective_identity: Option<String>,
    pub transaction_state: TransactionState,
    pub autocommit: Option<bool>,
    pub confidence: ContextConfidence,
}

/// `AttachmentState`.
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentState {
    Attached,
    Detached,
    Expired,
}

/// `SessionView` — the snapshot the controller publishes.
#[allow(dead_code)] // main observable state; remove when the view consumes it directly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionView {
    pub handle: SessionHandle,
    pub connection_id: String,
    pub config_revision: Counter,
    pub owner: OwnerRef,
    pub initial_target: ExecutionTarget,
    pub observed_context: SessionContext,
    pub context_revision: Counter,
    pub state: SessionState,
    pub attachment_state: AttachmentState,
    pub active_execution_id: Option<String>,
    pub expires_at: Option<i64>,
}

/// `ExecutionState` of an execution receipt/view.
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionState {
    Queued,
    Running,
    CancelRequested,
    Succeeded,
    Failed,
    Cancelled,
}

/// `ExecutionReceipt` — acceptance, not statement success.
#[allow(dead_code)] // active execution receipt; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionReceipt {
    pub execution_id: String,
    pub stream_id: String,
    pub state: ExecutionState,
}

/// `ExecutionErrorCode` — a namespace disjoint from `ApiError.code`.
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionErrorCode {
    SqlError,
    ProtocolError,
    Cancelled,
    Timeout,
    ResourceLost,
    PipelineAborted,
    HostRejected,
}

/// Minimal `ExecutionView`: `getExecution` only reads `errorCode`.
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionView {
    pub execution_id: String,
    pub error_code: Option<ExecutionErrorCode>,
}

/// `SubmissionToken` — the host-issued idempotency key every mutating call carries.
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmissionToken {
    pub idempotency_key: String,
    pub expires_at: i64,
}

/// `OpenSessionReceipt`.
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenSessionReceipt {
    pub session: SessionView,
    pub attachment_token: String,
}

/// `ContextChangeReceipt` — a non-null `replacedSessionId` means a replacement.
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextChangeReceipt {
    pub session: SessionView,
    pub replaced_session_id: Option<String>,
    pub attachment_token: Option<String>,
}

/// Terminal state carried by a `CloseReceipt`.
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseState {
    Closing,
    Closed,
    Lost,
}

/// `CloseReceipt` (only the fields the controller reads).
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloseReceipt {
    pub db_session_id: String,
    pub state: CloseState,
}

/// `CancelReceipt` (only the field the controller reads).
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CancelReceipt {
    pub execution_id: String,
}

/// `PlatformIdentity` — what `getPlatformIdentity` yields.
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformIdentity {
    pub client_instance_id: String,
}

/// `CommandCall` — opaque driver command (`input` stays a string here).
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandCall {
    pub command: String,
    pub input: String,
}

/// `CloseMode`.
#[allow(dead_code)] // DTO mirror; remove with the controller-driven view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseMode {
    RequireNoTransaction,
    RollbackAndClose,
}

/// Request structs (field-for-field with the `backend-client` request types).
#[allow(dead_code)] // request mirror; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenSessionRequest {
    pub initial_target: ExecutionTarget,
    pub owner: OwnerRef,
    pub idempotency_key: String,
}
#[allow(dead_code)] // request mirror; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecuteRequest {
    pub handle: SessionHandle,
    pub expected_context_revision: Counter,
    pub call: CommandCall,
    pub idempotency_key: String,
}
#[allow(dead_code)] // request mirror; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetContextRequest {
    pub handle: SessionHandle,
    pub expected_context_revision: Counter,
    pub desired: NamespaceTarget,
    pub idempotency_key: String,
}
#[allow(dead_code)] // request mirror; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenContext {
    pub connection_id: String,
    pub owner: OwnerRef,
}

/// `ApiError.code` — only the three session codes matter to
/// `acceptSessionError`; `Other` covers the rest verbatim.
#[allow(dead_code)] // error mirror; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiErrorCode {
    SessionLost,
    SessionNotFound,
    RuntimeEpochMismatch,
    Other(String),
}

impl ApiErrorCode {
    pub fn is_session_error(&self) -> bool {
        matches!(self, Self::SessionLost | Self::SessionNotFound | Self::RuntimeEpochMismatch)
    }
}

/// `ApiError` — the host maps a rejection into this shape before calling `on_*`.
#[allow(dead_code)] // error mirror; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionError {
    pub code: ApiErrorCode,
    pub message: String,
}

impl SessionError {
    #[allow(dead_code)] // mirrors the explicit `new ApiError('SessionLost', ...)` in ensureSession.
    pub fn session_lost(message: impl Into<String>) -> Self {
        Self { code: ApiErrorCode::SessionLost, message: message.into() }
    }
}

/// The host-injected client. Effects describe the call; the host executes it
/// against this port and reports the outcome with the matching `on_*` method.
#[allow(dead_code)] // host executor contract; remove when the app wires the port.
pub trait SessionPort {
    fn issue_submission_token(
        &self,
        operation: &str,
        handle: Option<&SessionHandle>,
        context: &TokenContext,
    ) -> Result<SubmissionToken, SessionError>;
    fn open_session(
        &self,
        request: &OpenSessionRequest,
        token: &SubmissionToken,
    ) -> Result<OpenSessionReceipt, SessionError>;
    fn get_execution(&self, execution_id: &str) -> Result<ExecutionView, SessionError>;
    fn execute_in_session(
        &self,
        request: &ExecuteRequest,
        token: &SubmissionToken,
    ) -> Result<ExecutionReceipt, SessionError>;
    fn set_session_context(
        &self,
        request: &SetContextRequest,
        token: &SubmissionToken,
    ) -> Result<ContextChangeReceipt, SessionError>;
    fn get_session(&self, handle: &SessionHandle) -> Result<SessionView, SessionError>;
    fn close_session(
        &self,
        handle: &SessionHandle,
        mode: CloseMode,
    ) -> Result<CloseReceipt, SessionError>;
    fn detach_session(
        &self,
        handle: &SessionHandle,
        attachment_token: &str,
    ) -> Result<SessionView, SessionError>;
    fn attach_session(
        &self,
        handle: &SessionHandle,
        attachment_token: &str,
    ) -> Result<SessionView, SessionError>;
    fn cancel_execution(
        &self,
        execution_id: &str,
        token: &SubmissionToken,
    ) -> Result<CancelReceipt, SessionError>;
    fn get_platform_identity(&self) -> Result<PlatformIdentity, SessionError>;
}

/// Why a `close` is being emitted; decides how its result is handled.
#[allow(dead_code)] // effect discriminator; remove with the host executor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClosePurpose {
    /// The queued `close()` operation: rebind session state and publish.
    Operation,
    /// `releaseReplacedSession`: idempotent cleanup, swallow every outcome.
    ReleaseReplaced,
    /// Page-unload closer: `rollbackAndClose`, swallow every outcome.
    PageUnload,
}

/// Work the host must perform. This is the immediate-mode stand-in for the
/// React injected async client calls plus the page-unload listeners.
#[allow(dead_code)] // emitted work; remove when the host executor lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionEffect {
    IssueToken { call_id: u64, operation: String, handle: Option<SessionHandle>, context: TokenContext },
    Open { call_id: u64, request: OpenSessionRequest, token: SubmissionToken },
    Execute { call_id: u64, request: ExecuteRequest, token: SubmissionToken },
    SetContext { call_id: u64, request: SetContextRequest, token: SubmissionToken },
    GetExecution { call_id: u64, execution_id: String },
    GetSession { call_id: u64, handle: SessionHandle },
    Close { call_id: u64, handle: SessionHandle, mode: CloseMode, purpose: ClosePurpose },
    Detach { call_id: u64, handle: SessionHandle, token: String },
    Attach { call_id: u64, handle: SessionHandle, token: String },
    Cancel { call_id: u64, execution_id: String, token: SubmissionToken },
    Identity { call_id: u64 },
    RegisterPageCloser { controller_id: u64 },
    UnregisterPageCloser { controller_id: u64 },
}

// Re-export the shared handle type so consumers need a single import path.
#[allow(unused_imports)]
pub use crate::session::counters::SessionHandle;

//! State machine ported from
//! `packages/frontend/src/lib/session/EditorSessionController.ts`.
//!
//! # One instance per editor / pane
//!
//! The runtime handle, its execution receipt and the attachment token never
//! leave this in-memory owner. Every field of the React class has an explicit
//! counterpart: `latestTarget`, `session`, the `opening` promise, the `tail`
//! serial queue, `attachmentToken`, `execution`, `listeners` and the one-shot
//! page-unload closer.
//!
//! # Immediate-mode adaptation (honest, no fake async)
//!
//! Rune has no promises and no `await`. The React injected async client calls
//! become emitted [`SessionEffect`]s; the host executes each against the
//! [`SessionPort`] trait and feeds the outcome back through the `on_*` methods,
//! exactly like `crate::ai::grant_state`. Nothing here blocks, spawns or
//! pretends a `Future` exists. `subscribe`/`unsubscribe` are kept for observers
//! that want change notifications; rendering reads [`Self::snapshot`] each
//! frame.
//!
//! # Serial queue
//!
//! React `serial` is `tail.then(op, op); tail = next.catch(() => undefined)`.
//! [`Self::advance_queue`] is that: the next operation always runs (even after
//! a rejection) and a failed operation never poisons the ones behind it.

use std::collections::{HashMap, VecDeque};

use super::counters::{counter_value, same_handle, CounterInput};

#[path = "editor_session_types.rs"]
pub(crate) mod types;

// Re-export so a single import path serves the tests and the future view; this
// also brings every DTO (including SessionHandle) into scope below.
pub(crate) use types::*;

type SessionListener = Box<dyn FnMut()>;

/// Result of the most recent queued operation, kept for host observability.
#[allow(dead_code)] // diagnostic surface; remove with the controller-driven view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OpResult {
    Session(SessionView),
    Execution(ExecutionReceipt),
    ExecutionView(ExecutionView),
    Close(CloseReceipt),
    Cancel(CancelReceipt),
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpenPhase {
    AwaitToken,
    AwaitOpen,
}

/// The `opening` promise, made explicit. `resume_op` records that a queued
/// operation is waiting for this open (React's shared promise).
struct Opening {
    call_id: u64,
    phase: OpenPhase,
    resume_op: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpKind {
    Execute,
    SetContext,
    Close,
    OpenNew,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    WaitOpening,
    NeedSession,
    IssueToken,
    Call,
    Release,
}

/// One in-flight queued operation, i.e. the async closure handed to `serial`.
struct Operation {
    kind: OpKind,
    phase: Phase,
    call_id: u64,
    session: Option<SessionView>,
    call: Option<CommandCall>,
    desired: Option<NamespaceTarget>,
    mode: CloseMode,
    previous_handle: Option<SessionHandle>,
}

impl Operation {
    fn new(kind: OpKind, call: Option<CommandCall>, desired: Option<NamespaceTarget>, mode: CloseMode) -> Self {
        let phase = match kind {
            OpKind::Execute | OpKind::SetContext => Phase::NeedSession,
            // Both await any in-flight `opening` first; close then acts on the
            // current session, openNew guards/clears it before reopening.
            OpKind::Close | OpKind::OpenNew => Phase::WaitOpening,
        };
        Self { kind, phase, call_id: 0, session: None, call, desired, mode, previous_handle: None }
    }
}

/// A non-queued injected call (`getExecution`/`refresh`/`detach`/`attach`/
/// `cancel` and the page-unload closer), tracked so a late result is routed
/// with the handle captured when the effect was emitted.
#[derive(Clone)]
enum PublicCall {
    GetExecution { handle: Option<SessionHandle> },
    GetSession { handle: SessionHandle },
    Detach,
    Attach,
    CancelAwaitToken { execution_id: String },
    CancelAwaitResult,
    PageUnload,
}

/// `EditorSessionController` — embed in the host state slice, drive with the
/// `on_*` methods and execute the returned effects against a [`SessionPort`].
#[allow(dead_code)] // T083; the editor view that embeds this has not landed. Remove then.
pub(crate) struct EditorSessionController {
    id: u64,
    initial_target: ExecutionTarget,
    latest_target: ExecutionTarget,
    owner: OwnerRef,
    session: Option<SessionView>,
    opening: Option<Opening>,
    queue: VecDeque<Operation>,
    current: Option<Operation>,
    attachment_token: Option<String>,
    execution: Option<ExecutionReceipt>,
    listeners: Vec<(u64, SessionListener)>,
    next_listener_id: u64,
    page_closer_armed: bool,
    next_call_id: u64,
    pending_public: HashMap<u64, PublicCall>,
    pub(crate) last_result: Option<Result<OpResult, SessionError>>,
}

impl EditorSessionController {
    /// Mirrors the React constructor. `id` is the stable identity the host uses
    /// to register/unregister the one page-unload closer.
    #[allow(dead_code)] // constructor; remove with the controller-driven view.
    pub(crate) fn new(id: u64, initial_target: ExecutionTarget, owner: OwnerRef) -> Self {
        Self {
            id,
            latest_target: initial_target.clone(),
            initial_target,
            owner,
            session: None,
            opening: None,
            queue: VecDeque::new(),
            current: None,
            attachment_token: None,
            execution: None,
            listeners: Vec::new(),
            next_listener_id: 1,
            page_closer_armed: false,
            next_call_id: 1,
            pending_public: HashMap::new(),
            last_result: None,
        }
    }

    /// `get snapshot`.
    #[allow(dead_code)] // read each frame by the view; remove then.
    pub(crate) fn snapshot(&self) -> Option<&SessionView> {
        self.session.as_ref()
    }

    /// `get activeExecution`.
    #[allow(dead_code)] // read by the view to drive cancel; remove then.
    pub(crate) fn active_execution(&self) -> Option<&ExecutionReceipt> {
        self.execution.as_ref()
    }

    /// The current attachment token, observed by the host after a context
    /// change decides whether to keep or replace it.
    #[allow(dead_code)] // read by the host attachment bridge; remove then.
    pub(crate) fn attachment_token(&self) -> Option<&str> {
        self.attachment_token.as_deref()
    }

    /// `latestTarget` — updated to the confirmed observed namespace whenever a
    /// session is accepted, opened or rebound.
    #[allow(dead_code)] // read by the host when opening a replacement; remove then.
    pub(crate) fn latest_target(&self) -> &ExecutionTarget {
        &self.latest_target
    }

    /// `subscribe(listener)` — returns an id for [`Self::unsubscribe`].
    #[allow(dead_code)] // observer API; remove when the view reads snapshot only.
    pub(crate) fn subscribe(&mut self, listener: SessionListener) -> u64 {
        let id = self.next_listener_id;
        self.next_listener_id += 1;
        self.listeners.push((id, listener));
        id
    }

    #[allow(dead_code)] // observer API; remove with subscribe.
    pub(crate) fn unsubscribe(&mut self, id: u64) {
        self.listeners.retain(|(candidate, _)| *candidate != id);
    }

    fn publish(&mut self) {
        for (_, listener) in &mut self.listeners {
            listener();
        }
    }

    fn alloc_call_id(&mut self) -> u64 {
        let id = self.next_call_id;
        self.next_call_id += 1;
        id
    }

    fn update_latest_namespace(&mut self, session: &SessionView) {
        if session.observed_context.confidence == ContextConfidence::Confirmed {
            self.latest_target.namespace = session.observed_context.namespace.clone();
        }
    }

    /// `token(operation)`: `issueSubmissionToken(operation, session?.handle,
    /// { connectionId: initialTarget.connectionId, owner })`.
    fn token_effects(&mut self, operation: &str) -> (u64, Vec<SessionEffect>) {
        let id = self.alloc_call_id();
        let handle = self.session.as_ref().map(|session| session.handle.clone());
        let effect = SessionEffect::IssueToken {
            call_id: id,
            operation: operation.to_owned(),
            handle,
            context: TokenContext {
                connection_id: self.initial_target.connection_id.clone(),
                owner: self.owner.clone(),
            },
        };
        (id, vec![effect])
    }

    // --- markSessionLost / acceptSessionError / acceptSession ----------------

    /// `markSessionLost(handle?)` — keeps the handle and execution receipt,
    /// only flips state and the observed transaction state.
    #[allow(dead_code)] // public recovery hook; remove with the controller-driven view.
    pub(crate) fn mark_session_lost(&mut self, handle: Option<&SessionHandle>) -> bool {
        let matches = matches!((self.session.as_ref(), handle), (Some(session), Some(handle)) if same_handle(Some(handle), Some(&session.handle)));
        if !matches {
            return false;
        }
        if let Some(session) = self.session.as_mut() {
            session.state = SessionState::Lost;
            session.observed_context.transaction_state = TransactionState::Unknown;
        }
        self.publish();
        true
    }

    /// `acceptSessionError` — only the three session codes are consumed.
    fn accept_session_error(&mut self, error: &SessionError, handle: Option<&SessionHandle>) -> bool {
        if !error.code.is_session_error() {
            return false;
        }
        self.mark_session_lost(handle)
    }

    /// `acceptSession(view)` — handle, lost-revival and revision guards.
    #[allow(dead_code)] // public observation hook; remove with the controller-driven view.
    pub(crate) fn accept_session(&mut self, view: SessionView) -> bool {
        let Some(current) = self.session.clone() else {
            return false;
        };
        if !same_handle(Some(&view.handle), Some(&current.handle)) {
            return false;
        }
        if current.state == SessionState::Lost
            && !matches!(view.state, SessionState::Lost | SessionState::Closed)
        {
            return false;
        }
        let decreasing = match (
            counter_value(CounterInput::Text(view.context_revision.clone())),
            counter_value(CounterInput::Text(current.context_revision.clone())),
        ) {
            (Ok(incoming), Ok(existing)) => incoming < existing,
            // A revision that cannot be trusted is never allowed to move the
            // session — refuse defensively instead of guessing.
            _ => true,
        };
        if decreasing {
            return false;
        }
        self.session = Some(view.clone());
        self.update_latest_namespace(&view);
        self.publish();
        true
    }

    // --- ensureSession -------------------------------------------------------

    fn start_open(&mut self, resume_op: bool) -> Vec<SessionEffect> {
        let (call_id, effects) = self.token_effects("openSession");
        self.opening = Some(Opening { call_id, phase: OpenPhase::AwaitToken, resume_op });
        effects
    }

    /// `ensureSession()` — refuses reuse, dedups concurrent opens via
    /// `opening`. Returns the error React would throw instead of a promise.
    #[allow(dead_code)] // public entry; remove with the controller-driven view.
    pub(crate) fn ensure_session(&mut self) -> Result<Vec<SessionEffect>, SessionError> {
        self.ensure_session_internal(false)
    }

    fn ensure_session_internal(&mut self, resume_op: bool) -> Result<Vec<SessionEffect>, SessionError> {
        if let Some(session) = self.session.as_ref() {
            if session.state.refuses_reuse() {
                return Err(SessionError::session_lost("Explicitly open a new session to continue."));
            }
            return Ok(Vec::new());
        }
        if let Some(opening) = self.opening.as_mut() {
            // A queued operation joining a public open must still be resumed
            // when the shared `opening` promise settles.
            if resume_op {
                opening.resume_op = true;
            }
            return Ok(Vec::new());
        }
        Ok(self.start_open(resume_op))
    }

    // --- serial queue --------------------------------------------------------

    fn enqueue(&mut self, op: Operation) -> Vec<SessionEffect> {
        self.queue.push_back(op);
        if self.current.is_none() {
            if let Some(next) = self.queue.pop_front() {
                self.current = Some(next);
                return self.start_current();
            }
        }
        Vec::new()
    }

    /// `tail = next.catch(() => undefined); const next = tail.then(op, op)`:
    /// after any outcome the next operation starts, and failures never leak.
    fn advance_queue(&mut self) -> Vec<SessionEffect> {
        self.current = None;
        if let Some(next) = self.queue.pop_front() {
            self.current = Some(next);
            return self.start_current();
        }
        Vec::new()
    }

    fn op_token_name(kind: OpKind) -> &'static str {
        match kind {
            OpKind::Execute => "executeInSession",
            OpKind::SetContext => "setSessionContext",
            OpKind::Close | OpKind::OpenNew => "",
        }
    }

    /// Drive the current operation to its next emitted effect.
    fn start_current(&mut self) -> Vec<SessionEffect> {
        let Some(op) = self.current.as_ref() else {
            return Vec::new();
        };
        let (kind, phase) = (op.kind, op.phase);

        match phase {
            Phase::WaitOpening => {
                if let Some(opening) = self.opening.as_mut() {
                    opening.resume_op = true;
                    return Vec::new();
                }
                if kind == OpKind::Close {
                    if self.session.is_none() {
                        self.last_result = Some(Ok(OpResult::None));
                        return self.advance_queue();
                    }
                    let handle = match self.session.as_ref() {
                        Some(session) => session.handle.clone(),
                        None => return self.advance_queue(),
                    };
                    let mode = self.current.as_ref().map_or(CloseMode::RequireNoTransaction, |op| op.mode);
                    let id = self.alloc_call_id();
                    if let Some(op) = self.current.as_mut() {
                        op.phase = Phase::Call;
                        op.call_id = id;
                    }
                    vec![SessionEffect::Close {
                        call_id: id,
                        handle,
                        mode,
                        purpose: ClosePurpose::Operation,
                    }]
                } else {
                    // openNewSession: an existing live session must be closed first.
                    let blocked = self
                        .session
                        .as_ref()
                        .is_some_and(|session| !session.state.is_lost_or_closed());
                    if blocked {
                        let message = "Close the current session before opening its replacement.";
                        self.last_result = Some(Err(SessionError {
                            code: ApiErrorCode::Other(message.to_owned()),
                            message: message.to_owned(),
                        }));
                        return self.advance_queue();
                    }
                    self.session = None;
                    self.execution = None;
                    self.attachment_token = None;
                    if let Some(op) = self.current.as_mut() {
                        op.phase = Phase::NeedSession;
                    }
                    self.start_current()
                }
            }
            Phase::NeedSession => {
                match self.ensure_session_internal(true) {
                    Err(error) => {
                        // `await this.ensureSession()` is outside execute's try.
                        self.last_result = Some(Err(error));
                        return self.advance_queue();
                    }
                    Ok(effects) => {
                        if self.session.is_some() {
                            let session = self.session.clone();
                            if let Some(session) = session {
                                let name = Self::op_token_name(kind);
                                let (id, token_effects) = self.token_effects(name);
                                if let Some(op) = self.current.as_mut() {
                                    op.session = Some(session);
                                    op.phase = Phase::IssueToken;
                                    op.call_id = id;
                                }
                                return token_effects;
                            }
                        }
                        effects
                    }
                }
            }
            _ => Vec::new(),
        }
    }

    // --- public commands -----------------------------------------------------

    /// `execute(call)` — serialized, captured expected revision.
    #[allow(dead_code)] // public command; remove with the controller-driven view.
    pub(crate) fn execute(&mut self, call: CommandCall) -> Vec<SessionEffect> {
        self.enqueue(Operation::new(OpKind::Execute, Some(call), None, CloseMode::RequireNoTransaction))
    }

    /// `setContext(desired)` — two-phase with revision.
    #[allow(dead_code)] // public command; remove with the controller-driven view.
    pub(crate) fn set_context(&mut self, desired: NamespaceTarget) -> Vec<SessionEffect> {
        self.enqueue(Operation::new(OpKind::SetContext, None, Some(desired), CloseMode::RequireNoTransaction))
    }

    /// `close(mode)` — serialized; awaits an in-flight open first.
    #[allow(dead_code)] // public command; remove with the controller-driven view.
    pub(crate) fn close(&mut self, mode: CloseMode) -> Vec<SessionEffect> {
        self.enqueue(Operation::new(OpKind::Close, None, None, mode))
    }

    /// `openNewSession()` — serialized replacement.
    #[allow(dead_code)] // public command; remove with the controller-driven view.
    pub(crate) fn open_new_session(&mut self) -> Vec<SessionEffect> {
        self.enqueue(Operation::new(OpKind::OpenNew, None, None, CloseMode::RequireNoTransaction))
    }

    /// `getExecution(executionId)` — captures the handle before the call.
    #[allow(dead_code)] // public command; remove with the controller-driven view.
    pub(crate) fn get_execution(&mut self, execution_id: String) -> Vec<SessionEffect> {
        let handle = self.session.as_ref().map(|session| session.handle.clone());
        let id = self.alloc_call_id();
        self.pending_public.insert(id, PublicCall::GetExecution { handle });
        vec![SessionEffect::GetExecution { call_id: id, execution_id }]
    }

    /// `refresh()` — captures the handle, then `acceptSession`.
    #[allow(dead_code)] // public command; remove with the controller-driven view.
    pub(crate) fn refresh(&mut self) -> Vec<SessionEffect> {
        let Some(handle) = self.session.as_ref().map(|session| session.handle.clone()) else {
            return Vec::new();
        };
        let id = self.alloc_call_id();
        self.pending_public.insert(id, PublicCall::GetSession { handle: handle.clone() });
        vec![SessionEffect::GetSession { call_id: id, handle }]
    }

    /// `detach()` — only when both session and attachment token exist.
    #[allow(dead_code)] // public command; remove with the controller-driven view.
    pub(crate) fn detach(&mut self) -> Vec<SessionEffect> {
        let (handle, token) = match (self.session.as_ref(), self.attachment_token.as_ref()) {
            (Some(session), Some(token)) => (session.handle.clone(), token.clone()),
            _ => return Vec::new(),
        };
        let id = self.alloc_call_id();
        self.pending_public.insert(id, PublicCall::Detach);
        vec![SessionEffect::Detach { call_id: id, handle, token }]
    }

    /// `attach()` — only when both session and attachment token exist.
    #[allow(dead_code)] // public command; remove with the controller-driven view.
    pub(crate) fn attach(&mut self) -> Vec<SessionEffect> {
        let (handle, token) = match (self.session.as_ref(), self.attachment_token.as_ref()) {
            (Some(session), Some(token)) => (session.handle.clone(), token.clone()),
            _ => return Vec::new(),
        };
        let id = self.alloc_call_id();
        self.pending_public.insert(id, PublicCall::Attach);
        vec![SessionEffect::Attach { call_id: id, handle, token }]
    }

    /// `cancel()` — token first, then the cancel call with the captured id.
    #[allow(dead_code)] // public command; remove with the controller-driven view.
    pub(crate) fn cancel(&mut self) -> Vec<SessionEffect> {
        let Some(execution_id) = self.execution.as_ref().map(|execution| execution.execution_id.clone()) else {
            return Vec::new();
        };
        let (id, effects) = self.token_effects("cancelExecution");
        self.pending_public.insert(id, PublicCall::CancelAwaitToken { execution_id });
        effects
    }

    /// Fire the one-shot page-unload closer with the *current* handle and
    /// `rollbackAndClose` (never `requireNoTransaction`, which would leak).
    #[allow(dead_code)] // host teardown hook; remove with the controller-driven view.
    pub(crate) fn fire_page_unload(&mut self) -> Vec<SessionEffect> {
        if !self.page_closer_armed {
            return Vec::new();
        }
        let Some(handle) = self.session.as_ref().map(|session| session.handle.clone()) else {
            return Vec::new();
        };
        self.page_closer_armed = false;
        let id = self.alloc_call_id();
        self.pending_public.insert(id, PublicCall::PageUnload);
        vec![SessionEffect::Close {
            call_id: id,
            handle,
            mode: CloseMode::RollbackAndClose,
            purpose: ClosePurpose::PageUnload,
        }]
    }

    fn disarm_page_closer(&mut self) -> Vec<SessionEffect> {
        if !self.page_closer_armed {
            return Vec::new();
        }
        self.page_closer_armed = false;
        vec![SessionEffect::UnregisterPageCloser { controller_id: self.id }]
    }

    /// `clone(owner)` — rejects a second editor with the same pane id. `id` is
    /// the host-assigned key for the clone's own page-unload closer.
    #[allow(dead_code)] // public command; remove with the controller-driven view.
    pub(crate) fn clone(&self, id: u64, owner: OwnerRef) -> Result<Self, &'static str> {
        if owner.same_editor_session(&self.owner) {
            return Err("Cloned editors require a distinct owner ID");
        }
        Ok(Self::new(id, self.latest_target.clone(), owner))
    }

}

#[path = "editor_session_results.rs"]
mod results;

/// Start of `createEditorSessionController`: emits the identity lookup. The
/// host resolves it against [`SessionPort::get_platform_identity`] and calls
/// [`ControllerCreation::complete`].
#[allow(dead_code)] // factory; remove with the controller-driven view.
pub(crate) struct ControllerCreation {
    call_id: u64,
    effects: Vec<SessionEffect>,
    initial_target: ExecutionTarget,
    editor_session_id: String,
}

#[allow(dead_code)] // factory; remove with the controller-driven view.
pub(crate) fn create_editor_session_controller(
    initial_target: ExecutionTarget,
    editor_session_id: String,
) -> ControllerCreation {
    ControllerCreation {
        call_id: 1,
        effects: vec![SessionEffect::Identity { call_id: 1 }],
        initial_target,
        editor_session_id,
    }
}

impl ControllerCreation {
    #[allow(dead_code)] // factory completion; remove with the controller-driven view.
    pub(crate) fn effects(&self) -> &[SessionEffect] {
        &self.effects
    }

    /// Mirrors the React factory: a blank `clientInstanceId`/`editorSessionId`
    /// throws `Missing editor owner identity`.
    #[allow(dead_code)] // factory completion; remove with the controller-driven view.
    pub(crate) fn complete(
        self,
        call_id: u64,
        result: Result<PlatformIdentity, SessionError>,
    ) -> Result<EditorSessionController, &'static str> {
        if call_id != self.call_id {
            return Err("stale identity response");
        }
        let identity = result.map_err(|_| "Missing editor owner identity")?;
        if identity.client_instance_id.is_empty() || self.editor_session_id.is_empty() {
            return Err("Missing editor owner identity");
        }
        Ok(EditorSessionController::new(
            1,
            self.initial_target,
            OwnerRef::Editor {
                client_instance_id: identity.client_instance_id,
                editor_session_id: self.editor_session_id,
            },
        ))
    }
}
